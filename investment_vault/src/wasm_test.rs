extern crate std;

#[test]
fn test_wasm_snapshot() {
    use sha2::{Digest, Sha256};
    let wasm =
        std::fs::read("../target/wasm32v1-none/release/investment_vault.wasm").unwrap_or_default();
    if !wasm.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(&wasm);
        let hash = std::format!("{:x}", hasher.finalize());
        std::println!("WASM Hash: {}", hash);
        std::fs::write("test_snapshots/wasm_hash.txt", hash).unwrap();
    }
}

/// #260: test.rs registers `InvestmentVault` by its Rust type, which
/// soroban-sdk executes as native code linked directly into the test
/// binary — none of those tests actually run the compiled WASM.
/// test_wasm_snapshot only hashes the WASM file; it never executes it.
/// This test registers the real compiled artifact (mirroring the pattern
/// test.rs already uses to import project_registry's WASM as a
/// cross-contract dependency) and exercises a representative slice of the
/// public API through it, to catch any WASM-codegen-specific divergence
/// from the native-execution test suite.
mod wasm_smoke {
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{token::StellarAssetClient, Address, Env, String};

    mod vault_wasm {
        soroban_sdk::contractimport!(
            file = "../target/wasm32v1-none/release/investment_vault.wasm"
        );
    }
    mod registry_wasm {
        soroban_sdk::contractimport!(
            file = "../target/wasm32v1-none/release/project_registry.wasm"
        );
    }

    #[test]
    fn test_deposit_withdraw_and_admin_flow_via_compiled_wasm() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let registry_id = env.register(registry_wasm::WASM, (&admin, &admin));

        let usdc_admin = Address::generate(&env);
        let usdc_sac = env
            .register_stellar_asset_contract_v2(usdc_admin.clone())
            .address();

        let vault_id = env.register(vault_wasm::WASM, (&admin, &usdc_sac, &registry_id));
        let vault = vault_wasm::Client::new(&env, &vault_id);

        assert!(!vault.is_paused());
        assert_eq!(vault.state_version(), 1u32);

        let investor = Address::generate(&env);
        StellarAssetClient::new(&env, &usdc_sac).mint(&investor, &1_000_0000000i128);
        let shares = vault.deposit(&investor, &1_000_0000000i128);
        assert!(shares > 0);
        assert_eq!(vault.balance(&investor), shares);
        assert_eq!(vault.total_assets(), 1_000_0000000i128);

        let registry = registry_wasm::Client::new(&env, &registry_id);
        let creator = Address::generate(&env);
        registry.set_whitelist(&creator, &true);
        let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
        let project_id = registry.create_project(
            &creator,
            &String::from_str(&env, "ipfs://QmWasmSmoke"),
            &0u64,
            &hash,
        );
        assert_eq!(registry.get_project(&project_id).owner, creator);

        vault.fund_project(&project_id, &100_0000000i128);
        assert_eq!(vault.get_project_investment(&project_id), 100_0000000i128);

        env.ledger().with_mut(|li| {
            li.sequence_number += 1;
        });
        // May be paid immediately or enqueued (returns 0) depending on
        // liquidity remaining after fund_project — either is valid, the
        // point is the call succeeds without panicking.
        let returned = vault.withdraw(&investor, &shares, &0);
        assert!(returned >= 0);

        vault.pause();
        assert!(vault.is_paused());
        vault.unpause();
        assert!(!vault.is_paused());
    }
}
