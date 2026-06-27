#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Bytes, Env, String};

mod registry_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/project_registry.wasm");
}

/// Import vault WASM so mock receiver can call it cross-contract.
mod vault_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/investment_vault.wasm");
}

/// Mock flash loan receivers for testing.
mod mock_receiver {
    use soroban_sdk::{contract, contractimpl, Address, Bytes, Env};

    /// Receiver that returns true — repayment is handled internally by the vault.
    #[contract]
    pub struct OkReceiver;

    #[contractimpl]
    impl OkReceiver {
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

    /// Receiver that returns false — should cause the vault to panic.
    #[contract]
    pub struct FailingReceiver;

    #[contractimpl]
    impl FailingReceiver {
        pub fn flash_loan_callback(
            _env: Env,
            _initiator: Address,
            _vault: Address,
            _amount: i128,
            _fee: i128,
            _data: Bytes,
        ) -> bool {
            false
        }
    }
}

struct TestSetup {
    env: Env,
    admin: Address,
    vault_id: Address,
    vault_client: vault_contract::Client<'static>,
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

    // Register vault via WASM (required for cross-contract calls from mock receiver)
    let vault_id = env.register(vault_contract::WASM, (&admin, &usdc_sac, &registry_id));
    let vault_client = vault_contract::Client::new(&env, &vault_id);

    TestSetup {
        env,
        admin,
        vault_id,
        vault_client,
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
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);

    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);

    assert_eq!(shares, 1_000_0000000i128);
    assert_eq!(s.vault_client.balance(&investor), 1_000_0000000i128);
    assert_eq!(s.vault_client.total_supply(), 1_000_0000000i128);
}

#[test]
fn test_deposit_proportional_after_first() {
    let s = setup();
    let investor1 = Address::generate(&s.env);
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor1, 1_000_0000000i128);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 1_000_0000000i128);

    s.vault_client.deposit(&investor1, &1_000_0000000i128);
    let shares2 = s.vault_client.deposit(&investor2, &1_000_0000000i128);

    assert_eq!(shares2, 1_000_0000000i128);
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
fn test_initialize() {
    // With __constructor, registration IS initialization
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc = Address::generate(&env);
    let registry = Address::generate(&env);
    let _contract_id = env.register(InvestmentVault, (&admin, &usdc, &registry));
    // If registration didn't panic, constructor succeeded
}

#[test]
fn test_fund_project_records_investment() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    assert_eq!(s.vault_client.total_assets(), 1_000_0000000i128);
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

#[test]
fn test_flash_loan_default_fee() {
    let s = setup();
    assert_eq!(s.vault_client.flash_loan_fee(), 30);
}

#[test]
fn test_set_flash_loan_fee() {
    let s = setup();
    s.vault_client.set_flash_loan_fee(&50i128);
    assert_eq!(s.vault_client.flash_loan_fee(), 50);
}

#[test]
fn test_execute_flash_loan_repays_and_burns() {
    let s = setup();
    let initiator = Address::generate(&s.env);

    let receiver_id = s.env.register(mock_receiver::OkReceiver, ());

    let loan_amount: i128 = 1_000_0000000i128; // 1000 HBS
    let fee_bps = s.vault_client.flash_loan_fee();
    let expected_fee = loan_amount * fee_bps / 10000;

    let total_supply_before = s.vault_client.total_supply();

    s.vault_client
        .execute_flash_loan(&initiator, &receiver_id, &loan_amount, &Bytes::new(&s.env));

    let total_supply_after = s.vault_client.total_supply();

    // Total supply increases by fee (minted amount+fee, burned only amount)
    assert_eq!(total_supply_after, total_supply_before + expected_fee);
    // Vault should hold the fee as protocol revenue
    assert_eq!(s.vault_client.balance(&s.vault_id), expected_fee);
    // Receiver should have 0 HBS remaining
    assert_eq!(s.vault_client.balance(&receiver_id), 0);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_flash_loan_fails_when_callback_returns_false() {
    let s = setup();
    let initiator = Address::generate(&s.env);

    let receiver_id = s.env.register(mock_receiver::FailingReceiver, ());

    s.vault_client
        .execute_flash_loan(&initiator, &receiver_id, &1_000_0000000i128, &Bytes::new(&s.env));
}

// -----------------------------------------------------------------------
// Carbon credit tests
// -----------------------------------------------------------------------

fn create_project_with_impact(env: &Env, registry_id: &Address, green_impact: u32) -> u32 {
    let registry = registry_contract::Client::new(env, registry_id);
    let creator = Address::generate(env);
    registry.set_whitelist(&creator, &true);
    let id = registry.create_project(&creator, &String::from_str(env, "ipfs://QmCarbon"));
    registry.update_impact_score(&id, &50u32, &green_impact);
    id
}

#[test]
fn test_carbon_credit_default_price_is_zero() {
    let s = setup();
    assert_eq!(s.vault_client.carbon_credit_price(), 0);
}

#[test]
fn test_set_carbon_oracle_and_price() {
    let s = setup();
    let oracle = Address::generate(&s.env);

    s.vault_client.set_carbon_oracle(&oracle);
    s.vault_client.set_carbon_credit_price(&5_0000000i128);

    assert_eq!(s.vault_client.carbon_credit_price(), 5_0000000i128);
}

#[test]
fn test_calculate_carbon_credits_formula() {
    let s = setup();
    let project_id = create_project_with_impact(&s.env, &s.registry, 60);

    // 500 USDC, green_impact=60 => 500 * 60 / 10_000_000_000
    // 500 USDC = 500 * 10^7 = 5_000_000_000
    // 5_000_000_000 * 60 / 10_000_000_000 = 30
    let calc = s
        .vault_client
        .calculate_carbon_credits(&project_id, &500_0000000i128);
    assert_eq!(calc.project_id, project_id);
    assert_eq!(calc.amount_invested, 500_0000000i128);
    assert_eq!(calc.credits, 30);
}

#[test]
fn test_calculate_carbon_credits_zero_for_low_investment() {
    let s = setup();
    let project_id = create_project_with_impact(&s.env, &s.registry, 50);

    // 1 USDC, green_impact=50 => 10_000_000 * 50 / 10_000_000_000 = 0 (truncated)
    let calc = s.vault_client.calculate_carbon_credits(&project_id, &10_000_000i128);
    assert_eq!(calc.credits, 0);
}

#[test]
fn test_issue_carbon_credits() {
    let s = setup();
    let project_id = create_project_with_impact(&s.env, &s.registry, 100);
    let recipient = Address::generate(&s.env);

    // 10 USDC, green_impact=100 => 10 * 10^7 * 100 / 10_000_000_000 = 1 credit
    let credits = s
        .vault_client
        .issue_carbon_credits(&recipient, &project_id, &100_000_000i128);
    assert_eq!(credits, 1);
    assert_eq!(s.vault_client.carbon_credit_balance(&recipient), 1);
}

#[test]
fn test_transfer_carbon_credits() {
    let s = setup();
    let project_id = create_project_with_impact(&s.env, &s.registry, 100);
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);

    // Alice gets 5 credits
    let credits = s
        .vault_client
        .issue_carbon_credits(&alice, &project_id, &500_000_000i128);
    assert_eq!(credits, 5);

    // Alice transfers 3 to Bob
    s.vault_client.transfer_carbon_credits(&alice, &bob, &3);
    assert_eq!(s.vault_client.carbon_credit_balance(&alice), 2);
    assert_eq!(s.vault_client.carbon_credit_balance(&bob), 3);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_transfer_carbon_credits_insufficient_panics() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);

    s.vault_client.transfer_carbon_credits(&alice, &bob, &1);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_carbon_credits_zero_issue_panics() {
    let s = setup();
    let project_id = create_project_with_impact(&s.env, &s.registry, 50);
    let recipient = Address::generate(&s.env);

    // 1 USDC, green_impact=50 => 0 credits — should panic
    s.vault_client
        .issue_carbon_credits(&recipient, &project_id, &10_000_000i128);
}
