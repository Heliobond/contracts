#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Register a registry and return a client bound to it (#947).
fn register(env: &Env, admin: &Address, whitelister: &Address) -> ProjectRegistryClient<'static> {
    let contract_id = env.register(ProjectRegistry, (admin, whitelister));
    ProjectRegistryClient::new(env, &contract_id)
}

#[test]
fn test_constructor_stores_whitelister_argument() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let client = register(&env, &admin, &whitelister);

    assert_eq!(client.get_whitelister(), whitelister);
}

#[test]
fn test_constructor_records_state_version_and_zero_counters() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let client = register(&env, &admin, &whitelister);

    assert_eq!(client.state_version(), STATE_VERSION);
    assert_eq!(client.stored_state_version(), STATE_VERSION);
    assert_eq!(client.total_projects(), 0);
}

#[test]
fn test_constructor_accepts_the_same_address_for_both_roles() {
    let env = Env::default();
    env.mock_all_auths();
    let both = Address::generate(&env);
    let client = register(&env, &both, &both);

    assert_eq!(client.get_whitelister(), both);
}
