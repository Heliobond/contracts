#![cfg(test)]
#![allow(clippy::inconsistent_digit_grouping)]
extern crate std;
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    token::TokenClient,
    Address, Env, IntoVal, String,
};

mod registry_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/project_registry.wasm");
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
    s.vault_client.withdraw(&investor, &10_0000000i128);
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
    let returned = s.vault_client.withdraw(&investor, &shares);

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
    );
    let project2 = registry_client.create_project(
        &creator2,
        &String::from_str(&s.env, "ipfs://QmBatchFund2"),
        &0u64,
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
fn test_initialize() {
    // With __constructor, registration IS initialization
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc = Address::generate(&env);
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
    let usdc = Address::generate(&env);
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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);

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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);

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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);

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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);

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
    );
    // Fund with all deployable USDC (liquid − insurance = 995); vault liquid drops to 5
    s.vault_client.fund_project(&project_id, &995_0000000i128);

    // Full share redemption requires ~1000 USDC but only 5 liquid remain
    s.vault_client.withdraw(&investor, &shares);
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

    // Full withdrawal with no outstanding investments drains the vault cleanly
    s.vault_client.withdraw(&investor, &shares);

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
    );
    // Fund 1000 USDC; vault liquid = 1000 but total assets = 2000
    s.vault_client.fund_project(&project_id, &1_000_0000000i128);

    // Full share redemption needs 2000 USDC but only 1000 liquid — must fail
    s.vault_client.withdraw(&investor, &shares);
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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://test"), &0u64);
    // Fund 490 USDC (49% utilization — below the 50% limit threshold so the full
    // withdrawal is allowed but only ~510 USDC is liquid, causing a queue.
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Shares are worth ~1000 USDC but only ~510 USDC is liquid — should enqueue.
    let returned = s.vault_client.withdraw(&investor, &shares);

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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://test"), &0u64);
    // Fund 490 USDC (49% util) to stay below the 50% graduated withdrawal limit.
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Queue the withdrawal.
    let owed = s.vault_client.convert_to_assets(&shares);
    s.vault_client.withdraw(&investor1, &shares);

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

    s.vault_client.withdraw(&investor, &shares);

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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
    // Fund 490 USDC (49% util) to stay below the 50% graduated withdrawal limit.
    s.vault_client.fund_project(&project_id, &490_0000000i128);

    // Withdrawal exceeds liquid USDC — should enqueue and emit WithdrawQueued.
    let returned = s.vault_client.withdraw(&investor, &shares);
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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
    // Fund 490 USDC (49% util) to stay below the 50% graduated withdrawal limit.
    s.vault_client.fund_project(&project_id, &490_0000000i128);
    s.vault_client.withdraw(&investor, &shares);

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

    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    let registry_client = registry_contract::Client::new(&s.env, &s.registry);
    registry_client.set_whitelist(&creator, &true);
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
    // Fund 800 USDC: liquid = 200, investments = 800, utilization = 800/(200+800) = 80%
    s.vault_client.fund_project(&project_id, &800_0000000i128);

    assert!(
        s.vault_client.get_utilization_bps() >= 7_000,
        "utilization should be at or above warning threshold"
    );

    // Withdraw a small amount within the utilization limit — warning event should fire.
    let small_shares = shares / 100; // 1% of total shares
    s.vault_client.withdraw(&investor, &small_shares);

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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
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
    let project_id =
        registry_client.create_project(&creator, &String::from_str(&s.env, "ipfs://Qm"), &0u64);
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
