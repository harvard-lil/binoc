//! Plugin test vectors: binoc-row-reorder/test-vectors/. Uses the shared harness
//! from binoc_stdlib::test_vectors.
//!
use std::path::PathBuf;

use binoc_stdlib::test_vectors::{
    discover_vectors, run_vector_with_correspondence_engine_config, stdlib_materializers,
    VectorMaterializer,
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
    let materializers = stdlib_materializers();
    let materializer_refs: Vec<&dyn VectorMaterializer> =
        materializers.iter().map(|m| &**m).collect();
    for vector in &vectors {
        run_vector_with_correspondence_engine_config(
            vector,
            &vectors_dir(),
            &materializer_refs,
            |dataset| {
                let mut config =
                    binoc_stdlib::correspondence::engine_config_for_dataset_config(dataset);
                binoc_row_reorder::register_correspondence_rules(&mut config);
                config
            },
        );
    }
}
