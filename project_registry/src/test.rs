#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, IntoVal, String,
};

use investment_vault::{InvestmentVault, InvestmentVaultClient};

/// Deterministic placeholder metadata hash for tests that don't exercise
/// verify_metadata_hash directly (#44).
fn test_metadata_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
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

    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmTest"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
}

#[test]
fn test_sequential_project_ids() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    let id1 = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id2 = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm2"),
        &0u64,
        &test_metadata_hash(&env),
    );

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(client.total_projects(), 2);
}

#[test]
fn test_update_impact_score() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
        &test_metadata_hash(&env),
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
        &test_metadata_hash(&env),
    );

    client.set_multisig_admin(&soroban_sdk::vec![&env, signer1.clone(), signer2], &2u32);
    client.update_impact_score_approved(&id, &80u32, &90u32, &soroban_sdk::vec![&env, signer1]);
}

// ── #208: replayed-approval rejection ────────────────────────────────────────

#[test]
#[should_panic]
fn test_multisig_update_impact_score_rejects_replayed_approval() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmReplay"),
        &0u64,
        &test_metadata_hash(&env),
    );

    // threshold=2, two distinct signers configured
    client.set_multisig_admin(
        &soroban_sdk::vec![&env, signer1.clone(), signer2.clone()],
        &2u32,
    );

    // Pass signer1 twice — same approval counted toward threshold.
    // require_admin_approval must reject this as DuplicateApproval.
    client.update_impact_score_approved(
        &id,
        &80u32,
        &90u32,
        &soroban_sdk::vec![&env, signer1.clone(), signer1],
    );
}

#[test]
fn bench_registry_create_and_score_project() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmBench"),
        &0u64,
        &test_metadata_hash(&env),
    );
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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm2"),
        &0u64,
        &test_metadata_hash(&env),
    );

    let all = client.get_all_projects();
    assert_eq!(all.len(), 2);
    assert_eq!(all.get(0).unwrap().0, 1);
    assert_eq!(all.get(1).unwrap().0, 2);
}

#[test]
fn test_get_projects_page_returns_stable_ordering_across_pages() {
    // #269: fetching page 1 then page 2 must never skip or duplicate a
    // project when no writes occur in between, and must match the order
    // get_all_projects returns them in.
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    for i in 1..=5u32 {
        client.create_project(
            &creator,
            &String::from_str(&env, &std::format!("ipfs://Qm{i}")),
            &0u64,
            &test_metadata_hash(&env),
        );
    }

    let page1 = client.get_projects_page(&0u32, &2u32);
    let page2 = client.get_projects_page(&2u32, &2u32);
    let page3 = client.get_projects_page(&4u32, &2u32);

    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page3.len(), 1);

    let mut paged_ids: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    for page in [&page1, &page2, &page3] {
        for entry in page.iter() {
            paged_ids.push_back(entry.0);
        }
    }

    let all = client.get_all_projects();
    let mut all_ids: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    for entry in all.iter() {
        all_ids.push_back(entry.0);
    }

    assert_eq!(
        paged_ids, all_ids,
        "paginated ids must match get_all_projects order exactly, with no skips or duplicates"
    );
}

#[test]
fn test_get_projects_page_limit_larger_than_total_projects() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm2"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm3"),
        &0u64,
        &test_metadata_hash(&env),
    );

    let page = client.get_projects_page(&0u32, &10u32);

    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap().0, 1);
    assert_eq!(page.get(1).unwrap().0, 2);
    assert_eq!(page.get(2).unwrap().0, 3);
}

#[test]
fn test_get_projects_page_zero_limit_returns_empty() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    let page = client.get_projects_page(&0u32, &0u32);
    assert_eq!(page.len(), 0);
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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.certify_project(&whitelister, &id, &CertificationStatus::Certified);

    let project = client.get_project(&id);
    assert_eq!(project.certification_status, CertificationStatus::Certified);
}

#[test]
#[should_panic]
fn test_certify_already_certified_project_panics() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.certify_project(&whitelister, &id, &CertificationStatus::Certified);
    client.certify_project(&whitelister, &id, &CertificationStatus::Certified);
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
        &test_metadata_hash(&env),
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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_credit_quality_score(&id, &101u32);
}

#[test]
fn test_update_credit_quality_score_boundary_values() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Q"),
        &0u64,
        &test_metadata_hash(&env),
    );
    assert_eq!(id, 1);
}

#[test]
#[should_panic]
fn test_uri_below_min_length_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    // 7 chars — one below MIN_URI_LEN
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://"),
        &0u64,
        &test_metadata_hash(&env),
    );
}

#[test]
#[should_panic]
fn test_create_project_empty_uri_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    client.create_project(
        &creator,
        &String::from_str(&env, ""),
        &0u64,
        &test_metadata_hash(&env),
    );
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
    let id = client.create_project(&creator, &uri, &0u64, &test_metadata_hash(&env));
    let project = client.get_project(&id);

    assert_eq!(id, 1);
    assert_eq!(project.uri.len(), 512);
    assert_eq!(project.uri, uri);
    assert!(env.cost_estimate().resources().instructions > 0);
}

#[test]
fn test_uri_with_special_characters_and_unicode_accepted() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let uri = String::from_str(&env, "ipfs://QmSolar-%E2%98%80-東京?panel=42&region=na");

    let id = client.create_project(&creator, &uri, &0u64, &test_metadata_hash(&env));
    let project = client.get_project(&id);

    assert_eq!(id, 1);
    assert_eq!(project.uri, uri);
    assert_eq!(client.total_projects(), 1);
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
    client.create_project(&creator, &uri, &0u64, &test_metadata_hash(&env));
}

// ── Collateral management (#128) ──────────────────────────────────────────────

#[test]
fn test_deposit_and_get_collateral() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
fn test_deposit_zero_collateral_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    let token_admin = Address::generate(&env);
    let token_sac = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);

    client.deposit_collateral(&project_id, &creator, &token_sac, &0i128);
}

#[test]
#[should_panic]
fn test_non_owner_cannot_deposit_collateral() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
#[should_panic]
fn test_liquidate_collateral_before_maturity_panics() {
    let (env, admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    let now = env.ledger().timestamp();
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &(now + 1000),
        &test_metadata_hash(&env),
    );

    let token_sac = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);
    client.deposit_collateral(&project_id, &creator, &token_sac, &800i128);

    let recipient = Address::generate(&env);
    client.liquidate_collateral(&project_id, &token_sac, &recipient);
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
        &test_metadata_hash(&env),
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

// ── #209: release_collateral pre-maturity rejection ──────────────────────────

#[test]
#[should_panic]
fn test_release_collateral_before_maturity_panics() {
    let (env, admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let now = env.ledger().timestamp();
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmPreMature"),
        &(now + 1000),
        &test_metadata_hash(&env),
    );

    let token_sac = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    soroban_sdk::token::StellarAssetClient::new(&env, &token_sac).mint(&creator, &1_000i128);
    client.deposit_collateral(&project_id, &creator, &token_sac, &500i128);

    // Ledger time is still before maturity_date — must panic with ProjectNotMature.
    client.release_collateral(&project_id, &creator, &token_sac);
}

// ── Interest rate (#129) ───────────────────────────────────────────────────────

#[test]
fn test_interest_rate_zero_scores_is_base_rate() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    // credit_quality = 0, green_impact = 0 (default) → rate = 1000 bps (10%)
    assert_eq!(client.get_interest_rate(&id), 1_000u32);
}

#[test]
fn test_interest_rate_perfect_scores_is_minimum() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &100u32, &100u32);
    // avg = 100, discount = 100 * 500 / 100 = 500 → rate = 1000 - 500 = 500 bps (5 %)
    assert_eq!(client.get_interest_rate(&id), 500u32);
}

#[test]
fn test_interest_rate_mid_scores() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &80u32, &60u32);
    // avg = (80 + 60) / 2 = 70, discount = 70 * 500 / 100 = 350 → rate = 1000 - 350 = 650 bps
    assert_eq!(client.get_interest_rate(&id), 650u32);
}

#[test]
fn test_interest_rate_boundary_combined_score_50() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &50u32, &50u32);
    // avg = (50 + 50) / 2 = 50, discount = 50 * 500 / 100 = 250 → rate = 1000 - 250 = 750 bps
    assert_eq!(client.get_interest_rate(&id), 750u32);
}

#[test]
fn test_interest_rate_boundary_combined_score_1() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &1u32, &1u32);
    // avg = (1 + 1) / 2 = 1, discount = 1 * 500 / 100 = 5 → rate = 1000 - 5 = 995 bps
    assert_eq!(client.get_interest_rate(&id), 995u32);
}

#[test]
fn test_interest_rate_boundary_combined_score_99() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &99u32, &99u32);
    // avg = (99 + 99) / 2 = 99, discount = 99 * 500 / 100 = 495 → rate = 1000 - 495 = 505 bps
    assert_eq!(client.get_interest_rate(&id), 505u32);
}

#[test]
fn test_interest_rate_boundary_asymmetric_0_100() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &0u32, &100u32);
    // avg = (0 + 100) / 2 = 50, discount = 50 * 500 / 100 = 250 → rate = 1000 - 250 = 750 bps
    assert_eq!(client.get_interest_rate(&id), 750u32);
}

#[test]
fn test_interest_rate_boundary_asymmetric_100_0() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.update_impact_score(&id, &100u32, &0u32);
    // avg = (100 + 0) / 2 = 50, discount = 50 * 500 / 100 = 250 → rate = 1000 - 250 = 750 bps
    assert_eq!(client.get_interest_rate(&id), 750u32);
}

// ── Issue #55: event emission verification tests ──────────────────────────────

#[test]
fn test_create_project_emits_event() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmTest"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &80u32, &60u32);
}

#[test]
fn test_score_changed_event_credit_quality_path() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    // Set initial scores
    client.update_impact_score(&id, &50u32, &70u32);

    // Update only credit quality via the credit-quality path
    client.update_credit_quality_score(&id, &85u32);

    // Verify ScoreChanged event is emitted
    let events = env.events().all().filter_by_contract(&client.address);
    assert!(
        events.events().len() >= 1,
        "update_credit_quality_score should emit at least one event"
    );
}

#[test]
fn test_certify_project_emits_event() {
    let (env, _admin, whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

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

    let vault_id = env.register(InvestmentVault, (&admin, &usdc_sac, &registry_id));
    let vault = InvestmentVaultClient::new(&env, &vault_id);

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
        &test_metadata_hash(&env),
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
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
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

#[test]
fn test_update_impact_score_boundary_values() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    // Score of exactly 0
    client.update_impact_score(&id, &0u32, &0u32);
    let mut project = client.get_project(&id);
    assert_eq!(project.credit_quality, 0);
    assert_eq!(project.green_impact, 0);

    // Score of exactly 100
    client.update_impact_score(&id, &100u32, &100u32);
    project = client.get_project(&id);
    assert_eq!(project.credit_quality, 100);
    assert_eq!(project.green_impact, 100);
}

#[test]
#[should_panic]
fn test_update_impact_score_exceeds_100_panics_credit_quality() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &101u32, &50u32);
}

#[test]
#[should_panic]
fn test_update_impact_score_exceeds_100_panics_green_impact() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &50u32, &101u32);
}

#[test]
#[should_panic]
fn test_update_impact_score_max_value_panics() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &u32::MAX, &50u32);
}

#[test]
fn test_multiple_creators_sequential_ids() {
    let (env, _admin, _whitelister, client) = setup();
    let creator1 = Address::generate(&env);
    let creator2 = Address::generate(&env);
    let creator3 = Address::generate(&env);

    client.set_whitelist(&creator1, &true);
    client.set_whitelist(&creator2, &true);
    client.set_whitelist(&creator3, &true);

    let id1 = client.create_project(
        &creator1,
        &String::from_str(&env, "ipfs://Qm1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id2 = client.create_project(
        &creator2,
        &String::from_str(&env, "ipfs://Qm2"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id3 = client.create_project(
        &creator1,
        &String::from_str(&env, "ipfs://Qm3"),
        &0u64,
        &test_metadata_hash(&env),
    );

    // Revoke whitelist for creator2, shouldn't affect existing projects
    client.set_whitelist(&creator2, &false);
    let id4 = client.create_project(
        &creator3,
        &String::from_str(&env, "ipfs://Qm4"),
        &0u64,
        &test_metadata_hash(&env),
    );

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
    assert_eq!(id4, 4);

    let p1 = client.get_project(&id1);
    assert_eq!(p1.owner, creator1);

    let p2 = client.get_project(&id2);
    assert_eq!(p2.owner, creator2);

    let p3 = client.get_project(&id3);
    assert_eq!(p3.owner, creator1);

    let p4 = client.get_project(&id4);
    assert_eq!(p4.owner, creator3);

    assert_eq!(client.total_projects(), 4);
}

// ── #210: interleaved creator sequential ID allocation ──────────────────────

#[test]
fn test_interleaved_creators_sequential_ids() {
    // Verify that project IDs remain globally sequential when multiple
    // creators interleave their create_project() calls, not just when
    // one creator creates all projects or when creators act in blocks.
    let (env, _admin, _whitelister, client) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.set_whitelist(&alice, &true);
    client.set_whitelist(&bob, &true);
    client.set_whitelist(&carol, &true);

    // Interleave: alice, bob, carol, alice, bob, carol
    let id1 = client.create_project(
        &alice,
        &String::from_str(&env, "ipfs://QmA1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id2 = client.create_project(
        &bob,
        &String::from_str(&env, "ipfs://QmB1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id3 = client.create_project(
        &carol,
        &String::from_str(&env, "ipfs://QmC1"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id4 = client.create_project(
        &alice,
        &String::from_str(&env, "ipfs://QmA2"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id5 = client.create_project(
        &bob,
        &String::from_str(&env, "ipfs://QmB2"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let id6 = client.create_project(
        &carol,
        &String::from_str(&env, "ipfs://QmC2"),
        &0u64,
        &test_metadata_hash(&env),
    );

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
    assert_eq!(id4, 4);
    assert_eq!(id5, 5);
    assert_eq!(id6, 6);
    assert_eq!(client.total_projects(), 6);

    // Verify ownership is correct for each interleaved project
    assert_eq!(client.get_project(&id1).owner, alice);
    assert_eq!(client.get_project(&id2).owner, bob);
    assert_eq!(client.get_project(&id3).owner, carol);
    assert_eq!(client.get_project(&id4).owner, alice);
    assert_eq!(client.get_project(&id5).owner, bob);
    assert_eq!(client.get_project(&id6).owner, carol);
}

// Integration: full Heliobond flow across both contracts
mod integration {
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token::StellarAssetClient,
        Address, Env, String,
    };

    use investment_vault::{InvestmentVault, InvestmentVaultClient};

    use super::{test_metadata_hash, ProjectRegistry, ProjectRegistryClient};

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

        // Deploy vault with constructor
        let vault_id = env.register(InvestmentVault, (&admin, &usdc_sac, &registry_id));
        let vault = InvestmentVaultClient::new(&env, &vault_id);

        // Create a project (no maturity date)
        registry.set_whitelist(&project_creator, &true);
        let project_id = registry.create_project(
            &project_creator,
            &String::from_str(&env, "ipfs://QmHeliobond"),
            &0u64,
            &test_metadata_hash(&env),
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
        env.ledger().with_mut(|li| {
            li.sequence_number += 1;
        });
        let returned = vault.withdraw(&investor, &half_shares, &0);
        assert_eq!(returned, 11_750_000_000i128);

        // Remaining shares = half of investable
        assert_eq!(vault.balance(&investor), investable / 2);
        // Remaining shares and balance (1990 / 2 = 995)
        assert_eq!(vault.balance(&investor), 9_950_000_000i128);

        // ── Extend the flow: certify -> deposit collateral -> mature -> release (#268) ──

        // Whitelister certifies the project.
        registry.certify_project(
            &whitelister,
            &project_id,
            &crate::types::CertificationStatus::Certified,
        );
        assert_eq!(
            registry.get_project(&project_id).certification_status,
            crate::types::CertificationStatus::Certified
        );

        // Project owner posts collateral (in the same USDC asset, for
        // simplicity). project_creator already holds the 500 USDC transferred
        // in by the earlier fund_project() call, so compare against a
        // captured baseline rather than assuming a zero starting balance.
        let collateral_amount = 200_0000000i128;
        let usdc = soroban_sdk::token::TokenClient::new(&env, &usdc_sac);
        let balance_before_collateral = usdc.balance(&project_creator);
        StellarAssetClient::new(&env, &usdc_sac).mint(&project_creator, &collateral_amount);
        registry.deposit_collateral(&project_id, &project_creator, &usdc_sac, &collateral_amount);
        assert_eq!(
            registry.get_collateral(&project_id, &usdc_sac),
            collateral_amount
        );
        assert_eq!(usdc.balance(&project_creator), balance_before_collateral);

        // This project was created with maturity_date = 0 ("no maturity date
        // set"), so release_collateral's maturity check never blocks it —
        // matches is_mature(project_id) == false throughout.
        assert!(!registry.is_mature(&project_id));
        registry.release_collateral(&project_id, &project_creator, &usdc_sac);
        assert_eq!(registry.get_collateral(&project_id, &usdc_sac), 0);
        assert_eq!(
            usdc.balance(&project_creator),
            balance_before_collateral + collateral_amount
        );
    }
}

// ── Score history tests (#123) ─────────────────────────────────────────────────

#[test]
fn test_score_history_records_entry_on_update() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &70u32, &80u32);

    let history = client.get_score_history(&id);
    assert_eq!(history.len(), 1);
    let entry = history.get(0).unwrap();
    assert_eq!(entry.credit_quality, 70);
    assert_eq!(entry.green_impact, 80);
}

#[test]
fn test_score_history_noop_does_not_append() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &70u32, &80u32);
    client.update_impact_score(&id, &70u32, &80u32); // no-op — same values

    let history = client.get_score_history(&id);
    assert_eq!(history.len(), 1);
}

#[test]
fn test_score_history_multiple_updates_ordered() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_impact_score(&id, &10u32, &20u32);
    env.ledger().with_mut(|li| li.timestamp += 1);
    client.update_impact_score(&id, &30u32, &40u32);
    env.ledger().with_mut(|li| li.timestamp += 1);
    client.update_impact_score(&id, &50u32, &60u32);

    let history = client.get_score_history(&id);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().credit_quality, 10); // oldest
    assert_eq!(history.get(1).unwrap().credit_quality, 30);
    assert_eq!(history.get(2).unwrap().credit_quality, 50); // newest

    // Explicit assertion on chronological ordering: timestamps must be strictly increasing
    assert!(history.get(0).unwrap().timestamp < history.get(1).unwrap().timestamp);
    assert!(history.get(1).unwrap().timestamp < history.get(2).unwrap().timestamp);
}

#[test]
fn test_credit_quality_score_history_recorded() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.update_credit_quality_score(&id, &55u32);
    client.update_credit_quality_score(&id, &55u32); // no-op

    let history = client.get_score_history(&id);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().credit_quality, 55);
}

#[test]
#[should_panic]
fn test_get_score_history_nonexistent_panics() {
    let (_env, _admin, _whitelister, client) = setup();
    client.get_score_history(&999u32);
}

// ── Circuit breaker tests (#72) ────────────────────────────────────────────────

#[test]
fn test_registry_pause_and_unpause() {
    let (_env, _admin, _whitelister, client) = setup();
    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_create_project_records_created_at() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    env.ledger().with_mut(|li| li.timestamp = 12345);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    let project = client.get_project(&id);
    assert_eq!(project.created_at, 12345);
}

#[test]
fn test_verify_metadata_hash_matches_recorded_hash() {
    // #44
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let hash = BytesN::from_array(&env, &[9u8; 32]);
    let id = client.create_project(&creator, &String::from_str(&env, "ipfs://Qm"), &0u64, &hash);

    assert!(client.verify_metadata_hash(&id, &hash));
    let wrong_hash = BytesN::from_array(&env, &[1u8; 32]);
    assert!(!client.verify_metadata_hash(&id, &wrong_hash));
}

#[test]
fn test_emergency_admin_can_pause_and_unpause_without_owner() {
    let (env, _admin, _whitelister, client) = setup();
    let emergency_admin = Address::generate(&env);

    assert_eq!(client.get_emergency_admin(), None);
    client.set_emergency_admin(&Some(emergency_admin.clone()));
    assert_eq!(client.get_emergency_admin(), Some(emergency_admin.clone()));

    assert!(!client.is_paused());
    client.emergency_pause(&emergency_admin);
    assert!(client.is_paused());
    client.emergency_unpause(&emergency_admin);
    assert!(!client.is_paused());
}

#[test]
#[should_panic]
fn test_emergency_pause_rejects_non_emergency_admin() {
    let (env, _admin, _whitelister, client) = setup();
    let emergency_admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.set_emergency_admin(&Some(emergency_admin));
    client.emergency_pause(&stranger);
}

#[test]
#[should_panic]
fn test_create_project_blocked_when_paused() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    client.pause();
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
}

#[test]
#[should_panic]
fn test_update_impact_score_blocked_when_paused() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.pause();
    client.update_impact_score(&id, &50u32, &50u32);
}

#[test]
fn test_getters_work_when_paused() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
    client.pause();
    // Read-only operations are unaffected by pause
    let project = client.get_project(&id);
    assert_eq!(project.owner, creator);
    assert_eq!(client.total_projects(), 1);
    assert_eq!(client.is_paused(), true);
}

// ── Storage compaction tests (#88) ────────────────────────────────────────────

#[test]
fn test_compact_storage_removes_zero_collateral() {
    use soroban_sdk::token::StellarAssetClient;

    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    // Use a future maturity date so collateral can be released at maturity
    let maturity = env.ledger().timestamp() + 10_000;
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &maturity,
        &test_metadata_hash(&env),
    );

    // Mint a token and deposit collateral
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    StellarAssetClient::new(&env, &token).mint(&creator, &1_000i128);
    client.deposit_collateral(&id, &creator, &token, &1_000i128);
    assert_eq!(client.get_collateral(&id, &token), 1_000i128);

    // Advance ledger past maturity and release collateral (lazy remove)
    env.ledger().with_mut(|l| l.timestamp = maturity + 1);
    client.release_collateral(&id, &creator, &token);
    assert_eq!(client.get_collateral(&id, &token), 0i128);

    // compact_storage on the now-empty key finds nothing (already removed lazily)
    let removed = client.compact_storage(
        &soroban_sdk::vec![&env, id],
        &soroban_sdk::vec![&env, token],
    );
    assert_eq!(removed, 0u32);
}

// ── Migration tests (#64) ──────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_migrate_state_rejects_wrong_from_version() {
    let (_env, _admin, _whitelister, client) = setup();
    // Stored version is 1; passing from_version = 0 should panic.
    client.migrate_state(&0u32);
}

#[test]
fn test_migrate_state_noop_on_current_version() {
    let (_env, _admin, _whitelister, client) = setup();
    // Stored version is 1; migrate_state(1) should succeed and return 1.
    let result = client.migrate_state(&1u32);
    assert_eq!(result, 1u32);
}

#[test]
fn test_state_version_matches_stored() {
    let (_env, _admin, _whitelister, client) = setup();
    assert_eq!(client.state_version(), client.stored_state_version());
}

#[test]
#[should_panic]
fn test_stale_stored_version_blocks_normal_calls() {
    // MIGRATION.md: "require_current_state rejects calls if the stored
    // version does not match the compiled STATE_VERSION. This prevents
    // accidentally running new logic against an old storage layout." (#275)
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    // Simulate a deployment whose stored schema version is ahead of this
    // build's compiled STATE_VERSION (e.g. rolled back to older code after a
    // migration ran). Note: stored version 0 ("pre-versioned deployment") is
    // deliberately grandfathered through by require_current_state and does
    // NOT panic here — only a genuine mismatch does.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&crate::types::DataKey::StateVersion, &2u32);
    });

    // A normal state-mutating call must be blocked until migrate_state runs.
    client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );
}

// ── Ownership transfer event test (#30) ───────────────────────────────────────

#[test]
fn test_transfer_ownership_emits_event() {
    let (env, _admin, _whitelister, client) = setup();
    let new_owner = Address::generate(&env);

    // Initiate transfer; live_until_ledger = 1000 (well beyond current ledger 0).
    client.transfer_ownership(&new_owner, &1000u32);

    let events = env.events().all().filter_by_contract(&client.address);
    // transfer_ownership emits: stellar-access OwnershipTransfer + our OwnershipTransferred.
    assert_eq!(
        events.events().len(),
        2,
        "transfer_ownership should emit 2 events (stellar-access + project-specific)"
    );
}

// ── Consolidated admin-only enumeration (#266) ─────────────────────────────────
//
// Several admin-only functions already have their own dedicated
// should_panic test (e.g. test_set_whitelister_is_admin_only). This test
// instead enumerates every #[only_owner] entry point on ProjectRegistry in
// one place and confirms each rejects a non-admin caller, so a future
// #[only_owner] entry point that's accidentally left off both this list and
// its own dedicated test won't go unnoticed.
#[test]
fn test_all_only_owner_functions_reject_non_admin_caller() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let project_id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmAdminOnly"),
        &0u64,
        &test_metadata_hash(&env),
    );

    let stranger = Address::generate(&env);
    // Restrict auth to `stranger` for an unrelated invocation, so every
    // #[only_owner] call below has no matching auth entry for the real
    // owner and must fail at the `owner.require_auth()` check.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &stranger,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "total_projects",
            args: soroban_sdk::vec![&env],
            sub_invokes: &[],
        },
    }]);

    let addr = || Address::generate(&env);
    let hash32 = BytesN::from_array(&env, &[0u8; 32]);

    let results: soroban_sdk::Vec<bool> = soroban_sdk::vec![
        &env,
        client.try_migrate_state(&1u32).is_err(),
        client.try_archive_project(&project_id).is_err(),
        client.try_delete_project(&project_id).is_err(),
        client.try_compact_archive(&project_id).is_err(),
        client
            .try_update_impact_score(&project_id, &1u32, &1u32)
            .is_err(),
        client
            .try_update_credit_quality_score(&project_id, &1u32)
            .is_err(),
        client
            .try_liquidate_collateral(&project_id, &addr(), &addr())
            .is_err(),
        client
            .try_set_multisig_admin(&soroban_sdk::vec![&env, addr()], &1u32)
            .is_err(),
        client.try_clear_multisig_admin().is_err(),
        client.try_set_whitelister(&addr()).is_err(),
        client.try_upgrade(&hash32).is_err(),
        client.try_pause().is_err(),
        client.try_unpause().is_err(),
        client.try_set_emergency_admin(&None).is_err(),
        client
            .try_compact_storage(
                &soroban_sdk::vec![&env, project_id],
                &soroban_sdk::vec![&env, addr()]
            )
            .is_err(),
    ];

    for (i, rejected) in results.iter().enumerate() {
        assert!(
            rejected,
            "only_owner function at index {i} did not reject a non-admin caller"
        );
    }
}

// ── Combined pause + read-only getters test (#213) ───────────────────────────

#[test]
fn test_pause_blocks_mutations_but_not_getters() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    // Create a project before pausing — succeeds.
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://Qm"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.pause();

    // Verify that a mutating call is blocked while paused.
    let blocked = client.try_create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmNew"),
        &0u64,
        &test_metadata_hash(&env),
    );
    assert!(
        blocked.is_err(),
        "create_project should be blocked when paused"
    );

    // Verify that all read-only getters still succeed.
    let project = client.get_project(&id);
    assert_eq!(project.owner, creator);
    assert_eq!(client.total_projects(), 1);
    assert_eq!(client.is_paused(), true);
    assert_eq!(client.state_version(), client.stored_state_version());
}

// ── migrate_state rejects unknown target version (#214) ──────────────────────

#[test]
#[should_panic]
fn test_migrate_state_rejects_unknown_future_target_version() {
    let (env, _admin, _whitelister, client) = setup();

    // Set stored version to a future value the contract does not recognise.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&crate::types::DataKey::StateVersion, &99u32);
    });

    // from_version = 99 matches stored, but 99 > STATE_VERSION (1), so the
    // `current > STATE_VERSION` guard in migrate_state must reject it.
    client.migrate_state(&99u32);
}

// ── Gas benchmark: get_all_projects at high project counts (#215) ────────────

#[test]
fn bench_registry_get_all_projects_50() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);

    for _ in 0..50 {
        client.create_project(
            &creator,
            &String::from_str(&env, "ipfs://Qm"),
            &0u64,
            &test_metadata_hash(&env),
        );
    }

    let projects = client.get_all_projects();
    assert_eq!(projects.len(), 50);
}

// ── Fuzz targets (#257) ─────────────────────────────────────────────────────
//
// project_registry had no proptest-based fuzz coverage at all (unlike
// investment_vault's test_vault_arithmetic_fuzz). Adds coverage for
// create_project's URI length validation and set_creator_reputation's
// 0..=100 score range, across both the accepted and rejected sides of each
// boundary.

use proptest::prelude::*;

fn make_uri(total_len: usize) -> std::string::String {
    // "ipfs://" is 7 bytes; pad the rest with filler so the full string is
    // exactly `total_len` bytes and still has a valid scheme prefix.
    let prefix = "ipfs://";
    if total_len <= prefix.len() {
        return "x".repeat(total_len);
    }
    let mut s = std::string::String::from(prefix);
    s.push_str(&"a".repeat(total_len - prefix.len()));
    s
}

proptest! {
    #[test]
    fn test_create_project_accepts_uris_within_valid_length_range_fuzz(
        len in 8usize..=512usize
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
        let client = ProjectRegistryClient::new(&env, &contract_id);
        let creator = Address::generate(&env);
        client.set_whitelist(&creator, &true);

        let uri = String::from_str(&env, &make_uri(len));
        let id = client.create_project(&creator, &uri, &0u64, &test_metadata_hash(&env));
        prop_assert_eq!(id, 1);
    }

    #[test]
    fn test_create_project_rejects_uris_outside_valid_length_range_fuzz(
        len in prop_oneof![0usize..8usize, 513usize..1000usize]
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
        let client = ProjectRegistryClient::new(&env, &contract_id);
        let creator = Address::generate(&env);
        client.set_whitelist(&creator, &true);

        let uri = String::from_str(&env, &make_uri(len));
        let result = client.try_create_project(&creator, &uri, &0u64, &test_metadata_hash(&env));
        prop_assert!(result.is_err());
    }

    #[test]
    fn test_set_creator_reputation_accepts_and_stores_valid_scores_fuzz(
        score in 0u32..=100u32
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
        let client = ProjectRegistryClient::new(&env, &contract_id);
        let creator = Address::generate(&env);

        client.set_creator_reputation(&whitelister, &creator, &score);
        prop_assert_eq!(client.get_creator_reputation(&creator), score);
    }

    #[test]
    fn test_set_creator_reputation_rejects_out_of_range_scores_fuzz(
        score in 101u32..=100_000u32
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let contract_id = env.register(ProjectRegistry, (&admin, &whitelister));
        let client = ProjectRegistryClient::new(&env, &contract_id);
        let creator = Address::generate(&env);

        let result = client.try_set_creator_reputation(&whitelister, &creator, &score);
        prop_assert!(result.is_err());
    }
}
// ── Issue #326: governance proposal coverage ──────────────────────────────────

#[test]
fn test_create_proposal_and_get_proposal() {
    let (env, _admin, _whitelister, client) = setup();
    let proposer = Address::generate(&env);
    let id = client.create_proposal(
        &proposer,
        &String::from_str(&env, "Increase green threshold"),
        &MIN_VOTING_PERIOD,
    );
    assert_eq!(id, 1);

    let p = client.get_proposal(&id);
    assert_eq!(p.proposer, proposer);
    assert_eq!(p.votes_for, 0);
    assert_eq!(p.votes_against, 0);
    assert!(!p.executed);
    assert!(p.voting_ends_at >= MIN_VOTING_PERIOD);
}

#[test]
fn test_create_proposal_rejects_too_short_voting_period() {
    let (env, _admin, _whitelister, client) = setup();
    let proposer = Address::generate(&env);
    let r = client.try_create_proposal(
        &proposer,
        &String::from_str(&env, "Too short"),
        &(MIN_VOTING_PERIOD - 1),
    );
    assert!(r.is_err());
}

#[test]
fn test_cast_vote_validation() {
    let (env, _admin, _whitelister, client) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = client.create_proposal(
        &proposer,
        &String::from_str(&env, "Vote validation"),
        &MIN_VOTING_PERIOD,
    );

    // weight <= 0 rejected.
    assert!(client.try_cast_vote(&voter, &id, &true, &0i128).is_err());
    // Unknown proposal rejected.
    assert!(client.try_cast_vote(&voter, &999u32, &true, &1i128).is_err());
    // First vote succeeds; a second vote from the same voter is rejected.
    client.cast_vote(&voter, &id, &true, &10i128);
    assert!(client.try_cast_vote(&voter, &id, &true, &5i128).is_err());
}

#[test]
fn test_proposal_full_flow_pass_and_double_execution_guard() {
    let (env, _admin, _whitelister, client) = setup();
    let proposer = Address::generate(&env);
    let voter_for = Address::generate(&env);
    let voter_against = Address::generate(&env);
    let id = client.create_proposal(
        &proposer,
        &String::from_str(&env, "Full governance flow"),
        &MIN_VOTING_PERIOD,
    );

    // Voting still open → execution rejected.
    assert!(client.try_execute_proposal(&id).is_err());

    client.cast_vote(&voter_for, &id, &true, &100i128);
    client.cast_vote(&voter_against, &id, &false, &40i128);

    // Advance time past the voting deadline.
    let deadline = client.get_proposal(&id).voting_ends_at;
    env.ledger().with_mut(|li| {
        li.timestamp = deadline + 1;
    });

    // Voting ended → further votes rejected.
    assert!(client.try_cast_vote(&voter_for, &id, &true, &1i128).is_err());

    // Execution passes (100 > 40).
    assert!(client.execute_proposal(&id));

    // Double execution rejected.
    assert!(client.try_execute_proposal(&id).is_err());

    let p = client.get_proposal(&id);
    assert!(p.executed);
    assert_eq!(p.votes_for, 100i128);
    assert_eq!(p.votes_against, 40i128);
}

#[test]
fn test_get_proposal_not_found() {
    let (env, _admin, _whitelister, client) = setup();
    assert!(client.try_get_proposal(&999u32).is_err());
}
// ── Issue #327: project archive/delete/compact lifecycle coverage ─────────────

#[test]
fn test_archive_project_flips_status_and_excludes_from_listings() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmArchive"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.archive_project(&id);

    let project = client.get_project(&id);
    assert_eq!(project.status, crate::types::ProjectStatus::Archived);

    // get_all_projects excludes archived entries.
    let active = client.get_all_projects();
    assert!(active.iter().all(|entry| entry.0 != id));

    // get_all_projects_with_archived still includes it.
    let all = client.get_all_projects_with_archived();
    assert!(all.iter().any(|entry| entry.0 == id));
}

#[test]
fn test_delete_project_removes_entry() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmDelete"),
        &0u64,
        &test_metadata_hash(&env),
    );

    client.delete_project(&id);

    // get_project must fail with ProjectNotFound after deletion.
    assert!(client.try_get_project(&id).is_err());
}

#[test]
fn test_compact_archive_matches_pre_compaction_data() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmCompact"),
        &12345u64,
        &test_metadata_hash(&env),
    );

    let before = client.get_project(&id);
    client.archive_project(&id);
    client.compact_archive(&id);

    let summary = client.get_archive_summary(&id);
    assert_eq!(summary.owner, before.owner);
    assert_eq!(summary.final_credit_quality, before.credit_quality);
    assert_eq!(summary.final_green_impact, before.green_impact);
    assert_eq!(summary.maturity_date, before.maturity_date);
    assert_eq!(summary.certification_status, before.certification_status);

    // Full project data is gone after compaction.
    assert!(client.try_get_project(&id).is_err());
}

#[test]
fn test_compact_archive_requires_prior_archiving() {
    let (env, _admin, _whitelister, client) = setup();
    let creator = Address::generate(&env);
    client.set_whitelist(&creator, &true);
    let id = client.create_project(
        &creator,
        &String::from_str(&env, "ipfs://QmCompactGuard"),
        &0u64,
        &test_metadata_hash(&env),
    );

    // Not archived yet → compact_archive must panic with ProjectNotArchived.
    assert!(client.try_compact_archive(&id).is_err());
}
