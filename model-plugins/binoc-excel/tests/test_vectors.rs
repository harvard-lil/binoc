//! Plugin test vectors: binoc-excel/test-vectors/. Uses the shared harness from
//! binoc_stdlib::test_vectors; the ExcelMaterializer that builds `.xlsx` from
//! `.xlsx.d` lives in `binoc_excel::test_support`.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

use std::path::PathBuf;

use binoc_excel::test_support::ExcelMaterializer;
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

    let stdlib = stdlib_materializers();
    let excel = ExcelMaterializer;
    let mut materializers: Vec<&dyn VectorMaterializer> = stdlib
        .iter()
        .map(|m| &**m as &dyn VectorMaterializer)
        .collect();
    materializers.push(&excel);

    for vector in &vectors {
        run_vector_with_correspondence_engine_config(
            vector,
            &vectors_dir(),
            &materializers,
            |dataset| {
                let mut config =
                    binoc_stdlib::correspondence::engine_config_for_dataset_config(dataset);
                binoc_excel::register_correspondence_rules(&mut config);
                config
            },
        );
    }
}
