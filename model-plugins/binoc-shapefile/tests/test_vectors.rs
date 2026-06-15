//! Plugin test vectors: binoc-shapefile/test-vectors/. Uses the shared harness
//! from binoc_stdlib::test_vectors; the ShapefileMaterializer that builds `.shp`
//! from `.shp.d/geometry.json` lives in `binoc_shapefile::test_support`.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

use std::path::PathBuf;

use binoc_dbf::test_support::DbfMaterializer;
use binoc_shapefile::test_support::ShapefileMaterializer;
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
    // The shapefile materializer builds `.shp` + sidecars from `.shp.d`; the dbf
    // materializer builds a standalone `.dbf` from `.dbf.d` (used by the
    // fusion-decline vector to prove a lone `.dbf` still parses as a table).
    let shapefile = ShapefileMaterializer;
    let dbf = DbfMaterializer;
    let mut materializers: Vec<&dyn VectorMaterializer> = stdlib
        .iter()
        .map(|m| &**m as &dyn VectorMaterializer)
        .collect();
    materializers.push(&shapefile);
    materializers.push(&dbf);

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
                    binoc_shapefile::register_correspondence_rules(&mut config);
                    // Register the dbf single-input parser so a subsumed/standalone
                    // `.dbf` has a size-1 claim to fall through to (the
                    // fusion-decline case), and so the size-5 shapefile claim wins
                    // it by arity precedence in the fusion case.
                    binoc_dbf::register_correspondence_rules(&mut config);
                }
                config
            },
        );
    }
}
