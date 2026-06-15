//! Plugin test vectors: binoc-xml/test-vectors/. Uses the shared harness from
//! binoc_stdlib::test_vectors. XML vectors commit their `.xml` text directly
//! (like the plist/ion vectors in binoc-binformats), so no plugin-specific
//! VectorMaterializer is required.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

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

    let stdlib = stdlib_materializers();
    let materializers: Vec<&dyn VectorMaterializer> = stdlib
        .iter()
        .map(|m| &**m as &dyn VectorMaterializer)
        .collect();

    for vector in &vectors {
        let is_without_plugin = vector
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "without-plugin");
        run_vector_with_correspondence_engine_config(
            vector,
            &vectors_dir(),
            &materializers,
            |dataset| {
                let mut config =
                    binoc_stdlib::correspondence::engine_config_for_dataset_config(dataset);
                if !is_without_plugin {
                    binoc_xml::register_correspondence_rules(&mut config);
                }
                config
            },
        );
    }
}
