//! Workspace test vectors: test-vectors/ at repo root. Uses the shared harness
//! from binoc_stdlib::test_vectors so plugins can do the same without duplicating logic.
//!
//! All stdlib plugins are wrapped in ABI wrappers so every call goes through the
//! JSON wire format. ABI and DataAccess interactions are snapshotted as golden files.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

use std::path::{Path, PathBuf};

use binoc_stdlib::test_vectors::{
    abi_wrapped_default_registry, discover_vectors, run_vector_with_abi_log,
};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-vectors")
}

#[test]
fn test_all_vectors() {
    let vectors = discover_vectors(&vectors_dir());
    assert!(
        !vectors.is_empty(),
        "No test vectors found at {}",
        vectors_dir().display()
    );
    for vector in &vectors {
        let (registry, collectors, _counter) = abi_wrapped_default_registry();
        let collector_refs: Vec<&dyn binoc_sdk::test_support::AbiLogCollector> =
            collectors.iter().map(|c| c.as_ref()).collect();
        run_vector_with_abi_log(
            vector,
            &vectors_dir(),
            move || registry,
            None::<fn(&Path, &Path)>,
            &collector_refs,
        );
    }
}
