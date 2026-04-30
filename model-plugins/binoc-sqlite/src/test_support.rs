//! Test-vector helpers for `binoc-sqlite`. Enabled by the `test-support`
//! feature. Provides [`SqliteMaterializer`], a [`VectorMaterializer`] that
//! builds `.sqlite` / `.db` files from `.sqlite.d` / `.db.d` staging
//! directories containing `.sql` scripts. Used by both the plugin's test
//! vectors and its `materialize-test-vectors` binary so `just test` and
//! `just materialize` produce identical trees.

use std::path::{Path, PathBuf};

use binoc_stdlib::test_vectors::VectorMaterializer;

/// Builds SQLite databases from staging directories containing numbered `.sql`
/// scripts. Scripts are executed in sorted order against a fresh database file
/// at the sibling artifact path.
pub struct SqliteMaterializer;

impl VectorMaterializer for SqliteMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".sqlite.d", ".db.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let mut sql_files: Vec<PathBuf> = std::fs::read_dir(staging_dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", staging_dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "sql"))
            .collect();
        sql_files.sort();

        if out_path.exists() {
            std::fs::remove_file(out_path)
                .unwrap_or_else(|e| panic!("remove_file {}: {e}", out_path.display()));
        }
        let conn = rusqlite::Connection::open(out_path)
            .unwrap_or_else(|e| panic!("open {}: {e}", out_path.display()));
        for sql_path in &sql_files {
            let sql = std::fs::read_to_string(sql_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", sql_path.display()));
            conn.execute_batch(&sql).unwrap_or_else(|e| {
                panic!(
                    "execute {} on {}: {e}",
                    sql_path.display(),
                    out_path.display()
                )
            });
        }
    }
}
