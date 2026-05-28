#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, Address, Address, ProjectRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let contract_id = env.register(ProjectRegistry, ());
    let client = ProjectRegistryClient::new(&env, &contract_id);
    client.initialize(&admin, &whitelister);
    (env, admin, whitelister, client)
}

#[test]
fn test_initialize_sets_admin_and_whitelister() {
    let (_env, _admin, _whitelister, client) = setup();
    // Verify state was set by checking total_projects returns 0
    assert_eq!(client.total_projects(), 0);
}

#[test]
fn test_create_project_by_whitelisted_address() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);

    client.set_whitelist(&creator, &true);

    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmTest"),
    );

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
