#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env, IntoVal, String,
};

fn setup() -> (Env, Address, Address, ProjectRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
    let client = ProjectRegistryClient::new(&env, &contract_id);
    (env, admin, whitelister, client)
}

#[test]
fn test_initialize_sets_admin_and_whitelister() {
    let (_env, _admin, _whitelister, client) = setup();
    assert_eq!(client.total_projects(), 0);
}

#[test]
fn test_create_project_by_whitelisted_address() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);

    client.set_whitelist(&creator, &true);

    let project_id = client.create_project(&creator, &String::from_str(&env, "ipfs://QmTest"), &0u64);

    assert_eq!(project_id, 1);
    let project = client.get_project(&1);
    assert_eq!(project.owner, creator);
    assert_eq!(project.credit_quality, 0);
    assert_eq!(project.green_impact, 0);
    assert_eq!(project.maturity_date, 0);
    assert_eq!(project.certification_status, CertificationStatus::None);
    assert_eq!(client.total_projects(), 1);
}

#[test]
#[should_panic]
fn test_create_project_by_non_whitelisted_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
}

#[test]
fn test_sequential_project_ids() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    let id1 = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm1"), &0u64);
    let id2 = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm2"), &0u64);

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(client.total_projects(), 2);
}

#[test]
fn test_update_impact_score() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.update_impact_score(&id, &80u32, &90u32);

    let project = client.get_project(&id);
    assert_eq!(project.credit_quality, 80);
    assert_eq!(project.green_impact, 90);
}

#[test]
fn test_update_impact_score_noop_identical_values() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    client.update_impact_score(&id, &80u32, &90u32);

    // Second call with identical scores should be a no-op (no panic, no storage write)
    client.update_impact_score(&id, &80u32, &90u32);

    let project = client.get_project(&id);
    assert_eq!(project.credit_quality, 80);
    assert_eq!(project.green_impact, 90);
}

#[test]
#[should_panic]
fn test_update_score_non_admin_panics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let creator = Address::generate(&env);

    env.mock_all_auths();
    let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
    let client = ProjectRegistryClient::new(&env, &contract_id);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    let non_admin = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_impact_score",
            args: soroban_sdk::vec![
                &env,
                id.into_val(&env),
                50u32.into_val(&env),
                50u32.into_val(&env),
            ],
            sub_invokes: &[],
        },
    }]);
    client.update_impact_score(&id, &50u32, &50u32);
}

#[test]
fn test_get_all_projects() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    client.create_project(&creator, &String::from_str(&env, "ipfs://Qm1"), &0u64);
    client.create_project(&creator, &String::from_str(&env, "ipfs://Qm2"), &0u64);

    let all = client.get_all_projects();
    assert_eq!(all.len(), 2);
    assert_eq!(all.get(0).unwrap().0, 1);
    assert_eq!(all.get(1).unwrap().0, 2);
}

#[test]
#[should_panic(expected = "project 999 not found")]
fn test_update_impact_score_nonexistent_project_panics() {
    let (_env, _admin, _whitelister, client) = setup();
    client.update_impact_score(&999u32, &50u32, &50u32);
}

#[test]
fn test_certify_project() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.certify_project(&whitelister, &id, &CertificationStatus::Certified);

    let project = client.get_project(&id);
    assert_eq!(project.certification_status, CertificationStatus::Certified);
}

#[test]
fn test_maturity_date_is_mature() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    // Set maturity 1000 seconds in future relative to current ledger time
    let now = env.ledger().timestamp();
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &(now + 1000),
    );

    assert!(!client.is_mature(&id));

    // Advance ledger past maturity
    env.ledger().set_timestamp(now + 1001);
    assert!(client.is_mature(&id));
}

// ── URI length edge cases (#119) ──────────────────────────────────────────────

#[test]
fn test_uri_exactly_min_length_accepted() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    // 8 chars exactly equals MIN_URI_LEN
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Q"), &0u64);
    assert_eq!(id, 1);
}

#[test]
#[should_panic(expected = "uri too short")]
fn test_uri_below_min_length_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    // 7 chars — one below MIN_URI_LEN
    client.create_project(&creator, &String::from_str(&env, "ipfs://"), &0u64);
}

#[test]
fn test_uri_exactly_max_length_accepted() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    // 512-byte stack buffer: prefix + 'A' padding — no alloc needed
    let mut buf = [b'A'; 512];
    buf[..9].copy_from_slice(b"ipfs://Qm");
    let uri = String::from_str(&env, core::str::from_utf8(&buf).unwrap());
    let id = client.create_project(&creator, &uri, &0u64);
    assert_eq!(id, 1);
}

#[test]
#[should_panic(expected = "uri too long")]
fn test_uri_above_max_length_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    // 513-byte stack buffer — one above MAX_URI_LEN
    let mut buf = [b'A'; 513];
    buf[..9].copy_from_slice(b"ipfs://Qm");
    let uri = String::from_str(&env, core::str::from_utf8(&buf).unwrap());
    client.create_project(&creator, &uri, &0u64);
}

// ── Collateral management (#128) ──────────────────────────────────────────────

#[test]
fn test_deposit_and_get_collateral() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    let token_admin = Address::generate(&env);
    let token_sac = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);

    client.deposit_collateral(&project_id, &creator, &token_sac, &500i128);

    assert_eq!(client.get_collateral(&project_id, &token_sac), 500i128);
}

#[test]
#[should_panic(expected = "only the project owner may deposit collateral")]
fn test_non_owner_cannot_deposit_collateral() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    let token_admin = Address::generate(&env);
    let token_sac = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let stranger = Address::generate(&env);
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&stranger, &1_000i128);

    client.deposit_collateral(&project_id, &stranger, &token_sac, &500i128);
}

#[test]
fn test_liquidate_collateral_by_admin() {
    let (env, admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    let token_sac = env.register_stellar_asset_contract_v2(admin.clone()).address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);
    client.deposit_collateral(&project_id, &creator, &token_sac, &800i128);

    let recipient = Address::generate(&env);
    client.liquidate_collateral(&project_id, &token_sac, &recipient);

    assert_eq!(client.get_collateral(&project_id, &token_sac), 0i128);
}

#[test]
fn test_release_collateral_after_maturity() {
    let (env, admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let now = env.ledger().timestamp();
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &(now + 1000),
    );

    let token_sac = env.register_stellar_asset_contract_v2(admin.clone()).address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);
    client.deposit_collateral(&project_id, &creator, &token_sac, &600i128);

    env.ledger().set_timestamp(now + 1001);
    client.release_collateral(&project_id, &creator, &token_sac);

    assert_eq!(client.get_collateral(&project_id, &token_sac), 0i128);
}

// ── Interest rate (#129) ───────────────────────────────────────────────────────

#[test]
fn test_interest_rate_zero_scores_is_base_rate() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    // credit_quality = 0, green_impact = 0 → rate = 1000 bps (10 %)
    assert_eq!(client.get_interest_rate(&id), 1_000u32);
}

#[test]
fn test_interest_rate_perfect_scores_is_minimum() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    client.update_impact_score(&id, &100u32, &100u32);
    // avg = 100, discount = 100 * 500 / 100 = 500 → rate = 1000 - 500 = 500 bps (5 %)
    assert_eq!(client.get_interest_rate(&id), 500u32);
}

#[test]
fn test_interest_rate_mid_scores() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    client.update_impact_score(&id, &80u32, &60u32);
    // avg = (80 + 60) / 2 = 70, discount = 70 * 500 / 100 = 350 → rate = 1000 - 350 = 650 bps
    assert_eq!(client.get_interest_rate(&id), 650u32);
}

// Integration: full Heliobond flow across both contracts
mod integration {
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, String};

    mod vault_contract {
        soroban_sdk::contractimport!(
            file = "../target/wasm32v1-none/release/investment_vault.wasm"
        );
    }

    use super::{ProjectRegistry, ProjectRegistryClient};

    #[test]
    fn test_full_heliobond_flow() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let project_creator = Address::generate(&env);
        let investor = Address::generate(&env);

        // Deploy mock USDC
        let usdc_sac = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &usdc_sac).mint(&investor, &2_000_0000000i128);

        // Deploy registry with constructor
        let registry_id = env.register(ProjectRegistry, (&admin, &whitelister));
        let registry = ProjectRegistryClient::new(&env, &registry_id);

        // Deploy vault (using the imported WASM) with constructor
        let vault_id = env.register(vault_contract::WASM, (&admin, &usdc_sac, &registry_id));
        let vault = vault_contract::Client::new(&env, &vault_id);

        // Create a project (no maturity date)
        registry.set_whitelist(&project_creator, &true);
        let project_id = registry.create_project(
            &project_creator,
            &String::from_str(&env, "ipfs://QmHeliobond"),
            &0u64,
        );
        assert_eq!(project_id, 1);

        // Investor deposits 2000 USDC → receives 2000 shares (1:1 on first deposit)
        let shares = vault.deposit(&investor, &2_000_0000000i128);
        assert_eq!(shares, 2_000_0000000i128);
        assert_eq!(vault.balance(&investor), 2_000_0000000i128);

        // Admin updates impact scores (oracle step)
        registry.update_impact_score(&project_id, &80u32, &60u32);

        // Admin funds project with 500 USDC from vault
        vault.fund_project(&project_id, &500_0000000i128);

        // expected_returns = 500 * (80 + 60) / 200 = 500 * 0.7 = 350 USDC
        let expected_returns = vault.get_expected_returns();
        assert_eq!(expected_returns, 350_0000000i128);

        // total_assets = 1500 liquid + 500 investments + 350 expected_returns = 2350
        let total = vault.total_assets();
        assert_eq!(total, 2_350_0000000i128);

        // Investor withdraws half their shares (1000 out of 2000)
        let half_shares = shares / 2;
        let returned = vault.withdraw(&investor, &half_shares);
        assert_eq!(returned, 1_175_0000000i128);

        // Remaining shares and balance
        assert_eq!(vault.balance(&investor), 1_000_0000000i128);
    }
}
