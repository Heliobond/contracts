#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::{Address as _, Events as _}, token::StellarAssetClient, token::TokenClient, Address, Env, String};

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

    // After investor1: total_supply=995, total_assets=1000 (USDC).
    // investor2 investable=995 → shares2 = 995 * 995 / 1000 = 990.025 → 990_0250000
    assert_eq!(shares2, 9_900_250_000i128);
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
    let registry = env.register(registry_contract::WASM, (&admin, &admin));
    let _contract_id = env.register(InvestmentVault, (&admin, &usdc, &registry));
    // If registration didn't panic, constructor succeeded with a valid registry
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
#[should_panic(expected = "insufficient deployable USDC")]
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
    );

    // Fund with all deployable USDC: liquid (1000) - insurance_reserve (5) = 995
    s.vault_client.fund_project(&project_id, &995_0000000i128);

    // Vault now has only 5 USDC liquid (= insurance_reserve), deployable = 0.
    // Any further funding must panic.
    s.vault_client.fund_project(&project_id, &1_0000000i128);
}

#[test]
#[should_panic(expected = "insufficient deployable USDC")]
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
    );

    // Two partial fundings that together stay within the deployable amount.
    s.vault_client.fund_project(&project_id, &300_0000000i128);
    s.vault_client.fund_project(&project_id, &200_0000000i128);

    // total_assets = 500 liquid + 500 invested + 0 expected_returns = 1000 USDC
    assert_eq!(s.vault_client.total_assets(), 1_000_0000000i128);
}

#[test]
#[should_panic(expected = "insufficient deployable USDC")]
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
    );

    // First call: fund 600 USDC — leaves 400 liquid (5 reserved) → 395 deployable.
    s.vault_client.fund_project(&project_id, &600_0000000i128);

    // Second call: attempt to deploy 400 USDC, which exceeds the 395 deployable.
    s.vault_client.fund_project(&project_id, &400_0000000i128);
}

// ── Issue #116: descriptive liquidity error ────────────────────────────────

#[test]
#[should_panic(expected = "insufficient liquid USDC")]
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
#[should_panic(expected = "transfer to vault address not allowed")]
fn test_transfer_to_vault_address_rejected() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    s.vault_client.deposit(&investor, &1_000_0000000i128);

    // Attempt to send HBS shares to the vault contract itself
    s.vault_client.transfer(&investor, &s.vault_address, &100_0000000i128);
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
#[should_panic(expected = "insufficient liquid USDC")]
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

    let test_amounts = [1i128, 3, 7, 1_0000000, 100_0000000, 999_9999999, 1_000_0000000];
    for &amount in test_amounts.iter() {
        let shares = s.vault_client.convert_to_shares(&amount);
        let assets = s.vault_client.convert_to_assets(&shares);
        assert!(
            assets <= amount,
            "rounding favored withdrawer: amount={} assets={}",
            amount, assets
        );
        assert!(
            amount - assets <= 1,
            "roundtrip loss > 1 stroop: amount={} assets={}",
            amount, assets
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
    );
    s.vault_client.fund_project(&project_id, &500_0000000i128);

    // Shares are worth deposit_amount USDC but only 500 USDC is liquid — should enqueue.
    let returned = s.vault_client.withdraw(&investor, &shares);

    assert_eq!(returned, 0); // queued, not immediate
    assert_eq!(s.vault_client.balance(&investor), 0); // shares burned at enqueue
    // Investor still has no USDC (claim not settled yet)
    assert_eq!(
        TokenClient::new(&s.env, &s.usdc_sac).balance(&investor),
        0
    );
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
    );
    s.vault_client.fund_project(&project_id, &500_0000000i128);

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

    let events = s.env.events().all();
    assert_eq!(events.len(), 1, "deposit should emit exactly one event");
    let (emitting_contract, _topics, _data) = &events[0];
    assert_eq!(*emitting_contract, s.vault_address);
}

#[test]
fn test_withdraw_emits_event() {
    let s = setup();
    let investor = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor, 1_000_0000000i128);
    let shares = s.vault_client.deposit(&investor, &1_000_0000000i128);
    let count_before = s.env.events().all().len();

    s.vault_client.withdraw(&investor, &shares);

    let events = s.env.events().all();
    assert_eq!(
        events.len(),
        count_before + 1,
        "withdraw should emit exactly one additional event"
    );
    let (emitting_contract, _topics, _data) = &events[events.len() - 1];
    assert_eq!(*emitting_contract, s.vault_address);
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
    );
    let count_before = s.env.events().all().len();

    s.vault_client.fund_project(&project_id, &100_0000000i128);

    let events = s.env.events().all();
    assert_eq!(
        events.len(),
        count_before + 1,
        "fund_project should emit exactly one additional event"
    );
    let (emitting_contract, _topics, _data) = &events[events.len() - 1];
    assert_eq!(*emitting_contract, s.vault_address);
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
    );
    s.vault_client.fund_project(&project_id, &500_0000000i128);

    let count_before = s.env.events().all().len();

    // Withdrawal exceeds liquid USDC — should enqueue and emit WithdrawQueued.
    let returned = s.vault_client.withdraw(&investor, &shares);
    assert_eq!(returned, 0);

    let events = s.env.events().all();
    assert_eq!(
        events.len(),
        count_before + 1,
        "queued withdrawal should emit exactly one additional event"
    );
    let (emitting_contract, _topics, _data) = &events[events.len() - 1];
    assert_eq!(*emitting_contract, s.vault_address);
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
    );
    s.vault_client.fund_project(&project_id, &500_0000000i128);
    s.vault_client.withdraw(&investor, &shares);

    // Restore liquidity so claim() can settle.
    let investor2 = Address::generate(&s.env);
    mint_usdc(&s.env, &s.usdc_sac, &investor2, 2_000_0000000i128);
    s.vault_client.deposit(&investor2, &2_000_0000000i128);

    let count_before = s.env.events().all().len();

    s.vault_client.claim();

    let events = s.env.events().all();
    assert!(
        events.len() > count_before,
        "claim() should emit at least one event when settling a queued redemption"
    );
    let (emitting_contract, _topics, _data) = &events[events.len() - 1];
    assert_eq!(*emitting_contract, s.vault_address);
}

#[test]
fn test_management_fee_set_emits_event() {
    let s = setup();
    let recipient = Address::generate(&s.env);

    s.vault_client.set_management_fee(&200u32, &recipient);

    let events = s.env.events().all();
    assert_eq!(events.len(), 1, "set_management_fee should emit exactly one event");
    let (emitting_contract, _topics, _data) = &events[0];
    assert_eq!(*emitting_contract, s.vault_address);
}

#[test]
fn test_enable_secondary_trading_emits_event() {
    let s = setup();

    s.vault_client.enable_secondary_trading();

    let events = s.env.events().all();
    assert_eq!(events.len(), 1, "enable_secondary_trading should emit exactly one event");
    let (emitting_contract, _topics, _data) = &events[0];
    assert_eq!(*emitting_contract, s.vault_address);
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
    let project_id = registry_client.create_project(
        &creator,
        &String::from_str(&s.env, "ipfs://Qm"),
        &0u64,
    );
    // Fund 800 USDC: liquid = 200, investments = 800, utilization = 800/(200+800) = 80%
    s.vault_client.fund_project(&project_id, &800_0000000i128);

    assert!(s.vault_client.get_utilization_bps() >= 7_000,
        "utilization should be at or above warning threshold");

    let count_before = s.env.events().all().len();

    // Withdraw a small amount within the utilization limit — warning event should fire.
    let small_shares = shares / 100; // 1% of total shares
    s.vault_client.withdraw(&investor, &small_shares);

    let events = s.env.events().all();
    assert!(
        events.len() > count_before,
        "withdrawal at high utilization should emit utilization warning event"
    );
}
