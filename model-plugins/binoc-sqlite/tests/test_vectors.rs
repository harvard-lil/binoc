//! Plugin test vectors: binoc-sqlite/test-vectors/. Uses the shared harness from
//! binoc_stdlib::test_vectors; building SQLite from .sqlite.d/.db.d is done here
//! via the prepare callback (a stdlib concern would not depend on rusqlite).
//!
//! All plugins (stdlib + sqlite) are wrapped in ABI wrappers so every call goes
//! through the JSON wire format. ABI and DataAccess interactions are snapshotted
//! as golden files.
//!
//! Auto-discovers all vectors — add a new directory with manifest.toml + snapshots
//! and it will be tested automatically.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use binoc_sdk::test_support::{AbiComparator, AbiLogCollector};
use binoc_sqlite::SqliteComparator;
use binoc_stdlib::test_vectors::{
    abi_wrapped_default_registry, discover_vectors, run_vector_with_abi_log,
};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-vectors")
}

/// Build .sqlite/.db from .sqlite.d/.db.d in both snapshot dirs, then remove the .d dirs.
fn prepare_sqlite(snap_a: &Path, snap_b: &Path) {
    build_sqlite_in_dir(snap_a);
    build_sqlite_in_dir(snap_b);
    remove_sqlite_dirs(snap_a);
    remove_sqlite_dirs(snap_b);
}

fn build_sqlite_in_dir(dir: &Path) {
    if !dir.exists() {
        return;
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flat_map(|rd| rd.into_iter())
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    for entry in entries {
        if entry.is_dir() {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".sqlite.d") && !name.ends_with(".db.d") {
                build_sqlite_in_dir(&entry);
                continue;
            }
            let db_name = name.trim_end_matches(".d");
            let db_path = dir.join(db_name);
            create_sqlite_from_sql_dir(&entry, &db_path);
        }
    }
}

fn create_sqlite_from_sql_dir(source_dir: &Path, db_path: &Path) {
    let mut sql_files: Vec<PathBuf> = std::fs::read_dir(source_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    sql_files.sort();
    let conn = rusqlite::Connection::open(db_path)
        .unwrap_or_else(|e| panic!("Failed to create {}: {e}", db_path.display()));
    for sql_path in &sql_files {
        let sql = std::fs::read_to_string(sql_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", sql_path.display()));
        conn.execute_batch(&sql).unwrap_or_else(|e| {
            panic!(
                "Failed to run {} on {}: {e}",
                sql_path.display(),
                db_path.display()
            )
        });
    }
}

fn remove_sqlite_dirs(dir: &Path) {
    if !dir.exists() {
        return;
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flat_map(|rd| rd.into_iter())
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    for entry in entries {
        if entry.is_dir() {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".sqlite.d") || name.ends_with(".db.d") {
                std::fs::remove_dir_all(&entry).ok();
            } else {
                remove_sqlite_dirs(&entry);
            }
        }
    }
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
            move || registry,
            Some(prepare_sqlite),
            &collector_refs,
        );
    }
}
