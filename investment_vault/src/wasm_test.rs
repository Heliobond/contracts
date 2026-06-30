#[test]
fn test_wasm_snapshot() {
    use sha2::{Sha256, Digest};
    let wasm = std::fs::read("../target/wasm32v1-none/release/investment_vault.wasm").unwrap_or_default();
    if !wasm.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(&wasm);
        let hash = format!("{:x}", hasher.finalize());
        std::println!("WASM Hash: {}", hash);
        std::fs::write("test_snapshots/wasm_hash.txt", hash).unwrap();
    }
}
