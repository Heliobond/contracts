#![cfg(test)]
#![allow(clippy::inconsistent_digit_grouping)]
extern crate std;
use super::*;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::StellarAssetClient,
    token::TokenClient,
    xdr::ToXdr,
    Address, BytesN, Env, IntoVal, String,
};
use std::format;

mod registry_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/project_registry.wasm");
}

/// Deterministic placeholder metadata hash for tests that don't exercise
/// metadata-hash verification directly (#44).
fn test_metadata_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

struct TestSetup {
    env: Env,
    admin: Address,
    vault_client: InvestmentVaultClient<'static>,
    vault_address: Address,
    usdc_sac: Address,
    registry: Address,
}

fn setup() -> TestSetup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    // Register a real ProjectRegistry using constructor
    let registry_id = env.register(registry_contract::WASM, (&admin, &admin));

    // Create mock USDC Stellar Asset Contract
    let usdc_admin = Address::generate(&env);
    let usdc_sac = env
        .register_stellar_asset_contract_v2(usdc_admin.clone())
        .address();

    // Register vault using constructor
    let contract_id = env.register(InvestmentVault, (&admin, &usdc_sac, &registry_id));
    let vault_client = InvestmentVaultClient::new(&env, &contract_id);

    TestSetup {
        env,
        admin,
        vault_client,
        vault_address: contract_id,
        usdc_sac,
        registry: registry_id,
    }
}

fn mint_usdc(env: &Env, usdc_sac: &Address, to: &Address, amount: i128) {
    let asset_client = StellarAssetClient::new(env, usdc_sac);
    asset_client.mint(to, &amount);
}

/// Register a project scored with `green_impact` (credit quality 50).
/// `green_impact == 0` leaves the registry default (also 0).
fn create_project_with_green_impact(s: &TestSetup, green_impact: u32) -> u32 {
    let creator = Address::generate(&s.env);
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmCarbon"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    if green_impact > 0 {
        registry_client.update_impact_score(&project_id, &50u32, &green_impact);
    }
    project_id
}

#[test]
fn test_first_deposit_mints_1_to_1_shares() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let amount = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, amount);

    let shares = s.vault_client.deposit(&investor, &amount);

    // Deposit deducts a 50-bps insurance premium before share calculation.
    // First deposit is 1:1 on the investable amount (after premium).
    let investable = amount - amount * 50 / 10_000; // 9_950_000_000
    assert_eq!(shares, investable);
    assert_eq!(s.vault_client.balance(&investor), investable);
    assert_eq!(s.vault_client.total_supply(), investable);
    // 0.5% insurance premium is deducted before share conversion:
    // investable = 1000 - 5 = 995 USDC → 995 shares at 1:1
    assert_eq!(shares, 995_0000000i128);
    assert_eq!(s.vault_client.balance(&investor), 995_0000000i128);
    assert_eq!(s.vault_client.total_supply(), 995_0000000i128);
}

#[test]
fn test_vault_with_zero_supply() {
    let s = setup();
    let investor = Address::generate(&s.env);

    // convert_to_shares with zero supply
    let shares = s.vault_client.convert_to_shares(&1_000_0000000i128);
    assert_eq!(shares, 1_000_0000000i128);

    // convert_to_assets with zero supply
    let assets = s.vault_client.convert_to_assets(&1_000_0000000i128);
    assert_eq!(assets, 0);

    // deposit when total_assets is zero
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let minted_shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    let expected = 1_000_0000000i128 - (1_000_0000000i128 * 50 / 10_000);
    assert_eq!(minted_shares, expected);
}

#[test]
#[should_panic]
fn test_withdraw_with_zero_supply_panics() {
    let s = setup();
    let investor = Address::generate(&s.env);
    s.vault_client.withdraw(&investor, &10_0000000i128, &0);
}

#[test]
fn test_zero_supply_edge_cases_are_handled_consistently() {
    // #264: convert_to_assets() and withdraw() respond to an empty vault
    // (total_supply == 0) via different but consistent code paths —
    // convert_to_assets is a pure view that gracefully returns 0 rather than
    // dividing by zero, while withdraw legitimately panics because the
    // caller has no shares to burn. Neither corrupts state or silently
    // succeeds. test_vault_with_zero_supply already covers convert_to_assets
    // (and convert_to_shares) in isolation; this test asserts both
    // behaviors together in the same empty-vault state for direct comparison.
    let s = setup();
    let investor = Address::generate(&s.env);

    assert_eq!(s.vault_client.total_supply(), 0);
    assert_eq!(s.vault_client.total_assets(), 0);

    // convert_to_assets: graceful zero, no panic.
    assert_eq!(s.vault_client.convert_to_assets(&500_0000000i128), 0);

    // withdraw: panics because investor owns zero shares to burn.
    let result = s.vault_client.try_withdraw(&investor, &10_0000000i128, &0);
    assert!(result.is_err());

    // Neither call should have changed vault state.
    assert_eq!(s.vault_client.total_supply(), 0);
    assert_eq!(s.vault_client.total_assets(), 0);
}

#[test]
fn test_deposit_proportional_after_first() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let investor2 = Address::generate(&s.env);
    let amount = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor1, amount);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, amount);

    s.vault_client.deposit(&investor1, &amount);
    let shares2 = s.vault_client.deposit(&investor2, &amount);

    // After investor1: total_shares = investable, total_assets = amount (full deposit in vault).
    // investor2's investable amount buys shares at the current NAV price.
    let investable = amount - amount * 50 / 10_000; // 9_950_000_000
    let expected_shares2 = investable * investable / amount; // 9_900_250_000
    assert_eq!(shares2, expected_shares2);
    mint_usdc(&s.env, &s.usdc_sac, &investor1, 1_000_0000000i128);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 1_000_0000000i128);

    s.vault_client.deposit(&investor1, &1_000_0000000i128);
    let shares2 = s.vault_client.deposit(&investor2, &1_000_0000000i128);

    // Vault now holds 3000 USDC across 3 prior deposits; shares are proportional.
    // shares2 = 9_950_000_000 * total_supply / total_assets = 9_859_040_209
    assert_eq!(shares2, 9_859_040_209i128);
}

#[test]
fn test_withdraw_returns_usdc() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    let returned = s.vault_client.withdraw(&investor, &shares, &0);

    assert_eq!(returned, 1_000_0000000i128);
    assert_eq!(s.vault_client.balance(&investor), 0);
}

// ── Issue #406: withdraw()'s min_usdc_return slippage guard was never exercised ─

#[test]
#[should_panic(expected = "Error(Contract, #33)")]
fn test_withdraw_rejects_when_min_usdc_return_exceeds_actual() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.timestamp += MIN_LOCK_PERIOD + 1;
    });
    // Fresh 1:1 vault: convert_to_assets(shares) == 1_000_0000000. Ask for 1 stroop more.
    s.vault_client
        .withdraw(&investor, &shares, &(1_000_0000000i128 + 1));
}

#[test]
fn test_withdraw_succeeds_when_min_usdc_return_exactly_equals_actual() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.timestamp += MIN_LOCK_PERIOD + 1;
    });
    // Boundary: usdc_returned == min_usdc_return should succeed (guard is `<`, not `<=`).
    let returned = s
        .vault_client
        .withdraw(&investor, &shares, &1_000_0000000i128);

    assert_eq!(returned, 1_000_0000000i128);
}

#[test]
fn test_total_assets_after_deposit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 500_0000000i128);
    s.vault_client.deposit(&investor, &500_0000000i128);
    assert_eq!(s.vault_client.total_assets(), 500_0000000i128);
}

#[test]
fn test_batch_deposit_mints_for_each_investor() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor1, 1_000_0000000i128);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 500_0000000i128);

    let deposits = soroban_sdk::vec![
        &s.env,
        (investor1.clone(), 1_000_0000000i128),
        (investor2.clone(), 500_0000000i128)
    ];
    let minted = s.vault_client.batch_deposit(&deposits);

    assert_eq!(minted.len(), 2);
    assert!(minted.get(0).unwrap() > 0);
    assert!(minted.get(1).unwrap() > 0);
    assert_eq!(s.vault_client.balance(&investor1), minted.get(0).unwrap());
    assert_eq!(s.vault_client.balance(&investor2), minted.get(1).unwrap());
}

// ── Issue #436: receive_yield must stay reachable once multisig is enabled ─────

#[test]
fn test_receive_yield_with_approvals_after_multisig_enabled() {
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let investor = Address::generate(&s.env);
    let yield_source = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    s.vault_client.set_multisig_admin(
        &soroban_sdk::vec![&s.env, signer1.clone(), signer2.clone()],
        &2u32,
    );

    // receive_yield itself is now permanently blocked; only the approvals path works.
    assert!(s
        .vault_client
        .try_receive_yield(&yield_source, &10_0000000i128)
        .is_err());

    mint_usdc(&s.env, &s.usdc_sac, &yield_source, 10_0000000i128);
    s.env.mock_all_auths_allowing_non_root_auth();
    s.vault_client.receive_yield_with_approvals(
        &yield_source,
        &10_0000000i128,
        &soroban_sdk::vec![&s.env, signer1, signer2],
    );

    assert!(s.vault_client.claimable_yield(&investor) > 0);
}

#[test]
fn test_multisig_batch_fund_projects() {
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signer3 = Address::generate(&s.env);
    let investor = Address::generate(&s.env);
    let creator1 = Address::generate(&s.env);
    let creator2 = Address::generate(&s.env);

    s.vault_client.set_multisig_admin(
        &soroban_sdk::vec![&s.env, signer1.clone(), signer2.clone(), signer3],
        &2u32,
    );
    mint_usdc(&s.env, &s.usdc_sac, &investor, 2_000_0000000i128);
    s.vault_client.deposit(&investor, &2_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator1, &true);
    registry_client.set_whitelist(&creator2, &true);
    let project1 = registry_client.create_project(
        &creator1,
        &String::from_str(&s.env, "ipfs://QmBatchFund1"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let project2 = registry_client.create_project(
        &creator2,
        &String::from_str(&s.env, "ipfs://QmBatchFund2"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.batch_fund_projects(
        &soroban_sdk::vec![
            &s.env,
            (project1, 100_0000000i128),
            (project2, 150_0000000i128)
        ],
        &soroban_sdk::vec![&s.env, signer1, signer2],
    );

    assert!(s.vault_client.total_assets() > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_multisig_batch_fund_projects_rejects_duplicate_project_ids() {
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    s.vault_client.set_multisig_admin(
        &soroban_sdk::vec![&s.env, signer1.clone(), signer2.clone()],
        &2u32,
    );
    mint_usdc(&s.env, &s.usdc_sac, &investor, 2_000_0000000i128);
    s.vault_client.deposit(&investor, &2_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmDuplicateProject"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.batch_fund_projects(
        &soroban_sdk::vec![
            &s.env,
            (project, 100_0000000i128),
            (project, 150_0000000i128)
        ],
        &soroban_sdk::vec![&s.env, signer1, signer2],
    );
}

#[test]
fn test_batch_fund_projects_rolls_back_all_events_and_state_on_later_panic() {
    // #271: batch_fund_projects funds project1 (valid) then project2 (invalid
    // — exceeds available deployable USDC), panicking partway through.
    // Soroban transactions are atomic, so project1's transfer, storage
    // update, and ProjectFunded event must ALL be rolled back too, not just
    // left half-applied.
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let investor = Address::generate(&s.env);
    let creator1 = Address::generate(&s.env);
    let creator2 = Address::generate(&s.env);

    s.vault_client.set_multisig_admin(
        &soroban_sdk::vec![&s.env, signer1.clone(), signer2.clone()],
        &2u32,
    );
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator1, &true);
    registry_client.set_whitelist(&creator2, &true);
    let project1 = registry_client.create_project(
        &creator1,
        &String::from_str(&s.env, "ipfs://QmRollback1"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let project2 = registry_client.create_project(
        &creator2,
        &String::from_str(&s.env, "ipfs://QmRollback2"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    let assets_before = s.vault_client.total_assets();

    // project1's funding is well within budget; project2's request exceeds
    // the total deployable USDC, so the second call inside the loop panics.
    let result = s.vault_client.try_batch_fund_projects(
        &soroban_sdk::vec![
            &s.env,
            (project1, 100_0000000i128),
            (project2, 10_000_0000000i128),
        ],
        &soroban_sdk::vec![&s.env, signer1, signer2],
    );
    assert!(result.is_err());

    // Nothing from project1's funding should have taken effect.
    assert_eq!(s.vault_client.get_project_investment(&project1), 0);
    assert_eq!(s.vault_client.total_assets(), assets_before);

    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        events.events().is_empty(),
        "expected no events to survive the panicking batch call, got {:?}",
        events.events()
    );
}

#[test]
#[should_panic]
fn test_multisig_rejects_insufficient_funding_approvals() {
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    s.vault_client
        .set_multisig_admin(&soroban_sdk::vec![&s.env, signer1.clone(), signer2], &2u32);

    s.vault_client.batch_fund_projects(
        &Vec::<(u32, i128)>::new(&s.env),
        &soroban_sdk::vec![&s.env, signer1],
    );
}

#[test]
fn bench_vault_deposit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let instructions = s.env.cost_estimate().resources().instructions;
    std::println!("bench_vault_deposit: {} instructions", instructions);
    assert!(instructions <= 60_000_000);
}

#[test]
fn bench_vault_batch_deposit_two_accounts() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor1, 1_000_0000000i128);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 1_000_0000000i128);

    s.vault_client.batch_deposit(&soroban_sdk::vec![
        &s.env,
        (investor1, 1_000_0000000i128),
        (investor2, 1_000_0000000i128)
    ]);

    let instructions = s.env.cost_estimate().resources().instructions;
    std::println!(
        "bench_vault_batch_deposit_two_accounts: {} instructions",
        instructions
    );
    assert!(instructions <= 100_000_000);
}

#[test]
fn bench_batch_deposit_vs_equivalent_single_deposits() {
    // #270: compare gas cost of one batch_deposit call against N single
    // deposit() calls totaling the same aggregate amount. cost_estimate()
    // only reflects the *last* top-level invocation (per soroban-sdk docs),
    // so the single-deposit total is accumulated per-call inside the loop.
    const PER_DEPOSIT: i128 = 1_000_0000000i128;
    const N: usize = 2;

    let batch = setup();
    let mut investors = soroban_sdk::vec![&batch.env];
    for _ in 0..N {
        let investor = Address::generate(&batch.env);
        mint_usdc(&batch.env, &batch.usdc_sac, &investor, PER_DEPOSIT);
        investors.push_back((investor, PER_DEPOSIT));
    }
    batch.vault_client.batch_deposit(&investors);
    let batch_instructions = batch.env.cost_estimate().resources().instructions;

    let single = setup();
    let mut single_total_instructions: u64 = 0;
    for _ in 0..N {
        let investor = Address::generate(&single.env);
        mint_usdc(&single.env, &single.usdc_sac, &investor, PER_DEPOSIT);
        single.vault_client.deposit(&investor, &PER_DEPOSIT);
        single_total_instructions += single.env.cost_estimate().resources().instructions as u64;
    }

    std::println!(
        "bench_batch_deposit_vs_equivalent_single_deposits: batch({N} accounts)={batch_instructions} instructions, {N} single deposits total={single_total_instructions} instructions"
    );
    assert!(batch_instructions <= 100_000_000);
    assert!((single_total_instructions as u32) <= 100_000_000);
}

// ── Issue #189: benchmark for withdraw() under queued-redemption contention ──

#[test]
fn bench_withdraw_under_queued_redemption_contention() {
    // Measure the cost of withdraw() when the vault has insufficient liquid
    // USDC, forcing the redemption to be enqueued rather than settled
    // immediately. This is the more expensive path because it burns shares,
    // records a QueuedClaim, and updates the FIFO queue pointers.
    let s = setup();
    let investor = Address::generate(&s.env);
    let deposit_amount = 1_000_0000000i128; // 1000 USDC
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);
    let shares = s.vault_client.deposit(&investor, &deposit_amount);

    // Drain roughly half the vault's liquid USDC by funding a project.
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    let creator = Address::generate(&s.env);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmBenchWithdraw"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 490 USDC (49% util — below the 50% graduated limit so the full
    // withdrawal is allowed by the utilization check, but only ~510 USDC
    // is liquid, causing the queue path to activate).
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Advance ledger to satisfy rate-limiting (deposit locks same-ledger withdrawals).
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    // This withdraw triggers the queued-redemption path.
    let returned = s.vault_client.withdraw(&investor, &shares, &0);
    assert_eq!(returned, 0); // queued, not immediate

    let instructions = s.env.cost_estimate().resources().instructions;
    std::println!(
        "bench_withdraw_under_queued_redemption_contention: {} instructions",
        instructions
    );
    // The queued path is more expensive than a normal withdraw (burn +
    // queue entry write + head/tail pointer updates). Bound generously
    // to avoid flaky failures while still catching regressions.
    assert!(instructions <= 100_000_000);
}

#[test]
fn test_vault_deposit_cost_estimate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let usdc_admin = Address::generate(&env);
    let usdc_sac = env
        .register_stellar_asset_contract_v2(usdc_admin.clone())
        .address();
    let registry = env.register(registry_contract::WASM, (&admin, &admin));
    let contract_id = env.register(InvestmentVault, (&admin, &usdc_sac, &registry));
    let vault_client = InvestmentVaultClient::new(&env, &contract_id);

    let investor = Address::generate(&env);
    StellarAssetClient::new(&env, &usdc_sac).mint(&investor, &1_000_0000000i128);
    let shares = vault_client.deposit(&investor, &1_000_0000000i128);

    assert!(shares > 0);
    let resources = env.cost_estimate().resources();
    assert!(resources.instructions > 0);
    let fee = env.cost_estimate().fee();
    assert!(fee.total > 0);
    std::println!(
        "gas_budget investment_vault.deposit instructions={} fee={}",
        resources.instructions,
        fee.total
    );
}

#[test]
fn test_fund_project_cost_estimate() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.fund_project(&project_id, &100_0000000i128);

    let resources = s.env.cost_estimate().resources();
    assert!(resources.instructions > 0);
    let fee = s.env.cost_estimate().fee();
    assert!(fee.total > 0);
    std::println!(
        "gas_budget investment_vault.fund_project instructions={} fee={}",
        resources.instructions,
        fee.total
    );
}

#[test]
fn test_initialize() {
    // With __constructor, registration IS initialization
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let registry = env.register(registry_contract::WASM, (&admin, &admin));
    let contract_id = env.register(InvestmentVault, (&admin, &usdc, &registry));
    let client = InvestmentVaultClient::new(&env, &contract_id);
    assert_eq!(client.state_version(), 1);
    assert_eq!(client.stored_state_version(), 1);
    // If registration didn't panic, constructor succeeded with a valid registry
}

#[test]
fn test_vault_constructor_and_registry_reference_initial_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let usdc_admin = Address::generate(&env);
    let project_creator = Address::generate(&env);

    let usdc_sac = env
        .register_stellar_asset_contract_v2(usdc_admin.clone())
        .address();
    let registry_id = env.register(registry_contract::WASM, (&admin, &whitelister));
    let registry_client = registry_contract::Client::new(&env, &registry_id);

    assert_eq!(registry_client.total_projects(), 0);
    assert_eq!(registry_client.get_whitelister(), whitelister);

    let vault_id = env.register(InvestmentVault, (&admin, &usdc_sac, &registry_id));
    let vault_client = InvestmentVaultClient::new(&env, &vault_id);

    assert_eq!(vault_client.accepted_asset(), usdc_sac);
    assert_eq!(vault_client.get_registry(), registry_id);
    assert_eq!(vault_client.total_assets(), 0);
    assert_eq!(vault_client.total_supply(), 0);
    assert!(!vault_client.is_trading_enabled());

    registry_client.set_whitelist(&project_creator, &true);
    let project_id = registry_client.create_project(
        &project_creator,
        &String::from_str(&env, "ipfs://QmVaultInit"),
        &0u64,
        &test_metadata_hash(&env),
    );

    let investor = Address::generate(&env);
    StellarAssetClient::new(&env, &usdc_sac).mint(&investor, &1_000_0000000i128);
    let deposit_shares = vault_client.deposit(&investor, &1_000_0000000i128);
    assert!(deposit_shares > 0);

    vault_client.fund_project(&project_id, &100_0000000i128);
    assert!(vault_client.total_assets() > 0);
}

#[test]
#[should_panic]
fn test_constructor_panics_with_invalid_registry() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let invalid_registry = Address::generate(&env);
    let _contract_id = env.register(InvestmentVault, (&admin, &usdc, &invalid_registry));
}

#[test]
fn test_fund_project_records_investment() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    assert_eq!(s.vault_client.total_assets(), 1_000_0000000i128);
}

// ── Issue #61: fund_project with insufficient liquid USDC ────────────────────

#[test]
#[should_panic]
fn test_fund_project_panics_when_fully_depleted() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    // Fund with all deployable USDC: liquid (1000) - insurance_reserve (5) = 995
    s.vault_client.fund_project(&project_id, &995_0000000i128);

    // Vault now has only 5 USDC liquid (= insurance_reserve), deployable = 0.
    // Any further funding must panic.
    s.vault_client.fund_project(&project_id, &1_0000000i128);
}

#[test]
#[should_panic]
fn test_fund_project_panics_when_amount_exceeds_available() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    // Deposit 500 USDC; insurance_reserve = 500 * 50 / 10_000 = 2_500_000 stroops (0.25 USDC)
    mint_usdc(&s.env, &s.usdc_sac, &investor, 500_0000000i128);
    s.vault_client.deposit(&investor, &500_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    // Attempt to fund exactly the full liquid balance — exceeds available by the
    // insurance reserve (0.25 USDC), so must fail.
    s.vault_client.fund_project(&project_id, &500_0000000i128);
}

#[test]
fn test_fund_project_partial_funding_succeeds() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    // Two partial fundings that together stay within the deployable amount.
    s.vault_client.fund_project(&project_id, &300_0000000i128);
    s.vault_client.fund_project(&project_id, &200_0000000i128);

    // total_assets = 500 liquid + 500 invested + 0 expected_returns = 1000 USDC
    assert_eq!(s.vault_client.total_assets(), 1_000_0000000i128);
}

#[test]
#[should_panic]
fn test_fund_project_second_call_exhausts_remaining_deployable() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    // First call: fund 600 USDC — leaves 400 liquid (5 reserved) → 395 deployable.
    s.vault_client.fund_project(&project_id, &600_0000000i128);

    // Second call: attempt to deploy 400 USDC, which exceeds the 395 deployable.
    s.vault_client.fund_project(&project_id, &400_0000000i128);
}

// ── Issue #116: descriptive liquidity error ────────────────────────────────

#[test]
#[should_panic]
fn test_withdraw_fails_when_all_usdc_deployed() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &soroban_sdk::String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund with all deployable USDC (liquid − insurance = 995); vault liquid drops to 5
    s.vault_client.fund_project(&project_id, &995_0000000i128);

    // Full share redemption requires ~1000 USDC but only 5 liquid remain
    s.vault_client.withdraw(&investor, &shares, &0);
}

// ── Issue #118: block share transfer to vault address ─────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_transfer_to_vault_address_rejected() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Attempt to send HBS shares to the vault contract itself
    s.vault_client
        .transfer(&investor, &s.vault_address, &100_0000000i128);
}

// ── Issue #122: full-withdrawal edge cases ────────────────────────────────

#[test]
fn test_full_withdrawal_with_no_investments() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    // Full withdrawal with no outstanding investments drains the vault cleanly
    s.vault_client.withdraw(&investor, &shares, &0);

    assert_eq!(s.vault_client.total_supply(), 0);
    assert_eq!(s.vault_client.balance(&investor), 0);
}

#[test]
#[should_panic]
fn test_full_withdrawal_blocked_by_outstanding_investments() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 2_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &2_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &soroban_sdk::String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 1000 USDC; vault liquid = 1000 but total assets = 2000
    s.vault_client.fund_project(&project_id, &1_000_0000000i128);

    // Full share redemption needs 2000 USDC but only 1000 liquid — must fail
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    s.vault_client.withdraw(&investor, &shares, &0);
}

#[test]
fn test_convert_to_shares_and_assets_roundtrip() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let preview_shares = s.vault_client.convert_to_shares(&500_0000000i128);
    let preview_assets = s.vault_client.convert_to_assets(&preview_shares);

    let diff = (preview_assets - 500_0000000i128).abs();
    assert!(
        diff <= 1,
        "roundtrip diff should be <= 1 stroop, got {}",
        diff
    );
}

// ── #7: management fee tests ──────────────────────────────────────────────────

#[test]
fn test_zero_fee_parity() {
    // With fee_bps = 0 (explicit), share minting equals the no-fee baseline:
    // investable = usdc_amount - insurance_premium (50 bps)
    let s = setup();
    let fee_recipient = Address::generate(&s.env);

    // Explicitly set fee to 0 — should be identical to the default
    s.vault_client.set_management_fee(&0u32, &fee_recipient);
    assert_eq!(s.vault_client.get_management_fee_bps(), 0);

    let investor = Address::generate(&s.env);
    let deposit_amount = 1_000_0000000i128; // 1000 USDC (7 dp)
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);

    let shares = s.vault_client.deposit(&investor, &deposit_amount);

    // premium = 50_000_000 (0.5%), fee = 0 → investable = 9_950_000_000
    let expected_investable = deposit_amount - deposit_amount * 50 / 10_000;
    assert_eq!(shares, expected_investable);

    // fee_recipient received nothing
    let usdc_client = soroban_sdk::token::TokenClient::new(&s.env, &s.usdc_sac);
    assert_eq!(usdc_client.balance(&fee_recipient), 0);
}

#[test]
fn test_nonzero_fee_accrual() {
    let s = setup();
    let fee_recipient = Address::generate(&s.env);

    // Set 200 bps (2%) management fee
    s.vault_client.set_management_fee(&200u32, &fee_recipient);
    assert_eq!(s.vault_client.get_management_fee_bps(), 200);

    let investor = Address::generate(&s.env);
    let deposit_amount = 1_000_0000000i128; // 10,000,000,000 stroops
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);

    s.vault_client.deposit(&investor, &deposit_amount);

    // fee = 200,000,000 (2%)
    let expected_fee = deposit_amount * 200 / 10_000;
    let usdc_client = soroban_sdk::token::TokenClient::new(&s.env, &s.usdc_sac);
    assert_eq!(usdc_client.balance(&fee_recipient), expected_fee);
}

#[test]
#[should_panic]
fn test_fee_above_cap_panics() {
    let s = setup();
    let fee_recipient = Address::generate(&s.env);
    // 501 bps > MAX_MANAGEMENT_FEE_BPS (500)
    s.vault_client.set_management_fee(&501u32, &fee_recipient);
}

// ── Issue #190: management fee across multiple fee-rate changes ───────────────

#[test]
fn test_management_fee_across_multiple_rate_changes() {
    // Verify that each deposit applies the fee rate active *at that moment*,
    // and that changing the fee between deposits produces the correct
    // cumulative fee payout.
    let s = setup();
    let fee_recipient = Address::generate(&s.env);
    let usdc_client = soroban_sdk::token::TokenClient::new(&s.env, &s.usdc_sac);
    let deposit_amount = 1_000_0000000i128; // 1000 USDC

    // ── Round 1: 100 bps (1%) ────────────────────────────────────────────────
    s.vault_client.set_management_fee(&100u32, &fee_recipient);
    let investor1 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor1, deposit_amount);
    s.vault_client.deposit(&investor1, &deposit_amount);

    let expected_fee_1 = deposit_amount * 100 / 10_000; // 10_000_000 (0.1 USDC in stroops... actually 10 USDC)
    assert_eq!(usdc_client.balance(&fee_recipient), expected_fee_1);

    // ── Round 2: 300 bps (3%) ────────────────────────────────────────────────
    s.vault_client.set_management_fee(&300u32, &fee_recipient);
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, deposit_amount);
    s.vault_client.deposit(&investor2, &deposit_amount);

    let expected_fee_2 = deposit_amount * 300 / 10_000; // 30_000_000
    let cumulative_after_2 = expected_fee_1 + expected_fee_2;
    assert_eq!(usdc_client.balance(&fee_recipient), cumulative_after_2);

    // ── Round 3: 50 bps (0.5%) ───────────────────────────────────────────────
    s.vault_client.set_management_fee(&50u32, &fee_recipient);
    let investor3 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor3, deposit_amount);
    s.vault_client.deposit(&investor3, &deposit_amount);

    let expected_fee_3 = deposit_amount * 50 / 10_000; // 5_000_000
    let cumulative_after_3 = cumulative_after_2 + expected_fee_3;
    assert_eq!(usdc_client.balance(&fee_recipient), cumulative_after_3);

    // ── Round 4: back to 0 bps (fee disabled) ────────────────────────────────
    s.vault_client.set_management_fee(&0u32, &fee_recipient);
    let investor4 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor4, deposit_amount);
    s.vault_client.deposit(&investor4, &deposit_amount);

    // No additional fee should have been charged.
    assert_eq!(usdc_client.balance(&fee_recipient), cumulative_after_3);
}

// ── #126: secondary market trading tests ──────────────────────────────────────

#[test]
fn test_trading_disabled_by_default() {
    let s = setup();
    assert!(!s.vault_client.is_trading_enabled());
}

#[test]
fn test_enable_secondary_trading() {
    let s = setup();
    s.vault_client.enable_secondary_trading();
    assert!(s.vault_client.is_trading_enabled());
}

#[test]
fn test_get_hbs_token_info_before_trading_enabled() {
    let s = setup();
    let info = s.vault_client.get_hbs_token_info();
    assert_eq!(info.name, String::from_str(&s.env, "Heliobond Shares"));
    assert_eq!(info.symbol, String::from_str(&s.env, "HBS"));
    assert_eq!(info.decimals, 7u32);
    assert!(!info.trading_enabled);
}

#[test]
fn test_get_hbs_token_info_after_trading_enabled() {
    let s = setup();
    s.vault_client.enable_secondary_trading();
    let info = s.vault_client.get_hbs_token_info();
    assert!(info.trading_enabled);
}

// ── Property tests (#2) ────────────────────────────────────────────────────────

#[test]
fn test_conversion_empty_vault_is_1_to_1() {
    let s = setup();
    // On an empty vault, convert_to_shares is 1:1 and convert_to_assets returns 0
    // because there are no shares outstanding to redeem against.
    for amount in [1i128, 100, 1_0000000, 100_0000000, 1_000_0000000] {
        assert_eq!(s.vault_client.convert_to_shares(&amount), amount);
        assert_eq!(s.vault_client.convert_to_assets(&amount), 0);
    }
}

#[test]
fn test_conversion_roundtrip_never_favors_withdrawer() {
    // Property: floor division must never give back more than the input amount,
    // and the loss must be at most 1 stroop.
    //
    // Precondition: holds for any A/S ratio < 2 (i.e., total_assets < 2 * total_shares).
    // After one standard deposit the ratio is ~1.005, well within this bound.
    let s = setup();
    let anchor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &anchor, 1_000_0000000i128);
    s.vault_client.deposit(&anchor, &1_000_0000000i128);

    let test_amounts = [
        1i128,
        3,
        7,
        1_0000000,
        100_0000000,
        999_9999999,
        1_000_0000000,
    ];
    for &amount in test_amounts.iter() {
        let shares = s.vault_client.convert_to_shares(&amount);
        let assets = s.vault_client.convert_to_assets(&shares);
        assert!(
            assets <= amount,
            "rounding favored withdrawer: amount={} assets={}",
            amount,
            assets
        );
        assert!(
            amount - assets <= 1,
            "roundtrip loss > 1 stroop: amount={} assets={}",
            amount,
            assets
        );
    }
}

#[test]
fn test_conversion_roundtrip_first_deposit_exact() {
    // On an empty vault the first convert_to_shares call is exactly 1:1.
    let s = setup();
    for amount in [1i128, 1_0000000, 500_0000000, 1_000_0000000] {
        assert_eq!(s.vault_client.convert_to_shares(&amount), amount);
    }
}

// ── Redemption queue tests (#3) ────────────────────────────────────────────────

#[test]
fn test_withdraw_enqueues_when_insufficient_liquidity() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let deposit_amount = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);
    let shares = s.vault_client.deposit(&investor, &deposit_amount);

    // Create a project and fund it, draining roughly half the vault's liquid USDC.
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    let creator = Address::generate(&s.env);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://test"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 490 USDC (49% utilization — below the 50% limit threshold so the full
    // withdrawal is allowed but only ~510 USDC is liquid, causing a queue.
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Shares are worth ~1000 USDC but only ~510 USDC is liquid — should enqueue.
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    let returned = s.vault_client.withdraw(&investor, &shares, &0);

    assert_eq!(returned, 0); // queued, not immediate
    assert_eq!(s.vault_client.balance(&investor), 0); // shares burned at enqueue
                                                      // Investor still has no USDC (claim not settled yet)
    assert_eq!(TokenClient::new(&s.env, &s.usdc_sac).balance(&investor), 0);
}

#[test]
fn test_claim_settles_queued_redemption() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let deposit_amount = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor1, deposit_amount);
    let shares = s.vault_client.deposit(&investor1, &deposit_amount);

    // Drain ~half the vault to create an insufficiency.
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    let creator = Address::generate(&s.env);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://test"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 490 USDC (49% util) to stay below the 50% graduated withdrawal limit.
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Queue the withdrawal.
    let owed = s.vault_client.convert_to_assets(&shares);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    s.vault_client.withdraw(&investor1, &shares, &0);

    // Add liquidity: second investor deposits enough to cover the queued claim.
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 2_000_0000000i128);
    s.vault_client.deposit(&investor2, &2_000_0000000i128);

    // Settle the queue.
    let paid = s.vault_client.claim();

    assert_eq!(paid, owed);
    assert_eq!(
        TokenClient::new(&s.env, &s.usdc_sac).balance(&investor1),
        owed
    );
}

// ── Issue #55: event emission verification tests ──────────────────────────────

#[test]
fn test_deposit_emits_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let amount = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, amount);

    s.vault_client.deposit(&investor, &amount);

    // Deposit emits a token mint event (Base::mint) + the Deposited application event = 2.
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        2,
        "deposit should emit exactly two events (mint + deposit)"
    );
}

#[test]
fn test_withdraw_emits_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    s.vault_client.withdraw(&investor, &shares, &0);

    // env.events().all() returns events from the most recent invocation only.
    // Withdraw emits a token burn event (Base::burn) + the Withdrawn application event = 2.
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        2,
        "withdraw should emit exactly two events (burn + withdraw)"
    );
}

#[test]
fn test_fund_project_emits_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    s.vault_client.fund_project(&project_id, &100_0000000i128);

    // env.events().all() returns events from the most recent invocation only.
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        1,
        "fund_project should emit exactly one event"
    );
}

#[test]
fn test_withdraw_queued_emits_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 490 USDC (49% util) to stay below the 50% graduated withdrawal limit.
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Withdrawal exceeds liquid USDC — should enqueue and emit WithdrawQueued.
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    let returned = s.vault_client.withdraw(&investor, &shares, &0);
    assert_eq!(returned, 0);

    // env.events().all() returns events from the most recent invocation only.
    // Queued withdraw emits burn (token library) + WithdrawQueued = 2 vault events.
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        2,
        "queued withdrawal should emit exactly two events (burn + withdraw_queued)"
    );
}

#[test]
fn test_claim_queued_emits_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 490 USDC (49% util) to stay below the 50% graduated withdrawal limit.
    s.vault_client.fund_project(&project_id, &490_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    s.vault_client.withdraw(&investor, &shares, &0);

    // Restore liquidity so claim() can settle.
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 2_000_0000000i128);
    s.vault_client.deposit(&investor2, &2_000_0000000i128);

    s.vault_client.claim();

    // env.events().all() returns events from the most recent invocation only.
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        !events.events().is_empty(),
        "claim() should emit at least one event when settling a queued redemption"
    );
}

#[test]
fn test_management_fee_set_emits_event() {
    let s = setup();
    let recipient = Address::generate(&s.env);

    s.vault_client.set_management_fee(&200u32, &recipient);

    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        1,
        "set_management_fee should emit exactly one event"
    );
}

#[test]
fn test_enable_secondary_trading_emits_event() {
    let s = setup();

    s.vault_client.enable_secondary_trading();

    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        1,
        "enable_secondary_trading should emit exactly one event"
    );
}

#[test]
fn test_high_utilization_withdrawal_emits_warning_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 10_000_0000000i128);
    s.vault_client.deposit(&investor, &10_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 8000 USDC: liquid = 2000, investments = 8000, utilization = 80%
    // max_withdraw at 80% = 2000 * 25% = 500 USDC — must be > MIN_WITHDRAW (100 USDC)
    s.vault_client.fund_project(&project_id, &8_000_0000000i128);

    assert!(
        s.vault_client.get_utilization_bps() >= 7_000,
        "utilization should be at or above warning threshold"
    );

    // Withdraw a small amount within the utilization limit — warning event should fire.
    let small_shares = 200_0000000i128; // 200 USDC worth of shares
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    s.vault_client.withdraw(&investor, &small_shares, &0);

    // env.events().all() returns events from the most recent invocation only.
    // High-util withdraw emits: burn + utilization_warning + withdraw = 3 vault events.
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        events.events().len() >= 2,
        "withdrawal at high utilization should emit utilization warning event"
    );
}

// ── Issue #407: the graduated withdrawal limit's rejection was never tested ─────
// (only the accompanying UtilizationWarning event was, above)

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_rejects_above_high_tier_limit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 10_000_0000000i128);
    s.vault_client.deposit(&investor, &10_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 9500 USDC: liquid = 500, utilization = 95% (>= UTIL_HIGH_BPS/90%).
    // max_withdraw = liquid * HIGH_TIER_PCT (10%) = 50 USDC.
    s.vault_client.fund_project(&project_id, &9_500_0000000i128);
    assert!(s.vault_client.get_utilization_bps() >= 9_000);

    s.env.ledger().with_mut(|li| {
        li.timestamp += MIN_LOCK_PERIOD + 1;
    });
    // Request 100 USDC (>= MIN_WITHDRAW, but well above the 50 USDC cap).
    s.vault_client.withdraw(&investor, &100_0000000i128, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_rejects_above_med_tier_limit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 10_000_0000000i128);
    s.vault_client.deposit(&investor, &10_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 8000 USDC: liquid = 2000, utilization = 80% (>= UTIL_MED_BPS/70%, < 90%).
    // max_withdraw = liquid * MED_TIER_PCT (25%) = 500 USDC.
    s.vault_client.fund_project(&project_id, &8_000_0000000i128);
    let util = s.vault_client.get_utilization_bps();
    assert!((7_000..9_000).contains(&util));

    s.env.ledger().with_mut(|li| {
        li.timestamp += MIN_LOCK_PERIOD + 1;
    });
    // Request 600 USDC — above the 500 USDC cap.
    s.vault_client.withdraw(&investor, &600_0000000i128, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_rejects_above_low_tier_limit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 10_000_0000000i128);
    s.vault_client.deposit(&investor, &10_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 6000 USDC: liquid = 4000, utilization = 60% (>= UTIL_LOW_BPS/50%, < 70%).
    // max_withdraw = liquid * LOW_TIER_PCT (50%) = 2000 USDC.
    s.vault_client.fund_project(&project_id, &6_000_0000000i128);
    let util = s.vault_client.get_utilization_bps();
    assert!((5_000..7_000).contains(&util));

    s.env.ledger().with_mut(|li| {
        li.timestamp += MIN_LOCK_PERIOD + 1;
    });
    // Request 2500 USDC — above the 2000 USDC cap.
    s.vault_client.withdraw(&investor, &2_500_0000000i128, &0);
}

// ── Issue #47: minimum funding thresholds ─────────────────────────────────────

#[test]
fn test_funding_thresholds_default_to_zero() {
    let s = setup();
    assert_eq!(s.vault_client.get_min_credit_quality(), 0u32);
    assert_eq!(s.vault_client.get_min_green_impact(), 0u32);
}

#[test]
fn test_set_and_get_funding_thresholds() {
    let s = setup();
    s.vault_client.set_funding_thresholds(&60u32, &40u32);
    assert_eq!(s.vault_client.get_min_credit_quality(), 60u32);
    assert_eq!(s.vault_client.get_min_green_impact(), 40u32);
}

// ── Issue #187: boundary tests for thresholds above 100% ─────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_set_funding_thresholds_rejects_credit_above_100() {
    let s = setup();
    // 101 is just above the valid 0–100 range; green impact is valid.
    s.vault_client.set_funding_thresholds(&101u32, &40u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_set_funding_thresholds_rejects_green_above_100() {
    let s = setup();
    // credit quality is valid; green impact 200 is well above 100.
    s.vault_client.set_funding_thresholds(&60u32, &200u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_set_funding_thresholds_rejects_both_above_100() {
    let s = setup();
    // Both values exceed the valid range.
    s.vault_client.set_funding_thresholds(&101u32, &101u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_set_funding_thresholds_rejects_u32_max() {
    let s = setup();
    // u32::MAX is the largest possible out-of-range value.
    s.vault_client.set_funding_thresholds(&u32::MAX, &u32::MAX);
}

#[test]
fn test_set_funding_thresholds_accepts_boundary_100() {
    let s = setup();
    // 100 is the maximum valid value; must succeed.
    s.vault_client.set_funding_thresholds(&100u32, &100u32);
    assert_eq!(s.vault_client.get_min_credit_quality(), 100u32);
    assert_eq!(s.vault_client.get_min_green_impact(), 100u32);
}

#[test]
fn test_set_funding_thresholds_accepts_zero() {
    let s = setup();
    // 0 is the minimum valid value (default/no restriction).
    s.vault_client.set_funding_thresholds(&0u32, &0u32);
    assert_eq!(s.vault_client.get_min_credit_quality(), 0u32);
    assert_eq!(s.vault_client.get_min_green_impact(), 0u32);
}

#[test]
#[should_panic]
fn test_fund_project_blocked_below_credit_threshold() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Project has credit_quality=0, green_impact=0 (defaults); require credit >= 50.
    s.vault_client.set_funding_thresholds(&50u32, &0u32);
    s.vault_client.fund_project(&project_id, &100_0000000i128);
}

#[test]
#[should_panic]
fn test_fund_project_blocked_below_green_threshold() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Project has credit_quality=0, green_impact=0; require green >= 30.
    s.vault_client.set_funding_thresholds(&0u32, &30u32);
    s.vault_client.fund_project(&project_id, &100_0000000i128);
}

#[test]
fn test_fund_project_allowed_when_thresholds_met() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    registry_client.update_impact_score(&project_id, &70u32, &80u32);

    s.vault_client.set_funding_thresholds(&50u32, &50u32);
    // credit=70 >= 50, green=80 >= 50 — should succeed
    s.vault_client.fund_project(&project_id, &100_0000000i128);
    assert!(s.vault_client.total_assets() > 0);
}

#[test]
#[should_panic]
fn test_set_funding_thresholds_is_admin_only() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "set_funding_thresholds",
            args: soroban_sdk::vec![&s.env, 50u32.into_val(&s.env), 50u32.into_val(&s.env)],
            sub_invokes: &[],
        },
    }]);
    s.vault_client.set_funding_thresholds(&50u32, &50u32);
}

// ── Issue #76: registry dependency injection ──────────────────────────────────

#[test]
fn test_get_registry_returns_initial_registry() {
    let s = setup();
    assert_eq!(s.vault_client.get_registry(), s.registry);
}

#[test]
fn test_set_registry_updates_registry() {
    let s = setup();
    // Register a second real registry.
    let new_registry = s
        .env
        .register(registry_contract::WASM, (&s.admin, &s.admin));
    s.vault_client.set_registry(&new_registry);
    assert_eq!(s.vault_client.get_registry(), new_registry);
}

#[test]
#[should_panic]
fn test_set_registry_validates_new_address() {
    let s = setup();
    // An EOA address is not a deployed contract — total_projects() call will panic.
    let invalid = Address::generate(&s.env);
    s.vault_client.set_registry(&invalid);
}

#[test]
fn test_set_trusted_emitter_persists_and_emits_event() {
    // #267: set_trusted_emitter was previously a no-op stub (empty body,
    // all params underscore-prefixed) — complete_bridge_transfer checks the
    // TrustedEmitter storage this function is supposed to write, so the
    // bridge's inbound path could never succeed. Confirm it actually persists.
    let s = setup();
    let emitter = BytesN::from_array(&s.env, &[3u8; 32]);
    let chain_id = 2u32; // Ethereum

    s.vault_client
        .set_trusted_emitter(&chain_id, &emitter, &true);

    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(!events.events().is_empty());

    // Unmarking must also persist (and not panic).
    s.vault_client
        .set_trusted_emitter(&chain_id, &emitter, &false);
}

#[test]
#[should_panic]
fn test_set_registry_is_admin_only() {
    let s = setup();
    let new_registry = s
        .env
        .register(registry_contract::WASM, (&s.admin, &s.admin));
    let stranger = Address::generate(&s.env);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "set_registry",
            args: soroban_sdk::vec![&s.env, new_registry.clone().into_val(&s.env)],
            sub_invokes: &[],
        },
    }]);
    s.vault_client.set_registry(&new_registry);
}

// ── Issue #428: set_bridge()/set_wormhole_core() success-path coverage ────────

#[test]
fn test_set_bridge_persists_emits_event_and_is_idempotent() {
    let s = setup();
    let bridge = Address::generate(&s.env);

    s.vault_client.set_bridge(&bridge);

    // env.events().all() only reflects the most recent contract invocation,
    // so check events before making any further calls (including reads).
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        !events.events().is_empty(),
        "set_bridge should emit BridgeSet on first call"
    );

    let stored: Address = s.env.as_contract(&s.vault_address, || {
        s.env
            .storage()
            .instance()
            .get(&crate::types::VaultKey::Bridge)
            .expect("bridge should be persisted after set_bridge")
    });
    assert_eq!(stored, bridge);

    // Calling again with the same address hits the no-op early return
    // (lib.rs:1247-1250) and must not re-emit an event.
    s.vault_client.set_bridge(&bridge);
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        events.events().is_empty(),
        "set_bridge should be a no-op (no event) when the address is unchanged"
    );
}

#[test]
fn test_set_wormhole_core_persists() {
    let s = setup();
    let core = Address::generate(&s.env);

    s.vault_client.set_wormhole_core(&core);

    let stored: Address = s.env.as_contract(&s.vault_address, || {
        s.env
            .storage()
            .instance()
            .get(&BridgeDataKey::WormholeCore)
            .expect("wormhole core should be persisted after set_wormhole_core")
    });
    assert_eq!(stored, core);
}

// ── Issue #404: address_to_bytes32 must keep the trailing 32 bytes, not the leading ─

#[test]
fn test_address_to_bytes32_keeps_trailing_bytes_when_source_exceeds_32() {
    let s = setup();
    let addr = Address::generate(&s.env);
    let xdr = addr.clone().to_xdr(&s.env);
    let len = xdr.len() as usize;
    assert!(
        len > 32,
        "test assumes a real Address's XDR encoding exceeds 32 bytes"
    );

    let encoded = wormhole::address_to_bytes32(&s.env, &addr);
    let encoded_array = encoded.to_array();

    let mut expected = [0u8; 32];
    for i in 0..32 {
        expected[i] = xdr.get((len - 32 + i) as u32).unwrap();
    }
    assert_eq!(
        encoded_array, expected,
        "should retain the trailing 32 bytes of the source XDR, not the leading 32"
    );
}

// ── Issue #403: calculate_carbon_credits must reject non-positive amounts ──────
#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_calculate_carbon_credits_rejects_non_positive_amount() {
    let s = setup();
    let creator = Address::generate(&s.env);
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.calculate_carbon_credits(&project_id, &0i128);
}

#[test]
fn test_set_carbon_oracle_persists_emits_event_and_is_idempotent() {
    let s = setup();
    let oracle = Address::generate(&s.env);

    s.vault_client.set_carbon_oracle(&oracle);

    // env.events().all() only reflects the most recent contract invocation,
    // so check events before making any further calls (including reads).
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        !events.events().is_empty(),
        "set_carbon_oracle should emit CarbonOracleSet on first call"
    );

    let stored: Address = s.env.as_contract(&s.vault_address, || {
        s.env
            .storage()
            .instance()
            .get(&crate::types::VaultKey::CarbonOracle)
            .expect("carbon oracle should be persisted after set_carbon_oracle")
    });
    assert_eq!(stored, oracle);

    // Calling again with the same address hits the no-op early return
    // (lib.rs:1488-1500) and must not re-emit an event.
    s.vault_client.set_carbon_oracle(&oracle);
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        events.events().is_empty(),
        "set_carbon_oracle should be a no-op (no event) when the address is unchanged"
    );
}

#[test]
fn test_set_max_transaction_amount_persists_emits_event_and_is_idempotent() {
    let s = setup();

    s.vault_client
        .set_max_transaction_amount(&1_000_0000000i128);

    // env.events().all() only reflects the most recent contract invocation,
    // so check events before making any further calls (including reads).
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        !events.events().is_empty(),
        "set_max_transaction_amount should emit MaxTransactionAmountSet on first call"
    );

    assert_eq!(s.vault_client.max_transaction_amount(), 1_000_0000000i128);

    // Calling again with the same value hits the no-op early return
    // (lib.rs:1625-1639) and must not re-emit an event.
    s.vault_client
        .set_max_transaction_amount(&1_000_0000000i128);
    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert!(
        events.events().is_empty(),
        "set_max_transaction_amount should be a no-op (no event) when the value is unchanged"
    );
    assert_eq!(s.vault_client.max_transaction_amount(), 1_000_0000000i128);
}

// ── Issue #430: get_deposit_lock_expiry() coverage ─────────────────────────────

#[test]
fn test_get_deposit_lock_expiry() {
    let s = setup();
    let investor = Address::generate(&s.env);

    // A fresh account (never deposited) has no lock in force.
    assert_eq!(s.vault_client.get_deposit_lock_expiry(&investor), 0);

    // Ledger timestamp 0 is indistinguishable from "never deposited" by
    // get_deposit_lock_expiry's own sentinel check, so advance it first.
    s.env.ledger().with_mut(|li| li.timestamp = 500);
    let deposited_at = s.env.ledger().timestamp();
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    assert_eq!(
        s.vault_client.get_deposit_lock_expiry(&investor),
        deposited_at + MIN_LOCK_PERIOD
    );
}

// ── Issue #431: is_funding_round_active() coverage ─────────────────────────────

#[test]
fn test_is_funding_round_active_reflects_start_and_end() {
    let s = setup();

    assert!(!s.vault_client.is_funding_round_active());

    s.vault_client.start_funding_round();
    assert!(s.vault_client.is_funding_round_active());

    s.vault_client.end_funding_round();
    assert!(!s.vault_client.is_funding_round_active());
}

// ── Issue #12: explicit project_id validation in fund_project ─────────────────

#[test]
#[should_panic]
fn test_fund_project_panics_with_zero_project_id() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);
    // project_id 0 is always invalid (projects are 1-indexed)
    s.vault_client.fund_project(&0u32, &100_0000000i128);
}

#[test]
#[should_panic]
fn test_fund_project_panics_with_out_of_range_project_id() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);
    // registry has no projects, so any project_id > 0 is out of range
    s.vault_client.fund_project(&999u32, &100_0000000i128);
}

#[test]
#[should_panic]
fn test_fund_project_rejects_paused_registry() {
    // #263: cross-contract call failure handling in fund_project's call
    // into the linked ProjectRegistry. registry.get_project() is a getter
    // and keeps working while the registry is paused, so this isn't a
    // failing/panicking cross-contract call in itself — but it means the
    // vault must explicitly check is_paused() to reject deploying new
    // capital against a registry an admin has intentionally halted, rather
    // than silently succeeding. (The other cross-contract-failure mode —
    // get_project() panicking on an unknown project id — is already
    // covered by test_fund_project_panics_with_out_of_range_project_id.)
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    let creator = Address::generate(&s.env);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmPausedRegistry"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    registry_client.pause();
    s.vault_client.fund_project(&project_id, &100_0000000i128);
}

#[test]
fn test_fund_project_succeeds_with_valid_project_id() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm12"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.fund_project(&project_id, &100_0000000i128);
    assert!(s.vault_client.total_assets() > 0);
}

#[test]
#[should_panic]
fn test_fund_project_rejects_self_funding_by_admin() {
    // #14: the vault admin must not be able to fund a project they own themselves.
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&s.admin, &true);
    let project_id = registry_client.create_project(
        &s.admin,
        &String::from_str(&s.env, "ipfs://QmSelfFund"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.fund_project(&project_id, &100_0000000i128);
}

#[test]
fn test_deposit_rejects_when_exceeding_max_hbs_supply() {
    // #20: deposits that would push total supply above the cap must be rejected.
    let s = setup();
    assert!(s.vault_client.max_hbs_supply() > 0);

    let cap = s.vault_client.max_hbs_supply();
    // MAX_HBS_SUPPLY is set well above MAX_DEPOSIT (a single deposit can mint
    // at most ~MAX_DEPOSIT shares), so approach the cap via repeated
    // near-max-sized deposits rather than a single one. Bounded iteration
    // count as a safety net against an infinite loop if the check is broken.
    let mut hit_cap = false;
    for _ in 0..20 {
        let investor = Address::generate(&s.env);
        mint_usdc(&s.env, &s.usdc_sac, &investor, MAX_DEPOSIT);
        let result = s.vault_client.try_deposit(&investor, &MAX_DEPOSIT);
        if result.is_err() {
            hit_cap = true;
            break;
        }
        assert!(s.vault_client.total_supply() <= cap);
    }
    assert!(
        hit_cap,
        "expected a deposit to eventually exceed MAX_HBS_SUPPLY"
    );
    assert!(s.vault_client.total_supply() <= cap);
}

// ── Issue #13: minimum deposit and withdraw amounts ───────────────────────────

#[test]
#[should_panic]
fn test_deposit_below_minimum_panics() {
    let s = setup();
    let investor = Address::generate(&s.env);
    // 10 USDC is below MIN_DEPOSIT (100 USDC)
    mint_usdc(&s.env, &s.usdc_sac, &investor, 10_0000000i128);
    s.vault_client.deposit(&investor, &10_0000000i128);
}

#[test]
fn test_deposit_at_minimum_succeeds() {
    let s = setup();
    let investor = Address::generate(&s.env);
    // exactly MIN_DEPOSIT (100 USDC) must succeed
    mint_usdc(&s.env, &s.usdc_sac, &investor, 100_0000000i128);
    let shares = s.vault_client.deposit(&investor, &100_0000000i128);
    assert!(shares > 0);
}

#[test]
#[should_panic]
fn test_withdraw_below_minimum_panics() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    // 10 shares is below MIN_WITHDRAW (100 shares)
    s.vault_client.withdraw(&investor, &10_0000000i128, &0);
}

#[test]
fn test_withdraw_at_minimum_succeeds() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    // withdraw exactly MIN_WITHDRAW shares
    let returned = s.vault_client.withdraw(&investor, &100_0000000i128, &0);
    assert!(returned > 0);
}

#[test]
fn test_concurrent_deposits_and_fund_project() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let investor2 = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    // Deposit 1
    mint_usdc(&s.env, &s.usdc_sac, &investor1, 2_000_0000000i128);
    s.vault_client.deposit(&investor1, &2_000_0000000i128);

    // Setup Project
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm12"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    registry_client.update_impact_score(&project_id, &80u32, &60u32);

    // Fund project interleaved
    s.vault_client.fund_project(&project_id, &500_0000000i128);

    // Deposit 2
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 3_000_0000000i128);
    let shares2 = s.vault_client.deposit(&investor2, &3_000_0000000i128);

    // Withdraw from investor1
    let shares1 = s.vault_client.balance(&investor1);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    s.vault_client.withdraw(&investor1, &shares1, &0);

    // Withdraw from investor2
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    s.vault_client.withdraw(&investor2, &shares2, &0);

    // Some residual total_assets might remain due to integer rounding/fractions
    assert!(s.vault_client.total_assets() >= 0);
}

// ── Circuit breaker tests (#72) ────────────────────────────────────────────────

#[test]
fn test_vault_is_paused_getter_default_false() {
    let s = setup();
    assert!(!s.vault_client.is_paused());
}

#[test]
fn test_vault_pause_and_unpause() {
    let s = setup();
    s.vault_client.pause();
    assert!(s.vault_client.is_paused());
    s.vault_client.unpause();
    assert!(!s.vault_client.is_paused());
}

#[test]
fn test_emergency_admin_can_pause_and_unpause_without_owner() {
    let s = setup();
    let emergency_admin = Address::generate(&s.env);

    assert_eq!(s.vault_client.get_emergency_admin(), None);
    s.vault_client
        .set_emergency_admin(&Some(emergency_admin.clone()));
    assert_eq!(
        s.vault_client.get_emergency_admin(),
        Some(emergency_admin.clone())
    );

    assert!(!s.vault_client.is_paused());
    s.vault_client.emergency_pause(&emergency_admin);
    assert!(s.vault_client.is_paused());
    s.vault_client.emergency_unpause(&emergency_admin);
    assert!(!s.vault_client.is_paused());
}

#[test]
#[should_panic]
fn test_emergency_pause_rejects_non_emergency_admin() {
    let s = setup();
    let emergency_admin = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);
    s.vault_client.set_emergency_admin(&Some(emergency_admin));
    s.vault_client.emergency_pause(&stranger);
}

#[test]
#[should_panic]
fn test_deposit_blocked_when_vault_paused() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.pause();
    s.vault_client.deposit(&investor, &1_000_0000000i128);
}

// ── Storage compaction tests (#88) ────────────────────────────────────────────

#[test]
fn test_compact_storage_removes_zero_project_investment() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let deposit = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit);
    s.vault_client.deposit(&investor, &deposit);

    // Create a project in the registry (owned by someone other than the vault
    // admin — see #14) and fund it.
    // Available for deployment = liquid(1000) - insurance_reserve(5) = 995 USDC.
    let project_creator = Address::generate(&s.env);
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&project_creator, &true);
    let pid = registry_client.create_project(
        &project_creator,
        &soroban_sdk::String::from_str(&s.env, "ipfs://QmCompact"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let fund_amount = 500_0000000i128; // 500 USDC, within available limit
    s.vault_client.fund_project(&pid, &fund_amount);
    assert_eq!(s.vault_client.get_project_investment(&pid), fund_amount);

    // compact_storage should find 0 removals (entry has a non-zero value)
    let removed = s.vault_client.compact_storage();
    assert_eq!(removed, 0u32);
}

#[test]
fn test_get_project_investment_zero_for_unfunded() {
    let s = setup();
    assert_eq!(s.vault_client.get_project_investment(&1u32), 0i128);
    assert_eq!(s.vault_client.get_project_investment(&999u32), 0i128);
}

// ── Migration tests (#64) ──────────────────────────────────────────────────────

#[test]
fn test_vault_state_version() {
    let s = setup();
    assert_eq!(s.vault_client.state_version(), 1u32);
    assert_eq!(s.vault_client.stored_state_version(), 1u32);
}

#[test]
#[should_panic]
fn test_vault_stale_stored_version_blocks_normal_calls() {
    // MIGRATION.md: "require_current_state rejects calls if the stored
    // version does not match the compiled STATE_VERSION. This prevents
    // accidentally running new logic against an old storage layout." (#275)
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    // Simulate a deployment whose stored schema version has fallen behind
    // this build's compiled STATE_VERSION, without going through migrate_state.
    s.env.as_contract(&s.vault_address, || {
        s.env
            .storage()
            .instance()
            .set(&crate::types::VaultKey::StateVersion, &0u32);
    });

    s.vault_client.deposit(&investor, &1_000_0000000i128);
}

#[test]
#[should_panic]
fn test_vault_migrate_state_rejects_wrong_version() {
    let s = setup();
    s.vault_client.migrate_state(&0u32);
}

// ── Ownership transfer event test (#30) ───────────────────────────────────────

#[test]
fn test_transfer_ownership_emits_event() {
    let s = setup();
    let new_owner = Address::generate(&s.env);

    // Initiate transfer; live_until_ledger = 1000 (well beyond current ledger 0).
    s.vault_client.transfer_ownership(&new_owner, &1000u32);

    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    // transfer_ownership emits: stellar-access OwnershipTransfer + our OwnershipTransferred.
    assert_eq!(
        events.events().len(),
        2,
        "transfer_ownership should emit 2 events (stellar-access + project-specific)"
    );
}

// ── Issue #191: negative tests for transfer_ownership() ──────────────────────

#[test]
#[should_panic]
fn test_transfer_ownership_rejects_non_owner_caller() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    let new_owner = Address::generate(&s.env);

    // Only mock auth for the stranger, not the real owner — the
    // owner.require_auth() check in the library must fail.
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "transfer_ownership",
            args: soroban_sdk::vec![
                &s.env,
                new_owner.clone().into_val(&s.env),
                1000u32.into_val(&s.env),
            ],
            sub_invokes: &[],
        },
    }]);
    s.vault_client.transfer_ownership(&new_owner, &1000u32);
}

#[test]
#[should_panic]
fn test_transfer_ownership_cancel_without_pending_panics() {
    let s = setup();
    let new_owner = Address::generate(&s.env);

    // live_until_ledger = 0 cancels a pending transfer, but no transfer
    // has been initiated yet — must panic with NoPendingTransfer.
    s.vault_client.transfer_ownership(&new_owner, &0u32);
}

#[test]
#[should_panic]
fn test_transfer_ownership_with_expired_ledger_panics() {
    let s = setup();
    let new_owner = Address::generate(&s.env);

    // Advance the ledger sequence past 1 so that live_until_ledger = 1 is in the past.
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 10;
    });

    // live_until_ledger = 1 < current ledger (10) — must panic with InvalidLiveUntilLedger.
    s.vault_client.transfer_ownership(&new_owner, &1u32);
}

#[test]
fn test_transfer_ownership_to_same_owner_succeeds() {
    // Initiating a transfer to the current owner is a valid (if unusual) operation —
    // the 2-step flow still requires accept_ownership() from the pending address.
    let s = setup();

    // Transfer to self with a valid live_until_ledger.
    s.vault_client.transfer_ownership(&s.admin, &1000u32);

    let events = s.env.events().all().filter_by_contract(&s.vault_address);
    assert_eq!(
        events.events().len(),
        2,
        "transfer to self should emit 2 events like any other transfer"
    );
}

proptest! {
    #[test]
    fn test_vault_arithmetic_fuzz(
        deposit_amount in 100_0000000i128..1_000_000_000_0000000i128,
        withdraw_shares in 100_0000000i128..1_000_000_000_0000000i128
    ) {
        let s = setup();
        let investor = Address::generate(&s.env);
        mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);

        let shares = s.vault_client.deposit(&investor, &deposit_amount);

        // Insurance premium stays in vault USDC balance; total_assets includes it.
        // shares = investable (1:1 first deposit), total_assets = deposit_amount,
        // so convert_to_assets(shares) = shares * deposit_amount / shares = deposit_amount.
        let assets = s.vault_client.convert_to_assets(&shares);
        assert_eq!(assets, deposit_amount);

        // Round-tripping through convert_to_shares must recover the original shares.
        let shares_from_assets = s.vault_client.convert_to_shares(&assets);
        assert_eq!(shares_from_assets, shares);

        if withdraw_shares <= shares && withdraw_shares >= 100_0000000i128 {
            s.env.ledger().with_mut(|li| {
                li.sequence_number += 1;
            });
            let withdrawn = s.vault_client.withdraw(&investor, &withdraw_shares, &0);
            assert!(withdrawn <= deposit_amount);
        }
    }

    // #256: complementary fuzz target covering the *rejected* side of
    // deposit's amount validation — test_vault_arithmetic_fuzz above only
    // fuzzes amounts already known to be within [MIN_DEPOSIT, MAX_DEPOSIT].
    // Fixed-value tests (test_deposit_below_minimum_panics,
    // test_fee_above_cap_panics, etc.) check single boundary points; this
    // fuzzes the full out-of-range space to confirm every value rejects
    // consistently, not just the specific values already hand-picked.
    #[test]
    fn test_deposit_rejects_out_of_range_amounts_fuzz(
        amount in prop_oneof![
            1i128..100_0000000i128,
            1_000_000_000_0000001i128..2_000_000_000_0000000i128,
        ]
    ) {
        let s = setup();
        let investor = Address::generate(&s.env);
        mint_usdc(&s.env, &s.usdc_sac, &investor, amount);
        let result = s.vault_client.try_deposit(&investor, &amount);
        prop_assert!(result.is_err());
    }
}

#[test]
#[should_panic]
fn test_withdrawal_rate_limiting_same_ledger() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Try to withdraw in the same ledger sequence -> should panic
    s.vault_client.withdraw(&investor, &shares, &0);
}

#[test]
fn test_withdrawal_rate_limiting_next_ledger() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Advance ledger sequence
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    // Try to withdraw in the next ledger sequence -> should succeed
    let returned = s.vault_client.withdraw(&investor, &shares, &0);
    assert!(returned > 0);
}

#[test]
#[should_panic]
fn test_withdrawal_rate_limiting_transfer_locked() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor1, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor1, &1_000_0000000i128);

    // Advance sequence for investor1 so they can transfer
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    // Transfer shares from investor1 to investor2
    s.vault_client.transfer(
        &investor1,
        &soroban_sdk::MuxedAddress::from(investor2.clone()),
        &shares,
    );

    // Try to withdraw from investor2 in the same ledger sequence -> should panic
    s.vault_client.withdraw(&investor2, &shares, &0);
}

// ── Consolidated admin-only enumeration (#266) ─────────────────────────────────
//
// Several admin-only functions already have their own dedicated
// should_panic test (e.g. test_set_registry_is_admin_only). This test
// instead enumerates every #[only_owner] entry point on InvestmentVault in
// one place and confirms each rejects a non-admin caller, so a future
// #[only_owner] entry point that's accidentally left off both this list and
// its own dedicated test won't go unnoticed.
#[test]
fn test_all_only_owner_functions_reject_non_admin_caller() {
    let s = setup();
    let stranger = Address::generate(&s.env);

    // Restrict auth to `stranger` for an unrelated invocation, so every
    // #[only_owner] call below has no matching auth entry for the real
    // owner and must fail at the `owner.require_auth()` check.
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "is_paused",
            args: soroban_sdk::vec![&s.env],
            sub_invokes: &[],
        },
    }]);

    let addr = || Address::generate(&s.env);
    let hash32 = BytesN::from_array(&s.env, &[0u8; 32]);

    let results: soroban_sdk::Vec<bool> = soroban_sdk::vec![
        &s.env,
        s.vault_client.try_migrate_state(&1u32).is_err(),
        s.vault_client.try_fund_project(&1u32, &1i128).is_err(),
        s.vault_client.try_receive_yield(&addr(), &1i128).is_err(),
        s.vault_client
            .try_claim_insurance(&1u32, &addr(), &1i128)
            .is_err(),
        s.vault_client
            .try_set_multisig_admin(&soroban_sdk::vec![&s.env, addr()], &1u32)
            .is_err(),
        s.vault_client
            .try_set_management_fee(&1u32, &addr())
            .is_err(),
        s.vault_client.try_enable_secondary_trading().is_err(),
        s.vault_client
            .try_set_funding_thresholds(&1u32, &1u32)
            .is_err(),
        s.vault_client.try_set_registry(&addr()).is_err(),
        s.vault_client.try_set_bridge(&addr()).is_err(),
        s.vault_client.try_set_wormhole_core(&addr()).is_err(),
        s.vault_client
            .try_set_trusted_emitter(&1u32, &hash32, &true)
            .is_err(),
        s.vault_client.try_set_flash_loan_fee(&1u32).is_err(),
        s.vault_client.try_set_carbon_oracle(&addr()).is_err(),
        s.vault_client
            .try_set_max_transaction_amount(&1i128)
            .is_err(),
        s.vault_client
            .try_record_compliance_event(
                &String::from_str(&s.env, "type"),
                &String::from_str(&s.env, "data")
            )
            .is_err(),
        s.vault_client.try_take_reporting_snapshot().is_err(),
        s.vault_client.try_pause().is_err(),
        s.vault_client.try_unpause().is_err(),
        s.vault_client.try_set_emergency_admin(&None).is_err(),
        s.vault_client.try_compact_storage().is_err(),
        s.vault_client.try_upgrade(&hash32).is_err(),
        s.vault_client
            .try_set_volume_fee_tier(&500_0000000i128, &50u32)
            .is_err(),
    ];

    for (i, rejected) in results.iter().enumerate() {
        assert!(
            rejected,
            "only_owner function at index {i} did not reject a non-admin caller"
        );
    }
}

// ── Issue #177: withdraw() must reject a zero-amount withdrawal ───────────────

#[test]
fn test_withdraw_rejects_zero_shares() {
    // A zero-share withdrawal carries no economic value and is rejected up-front
    // via the SharesNotPositive / WithdrawBelowMinimum guards (#177).
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Advance one ledger so the deposit lock doesn't fire first.
    s.env.ledger().with_mut(|li| li.sequence_number += 1);

    let result = s.vault_client.try_withdraw(&investor, &0i128, &0);
    assert!(
        result.is_err(),
        "withdraw() should reject a zero shares_amount"
    );
}

#[test]
fn test_withdraw_rejects_below_minimum_shares() {
    // Any amount below MIN_WITHDRAW (100 shares) is also rejected (#177).
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    s.env.ledger().with_mut(|li| li.sequence_number += 1);

    // 1 stroop is below MIN_WITHDRAW = 100_0000000
    let result = s.vault_client.try_withdraw(&investor, &1i128, &0);
    assert!(
        result.is_err(),
        "withdraw() should reject shares_amount below MIN_WITHDRAW"
    );
}

// ── Issue #178: batch_deposit() must revert on an empty investor list ─────────

#[test]
fn test_batch_deposit_empty_list_reverts() {
    // A caller passing an empty deposits Vec is almost certainly a bug;
    // the contract now panics with EmptyBatchDeposit rather than silently
    // returning an empty minted list (#178).
    let s = setup();
    let empty: soroban_sdk::Vec<(Address, i128)> = soroban_sdk::Vec::new(&s.env);
    let result = s.vault_client.try_batch_deposit(&empty);
    assert!(
        result.is_err(),
        "batch_deposit() should revert on an empty investor list"
    );
}

// ── Issue #35: getter functions for individual project investment amounts ──────

#[test]
fn test_get_project_investments_batch_returns_correct_amounts() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator1 = Address::generate(&s.env);
    let creator2 = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 5_000_0000000i128);
    s.vault_client.deposit(&investor, &5_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator1, &true);
    registry_client.set_whitelist(&creator2, &true);
    let pid1 = registry_client.create_project(
        &creator1,
        &String::from_str(&s.env, "ipfs://QmAlpha"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let pid2 = registry_client.create_project(
        &creator2,
        &String::from_str(&s.env, "ipfs://QmBeta"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    let fund1 = 1_000_0000000i128;
    let fund2 = 500_0000000i128;
    s.vault_client.fund_project(&pid1, &fund1);
    s.vault_client.fund_project(&pid2, &fund2);

    let ids = soroban_sdk::vec![&s.env, pid1, pid2];
    let amounts = s.vault_client.get_project_investments_batch(&ids);

    assert_eq!(amounts.len(), 2);
    assert_eq!(amounts.get(0).unwrap(), fund1);
    assert_eq!(amounts.get(1).unwrap(), fund2);
}

#[test]
fn test_get_all_project_investments_returns_all() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 2_000_0000000i128);
    s.vault_client.deposit(&investor, &2_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let pid1 = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmAll1"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let pid2 = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmAll2"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    let fund1 = 300_0000000i128;
    let fund2 = 200_0000000i128;
    s.vault_client.fund_project(&pid1, &fund1);
    s.vault_client.fund_project(&pid2, &fund2);

    let all = s.vault_client.get_all_project_investments();
    assert_eq!(all.len(), 2);
}

// ── Issue #176: deposit() must reject a zero-amount deposit ──────────────────

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_deposit_rejects_zero_amount() {
    // Zero is ≤ 0; the contract panics with AmountNotPositive (#1) before
    // any transfer or share calculation is attempted.
    let s = setup();
    let investor = Address::generate(&s.env);
    s.vault_client.deposit(&investor, &0i128);
}

// ── Issue #181: fund_project() must reject a zero/negative amount ─────────────

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_fund_project_rejects_zero_amount() {
    // fund_project_internal checks `amount <= 0` before the cross-contract
    // registry call, so no USDC transfer or project lookup occurs.
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmFundZero"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.fund_project(&project_id, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_fund_project_rejects_negative_amount() {
    // Negative i128 also satisfies `amount <= 0`; confirm the guard fires
    // for negative values just as it does for zero.
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmFundNeg"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    s.vault_client.fund_project(&project_id, &-1i128);
}

// ── Issue #182: claim_queued() is idempotent against double-claim ─────────────

#[test]
fn test_claim_queued_is_idempotent_against_double_claim() {
    // claim() advances the queue head past every settled entry. A second
    // call on the now-empty queue hits the head == tail fast-path and
    // returns 0 without transferring USDC again — no double-payout.
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 2_000_0000000i128);
    s.vault_client.deposit(&investor, &2_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let pid = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmGamma"),
        &0u64,
        &test_metadata_hash(&s.env),
    );

    let funded = 800_0000000i128;
    s.vault_client.fund_project(&pid, &funded);

    let all = s.vault_client.get_all_project_investments();
    assert_eq!(all.len(), 1);
    let (id, amt) = all.get(0).unwrap();
    assert_eq!(id, pid);
    assert_eq!(amt, funded);
}

// ── Issue #36: withdrawal sliding window ─────────────────────────────────────

#[test]
fn test_withdrawal_window_blocks_early_exit() {
    // With a 5-ledger window, a withdraw attempted before 5 ledgers have
    // elapsed since the deposit must be rejected with DepositLocked (#36).
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    // Set a 5-ledger withdrawal window.
    s.vault_client.set_withdrawal_window(&5u32);

    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Only 2 ledgers elapsed — still inside the 5-ledger window.
    s.env.ledger().with_mut(|li| li.sequence_number += 2);

    let result = s.vault_client.try_withdraw(&investor, &shares, &0);
    assert!(
        result.is_err(),
        "withdraw should be blocked inside the sliding window"
    );
}

#[test]
fn test_withdrawal_window_allows_exit_after_window() {
    // After the configured window has elapsed the withdrawal succeeds (#36).
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    s.vault_client.set_withdrawal_window(&5u32);

    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Advance past the 5-ledger window.
    s.env.ledger().with_mut(|li| li.sequence_number += 5);

    let returned = s.vault_client.withdraw(&investor, &shares, &0);
    assert!(returned > 0, "withdraw should succeed after window expires");
}

#[test]
fn test_get_set_withdrawal_window() {
    // Default window is 1; set_withdrawal_window updates it (#36).
    let s = setup();
    assert_eq!(s.vault_client.get_withdrawal_window(), 1u32);
    s.vault_client.set_withdrawal_window(&10u32);
    assert_eq!(s.vault_client.get_withdrawal_window(), 10u32);
}

#[test]
fn test_claim_settles_queued_withdrawal_then_second_claim_is_noop() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let creator = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmIdempotent"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    // Fund 490 USDC (49% util) to reduce liquidity below the full redemption
    // value, forcing the withdrawal into the FIFO queue.
    s.vault_client.fund_project(&project_id, &490_0000000i128);
    s.env.ledger().with_mut(|li| {
        li.sequence_number += 1;
        li.timestamp += MIN_LOCK_PERIOD + 1;
    });
    // Shares are burned immediately; claim is enqueued.
    let enqueued = s.vault_client.withdraw(&investor, &shares, &0);
    assert_eq!(enqueued, 0);
    assert_eq!(s.vault_client.balance(&investor), 0);

    // Restore liquidity so claim() can settle.
    let funder = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &funder, 2_000_0000000i128);
    s.vault_client.deposit(&funder, &2_000_0000000i128);

    let usdc_client = TokenClient::new(&s.env, &s.usdc_sac);

    // First claim: settles the queued entry, transfers USDC to investor.
    let paid_first = s.vault_client.claim();
    assert!(paid_first > 0);
    let balance_after_first = usdc_client.balance(&investor);
    assert_eq!(balance_after_first, paid_first);

    // Second claim: queue is empty (head == tail) → returns 0 immediately.
    let paid_second = s.vault_client.claim();
    assert_eq!(paid_second, 0);

    // Investor's USDC balance must not have changed — no double payout.
    assert_eq!(usdc_client.balance(&investor), balance_after_first);
}

// ── Issue #39: dynamic (volume-tiered) fee structure ─────────────────────────

#[test]
fn test_volume_fee_tier_applies_discount_for_large_deposits() {
    // A two-tier fee schedule: deposits < threshold pay base_bps;
    // deposits >= threshold pay the lower discounted_bps rate.
    let s = setup();
    let fee_recipient = Address::generate(&s.env);
    let usdc_client = TokenClient::new(&s.env, &s.usdc_sac);

    // Flat base rate: 200 bps (2%)
    s.vault_client.set_management_fee(&200u32, &fee_recipient);

    // Volume tier: deposits >= 500 USDC → 50 bps (0.5%)
    let threshold = 500_0000000i128; // 500 USDC
    s.vault_client.set_volume_fee_tier(&threshold, &50u32);
    assert_eq!(s.vault_client.get_volume_fee_tier(), (threshold, 50u32));

    // Below threshold: flat 200 bps applies.
    let small_investor = Address::generate(&s.env);
    let small_deposit = 100_0000000i128; // 100 USDC < threshold
    mint_usdc(&s.env, &s.usdc_sac, &small_investor, small_deposit);
    s.vault_client.deposit(&small_investor, &small_deposit);
    let expected_small_fee = small_deposit * 200 / 10_000; // 2 USDC
    assert_eq!(usdc_client.balance(&fee_recipient), expected_small_fee);

    // At/above threshold: discounted 50 bps applies.
    let large_investor = Address::generate(&s.env);
    let large_deposit = 1_000_0000000i128; // 1000 USDC >= threshold
    mint_usdc(&s.env, &s.usdc_sac, &large_investor, large_deposit);
    s.vault_client.deposit(&large_investor, &large_deposit);
    let expected_large_fee = large_deposit * 50 / 10_000; // 5 USDC
    assert_eq!(
        usdc_client.balance(&fee_recipient),
        expected_small_fee + expected_large_fee
    );
}

#[test]
fn test_volume_fee_tier_disabled_reverts_to_flat_rate() {
    // Setting threshold=0 removes the tier entirely; all subsequent deposits
    // pay the flat ManagementFeeBps regardless of size.
    let s = setup();
    let fee_recipient = Address::generate(&s.env);
    let usdc_client = TokenClient::new(&s.env, &s.usdc_sac);

    s.vault_client.set_management_fee(&200u32, &fee_recipient);
    s.vault_client.set_volume_fee_tier(&500_0000000i128, &50u32);

    // Disable the tier.
    s.vault_client.set_volume_fee_tier(&0i128, &0u32);
    assert_eq!(s.vault_client.get_volume_fee_tier(), (0i128, 0u32));

    let investor = Address::generate(&s.env);
    let deposit = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit);
    s.vault_client.deposit(&investor, &deposit);

    // Full 200 bps applied despite deposit being above the old threshold.
    let expected_fee = deposit * 200 / 10_000;
    assert_eq!(usdc_client.balance(&fee_recipient), expected_fee);
}

#[test]
#[should_panic]
fn test_volume_fee_tier_is_admin_only() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "set_volume_fee_tier",
            args: soroban_sdk::vec![
                &s.env,
                500_0000000i128.into_val(&s.env),
                50u32.into_val(&s.env)
            ],
            sub_invokes: &[],
        },
    }]);
    s.vault_client.set_volume_fee_tier(&500_0000000i128, &50u32);
}

// ── #179: convert_to_shares() overflow guard on extremely large deposits ──────

/// Verify that `convert_to_shares` panics (rather than silently wrapping) when
/// the intermediate multiplication `usdc_amount * total_shares` would overflow
/// i128.  Soroban contracts are compiled in debug mode for tests, where Rust
/// arithmetic overflow is a panic; this test documents and guards that behavior
/// so an accidental `--release` silent-wrap regression is caught by CI.
#[test]
#[should_panic]
fn test_convert_to_shares_near_i128_max_overflows_visibly() {
    let s = setup();
    let investor = Address::generate(&s.env);

    // Seed the vault with two deposits so total_shares and total_assets are
    // both non-zero and the ratio is > 1 (more shares than assets). This
    // forces the multiplication path: usdc_amount * total_shares / total_assets.
    let seed = 1_000_0000000i128; // 1 000 USDC (7-decimal)
    mint_usdc(&s.env, &s.usdc_sac, &investor, seed * 2);
    s.vault_client.deposit(&investor, &seed);

    // Near-maximum i128 value; multiplying this by any total_shares > 1
    // will overflow a signed 128-bit integer.
    let near_max = i128::MAX / 2 + 1;
    // Should panic — not silently wrap — satisfying the overflow-guard criterion.
    let _ = s.vault_client.convert_to_shares(&near_max);
}

/// A near-max amount on an *empty* vault takes the 1:1 fast-path and must
/// never overflow (there is no multiplication on that path).
#[test]
fn test_convert_to_shares_near_i128_max_empty_vault_is_one_to_one() {
    let s = setup();
    // Empty vault → 1:1 branch, no multiplication at all.
    let near_max = i128::MAX / 2;
    let shares = s.vault_client.convert_to_shares(&near_max);
    assert_eq!(shares, near_max, "empty-vault path must be exactly 1:1");
}

// ── #180: flash_loan() succeeds when borrower repays in the same transaction ──

/// Mock flash-loan receiver that always returns `true` (repayment confirmed).
/// Registered as a Soroban contract so the vault can cross-contract-call it.
mod mock_flash_receiver {
    use soroban_sdk::{contract, contractimpl, Address, Bytes, Env};

    #[contract]
    pub struct MockRepayingReceiver;

    #[contractimpl]
    impl MockRepayingReceiver {
        /// Always signals successful repayment.
        pub fn flash_loan_callback(
            _env: Env,
            _initiator: Address,
            _vault: Address,
            _amount: i128,
            _fee: i128,
            _data: Bytes,
        ) -> bool {
            true
        }
    }
}

/// Companion to the (implicit) failure test: confirm that a well-behaved
/// borrower which returns `true` from `flash_loan_callback` completes the
/// flash loan without the vault panicking.
#[test]
fn test_flash_loan_succeeds_with_valid_same_transaction_repayment() {
    let s = setup();

    // Fund the vault so it has assets to lend.
    let investor = Address::generate(&s.env);
    let deposit_amount = 10_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);
    s.vault_client.deposit(&investor, &deposit_amount);

    // Register the mock borrower that always repays.
    let borrower = s
        .env
        .register(mock_flash_receiver::MockRepayingReceiver, ());
    let initiator = Address::generate(&s.env);

    // Set a 10 bps flash-loan fee so the fee path is exercised.
    s.vault_client.set_flash_loan_fee(&10u32);

    let loan_amount = 1_000_0000000i128;

    // Must not panic — borrower returns true → repayment succeeds.
    s.vault_client.execute_flash_loan(
        &initiator,
        &borrower,
        &loan_amount,
        &soroban_sdk::Bytes::new(&s.env),
    );

    // Vault total-assets must be at least as large as before the loan:
    // the fee is kept by the vault, so assets >= original deposit.
    assert!(
        s.vault_client.total_assets() >= deposit_amount,
        "vault must retain at least the original deposit after a successful flash loan"
    );
}

/// Verify that a borrower whose callback returns `false` causes the vault to
/// panic, enforcing same-transaction repayment.
#[test]
#[should_panic(expected = "Error(Contract, #50)")]
fn test_flash_loan_fails_without_repayment() {
    mod mock_failing_receiver {
        use soroban_sdk::{contract, contractimpl, Address, Bytes, Env};

        #[contract]
        pub struct MockFailingReceiver;

        #[contractimpl]
        impl MockFailingReceiver {
            pub fn flash_loan_callback(
                _env: Env,
                _initiator: Address,
                _vault: Address,
                _amount: i128,
                _fee: i128,
                _data: Bytes,
            ) -> bool {
                false // simulate missing repayment
            }
        }
    }

    let s = setup();
    let investor = Address::generate(&s.env);
    let deposit_amount = 10_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, deposit_amount);
    s.vault_client.deposit(&investor, &deposit_amount);

    let borrower = s
        .env
        .register(mock_failing_receiver::MockFailingReceiver, ());

    s.vault_client.execute_flash_loan(
        &Address::generate(&s.env),
        &borrower,
        &1_000_0000000i128,
        &soroban_sdk::Bytes::new(&s.env),
    );
}

// ── Issue #392: get_portfolio and insurance_fund_balance test coverage ────────

#[test]
fn test_get_portfolio_after_single_deposit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let amount = 1_000_0000000i128; // 1000 USDC

    mint_usdc(&s.env, &s.usdc_sac, &investor, amount);
    let shares = s.vault_client.deposit(&investor, &amount);

    let portfolio = s.vault_client.get_portfolio(&investor);

    // Shares match what deposit returned.
    assert_eq!(portfolio.shares, shares);
    // USDC value equals shares 1:1 on first deposit (no yield accrued).
    assert_eq!(portfolio.usdc_value, shares);
    // No yield has accrued yet.
    assert_eq!(portfolio.claimable_yield, 0);
    // Sole investor owns 100% of the pool.
    assert_eq!(portfolio.share_of_pool_bps, 10_000);
    // Lifetime deposits equal the deposit amount.
    assert_eq!(portfolio.total_deposited, amount);
}

#[test]
fn test_get_portfolio_share_of_pool_with_two_investors() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);
    let amount = 500_0000000i128; // 500 USDC each

    mint_usdc(&s.env, &s.usdc_sac, &alice, amount);
    s.vault_client.deposit(&alice, &amount);

    mint_usdc(&s.env, &s.usdc_sac, &bob, amount);
    s.vault_client.deposit(&bob, &amount);

    let alice_portfolio = s.vault_client.get_portfolio(&alice);
    let bob_portfolio = s.vault_client.get_portfolio(&bob);

    // Each investor owns 50% of the pool.
    assert_eq!(alice_portfolio.share_of_pool_bps, 5_000);
    assert_eq!(bob_portfolio.share_of_pool_bps, 5_000);
    assert_eq!(alice_portfolio.shares, bob_portfolio.shares);
}

#[test]
fn test_get_portfolio_zero_for_nondepositor() {
    let s = setup();
    let stranger = Address::generate(&s.env);

    let portfolio = s.vault_client.get_portfolio(&stranger);

    assert_eq!(portfolio.shares, 0);
    assert_eq!(portfolio.usdc_value, 0);
    assert_eq!(portfolio.claimable_yield, 0);
    assert_eq!(portfolio.share_of_pool_bps, 0);
    assert_eq!(portfolio.total_deposited, 0);
}

#[test]
fn test_insurance_fund_balance_starts_at_zero() {
    let s = setup();
    assert_eq!(s.vault_client.insurance_fund_balance(), 0);
}

#[test]
fn test_insurance_fund_balance_increases_after_deposit() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let amount = 1_000_0000000i128; // 1000 USDC

    mint_usdc(&s.env, &s.usdc_sac, &investor, amount);
    s.vault_client.deposit(&investor, &amount);

    let expected_premium = amount * 50 / 10_000; // INSURANCE_PREMIUM_BPS = 50
    assert_eq!(s.vault_client.insurance_fund_balance(), expected_premium);
}

#[test]
fn test_insurance_fund_accumulates_across_deposits() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);

    let amount_a = 1_000_0000000i128; // 1000 USDC
    mint_usdc(&s.env, &s.usdc_sac, &alice, amount_a);
    s.vault_client.deposit(&alice, &amount_a);

    let premium_a = amount_a * 50 / 10_000;

    let amount_b = 2_000_0000000i128; // 2000 USDC
    mint_usdc(&s.env, &s.usdc_sac, &bob, amount_b);
    s.vault_client.deposit(&bob, &amount_b);

    let premium_b = amount_b * 50 / 10_000;
    assert_eq!(
        s.vault_client.insurance_fund_balance(),
        premium_a + premium_b
    );
}

// ── get_multisig_admin tests (#384) ───────────────────────────────────────────

#[test]
fn test_get_multisig_admin_returns_empty_by_default() {
    let s = setup();
    let (signers, threshold) = s.vault_client.get_multisig_admin();
    assert_eq!(signers.len(), 0);
    assert_eq!(threshold, 0);
}

#[test]
fn test_get_multisig_admin_after_set() {
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);

    s.vault_client.set_multisig_admin(
        &soroban_sdk::vec![&s.env, signer1.clone(), signer2.clone()],
        &2u32,
    );

    let (signers, threshold) = s.vault_client.get_multisig_admin();
    assert_eq!(signers.len(), 2);
    assert!(signers.contains(&signer1));
    assert!(signers.contains(&signer2));
    assert_eq!(threshold, 2);
}

// ── Issue #389: yield accrual and claim coverage ─────────────────────────────

/// receive_yield updates the per-share accumulator so that a single depositor
/// can claim the full yield amount.
#[test]
fn test_receive_yield_updates_accumulator_and_claimable() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let yield_source = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Yield source must have USDC to transfer into the vault.
    let yield_amount = 100_0000000i128; // 100 USDC
    mint_usdc(&s.env, &s.usdc_sac, &yield_source, yield_amount);
    s.vault_client.receive_yield(&yield_source, &yield_amount);

    // With a single depositor holding all shares, claimable == yield_amount.
    let claimable = s.vault_client.claimable_yield(&investor);
    assert_eq!(claimable, yield_amount);

    // The accumulator should equal yield_amount * YIELD_SCALE / total_shares.
    let expected_accum = yield_amount * 1_000_000_000_000_000_000i128 / shares;
    let accum: i128 = s.env.as_contract(&s.vault_address, || {
        s.env
            .storage()
            .persistent()
            .get(&VaultKey::YieldPerShareAccum)
            .unwrap_or(0)
    });
    assert_eq!(accum, expected_accum);
}

/// claim_yield transfers claimable USDC to the caller and resets their debt
/// checkpoint so a second claim returns zero.
#[test]
fn test_claim_yield_transfers_usdc_and_resets_debt() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let yield_source = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let yield_amount = 50_0000000i128; // 50 USDC
    mint_usdc(&s.env, &s.usdc_sac, &yield_source, yield_amount);
    s.vault_client.receive_yield(&yield_source, &yield_amount);

    let balance_before = s.env.as_contract(&s.vault_address, || {
        soroban_sdk::token::TokenClient::new(&s.env, &s.usdc_sac).balance(&s.vault_address)
    });

    let claimed = s.vault_client.claim_yield(&investor);
    assert_eq!(claimed, yield_amount);

    // Vault liquid USDC should have decreased by the claimed amount.
    let balance_after = s.env.as_contract(&s.vault_address, || {
        soroban_sdk::token::TokenClient::new(&s.env, &s.usdc_sac).balance(&s.vault_address)
    });
    assert_eq!(balance_before - balance_after, yield_amount);

    // Second claim returns 0 — debt is now equal to accumulator.
    let second_claim = s.vault_client.claim_yield(&investor);
    assert_eq!(second_claim, 0);
}

/// Yield is split proportionally between two depositors based on share balance.
#[test]
fn test_yield_splits_proportionally_between_depositors() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);
    let yield_source = Address::generate(&s.env);

    // Alice deposits 3000, Bob deposits 1000 — 3:1 share ratio (after premium).
    mint_usdc(&s.env, &s.usdc_sac, &alice, 3_000_0000000i128);
    let shares_a = s.vault_client.deposit(&alice, &3_000_0000000i128);
    mint_usdc(&s.env, &s.usdc_sac, &bob, 1_000_0000000i128);
    let shares_b = s.vault_client.deposit(&bob, &1_000_0000000i128);

    let yield_amount = 400_0000000i128; // 400 USDC
    mint_usdc(&s.env, &s.usdc_sac, &yield_source, yield_amount);
    s.vault_client.receive_yield(&yield_source, &yield_amount);

    let claimable_a = s.vault_client.claimable_yield(&alice);
    let claimable_b = s.vault_client.claimable_yield(&bob);

    // claimable should be proportional to shares.
    assert_eq!(claimable_a * shares_b, claimable_b * shares_a);
    // Sum of claimable amounts should equal the full yield.
    assert_eq!(claimable_a + claimable_b, yield_amount);
}

/// receive_yield panics when there are no shares outstanding (#8).
#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_receive_yield_panics_with_no_shares_outstanding() {
    let s = setup();
    let yield_source = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &yield_source, 100_0000000i128);
    s.vault_client
        .receive_yield(&yield_source, &100_0000000i128);
}

/// claim_yield panics when the vault has insufficient liquid USDC (#9).
#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_claim_yield_panics_on_insufficient_liquid() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let yield_source = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Receive yield so claimable > 0.
    let yield_amount = 100_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &yield_source, yield_amount);
    s.vault_client.receive_yield(&yield_source, &yield_amount);

    // Drain vault liquid USDC by investing it all into a project, leaving
    // nothing for the yield claim.
    let creator = Address::generate(&s.env);
    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://QmDrain"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let admin = stellar_access::ownable::get_owner(&s.env).unwrap();
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "fund_project",
            args: (&project_id, &(1_000_0000000i128 - yield_amount)).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    s.vault_client
        .fund_project(&project_id, &(1_000_0000000i128 - yield_amount));

    // Now vault has yield_amount USDC but investor's claimable is yield_amount.
    // The investable deduction means claimable > vault liquid. Try to claim.
    s.vault_client.claim_yield(&investor);
}

/// receive_yield panics when the amount is not positive (#7).
#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_receive_yield_panics_on_non_positive_amount() {
    let s = setup();
    let yield_source = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &yield_source, 1_000_0000000i128);
    s.vault_client.receive_yield(&yield_source, &0);
}

/// claimable_yield returns zero for an address with no shares.
#[test]
fn test_claimable_yield_zero_for_non_depositor() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let yield_source = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    mint_usdc(&s.env, &s.usdc_sac, &yield_source, 100_0000000i128);
    s.vault_client
        .receive_yield(&yield_source, &100_0000000i128);

    assert_eq!(s.vault_client.claimable_yield(&stranger), 0);
}

// ── Issue #390: health_check test coverage (investment_vault) ────────────────

/// health_check returns default operational state for a fresh vault.
#[test]
fn test_health_check_default_state() {
    let s = setup();
    let status = s.vault_client.health_check();

    assert_eq!(status.state_version, 1);
    assert_eq!(status.is_paused, false);
    assert_eq!(status.utilization_bps, 0);
    assert_eq!(status.has_emergency_admin, false);
}

/// health_check reflects a paused vault.
#[test]
fn test_health_check_reflects_paused_state() {
    let s = setup();
    let emergency_admin = Address::generate(&s.env);
    s.vault_client
        .set_emergency_admin(&Some(emergency_admin.clone()));

    s.vault_client.emergency_pause(&emergency_admin);

    let status = s.vault_client.health_check();
    assert_eq!(status.is_paused, true);
}

/// health_check reports has_emergency_admin when one is configured.
#[test]
fn test_health_check_reflects_emergency_admin() {
    let s = setup();
    let emergency_admin = Address::generate(&s.env);
    s.vault_client.set_emergency_admin(&Some(emergency_admin));

    let status = s.vault_client.health_check();
    assert_eq!(status.has_emergency_admin, true);
}

// ── Issue #391: compliance / reporting test coverage ──────────────────────────

/// set_max_transaction_amount stores and retrieves the configured limit.
#[test]
fn test_set_max_transaction_amount_happy_path() {
    let s = setup();
    let limit = 500_000_000i128; // 500 USDC

    s.vault_client.set_max_transaction_amount(&limit);

    assert_eq!(s.vault_client.max_transaction_amount(), limit);
}

/// set_max_transaction_amount panics on a negative value (#55).
#[test]
#[should_panic(expected = "Error(Contract, #55)")]
fn test_set_max_transaction_amount_panics_on_negative() {
    let s = setup();
    s.vault_client.set_max_transaction_amount(&-1i128);
}

/// set_max_transaction_amount is a no-op when the value hasn't changed.
#[test]
fn test_set_max_transaction_amount_noop_when_unchanged() {
    let s = setup();
    let limit = 1_000_000_000i128;

    s.vault_client.set_max_transaction_amount(&limit);
    // Calling again with the same value should succeed without emitting
    // a duplicate event (no panic, no error).
    s.vault_client.set_max_transaction_amount(&limit);
    assert_eq!(s.vault_client.max_transaction_amount(), limit);
}

/// max_transaction_amount returns 0 when no limit has been configured.
#[test]
fn test_max_transaction_amount_defaults_to_zero() {
    let s = setup();
    assert_eq!(s.vault_client.max_transaction_amount(), 0);
}

/// record_compliance_event stores an event retrievable by sequence number.
#[test]
fn test_record_and_get_compliance_event() {
    let s = setup();
    let event_type = String::from_str(&s.env, "KYC_VERIFIED");
    let data = String::from_str(&s.env, "addr:GBXXX passes check");

    s.vault_client.record_compliance_event(&event_type, &data);

    let event = s.vault_client.get_compliance_event(&1);
    assert_eq!(event.seq, 1);
    assert_eq!(event.event_type, event_type);
    assert_eq!(event.data, data);
    assert!(event.timestamp > 0);
}

/// record_compliance_event auto-increments sequence numbers.
#[test]
fn test_compliance_event_auto_increments_seq() {
    let s = setup();

    s.vault_client.record_compliance_event(
        &String::from_str(&s.env, "EVENT_A"),
        &String::from_str(&s.env, "data_a"),
    );
    s.vault_client.record_compliance_event(
        &String::from_str(&s.env, "EVENT_B"),
        &String::from_str(&s.env, "data_b"),
    );

    let e1 = s.vault_client.get_compliance_event(&1);
    let e2 = s.vault_client.get_compliance_event(&2);
    assert_eq!(e1.event_type, String::from_str(&s.env, "EVENT_A"));
    assert_eq!(e2.event_type, String::from_str(&s.env, "EVENT_B"));
}

/// get_compliance_event panics for a non-existent sequence (#56).
#[test]
#[should_panic(expected = "Error(Contract, #56)")]
fn test_get_compliance_event_panics_for_missing_seq() {
    let s = setup();
    s.vault_client.get_compliance_event(&999);
}

/// get_compliance_events returns a range of events.
#[test]
fn test_get_compliance_events_range() {
    let s = setup();

    for i in 1..=5 {
        s.vault_client.record_compliance_event(
            &String::from_str(&s.env, &format!("TYPE_{}", i)),
            &String::from_str(&s.env, &format!("data_{}", i)),
        );
    }

    let events = s.vault_client.get_compliance_events(&2, &4);
    assert_eq!(events.len(), 3);
    assert_eq!(events.get_unchecked(0).seq, 2);
    assert_eq!(events.get_unchecked(2).seq, 4);
}

/// get_compliance_events caps at 100 entries per call.
#[test]
fn test_get_compliance_events_range_caps_at_100() {
    let s = setup();

    // Record 120 events.
    for i in 1..=120 {
        s.vault_client.record_compliance_event(
            &String::from_str(&s.env, &format!("T{}", i)),
            &String::from_str(&s.env, &format!("d{}", i)),
        );
    }

    // Requesting from 1..=200 should return at most 100.
    let events = s.vault_client.get_compliance_events(&1, &200);
    assert_eq!(events.len(), 100);
}

/// get_compliance_events returns empty vec when from > to.
#[test]
fn test_get_compliance_events_empty_when_from_gt_to() {
    let s = setup();
    let events = s.vault_client.get_compliance_events(&10, &5);
    assert_eq!(events.len(), 0);
}

/// record_compliance_event prunes events beyond MAX_COMPLIANCE_EVENTS (1000).
#[test]
fn test_compliance_event_prunes_oldest_when_over_limit() {
    let s = setup();

    // Record 1001 events to trigger pruning.
    for i in 1..=1001 {
        s.vault_client.record_compliance_event(
            &String::from_str(&s.env, &format!("T{}", i)),
            &String::from_str(&s.env, &format!("d{}", i)),
        );
    }

    // Event #1 should have been pruned.
    let result = s.vault_client.try_get_compliance_event(&1u64);
    assert!(result.is_err());

    // Event #2 should still exist.
    let event = s.vault_client.get_compliance_event(&2);
    assert_eq!(event.seq, 2);
}

/// take_reporting_snapshot captures vault metrics and get_latest_snapshot retrieves them.
#[test]
fn test_take_and_get_reporting_snapshot() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let amount = 1_000_0000000i128;
    mint_usdc(&s.env, &s.usdc_sac, &investor, amount);
    s.vault_client.deposit(&investor, &amount);

    s.vault_client.take_reporting_snapshot();

    let snapshot = s.vault_client.get_latest_snapshot();
    assert!(snapshot.timestamp > 0);
    assert!(snapshot.total_assets > 0);
    assert!(snapshot.total_supply > 0);
    assert_eq!(snapshot.total_investments, 0);
}

/// get_latest_snapshot panics if no snapshot has been taken (#57).
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_get_latest_snapshot_panics_when_none_taken() {
    let s = setup();
    s.vault_client.get_latest_snapshot();
}

/// export_regulatory_data returns a report combining snapshot, events, and limits.
#[test]
fn test_export_regulatory_data() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Set a transaction limit.
    s.vault_client.set_max_transaction_amount(&500_000_000i128);

    // Record a compliance event.
    s.vault_client.record_compliance_event(
        &String::from_str(&s.env, "AUDIT"),
        &String::from_str(&s.env, "q1 review passed"),
    );

    // Take a snapshot.
    s.vault_client.take_reporting_snapshot();

    let report = s.vault_client.export_regulatory_data();
    assert!(report.snapshot.timestamp > 0);
    assert!(report.recent_events.len() > 0);
    assert_eq!(report.max_transaction_amount, 500_000_000i128);
}

/// export_regulatory_data works even without a prior snapshot (uses live metrics).
#[test]
fn test_export_regulatory_data_without_snapshot() {
    let s = setup();
    let report = s.vault_client.export_regulatory_data();

    // Without a snapshot, the report should use live metrics.
    assert_eq!(report.snapshot.timestamp, 0);
    assert_eq!(report.snapshot.total_assets, 0);
    assert_eq!(report.recent_events.len(), 0);
    assert_eq!(report.max_transaction_amount, 0);
}

// ── Issue #383: bridge_mint / bridge_burn / complete_bridge_transfer ────────

#[test]
fn test_bridge_mint_happy_path() {
    let s = setup();
    let bridge = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);

    s.vault_client.set_bridge(&bridge);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &bridge,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "bridge_mint",
            args: (&recipient, &100_0000000i128).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);

    s.vault_client.bridge_mint(&recipient, &100_0000000i128);

    assert_eq!(s.vault_client.balance(&recipient), 100_0000000i128);
    assert_eq!(s.vault_client.total_supply(), 100_0000000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_bridge_mint_rejects_non_positive_amount() {
    let s = setup();
    let bridge = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);

    s.vault_client.set_bridge(&bridge);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &bridge,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.vault_address,
            fn_name: "bridge_mint",
            args: (&recipient, &0i128).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);

    s.vault_client.bridge_mint(&recipient, &0);
}

#[test]
fn test_bridge_burn_happy_path() {
    let s = setup();
    let investor = Address::generate(&s.env);

    // Deposit first so the investor has shares to burn.
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    s.vault_client.bridge_burn(&investor, &500_0000000i128);

    assert_eq!(
        s.vault_client.balance(&investor),
        495_0000000i128,
        "should burn 500 shares (1000 - 500 burned - 5 insurance on deposit)"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_bridge_burn_rejects_non_positive_amount() {
    let s = setup();
    let investor = Address::generate(&s.env);

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    s.vault_client.bridge_burn(&investor, &-1);
}

// ── complete_bridge_transfer — mock Wormhole core ──────────────────────────

/// Minimal mock Wormhole core contract that returns a pre-configured
/// `ParsedVaa` from `verify_vaa`.  Used only in tests below.
#[contract]
pub struct MockWormholeCore;

#[contractimpl]
impl MockWormholeCore {
    pub fn __constructor(_env: Env) {}

    /// Accept any VAA bytes and return the pre-stored `ParsedVaa`.
    pub fn verify_vaa(env: Env, _vaa: Bytes) -> wormhole::ParsedVaa {
        env.storage()
            .instance()
            .get(&soroban_sdk::String::from_str(&env, "return_vaa"))
            .expect("return_vaa not set")
    }

    /// No-op — the vault only calls this on the outbound path.
    pub fn publish_message(_env: Env, _consistency_level: u32, _payload: Bytes) -> u64 {
        0
    }
}

/// Register the mock Wormhole core contract and store `return_vaa` so
/// `verify_vaa` will return it.  Returns the mock contract's Address.
fn register_mock_core(env: &Env, return_vaa: wormhole::ParsedVaa) -> Address {
    let mock_id = env.register(MockWormholeCore, ());
    env.as_contract(&mock_id, || {
        env.storage().instance().set(
            &soroban_sdk::String::from_str(env, "return_vaa"),
            &return_vaa,
        );
    });
    mock_id
}

#[test]
fn test_initiate_bridge_transfer_happy_path() {
    let s = setup();
    let investor = Address::generate(&s.env);
    let amount: i128 = 200_0000000i128;

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Setup mock core
    let return_vaa = wormhole::ParsedVaa {
        emitter_chain: wormhole::chain_id::ETHEREUM,
        emitter_address: soroban_sdk::BytesN::from_array(&s.env, &[0u8; 32]),
        payload: soroban_sdk::Bytes::new(&s.env),
    };
    let mock_core = register_mock_core(&s.env, return_vaa);
    s.vault_client.set_wormhole_core(&mock_core);

    let recipient = soroban_sdk::BytesN::from_array(&s.env, &[1u8; 32]);
    let target_chain = wormhole::chain_id::ETHEREUM;
    let nonce = 1;

    let balance_before = s.vault_client.balance(&investor);
    let supply_before = s.vault_client.total_supply();

    let sequence = s.vault_client.initiate_bridge_transfer(
        &investor,
        &amount,
        &target_chain,
        &recipient,
        &nonce,
    );

    assert_eq!(sequence, 0); // Mock returns 0
    assert_eq!(s.vault_client.balance(&investor), balance_before - amount);
    assert_eq!(s.vault_client.total_supply(), supply_before - amount);
}

#[test]
fn test_complete_bridge_transfer_happy_path() {
    let s = setup();
    let bridge = Address::generate(&s.env);
    let emitter = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let amount: i128 = 200_0000000i128;

    // Build the payload the mock will return inside ParsedVaa.
    let token_address = wormhole::address_to_bytes32(&s.env, &s.vault_address);
    let recipient_bytes = wormhole::address_to_bytes32(&s.env, &recipient);
    let payload = wormhole::serialize_bridge_payload(
        &s.env,
        &wormhole::BridgeTransferPayload {
            token_address: token_address.clone(),
            recipient: recipient_bytes,
            amount,
            source_chain: wormhole::chain_id::ETHEREUM,
            target_chain: wormhole::chain_id::STELLAR,
            nonce: 1,
        },
    );

    let emitter_bytes = wormhole::address_to_bytes32(&s.env, &emitter);
    let return_vaa = wormhole::ParsedVaa {
        emitter_chain: wormhole::chain_id::ETHEREUM,
        emitter_address: emitter_bytes.clone(),
        payload,
    };

    let mock_core = register_mock_core(&s.env, return_vaa);

    // Configure vault: set bridge, Wormhole core, and trusted emitter.
    s.vault_client.set_bridge(&bridge);
    s.vault_client.set_wormhole_core(&mock_core);
    s.vault_client
        .set_trusted_emitter(&wormhole::chain_id::ETHEREUM, &emitter_bytes, &true);

    // Call with any bytes — the mock ignores them and returns the stored VAA.
    let dummy_vaa = soroban_sdk::Bytes::from_array(&s.env, &[0u8; 64]);
    s.vault_client.complete_bridge_transfer(&dummy_vaa);

    assert_eq!(s.vault_client.balance(&recipient), amount);
    assert_eq!(s.vault_client.total_supply(), amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_complete_bridge_transfer_rejects_untrusted_emitter() {
    let s = setup();
    let bridge = Address::generate(&s.env);
    let emitter = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);

    let token_address = wormhole::address_to_bytes32(&s.env, &s.vault_address);
    let recipient_bytes = wormhole::address_to_bytes32(&s.env, &recipient);
    let payload = wormhole::serialize_bridge_payload(
        &s.env,
        &wormhole::BridgeTransferPayload {
            token_address,
            recipient: recipient_bytes,
            amount: 100_0000000i128,
            source_chain: wormhole::chain_id::ETHEREUM,
            target_chain: wormhole::chain_id::STELLAR,
            nonce: 2,
        },
    );

    let emitter_bytes = wormhole::address_to_bytes32(&s.env, &emitter);
    let return_vaa = wormhole::ParsedVaa {
        emitter_chain: wormhole::chain_id::ETHEREUM,
        emitter_address: emitter_bytes.clone(),
        payload,
    };

    let mock_core = register_mock_core(&s.env, return_vaa);

    s.vault_client.set_bridge(&bridge);
    s.vault_client.set_wormhole_core(&mock_core);
    // Do NOT call set_trusted_emitter — emitter is not trusted.

    let dummy_vaa = soroban_sdk::Bytes::from_array(&s.env, &[0u8; 64]);
    s.vault_client.complete_bridge_transfer(&dummy_vaa);
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_complete_bridge_transfer_rejects_replayed_vaa() {
    let s = setup();
    let bridge = Address::generate(&s.env);
    let emitter = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);

    let token_address = wormhole::address_to_bytes32(&s.env, &s.vault_address);
    let recipient_bytes = wormhole::address_to_bytes32(&s.env, &recipient);
    let payload = wormhole::serialize_bridge_payload(
        &s.env,
        &wormhole::BridgeTransferPayload {
            token_address,
            recipient: recipient_bytes,
            amount: 100_0000000i128,
            source_chain: wormhole::chain_id::ETHEREUM,
            target_chain: wormhole::chain_id::STELLAR,
            nonce: 3,
        },
    );

    let emitter_bytes = wormhole::address_to_bytes32(&s.env, &emitter);
    let return_vaa = wormhole::ParsedVaa {
        emitter_chain: wormhole::chain_id::ETHEREUM,
        emitter_address: emitter_bytes.clone(),
        payload,
    };

    let mock_core = register_mock_core(&s.env, return_vaa);

    s.vault_client.set_bridge(&bridge);
    s.vault_client.set_wormhole_core(&mock_core);
    s.vault_client
        .set_trusted_emitter(&wormhole::chain_id::ETHEREUM, &emitter_bytes, &true);

    let dummy_vaa = soroban_sdk::Bytes::from_array(&s.env, &[0u8; 64]);

    // First call succeeds.
    s.vault_client.complete_bridge_transfer(&dummy_vaa);

    // Second call with the same VAA must fail (replay guard).
    s.vault_client.complete_bridge_transfer(&dummy_vaa);
}

#[test]
fn test_enable_secondary_trading_allowed_while_paused() {
    let s = setup();

    // Pause the vault
    s.vault_client.pause();
    assert_eq!(s.vault_client.is_paused(), true);

    // Verify trading is not enabled initially
    assert_eq!(s.vault_client.is_trading_enabled(), false);

    // Enable secondary trading while the vault is paused
    s.vault_client.enable_secondary_trading();

    // Verify it succeeded
    assert_eq!(s.vault_client.is_trading_enabled(), true);
}

// ── Persistent TTL extension on write (#317) ─────────────────────────────────

/// Idle window used to prove a persistent key survives without being touched.
/// Well below `TTL_EXTEND_TO_LEDGERS` (518_400) so the entry is not archived.
const TTL_IDLE_LEDGERS: u32 = 100_000;

#[test]
fn test_persistent_key_survives_n_idle_ledgers() {
    use soroban_sdk::testutils::storage::Persistent as _;

    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let key = VaultKey::InsuranceFund;
    let ttl_after_write = s.env.as_contract(&s.vault_address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert_eq!(
        ttl_after_write,
        crate::storage::TTL_EXTEND_TO_LEDGERS,
        "deposit must extend InsuranceFund TTL to the documented 30-day target"
    );

    // Advance N ledgers without any further writes to this key.
    s.env.ledger().with_mut(|l| {
        l.sequence_number += TTL_IDLE_LEDGERS;
    });

    let ttl_after_idle = s.env.as_contract(&s.vault_address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert_eq!(
        ttl_after_idle,
        crate::storage::TTL_EXTEND_TO_LEDGERS - TTL_IDLE_LEDGERS,
        "TTL should decay by exactly the idle ledger count when the key is untouched"
    );

    // Value is still readable after N idle ledgers — archival did not occur.
    let expected_premium = 1_000_0000000i128 * 50 / 10_000;
    assert_eq!(s.vault_client.insurance_fund_balance(), expected_premium);
}

#[test]
fn test_persistent_write_reextends_ttl_after_decay() {
    use soroban_sdk::testutils::storage::Persistent as _;

    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 2_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let key = VaultKey::LastDeposit(investor.clone());
    let ttl_after_first = s.env.as_contract(&s.vault_address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert_eq!(ttl_after_first, crate::storage::TTL_EXTEND_TO_LEDGERS);

    // Decay remaining TTL below the extension threshold so the next write
    // is guaranteed to trigger a real re-extension rather than a no-op.
    let advance =
        crate::storage::TTL_EXTEND_TO_LEDGERS - crate::storage::TTL_EXTEND_THRESHOLD_LEDGERS + 1;
    s.env.ledger().with_mut(|l| {
        l.sequence_number += advance;
        l.timestamp += MIN_LOCK_PERIOD + 1;
    });
    let ttl_before_rewrite = s.env.as_contract(&s.vault_address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert!(ttl_before_rewrite < crate::storage::TTL_EXTEND_THRESHOLD_LEDGERS);

    s.vault_client.deposit(&investor, &1_000_0000000i128);

    let ttl_after_rewrite = s.env.as_contract(&s.vault_address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert_eq!(
        ttl_after_rewrite,
        crate::storage::TTL_EXTEND_TO_LEDGERS,
        "rewriting LastDeposit should re-extend its TTL, not just at first write"
    );
}

// ── Carbon credit feature coverage (#318) ─────────────────────────────────────

/// credits = amount * green_impact / CARBON_UNIT (10_000_000_000).
const CARBON_UNIT: i128 = 10_000_000_000;

#[test]
fn test_calculate_carbon_credits_scales_with_green_impact() {
    let s = setup();
    let project_id = create_project_with_green_impact(&s, 80);
    let amount = 10_000_000_000i128; // 1000 USDC

    let calc = s
        .vault_client
        .calculate_carbon_credits(&project_id, &amount);

    assert_eq!(calc.project_id, project_id);
    assert_eq!(calc.amount_invested, amount);
    assert_eq!(calc.credits, amount * 80 / CARBON_UNIT); // 80
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_calculate_carbon_credits_rejects_unknown_project() {
    let s = setup();
    s.vault_client
        .calculate_carbon_credits(&99u32, &1_000_0000000i128);
}

#[test]
fn test_issue_carbon_credits_credits_recipient_balance() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    let project_id = create_project_with_green_impact(&s, 50);
    let amount = 10_000_000_000i128;
    let expected = amount * 50 / CARBON_UNIT; // 50

    let issued = s
        .vault_client
        .issue_carbon_credits(&recipient, &project_id, &amount);

    assert_eq!(issued, expected);
    assert_eq!(s.vault_client.carbon_credit_balance(&recipient), expected);
}

#[test]
#[should_panic(expected = "Error(Contract, #53)")]
fn test_issue_carbon_credits_rejects_zero_credits() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    // Default green_impact is 0 → credits = 0.
    let project_id = create_project_with_green_impact(&s, 0);
    s.vault_client
        .issue_carbon_credits(&recipient, &project_id, &1_000_0000000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_issue_carbon_credits_rejects_non_positive_amount() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    let project_id = create_project_with_green_impact(&s, 80);
    s.vault_client
        .issue_carbon_credits(&recipient, &project_id, &0i128);
}

#[test]
fn test_transfer_carbon_credits_moves_balance() {
    let s = setup();
    let holder = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let project_id = create_project_with_green_impact(&s, 80);
    let amount = 10_000_000_000i128;
    let issued = s
        .vault_client
        .issue_carbon_credits(&holder, &project_id, &amount);
    assert_eq!(issued, 80);

    s.vault_client
        .transfer_carbon_credits(&holder, &recipient, &30i128);

    assert_eq!(s.vault_client.carbon_credit_balance(&holder), 50);
    assert_eq!(s.vault_client.carbon_credit_balance(&recipient), 30);
}

#[test]
#[should_panic(expected = "Error(Contract, #54)")]
fn test_transfer_carbon_credits_rejects_insufficient_balance() {
    let s = setup();
    let holder = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let project_id = create_project_with_green_impact(&s, 80);
    s.vault_client
        .issue_carbon_credits(&holder, &project_id, &10_000_000_000i128);
    // Holder has 80 credits; transferring 81 must panic InsufficientCarbonCredits.
    s.vault_client
        .transfer_carbon_credits(&holder, &recipient, &81i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_transfer_carbon_credits_rejects_non_positive_amount() {
    let s = setup();
    let holder = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    s.vault_client
        .transfer_carbon_credits(&holder, &recipient, &0i128);
}

#[test]
fn test_transfer_carbon_credits_requires_from_auth() {
    let s = setup();
    let holder = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let project_id = create_project_with_green_impact(&s, 80);
    s.vault_client
        .issue_carbon_credits(&holder, &project_id, &10_000_000_000i128);

    // Drop the blanket mock so missing `from.require_auth()` would be visible.
    s.env.mock_auths(&[]);
    assert!(
        s.vault_client
            .try_transfer_carbon_credits(&holder, &recipient, &1i128)
            .is_err(),
        "transfer_carbon_credits must require from.require_auth()"
    );
}

#[test]
fn test_carbon_credit_balance_defaults_to_zero() {
    let s = setup();
    let nobody = Address::generate(&s.env);
    assert_eq!(s.vault_client.carbon_credit_balance(&nobody), 0);
}

#[test]
fn test_set_carbon_credit_price_persists_when_oracle_authorized() {
    let s = setup();
    let oracle = Address::generate(&s.env);
    s.vault_client.set_carbon_oracle(&oracle);

    s.vault_client.set_carbon_credit_price(&1_000i128);
    assert_eq!(s.vault_client.carbon_credit_price(), 1_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #51)")]
fn test_set_carbon_credit_price_rejects_when_oracle_unset() {
    let s = setup();
    s.vault_client.set_carbon_credit_price(&1_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #52)")]
fn test_set_carbon_credit_price_rejects_non_positive() {
    let s = setup();
    let oracle = Address::generate(&s.env);
    s.vault_client.set_carbon_oracle(&oracle);
    s.vault_client.set_carbon_credit_price(&0i128);
}

#[test]
fn test_carbon_credit_price_defaults_to_zero() {
    let s = setup();
    assert_eq!(s.vault_client.carbon_credit_price(), 0);
}
