//! Workspace test vectors: test-vectors/ at repo root. Uses the shared harness
//! from binoc_stdlib::test_vectors so plugins can do the same without duplicating logic.

use std::path::{Path, PathBuf};

use binoc_stdlib::default_registry;
use binoc_stdlib::test_vectors::{discover_vectors, run_vector};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-vectors")
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
                default_registry,
                None::<fn(&Path, &Path)>,
            );
        }
    };
}

vector_test!(trivial_identical);
vector_test!(single_file_add);
vector_test!(single_file_remove);
vector_test!(single_file_modify_text);
vector_test!(single_file_modify_binary);
vector_test!(csv_column_reorder);
vector_test!(csv_row_addition);
vector_test!(csv_column_addition);
vector_test!(csv_column_removal);
vector_test!(csv_cell_changes);
vector_test!(csv_mixed_changes);
vector_test!(directory_file_move);
vector_test!(directory_file_copy);
vector_test!(directory_nested);
vector_test!(text_file_move);
vector_test!(zip_simple);
vector_test!(zip_nested);
vector_test!(tar_simple);
vector_test!(tar_nested);

#[test]
fn all_vectors_have_tests() {
    let known_vectors = [
        "trivial-identical",
        "single-file-add",
        "single-file-remove",
        "single-file-modify-text",
        "single-file-modify-binary",
        "csv-column-reorder",
        "csv-row-addition",
        "csv-column-addition",
        "csv-column-removal",
        "csv-cell-changes",
        "csv-mixed-changes",
        "directory-file-move",
        "directory-file-copy",
        "directory-nested",
        "text-file-move",
        "zip-simple",
        "zip-nested",
        "tar-simple",
        "tar-nested",
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
