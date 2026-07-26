extern crate std;

#[test]
fn test_wasm_snapshot() {
    use sha2::{Digest, Sha256};
    let wasm =
        std::fs::read("../target/wasm32v1-none/release/project_registry.wasm").unwrap_or_default();
    if !wasm.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(&wasm);
        let hash = std::format!("{:x}", hasher.finalize());
        std::println!("WASM Hash: {}", hash);
        // Compare with snapshot here or write it to a file
        std::fs::write("test_snapshots/wasm_hash.txt", hash).unwrap();
    }
}

/// #261: test.rs registers `ProjectRegistry` by its Rust type, which
/// soroban-sdk executes as native code linked directly into the test
/// binary — none of those tests actually run the compiled WASM.
/// test_wasm_snapshot only hashes the WASM file; it never executes it.
/// This test registers the real compiled artifact and exercises a
/// representative slice of the public API through it, to catch any
/// WASM-codegen-specific divergence from the native-execution test suite.
mod wasm_smoke {
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, BytesN, Env, String};

    mod registry_wasm {
        soroban_sdk::contractimport!(
            file = "../target/wasm32v1-none/release/project_registry.wasm"
        );
    }

    #[test]
    fn test_create_certify_and_admin_flow_via_compiled_wasm() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let registry_id = env.register(registry_wasm::WASM, (&admin, &whitelister));
        let registry = registry_wasm::Client::new(&env, &registry_id);

        assert!(!registry.is_paused());
        assert_eq!(registry.state_version(), 1u32);
        assert_eq!(registry.total_projects(), 0u32);

        let creator = Address::generate(&env);
        registry.set_whitelist(&creator, &true);

        let hash = BytesN::from_array(&env, &[2u8; 32]);
        let project_id = registry.create_project(
            &creator,
            &String::from_str(&env, "ipfs://QmWasmSmoke"),
            &0u64,
            &hash,
        );
        assert_eq!(project_id, 1u32);

        let project = registry.get_project(&project_id);
        assert_eq!(project.owner, creator);
        assert!(registry.verify_metadata_hash(&project_id, &hash));

        registry.update_impact_score(&project_id, &80u32, &70u32);
        let updated = registry.get_project(&project_id);
        assert_eq!(updated.credit_quality, 80);
        assert_eq!(updated.green_impact, 70);

        registry.pause();
        assert!(registry.is_paused());
        registry.unpause();
        assert!(!registry.is_paused());
    }
}
