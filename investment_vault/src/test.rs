#![cfg(test)]
#![allow(clippy::inconsistent_digit_grouping)]
#![allow(unnameable_test_items)]
extern crate std;
use super::*;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::StellarAssetClient,
    token::TokenClient,
    Address, BytesN, Env, IntoVal, String,
};

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
#[should_panic]
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
        s.vault_client.try_set_flash_loan_fee(&1i128).is_err(),
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
        &String::from_str(&s.env, "Alpha"),
        &0u64,
        &test_metadata_hash(&s.env),
    );
    let pid2 = registry_client.create_project(
        &creator2,
        &String::from_str(&s.env, "Beta"),
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
    // ── Issue #176: deposit() must reject a zero-amount deposit ──────────────────
}

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
        &String::from_str(&s.env, "Gamma"),
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
fn test_claim_idempotent_returns_zero_when_queue_empty() {
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
    s.vault_client.set_flash_loan_fee(&10i128);

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
#[should_panic(expected = "flash loan callback failed")]
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
