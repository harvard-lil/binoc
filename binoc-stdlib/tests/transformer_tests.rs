use binoc_core::data_access::LocalDataAccess;
use binoc_sdk::*;

use binoc_stdlib::transformers::column_reorder::ColumnReorderDetector;
use binoc_stdlib::transformers::copy_detector::CopyDetector;
use binoc_stdlib::transformers::move_detector::MoveDetector;
use binoc_stdlib::transformers::tabular_analyzer::TabularAnalyzer;

fn da() -> LocalDataAccess {
    LocalDataAccess::new()
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

    let result = MoveDetector.transform(container, &da());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(node.children.len(), 1);
            assert_eq!(node.children[0].action, "move");
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

    let result = MoveDetector.transform(container, &da());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn move_detector_unchanged_without_adds_and_removes() {
    let container = DiffNode::new("modify", "directory", "/").with_children(vec![DiffNode::new(
        "modify",
        "file",
        "/changed.txt",
    )]);

    let result = MoveDetector.transform(container, &da());
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

    let result = MoveDetector.transform(container, &da());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(node.children.len(), 3, "1 move + 1 modify + 1 add");
            let kinds: Vec<&str> = node.children.iter().map(|c| c.action.as_str()).collect();
            assert!(kinds.contains(&"move"));
            assert!(kinds.contains(&"modify"));
            assert!(kinds.contains(&"add"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn move_detector_descriptor() {
    let desc = MoveDetector.descriptor();
    assert!(desc.match_types.is_empty());
    assert_eq!(desc.node_shape, NodeShapeFilter::Container);
    assert_eq!(desc.scope, TransformScope::Subtree);
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

    let result = CopyDetector.transform(container, &da());
    match result {
        TransformResult::Replace(node) => {
            let copy = node.children.iter().find(|c| c.action == "copy");
            assert!(
                copy.is_some(),
                "should have a copy node, got: {:?}",
                node.children.iter().map(|c| &c.action).collect::<Vec<_>>()
            );
            let copy = copy.unwrap();
            assert_eq!(copy.path, "/duplicate.bin");
            assert_eq!(copy.source_path.as_deref(), Some("/original.bin"));
            assert!(copy.tags.contains("binoc.copy"));
            let identical = node.children.iter().find(|c| c.action == "identical");
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

    let result = CopyDetector.transform(container, &da());
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

    let result = CopyDetector.transform(container, &da());
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

    let result = CopyDetector.transform(container, &da());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(
                node.children.len(),
                4,
                "1 copy + 1 identical + 1 modify + 1 add"
            );
            let kinds: Vec<&str> = node.children.iter().map(|c| c.action.as_str()).collect();
            assert!(kinds.contains(&"copy"));
            assert!(kinds.contains(&"identical"));
            assert!(kinds.contains(&"modify"));
            assert!(kinds.contains(&"add"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn copy_detector_descriptor() {
    let desc = CopyDetector.descriptor();
    assert!(desc.match_types.is_empty());
    assert_eq!(desc.node_shape, NodeShapeFilter::Container);
    assert_eq!(desc.scope, TransformScope::Subtree);
}

// ── Column reorder detector ────────────────────────────────────────

#[test]
fn column_reorder_unchanged_without_artifacts() {
    let node = DiffNode::new("modify", "tabular", "data.csv").with_tag("binoc.column-reorder");

    let result = ColumnReorderDetector.transform(node, &da());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn column_reorder_descriptor() {
    let desc = ColumnReorderDetector.descriptor();
    assert!(desc.match_types.is_empty());
    assert_eq!(desc.match_tags, vec!["binoc.column-reorder".to_string()]);
    assert_eq!(desc.match_artifacts, vec![tabular_v1()]);
    assert_eq!(desc.scope, TransformScope::Node);
}

// ── Tabular analyzer ───────────────────────────────────────────────

fn publish_and_attach(
    data: &LocalDataAccess,
    node: DiffNode,
    left: Option<&TabularData>,
    right: Option<&TabularData>,
) -> DiffNode {
    let mut node = node;
    if let Some(l) = left {
        let bytes = serde_json::to_vec(l).unwrap();
        let desc = data
            .publish_artifact(&tabular_v1(), ArtifactSubject::Left, "test", &bytes)
            .unwrap();
        node = node.with_artifact(desc);
    }
    if let Some(r) = right {
        let bytes = serde_json::to_vec(r).unwrap();
        let desc = data
            .publish_artifact(&tabular_v1(), ArtifactSubject::Right, "test", &bytes)
            .unwrap();
        node = node.with_artifact(desc);
    }
    node
}

#[test]
fn tabular_analyzer_detects_cell_changes() {
    let data = da();
    let left = TabularData {
        headers: vec!["name".into(), "score".into()],
        rows: vec![
            vec!["Alice".into(), "85".into()],
            vec!["Bob".into(), "90".into()],
        ],
    };
    let right = TabularData {
        headers: vec!["name".into(), "score".into()],
        rows: vec![
            vec!["Alice".into(), "92".into()],
            vec!["Bob".into(), "88".into()],
        ],
    };
    let node = publish_and_attach(
        &data,
        DiffNode::new("modify", "tabular", "data.csv"),
        Some(&left),
        Some(&right),
    );

    match TabularAnalyzer.transform(node, &data) {
        TransformResult::Replace(n) => {
            assert!(n.tags.contains("binoc.cell-change"));
            assert_eq!(n.details["cells_changed"], 2);
            assert!(n.summary.as_ref().unwrap().contains("2 cells changed"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn tabular_analyzer_detects_column_addition() {
    let data = da();
    let left = TabularData {
        headers: vec!["name".into(), "age".into()],
        rows: vec![vec!["Alice".into(), "30".into()]],
    };
    let right = TabularData {
        headers: vec!["name".into(), "age".into(), "email".into()],
        rows: vec![vec!["Alice".into(), "30".into(), "a@test.com".into()]],
    };
    let node = publish_and_attach(
        &data,
        DiffNode::new("modify", "tabular", "data.csv"),
        Some(&left),
        Some(&right),
    );

    match TabularAnalyzer.transform(node, &data) {
        TransformResult::Replace(n) => {
            assert!(n.tags.contains("binoc.column-addition"));
            assert!(n.tags.contains("binoc.schema-change"));
            assert_eq!(n.details["columns_added"], serde_json::json!(["email"]));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn tabular_analyzer_detects_row_addition() {
    let data = da();
    let left = TabularData {
        headers: vec!["name".into(), "age".into()],
        rows: vec![vec!["Alice".into(), "30".into()]],
    };
    let right = TabularData {
        headers: vec!["name".into(), "age".into()],
        rows: vec![
            vec!["Alice".into(), "30".into()],
            vec!["Bob".into(), "25".into()],
        ],
    };
    let node = publish_and_attach(
        &data,
        DiffNode::new("modify", "tabular", "data.csv"),
        Some(&left),
        Some(&right),
    );

    match TabularAnalyzer.transform(node, &data) {
        TransformResult::Replace(n) => {
            assert!(n.tags.contains("binoc.row-addition"));
            assert_eq!(n.details["rows_added"], 1);
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn tabular_analyzer_handles_add_action() {
    let data = da();
    let right = TabularData {
        headers: vec!["name".into(), "age".into()],
        rows: vec![
            vec!["Alice".into(), "30".into()],
            vec!["Bob".into(), "25".into()],
        ],
    };
    let node = publish_and_attach(
        &data,
        DiffNode::new("add", "tabular", "data.csv"),
        None,
        Some(&right),
    );

    match TabularAnalyzer.transform(node, &data) {
        TransformResult::Replace(n) => {
            assert!(n.tags.contains("binoc.content-changed"));
            assert!(n.summary.as_ref().unwrap().contains("2 columns"));
            assert!(n.summary.as_ref().unwrap().contains("2 rows"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn tabular_analyzer_handles_remove_action() {
    let data = da();
    let left = TabularData {
        headers: vec!["x".into(), "y".into(), "z".into()],
        rows: vec![vec!["1".into(), "2".into(), "3".into()]],
    };
    let node = publish_and_attach(
        &data,
        DiffNode::new("remove", "tabular", "data.csv"),
        Some(&left),
        None,
    );

    match TabularAnalyzer.transform(node, &data) {
        TransformResult::Replace(n) => {
            assert!(n.tags.contains("binoc.content-changed"));
            assert!(n.summary.as_ref().unwrap().contains("3 columns"));
            assert!(n.summary.as_ref().unwrap().contains("1 row"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn tabular_analyzer_unchanged_without_artifacts() {
    let node = DiffNode::new("modify", "tabular", "data.csv");
    let result = TabularAnalyzer.transform(node, &da());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn tabular_analyzer_descriptor() {
    let desc = TabularAnalyzer.descriptor();
    assert_eq!(desc.name, "binoc.tabular_analyzer");
    assert_eq!(desc.match_artifacts, vec![tabular_v1()]);
    assert_eq!(desc.scope, TransformScope::Node);
}
