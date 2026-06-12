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
    abi_wrapped_default_registry, discover_vectors, run_vector_with_abi_log, stdlib_materializers,
    VectorMaterializer,
};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-vectors")
}

/// With this plugin registered, `binoc.cell-change` is produced only by
/// `binoc.tabular_analyzer` and matched only by us — which the
/// single-producer/single-consumer lint flags. That is acceptable here
/// and allowlisted: the tag is not a private dispatch channel (renderer
/// group configs consume it too), and this plugin needs its own multiset
/// scan that the analyzer's single pass does not subsume — it is the
/// model out-of-tree artifact consumer.
#[test]
fn cell_change_tag_handoff_is_known_and_allowlisted() {
    use binoc_sdk::test_support::single_producer_single_consumer_tags;
    use binoc_sdk::Transformer;

    let registry = binoc_stdlib::default_registry();
    let mut descriptors: Vec<_> = registry
        .transformer_names()
        .into_iter()
        .map(|name| registry.get_transformer(&name).unwrap().descriptor())
        .collect();
    descriptors.push(RowReorderDetector.descriptor());

    let warnings = single_producer_single_consumer_tags(&descriptors, &[]);
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert!(warnings[0].contains("binoc.cell-change"));

    let allowlisted = single_producer_single_consumer_tags(&descriptors, &["binoc.cell-change"]);
    assert!(allowlisted.is_empty(), "warnings: {allowlisted:?}");
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
            || {
                let mut direct = binoc_stdlib::default_registry();
                direct
                    .register_transformer(Arc::new(RowReorderDetector))
                    .expect("same-build plugin");
                direct
            },
            move || registry,
            &materializer_refs,
            &collector_refs,
        );
    }
}
