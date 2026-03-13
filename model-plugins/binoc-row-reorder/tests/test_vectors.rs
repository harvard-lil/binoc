//! Plugin test vectors: binoc-row-reorder/test-vectors/. Uses the shared harness
//! from binoc_stdlib::test_vectors.
//!
//! All plugins (stdlib + row-reorder) are wrapped in ABI wrappers so every call
//! goes through the JSON wire format. ABI and DataAccess interactions are
//! snapshotted as golden files.

use std::path::PathBuf;
use std::sync::Arc;

use binoc_row_reorder::RowReorderDetector;
use binoc_sdk::test_support::{AbiLogCollector, AbiTransformer};
use binoc_stdlib::test_vectors::{
    abi_wrapped_default_registry, discover_vectors, run_vector_with_abi_log,
};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-vectors")
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
        let (mut registry, mut collectors, counter) = abi_wrapped_default_registry();

        let reorder_trans = Arc::new(AbiTransformer::new(RowReorderDetector, counter));
        collectors.push(reorder_trans.clone());
        registry
            .register_transformer(reorder_trans)
            .expect("same-build plugin");

        let collector_refs: Vec<&dyn AbiLogCollector> =
            collectors.iter().map(|c| c.as_ref()).collect();
        run_vector_with_abi_log(
            vector,
            &vectors_dir(),
            move || registry,
            None::<fn(&std::path::Path, &std::path::Path)>,
            &collector_refs,
        );
    }
}
