use binoc_core::ir::DiffNode;
use binoc_core::traits::{CompareContext, Transformer};
use binoc_core::types::*;

use binoc_stdlib::transformers::column_reorder::ColumnReorderDetector;
use binoc_stdlib::transformers::copy_detector::CopyDetector;
use binoc_stdlib::transformers::move_detector::MoveDetector;

fn ctx() -> CompareContext {
    CompareContext::new()
}

// ── Move detector ──────────────────────────────────────────────────

#[test]
fn move_detector_collapses_matching_add_remove() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![
        DiffNode::new("remove", "file", "/old.bin")
            .with_detail("hash_left", serde_json::json!("abc123")),
        DiffNode::new("add", "file", "/new.bin")
            .with_detail("hash_right", serde_json::json!("abc123")),
    ]);

    let result = MoveDetector.transform(container, &ctx());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(node.children.len(), 1);
            assert_eq!(node.children[0].kind, "move");
            assert_eq!(node.children[0].path, "/new.bin");
            assert_eq!(node.children[0].source_path.as_deref(), Some("/old.bin"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn move_detector_ignores_non_matching_hashes() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![
        DiffNode::new("remove", "file", "/old.bin")
            .with_detail("hash_left", serde_json::json!("aaa")),
        DiffNode::new("add", "file", "/new.bin")
            .with_detail("hash_right", serde_json::json!("bbb")),
    ]);

    let result = MoveDetector.transform(container, &ctx());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn move_detector_unchanged_without_adds_and_removes() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![DiffNode::new(
        "modify",
        "file",
        "/changed.txt",
    )]);

    let result = MoveDetector.transform(container, &ctx());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn move_detector_preserves_non_moved_children() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![
        DiffNode::new("remove", "file", "/moved_old.bin")
            .with_detail("hash_left", serde_json::json!("abc")),
        DiffNode::new("add", "file", "/moved_new.bin")
            .with_detail("hash_right", serde_json::json!("abc")),
        DiffNode::new("modify", "file", "/untouched.txt"),
        DiffNode::new("add", "file", "/truly_new.bin")
            .with_detail("hash_right", serde_json::json!("xyz")),
    ]);

    let result = MoveDetector.transform(container, &ctx());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(node.children.len(), 3, "1 move + 1 modify + 1 add");
            let kinds: Vec<&str> = node.children.iter().map(|c| c.kind.as_str()).collect();
            assert!(kinds.contains(&"move"));
            assert!(kinds.contains(&"modify"));
            assert!(kinds.contains(&"add"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn move_detector_matches_directory_type() {
    assert!(MoveDetector.match_types().contains(&"directory"));
    assert!(MoveDetector.match_types().contains(&"zip_archive"));
    assert_eq!(MoveDetector.scope(), TransformScope::Subtree);
}

// ── Copy detector ──────────────────────────────────────────────────

#[test]
fn copy_detector_detects_add_matching_identical() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![
        DiffNode::new("identical", "file", "/original.bin")
            .with_detail("hash", serde_json::json!("abc123")),
        DiffNode::new("add", "file", "/duplicate.bin")
            .with_detail("hash_right", serde_json::json!("abc123")),
    ]);

    let result = CopyDetector.transform(container, &ctx());
    match result {
        TransformResult::Replace(node) => {
            let copy = node.children.iter().find(|c| c.kind == "copy");
            assert!(
                copy.is_some(),
                "should have a copy node, got: {:?}",
                node.children.iter().map(|c| &c.kind).collect::<Vec<_>>()
            );
            let copy = copy.unwrap();
            assert_eq!(copy.path, "/duplicate.bin");
            assert_eq!(copy.source_path.as_deref(), Some("/original.bin"));
            assert!(copy.tags.contains("binoc.copy"));
            let identical = node.children.iter().find(|c| c.kind == "identical");
            assert!(identical.is_some(), "identical node should be preserved");
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn copy_detector_unchanged_without_identicals() {
    let container = DiffNode::new("modify", "directory", "/")
        .with_children(vec![DiffNode::new("add", "file", "/new.bin")
            .with_detail("hash_right", serde_json::json!("abc123"))]);

    let result = CopyDetector.transform(container, &ctx());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn copy_detector_unchanged_when_hashes_differ() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![
        DiffNode::new("identical", "file", "/original.bin")
            .with_detail("hash", serde_json::json!("aaa")),
        DiffNode::new("add", "file", "/new.bin")
            .with_detail("hash_right", serde_json::json!("bbb")),
    ]);

    let result = CopyDetector.transform(container, &ctx());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn copy_detector_preserves_non_copy_children() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![
        DiffNode::new("identical", "file", "/source.bin")
            .with_detail("hash", serde_json::json!("abc")),
        DiffNode::new("add", "file", "/copied.bin")
            .with_detail("hash_right", serde_json::json!("abc")),
        DiffNode::new("modify", "file", "/changed.txt"),
        DiffNode::new("add", "file", "/truly_new.bin")
            .with_detail("hash_right", serde_json::json!("xyz")),
    ]);

    let result = CopyDetector.transform(container, &ctx());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(
                node.children.len(),
                4,
                "1 copy + 1 identical + 1 modify + 1 add"
            );
            let kinds: Vec<&str> = node.children.iter().map(|c| c.kind.as_str()).collect();
            assert!(kinds.contains(&"copy"));
            assert!(kinds.contains(&"identical"));
            assert!(kinds.contains(&"modify"));
            assert!(kinds.contains(&"add"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn copy_detector_matches_directory_type() {
    assert!(CopyDetector.match_types().contains(&"directory"));
    assert!(CopyDetector.match_types().contains(&"zip_archive"));
    assert_eq!(CopyDetector.scope(), TransformScope::Subtree);
}

// ── Column reorder detector ────────────────────────────────────────

#[test]
fn column_reorder_converts_pure_reorder() {
    let node = DiffNode::new("modify", "tabular", "data.csv")
        .with_tag("binoc.column-reorder")
        .with_detail("columns_added", serde_json::json!([]))
        .with_detail("columns_removed", serde_json::json!([]))
        .with_detail("rows_added", serde_json::json!(0))
        .with_detail("rows_removed", serde_json::json!(0))
        .with_detail("cells_changed", serde_json::json!(0));

    let result = ColumnReorderDetector.transform(node, &ctx());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(node.kind, "reorder");
            assert!(node.tags.contains("binoc.column-reorder"));
            assert_eq!(node.tags.len(), 1);
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn column_reorder_unchanged_when_other_changes_present() {
    let node = DiffNode::new("modify", "tabular", "data.csv")
        .with_tag("binoc.column-reorder")
        .with_tag("binoc.row-addition")
        .with_detail("columns_added", serde_json::json!([]))
        .with_detail("columns_removed", serde_json::json!([]))
        .with_detail("rows_added", serde_json::json!(5))
        .with_detail("rows_removed", serde_json::json!(0))
        .with_detail("cells_changed", serde_json::json!(0));

    let result = ColumnReorderDetector.transform(node, &ctx());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn column_reorder_unchanged_without_reorder_tag() {
    let node = DiffNode::new("modify", "tabular", "data.csv").with_tag("binoc.row-addition");

    let result = ColumnReorderDetector.transform(node, &ctx());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn column_reorder_matches_tabular_type() {
    assert!(ColumnReorderDetector.match_types().contains(&"tabular"));
    assert_eq!(ColumnReorderDetector.scope(), TransformScope::Node);
}
