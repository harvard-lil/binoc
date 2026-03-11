//! Plugin test vectors: binoc-sqlite/test-vectors/. Uses the shared harness from
//! binoc_stdlib::test_vectors; building SQLite from .sqlite.d/.db.d is done here
//! via the prepare callback (a stdlib concern would not depend on rusqlite).

use std::path::{Path, PathBuf};

use binoc_sqlite::register as register_sqlite;
use binoc_stdlib::default_registry;
use binoc_stdlib::test_vectors::{discover_vectors, run_vector};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-vectors")
}

fn registry_with_sqlite() -> binoc_core::config::PluginRegistry {
    let mut r = default_registry();
    register_sqlite(&mut r);
    r
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

macro_rules! vector_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            let dir = vectors_dir().join(stringify!($name).replace('_', "-"));
            if !dir.exists() {
                panic!("Test vector directory not found: {}", dir.display());
            }
            run_vector(
                &dir,
                &vectors_dir(),
                registry_with_sqlite,
                Some(prepare_sqlite),
            );
        }
    };
}

vector_test!(sqlite_row_addition);
vector_test!(sqlite_table_addition);
vector_test!(without_plugin);

#[test]
fn all_vectors_have_tests() {
    let known_vectors = [
        "sqlite-row-addition",
        "sqlite-table-addition",
        "without-plugin",
    ];
    let discovered: Vec<String> = discover_vectors(&vectors_dir())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    for v in &discovered {
        assert!(
            known_vectors.contains(&v.as_str()),
            "Test vector '{v}' discovered but has no corresponding test function. Add one with vector_test!({}).",
            v.replace('-', "_")
        );
    }
}
