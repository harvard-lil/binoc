//! Workspace test vectors: test-vectors/ at repo root. Uses the shared harness
//! from binoc_stdlib::test_vectors so plugins can do the same without duplicating logic.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

use std::path::PathBuf;

use binoc_stdlib::test_vectors::{
    discover_vectors, run_vector, stdlib_materializers, VectorMaterializer,
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
    let materializers = stdlib_materializers();
    let materializer_refs: Vec<&dyn VectorMaterializer> =
        materializers.iter().map(|m| &**m).collect();
    for vector in &vectors {
        run_vector(vector, &vectors_dir(), &materializer_refs);
    }
}
