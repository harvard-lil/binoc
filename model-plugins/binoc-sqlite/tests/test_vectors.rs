//! Plugin test vectors: binoc-sqlite/test-vectors/. Uses the shared harness from
//! binoc_stdlib::test_vectors; the SqliteMaterializer that builds `.sqlite` from
//! `.sqlite.d`/`.db.d` lives in `binoc_sqlite::test_support` so this test and
//! the `materialize-test-vectors` binary share one builder.
//!
//! All plugins (stdlib + sqlite) are wrapped in ABI wrappers so every call goes
//! through the JSON wire format. ABI and DataAccess interactions are snapshotted
//! as golden files.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

use std::path::PathBuf;
use std::sync::Arc;

use binoc_sdk::test_support::{AbiComparator, AbiLogCollector};
use binoc_sqlite::test_support::SqliteMaterializer;
use binoc_sqlite::SqliteComparator;
use binoc_stdlib::test_vectors::{
    abi_wrapped_default_registry, discover_vectors, run_vector_with_abi_log, stdlib_materializers,
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
    let sqlite = SqliteMaterializer;
    let mut materializers: Vec<&dyn VectorMaterializer> = stdlib
        .iter()
        .map(|m| &**m as &dyn VectorMaterializer)
        .collect();
    materializers.push(&sqlite);

    for vector in &vectors {
        let (mut registry, mut collectors, counter) = abi_wrapped_default_registry();

        let sqlite_comp = Arc::new(AbiComparator::new(SqliteComparator, counter));
        collectors.push(sqlite_comp.clone());
        registry
            .register_comparator(sqlite_comp)
            .expect("same-build plugin");

        let collector_refs: Vec<&dyn AbiLogCollector> =
            collectors.iter().map(|c| c.as_ref()).collect();
        run_vector_with_abi_log(
            vector,
            &vectors_dir(),
            || {
                let mut direct = binoc_stdlib::default_registry();
                direct
                    .register_comparator(Arc::new(SqliteComparator))
                    .expect("same-build plugin");
                direct
            },
            move || registry,
            &materializers,
            &collector_refs,
        );
    }
}
