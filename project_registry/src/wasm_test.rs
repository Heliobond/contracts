extern crate std;

#[test]
fn test_wasm_snapshot() {
    use sha2::{Sha256, Digest};
    let wasm = std::fs::read("../target/wasm32v1-none/release/project_registry.wasm").unwrap_or_default();
    if !wasm.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(&wasm);
        let hash = std::format!("{:x}", hasher.finalize());
        std::println!("WASM Hash: {}", hash);
        // Compare with snapshot here or write it to a file
        std::fs::write("test_snapshots/wasm_hash.txt", hash).unwrap();
    }
}
