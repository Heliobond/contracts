#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String};

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

    let project_id = client.create_project(&creator, &String::from_str(&env, "ipfs://QmTest"));

    assert_eq!(project_id, 1);
    let project = client.get_project(&1);
    assert_eq!(project.owner, creator);
    assert_eq!(project.credit_quality, 0);
    assert_eq!(project.green_impact, 0);
    assert_eq!(client.total_projects(), 1);
}

#[test]
#[should_panic]
fn test_create_project_by_non_whitelisted_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"));
}

#[test]
fn test_sequential_project_ids() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    let id1 = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm1"));
    let id2 = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm2"));

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(client.total_projects(), 2);
}

#[test]
fn test_update_impact_score() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"));

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

    // Use mock_all_auths for setup steps
    env.mock_all_auths();
    let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
    let client = ProjectRegistryClient::new(&env, &contract_id);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"));

    // Provide auth for a non-admin address only
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
    client.create_project(&creator, &String::from_str(&env, "ipfs://Qm1"));
    client.create_project(&creator, &String::from_str(&env, "ipfs://Qm2"));

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

        // Create a project
        registry.set_whitelist(&project_creator, &true);
        let project_id = registry.create_project(
            &project_creator,
            &String::from_str(&env, "ipfs://QmHeliobond"),
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
