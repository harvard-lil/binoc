use std::io::Write;

use binoc_core::traits::*;
use binoc_core::types::*;

use binoc_stdlib::comparators::binary::BinaryComparator;
use binoc_stdlib::comparators::csv_compare::CsvComparator;
use binoc_stdlib::comparators::directory::DirectoryComparator;
use binoc_stdlib::comparators::text::TextComparator;
use binoc_stdlib::comparators::zip_compare::ZipComparator;

fn ctx() -> CompareContext {
    CompareContext::new()
}

// ── Binary comparator ──────────────────────────────────────────────

#[test]
fn binary_identical_files() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.bin"), b"hello").unwrap();
    std::fs::write(tmp.path().join("b.bin"), b"hello").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.bin"), "file.bin"),
        Item::new(tmp.path().join("b.bin"), "file.bin"),
    );
    let result = BinaryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert_eq!(node.kind, "identical");
            assert!(node.details.contains_key("hash"));
        }
        _ => panic!("Expected Leaf with kind 'identical'"),
    }
}

#[test]
fn binary_different_files() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.bin"), b"hello").unwrap();
    std::fs::write(tmp.path().join("b.bin"), b"world").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.bin"), "file.bin"),
        Item::new(tmp.path().join("b.bin"), "file.bin"),
    );
    let result = BinaryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert_eq!(node.kind, "modify");
            assert!(node.tags.contains("binoc.content-changed"));
            assert!(node.details.contains_key("hash_left"));
            assert!(node.details.contains_key("hash_right"));
        }
        _ => panic!("Expected Leaf"),
    }
}

#[test]
fn binary_added_file_includes_hash() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("b.bin"), b"new content").unwrap();

    let pair = ItemPair::added(Item::new(tmp.path().join("b.bin"), "new.bin"));
    let result = BinaryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert_eq!(node.kind, "add");
            assert!(node.details.contains_key("hash_right"));
        }
        _ => panic!("Expected Leaf"),
    }
}

#[test]
fn binary_can_handle_returns_true() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a"), b"x").unwrap();
    let pair = ItemPair::added(Item::new(tmp.path().join("a"), "anything"));
    assert!(BinaryComparator.can_handle(&pair));
}

// ── Text comparator ────────────────────────────────────────────────

#[test]
fn text_identical_content() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), "hello\nworld\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "hello\nworld\n").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.txt"), "file.txt"),
        Item::new(tmp.path().join("b.txt"), "file.txt"),
    );
    let result = TextComparator.compare(&pair, &ctx()).unwrap();
    assert!(matches!(result, CompareResult::Identical));
}

#[test]
fn text_diff_counts_lines() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), "line1\nline2\nline3\n").unwrap();
    std::fs::write(
        tmp.path().join("b.txt"),
        "line1\nline2_changed\nline3\nline4\n",
    )
    .unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.txt"), "file.txt"),
        Item::new(tmp.path().join("b.txt"), "file.txt"),
    );
    let result = TextComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert_eq!(node.kind, "modify");
            assert_eq!(node.item_type, "text");
            let added = node.details["lines_added"].as_u64().unwrap();
            let removed = node.details["lines_removed"].as_u64().unwrap();
            assert!(added > 0);
            assert!(removed > 0);
        }
        _ => panic!("Expected Leaf"),
    }
}

#[test]
fn text_handles_txt_extension() {
    assert!(TextComparator.handles_extensions().contains(&".txt"));
    assert!(TextComparator.handles_extensions().contains(&".md"));
    assert!(TextComparator.handles_extensions().contains(&".rs"));
}

// ── CSV comparator ─────────────────────────────────────────────────

#[test]
fn csv_identical() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.csv"), "name,age\nAlice,30\n").unwrap();
    std::fs::write(tmp.path().join("b.csv"), "name,age\nAlice,30\n").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.csv"), "data.csv"),
        Item::new(tmp.path().join("b.csv"), "data.csv"),
    );
    let result = CsvComparator.compare(&pair, &ctx()).unwrap();
    assert!(matches!(result, CompareResult::Identical));
}

#[test]
fn csv_detects_column_addition() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.csv"), "name,age\nAlice,30\n").unwrap();
    std::fs::write(
        tmp.path().join("b.csv"),
        "name,age,email\nAlice,30,a@b.com\n",
    )
    .unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.csv"), "data.csv"),
        Item::new(tmp.path().join("b.csv"), "data.csv"),
    );
    let result = CsvComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert!(node.tags.contains("binoc.column-addition"));
            assert!(node.tags.contains("binoc.schema-change"));
            let cols_added = node.details["columns_added"].as_array().unwrap();
            assert_eq!(cols_added.len(), 1);
        }
        _ => panic!("Expected Leaf"),
    }
}

#[test]
fn csv_detects_row_addition() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.csv"), "name,age\nAlice,30\n").unwrap();
    std::fs::write(tmp.path().join("b.csv"), "name,age\nAlice,30\nBob,25\n").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.csv"), "data.csv"),
        Item::new(tmp.path().join("b.csv"), "data.csv"),
    );
    let result = CsvComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert!(node.tags.contains("binoc.row-addition"));
            assert_eq!(node.details["rows_added"].as_u64().unwrap(), 1);
        }
        _ => panic!("Expected Leaf"),
    }
}

#[test]
fn csv_detects_cell_changes() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.csv"), "name,score\nAlice,85\n").unwrap();
    std::fs::write(tmp.path().join("b.csv"), "name,score\nAlice,92\n").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.csv"), "data.csv"),
        Item::new(tmp.path().join("b.csv"), "data.csv"),
    );
    let result = CsvComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert!(node.tags.contains("binoc.cell-change"));
            assert!(node.details["cells_changed"].as_u64().unwrap() > 0);
        }
        _ => panic!("Expected Leaf"),
    }
}

#[test]
fn csv_detects_column_reorder() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.csv"), "name,age,city\nAlice,30,NYC\n").unwrap();
    std::fs::write(tmp.path().join("b.csv"), "city,name,age\nNYC,Alice,30\n").unwrap();

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.csv"), "data.csv"),
        Item::new(tmp.path().join("b.csv"), "data.csv"),
    );
    let result = CsvComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Leaf(node) => {
            assert!(node.tags.contains("binoc.column-reorder"));
        }
        _ => panic!("Expected Leaf"),
    }
}

// ── Directory comparator ───────────────────────────────────────────

#[test]
fn directory_can_handle_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let pair = ItemPair::both(Item::new(tmp.path(), "dir"), Item::new(tmp.path(), "dir"));
    assert!(DirectoryComparator.can_handle(&pair));
}

#[test]
fn directory_expands_children() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(a.join("file.txt"), "hello").unwrap();
    std::fs::write(b.join("file.txt"), "hello").unwrap();

    let pair = ItemPair::both(Item::new(&a, "root"), Item::new(&b, "root"));
    let result = DirectoryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Expand(node, children) => {
            assert_eq!(node.item_type, "directory");
            assert_eq!(children.len(), 1);
        }
        _ => panic!("Expected Expand"),
    }
}

#[test]
fn directory_detects_added_files() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("new.txt"), "new").unwrap();

    let pair = ItemPair::both(Item::new(&a, "root"), Item::new(&b, "root"));
    let result = DirectoryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Expand(_, children) => {
            assert_eq!(children.len(), 1);
            assert!(children[0].left.is_none());
            assert!(children[0].right.is_some());
        }
        _ => panic!("Expected Expand"),
    }
}

// ── Directory comparator: media type detection ─────────────────────

#[test]
fn directory_populates_media_type_from_content() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();

    // Write a PNG file (infer detects from magic bytes)
    let png_header = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
    std::fs::write(a.join("image.png"), png_header).unwrap();
    std::fs::write(b.join("image.png"), png_header).unwrap();

    let pair = ItemPair::both(Item::new(&a, "root"), Item::new(&b, "root"));
    let result = DirectoryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Expand(_, children) => {
            let child = &children[0];
            let item = child.right.as_ref().unwrap();
            assert_eq!(item.media_type.as_deref(), Some("image/png"));
        }
        _ => panic!("Expected Expand"),
    }
}

#[test]
fn directory_media_type_falls_back_to_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();

    // Plain text has no distinctive magic bytes; infer won't match, so
    // mime_guess from the .txt extension should provide the fallback.
    std::fs::write(a.join("readme.txt"), "hello world").unwrap();
    std::fs::write(b.join("readme.txt"), "hello world").unwrap();

    let pair = ItemPair::both(Item::new(&a, "root"), Item::new(&b, "root"));
    let result = DirectoryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Expand(_, children) => {
            let child = &children[0];
            let item = child.right.as_ref().unwrap();
            assert_eq!(item.media_type.as_deref(), Some("text/plain"));
        }
        _ => panic!("Expected Expand"),
    }
}

#[test]
fn directory_media_type_none_for_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();

    // A file with no extension and non-magic content
    std::fs::write(a.join("Makefile"), "all: build").unwrap();
    std::fs::write(b.join("Makefile"), "all: build").unwrap();

    let pair = ItemPair::both(Item::new(&a, "root"), Item::new(&b, "root"));
    let result = DirectoryComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Expand(_, children) => {
            let child = &children[0];
            let item = child.right.as_ref().unwrap();
            // No extension, no magic bytes → None
            assert!(item.media_type.is_none());
        }
        _ => panic!("Expected Expand"),
    }
}

// ── Zip comparator ─────────────────────────────────────────────────

#[test]
fn zip_handles_zip_extension() {
    assert!(ZipComparator.handles_extensions().contains(&".zip"));
}

#[test]
fn zip_handles_zip_media_type() {
    assert!(ZipComparator
        .handles_media_types()
        .contains(&"application/zip"));
}

#[test]
fn zip_expands_contents() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_zip(&tmp.path().join("a.zip"), &[("data.txt", "hello")]);
    create_test_zip(&tmp.path().join("b.zip"), &[("data.txt", "world")]);

    let pair = ItemPair::both(
        Item::new(tmp.path().join("a.zip"), "archive.zip"),
        Item::new(tmp.path().join("b.zip"), "archive.zip"),
    );
    let result = ZipComparator.compare(&pair, &ctx()).unwrap();
    match result {
        CompareResult::Expand(node, children) => {
            assert_eq!(node.item_type, "zip_archive");
            assert!(!children.is_empty());
        }
        _ => panic!("Expected Expand"),
    }
}

fn create_test_zip(path: &std::path::Path, entries: &[(&str, &str)]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}
