#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},

};
extern crate std;

mod vault_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/investment_vault.wasm");
}

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
    assert_eq!(client.state_version(), 1);
    assert_eq!(client.stored_state_version(), 1);
    assert_eq!(client.total_projects(), 0);
}

#[test]
fn test_create_project_by_whitelisted_address() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);

    client.set_whitelist(&creator, &true);

    let project_id =
        client.create_project(&creator, &String::from_str(&env, "ipfs://QmTest"), &0u64);

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
fn test_multisig_update_impact_score_approved() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmMultiSig"),
        &0u64,
    );

    client.set_multisig_admin(
        &soroban_sdk::vec![&env, signer1.clone(), signer2.clone(), signer3],
        &2u32,
    );
    client.update_impact_score_approved(
        &id,
        &80u32,
        &90u32,
        &soroban_sdk::vec![&env, signer1, signer2],
    );

    let project = client.get_project(&id);
    assert_eq!(project.credit_quality, 80);
    assert_eq!(project.green_impact, 90);
}

#[test]
#[should_panic]
fn test_multisig_update_impact_score_rejects_insufficient_approvals() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmMultiSig"),
        &0u64,
    );

    client.set_multisig_admin(&soroban_sdk::vec![&env, signer1.clone(), signer2], &2u32);
    client.update_impact_score_approved(&id, &80u32, &90u32, &soroban_sdk::vec![&env, signer1]);
}

#[test]
fn bench_registry_create_and_score_project() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://QmBench"), &0u64);
    client.update_impact_score(&id, &75u32, &85u32);

    let instructions = env.cost_estimate().resources().instructions;
    std::println!(
        "bench_registry_create_and_score_project: {} instructions",
        instructions
    );
    assert!(instructions <= 50_000_000);
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
#[should_panic]
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

// ── #6: credit-quality score tests ───────────────────────────────────────────

#[test]
fn test_update_credit_quality_score_success() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.update_credit_quality_score(&id, &75u32);

    let project = client.get_project(&id);
    assert_eq!(project.credit_quality, 75);
    // green_impact unchanged
    assert_eq!(project.green_impact, 0);
}

#[test]
#[should_panic]
fn test_update_credit_quality_score_out_of_range_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    client.update_credit_quality_score(&id, &101u32);
}

#[test]
fn test_update_credit_quality_score_boundary_values() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.update_credit_quality_score(&id, &0u32);
    assert_eq!(client.get_project(&id).credit_quality, 0);

    client.update_credit_quality_score(&id, &100u32);
    assert_eq!(client.get_project(&id).credit_quality, 100);
}

#[test]
fn test_update_credit_quality_independent_of_green_impact() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.update_impact_score(&id, &60u32, &80u32);
    client.update_credit_quality_score(&id, &45u32);

    let project = client.get_project(&id);
    assert_eq!(project.credit_quality, 45);
    assert_eq!(project.green_impact, 80); // unchanged
}

#[test]
fn test_credit_quality_score_changes_rate_correctly() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    // Set baseline: credit_quality=60, green_impact=40 → rate = avg(60,40)=50,
    // discount=50*500/100=250, rate=1000-250=750
    client.update_impact_score(&id, &60u32, &40u32);
    assert_eq!(client.get_interest_rate(&id), 750u32);

    // Update only credit_quality: 60 → 85 → new avg(85,40)=62,
    // discount=62*500/100=310, rate=1000-310=690
    client.update_credit_quality_score(&id, &85u32);
    assert_eq!(client.get_interest_rate(&id), 690u32);
    // green_impact unchanged
    assert_eq!(client.get_project(&id).green_impact, 40u32);
}

#[test]
fn test_update_credit_quality_score_noop_identical_values() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    client.update_credit_quality_score(&id, &75u32);
    let project_before = client.get_project(&id);

    // Second call with identical score should be a no-op
    client.update_credit_quality_score(&id, &75u32);

    let project_after = client.get_project(&id);
    assert_eq!(project_before.credit_quality, project_after.credit_quality);
    assert_eq!(project_before.green_impact, project_after.green_impact);
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
#[should_panic]
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
#[should_panic]
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
    let token_sac = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);

    client.deposit_collateral(&project_id, &creator, &token_sac, &500i128);

    assert_eq!(client.get_collateral(&project_id, &token_sac), 500i128);
}

#[test]
#[should_panic]
fn test_non_owner_cannot_deposit_collateral() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    let token_admin = Address::generate(&env);
    let token_sac = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
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

    let token_sac = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
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

    let token_sac = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
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
    // credit_quality = 0, green_impact = 0 (default) → rate = 1000 bps (10%)
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

// ── Issue #55: event emission verification tests ──────────────────────────────

#[test]
fn test_create_project_emits_event() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    client.create_project(&creator, &String::from_str(&env, "ipfs://QmTest"), &0u64);

    // In Soroban tests env.events().all() returns events from the most recent invocation only.
    let events = env.events().all().filter_by_contract(&client.address);
    assert_eq!(
        events.events().len(),
        1,
        "create_project should emit exactly one event"
    );
}

#[test]
fn test_set_whitelist_emits_event() {
    let (env, _admin, _whitelister, client) = setup();
    let account = Address::generate(&env);

    client.set_whitelist(&account, &true);

    let events = env.events().all().filter_by_contract(&client.address);
    assert_eq!(
        events.events().len(),
        1,
        "set_whitelist should emit exactly one event"
    );
}

#[test]
fn test_update_impact_score_emits_event() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.update_impact_score(&id, &80u32, &60u32);

    // update_impact_score emits ProjectUpdated + RateUpdated = 2 events per invocation.
    let events = env.events().all().filter_by_contract(&client.address);
    assert!(
        events.events().len() >= 2,
        "update_impact_score should emit at least two events"
    );
}

#[test]
fn test_score_changed_event_contains_old_and_new_values() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.update_impact_score(&id, &80u32, &60u32);


}

#[test]
fn test_certify_project_emits_event() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);

    client.certify_project(&whitelister, &id, &CertificationStatus::Certified);

    let events = env.events().all().filter_by_contract(&client.address);
    assert_eq!(
        events.events().len(),
        1,
        "certify_project should emit exactly one event"
    );
}

#[test]
fn test_set_creator_reputation_emits_event() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    client.set_creator_reputation(&whitelister, &creator, &75u32);

    let events = env.events().all().filter_by_contract(&client.address);
    assert_eq!(
        events.events().len(),
        1,
        "set_creator_reputation should emit exactly one event"
    );
}

// ── Issue #46: creator reputation tests ──────────────────────────────────────

#[test]
fn test_reputation_defaults_to_zero() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    assert_eq!(client.get_creator_reputation(&creator), 0u32);
}

#[test]
fn test_set_and_get_reputation() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_creator_reputation(&whitelister, &creator, &80u32);
    assert_eq!(client.get_creator_reputation(&creator), 80u32);
}

#[test]
fn test_reputation_can_be_updated() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_creator_reputation(&whitelister, &creator, &50u32);
    client.set_creator_reputation(&whitelister, &creator, &90u32);
    assert_eq!(client.get_creator_reputation(&creator), 90u32);
}

#[test]
#[should_panic]
fn test_reputation_above_100_panics() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_creator_reputation(&whitelister, &creator, &101u32);
}

#[test]
#[should_panic]
fn test_unauthorized_caller_cannot_set_reputation() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.set_creator_reputation(&stranger, &creator, &50u32);
}

#[test]
fn test_owner_can_set_reputation() {
    let (env, admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_creator_reputation(&admin, &creator, &60u32);
    assert_eq!(client.get_creator_reputation(&creator), 60u32);
}

#[test]
fn test_funding_limit_bps_scales_with_reputation() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);

    // 0 rep → 0 bps limit
    assert_eq!(client.get_creator_funding_limit_bps(&creator), 0u32);

    client.set_creator_reputation(&whitelister, &creator, &100u32);
    // 100 rep → 5000 bps (50% of vault assets)
    assert_eq!(client.get_creator_funding_limit_bps(&creator), 5_000u32);

    client.set_creator_reputation(&whitelister, &creator, &50u32);
    // 50 rep → 2500 bps (25% of vault assets)
    assert_eq!(client.get_creator_funding_limit_bps(&creator), 2_500u32);
}

// ── Issue #76: whitelister dependency injection ───────────────────────────────

#[test]
fn test_get_whitelister_returns_initial_whitelister() {
    let (_env, _admin, whitelister, client) = setup();
    assert_eq!(client.get_whitelister(), whitelister);
}

#[test]
fn test_registry_constructor_deployment_cost_estimate_and_initial_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let whitelister = Address::generate(&env);
    let usdc_admin = Address::generate(&env);
    let project_creator = Address::generate(&env);

    let usdc_sac = env
        .register_stellar_asset_contract_v2(usdc_admin.clone())
        .address();

    let registry_id = env.register(ProjectRegistry, (&admin, &whitelister));
    let registry = ProjectRegistryClient::new(&env, &registry_id);

    assert_eq!(registry.total_projects(), 0);
    assert_eq!(registry.get_whitelister(), whitelister);

    let resources = env.cost_estimate().resources();
    assert!(resources.instructions > 0);
    let fee = env.cost_estimate().fee();
    assert!(fee.total > 0);
    std::println!(
        "gas_budget project_registry.constructor instructions={} fee={}",
        resources.instructions,
        fee.total
    );

    let vault_id = env.register(vault_contract::WASM, (&admin, &usdc_sac, &registry_id));
    let vault = vault_contract::Client::new(&env, &vault_id);

    assert_eq!(vault.accepted_asset(), usdc_sac);
    assert_eq!(vault.get_registry(), registry_id);
    assert_eq!(vault.total_assets(), 0);
    assert_eq!(vault.total_supply(), 0);
    assert!(!vault.is_trading_enabled());

    registry.set_whitelist(&project_creator, &true);
    let project_id = registry.create_project(
        &project_creator,
        &String::from_str(&env, "ipfs://QmInitTest"),
        &0u64,
    );
    assert_eq!(project_id, 1);
}

#[test]
fn test_set_whitelister_updates_whitelister() {
    let (env, _admin, _whitelister, client) = setup();
    let new_whitelister = Address::generate(&env);
    client.set_whitelister(&new_whitelister);
    assert_eq!(client.get_whitelister(), new_whitelister);
}

#[test]
fn test_new_whitelister_can_set_whitelist() {
    let (env, _admin, _old_whitelister, client) = setup();
    let new_whitelister = Address::generate(&env);
    client.set_whitelister(&new_whitelister);

    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64);
    assert_eq!(id, 1);
}

#[test]
#[should_panic]
fn test_set_whitelister_is_admin_only() {
    let (env, _admin, _whitelister, client) = setup();
    let stranger = Address::generate(&env);
    let new_wl = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_whitelister",
            args: soroban_sdk::vec![&env, new_wl.clone().into_val(&env)],
            sub_invokes: &[],
        },
    }]);
    client.set_whitelister(&new_wl);
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
        StellarAssetClient::new(&env, &usdc_sac).mint(&investor, &20_000_000_000i128);

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

        // Investor deposits 2000 USDC. First deposit is 1:1 on the investable amount
        // (full deposit minus 0.5% insurance premium).
        let deposit_amount = 20_000_000_000i128;
        let investable = deposit_amount - deposit_amount * 50 / 10_000; // 19_900_000_000
        let shares = vault.deposit(&investor, &deposit_amount);
        assert_eq!(shares, investable);
        assert_eq!(vault.balance(&investor), investable);

        // Admin updates impact scores (oracle step)
        registry.update_impact_score(&project_id, &80u32, &60u32);

        // Admin funds project with 500 USDC from vault
        vault.fund_project(&project_id, &5_000_000_000i128);

        // expected_returns = 500 * (80 + 60) / 200 = 500 * 0.7 = 350 USDC
        let expected_returns = vault.get_expected_returns();
        assert_eq!(expected_returns, 3_500_000_000i128);

        // total_assets = 1500 liquid + 500 investments + 350 expected_returns = 2350
        let total = vault.total_assets();
        assert_eq!(total, 23_500_000_000i128);

        // Investor withdraws half their shares (995 out of 1990)
        // total_assets = 2350, total_supply = 1990
        // returned = 995 * 2350 / 1990 = 1175 USDC (insurance pool is part of total assets)
        let half_shares = shares / 2;
        let returned = vault.withdraw(&investor, &half_shares);
        assert_eq!(returned, 11_750_000_000i128);

        // Remaining shares = half of investable
        assert_eq!(vault.balance(&investor), investable / 2);
        // Remaining shares and balance (1990 / 2 = 995)
        assert_eq!(vault.balance(&investor), 9_950_000_000i128);
    }
}
