use std::fs;
use std::path::PathBuf;

use binoc_core::config::DatasetConfig;
use binoc_core::controller::Controller;
use binoc_core::ir::Migration;

fn setup_test_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn create_controller() -> Controller {
    let config = DatasetConfig::default_config();
    let registry = binoc_stdlib::default_registry();
    let resolved = registry.resolve(&config).unwrap();
    Controller::new(resolved.comparators, resolved.transformers)
}

#[test]
fn test_identical_files() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(dir_a.join("data.txt"), "hello world\n").unwrap();
    fs::write(dir_b.join("data.txt"), "hello world\n").unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    // Root directory expand produces a node, but all children are identical
    // so the directory node should have no children with actual diffs
    match &migration.root {
        Some(root) => {
            assert!(
                root.children.is_empty(),
                "identical files should produce no diff children, got: {:?}",
                root.children
            );
        }
        None => {} // Also acceptable
    }
}

#[test]
fn test_added_file() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(dir_a.join("existing.txt"), "hello\n").unwrap();
    fs::write(dir_b.join("existing.txt"), "hello\n").unwrap();
    fs::write(dir_b.join("new_file.txt"), "new content\n").unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    assert!(!root.children.is_empty(), "should have children");

    let added = root
        .children
        .iter()
        .find(|c| c.kind == "add")
        .expect("should have add node");
    assert!(added.path.contains("new_file.txt"));
}

#[test]
fn test_removed_file() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(dir_a.join("old_file.txt"), "old content\n").unwrap();
    fs::write(dir_a.join("kept.txt"), "kept\n").unwrap();
    fs::write(dir_b.join("kept.txt"), "kept\n").unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    let removed = root
        .children
        .iter()
        .find(|c| c.kind == "remove")
        .expect("should have remove node");
    assert!(removed.path.contains("old_file.txt"));
}

#[test]
fn test_modified_text_file() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(dir_a.join("notes.txt"), "line 1\nline 2\nline 3\n").unwrap();
    fs::write(
        dir_b.join("notes.txt"),
        "line 1\nline 2 modified\nline 3\nline 4\n",
    )
    .unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    let modified = root
        .children
        .iter()
        .find(|c| c.kind == "modify")
        .expect("should have modify node");
    assert_eq!(modified.item_type, "text");
    assert!(modified.tags.contains("binoc.content-changed"));

    let lines_added = modified
        .details
        .get("lines_added")
        .unwrap()
        .as_u64()
        .unwrap();
    let lines_removed = modified
        .details
        .get("lines_removed")
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(lines_added > 0);
    assert!(lines_removed > 0);
}

#[test]
fn test_csv_column_changes() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(
        dir_a.join("data.csv"),
        "name,age,city\nAlice,30,NYC\nBob,25,LA\n",
    )
    .unwrap();
    fs::write(
        dir_b.join("data.csv"),
        "name,age,city,email\nAlice,30,NYC,a@b.com\nBob,25,LA,b@c.com\nCharlie,35,SF,c@d.com\n",
    )
    .unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    let csv_node = root
        .children
        .iter()
        .find(|c| c.item_type == "tabular")
        .expect("should have tabular node");

    assert_eq!(csv_node.kind, "modify");
    assert!(csv_node.tags.contains("binoc.column-addition"));
    assert!(csv_node.tags.contains("binoc.row-addition"));

    let cols_added = csv_node
        .details
        .get("columns_added")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(cols_added.len(), 1);
    assert_eq!(cols_added[0], "email");
}

#[test]
fn test_csv_column_reorder_only() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(
        dir_a.join("data.csv"),
        "name,age,city\nAlice,30,NYC\nBob,25,LA\n",
    )
    .unwrap();
    fs::write(
        dir_b.join("data.csv"),
        "city,name,age\nNYC,Alice,30\nLA,Bob,25\n",
    )
    .unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    let csv_node = root
        .children
        .iter()
        .find(|c| c.item_type == "tabular")
        .expect("should have tabular node");

    // The column_reorder_detector transformer should have converted this to "reorder"
    assert_eq!(csv_node.kind, "reorder");
    assert!(csv_node.tags.contains("binoc.column-reorder"));
}

#[test]
fn test_move_detection() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    let content = "This is some specific content for move detection.\n";
    fs::write(dir_a.join("old_name.bin"), content).unwrap();
    fs::write(dir_b.join("new_name.bin"), content).unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    let move_node = root.children.iter().find(|c| c.kind == "move");
    assert!(
        move_node.is_some(),
        "should detect move, got: {:?}",
        root.children
            .iter()
            .map(|c| (&c.kind, &c.path))
            .collect::<Vec<_>>()
    );

    let move_node = move_node.unwrap();
    assert!(move_node.source_path.is_some());
}

#[test]
fn test_zip_comparison() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    // Create zip files
    create_test_zip(
        &dir_a.join("archive.zip"),
        &[("data.txt", "hello from zip a\n")],
    );
    create_test_zip(
        &dir_b.join("archive.zip"),
        &[
            ("data.txt", "hello from zip b\n"),
            ("extra.txt", "new file\n"),
        ],
    );

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let root = migration.root.expect("should have root");
    let zip_node = root
        .children
        .iter()
        .find(|c| c.item_type == "zip_archive")
        .expect("should have zip_archive node");

    assert!(
        !zip_node.children.is_empty() || zip_node.children.iter().any(|c| !c.children.is_empty()),
        "zip should have diffed contents"
    );
}

#[test]
fn test_json_serialization() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(dir_a.join("file.txt"), "before\n").unwrap();
    fs::write(dir_b.join("file.txt"), "after\n").unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let json = binoc_core::output::to_json(&migration).unwrap();
    let roundtrip: Migration = serde_json::from_str(&json).unwrap();
    assert_eq!(migration.from_snapshot, roundtrip.from_snapshot);
    assert_eq!(migration.to_snapshot, roundtrip.to_snapshot);
    assert!(roundtrip.root.is_some());
}

#[test]
fn test_markdown_output() {
    let tmp = setup_test_dir();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    fs::write(dir_a.join("data.csv"), "name,age\nAlice,30\n").unwrap();
    fs::write(dir_b.join("data.csv"), "name,age\nAlice,30\nBob,25\n").unwrap();

    let controller = create_controller();
    let migration = controller
        .diff(dir_a.to_str().unwrap(), dir_b.to_str().unwrap())
        .unwrap();

    let md_config = binoc_stdlib::outputters::markdown::MarkdownOutputterConfig::default();
    let md = binoc_stdlib::outputters::markdown::render_markdown(&[migration], &md_config);
    assert!(md.contains("Changelog:"));
    assert!(md.contains("data.csv"));
}

fn create_test_zip(path: &PathBuf, entries: &[(&str, &str)]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (name, content) in entries {
        zip.start_file(*name, options).unwrap();
        std::io::Write::write_all(&mut zip, content.as_bytes()).unwrap();
    }

    zip.finish().unwrap();
}
