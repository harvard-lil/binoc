use binoc_core::data_access::LocalDataAccess;
use binoc_sdk::*;

use binoc_stdlib::transformers::column_reorder::ColumnReorderDetector;
use binoc_stdlib::transformers::correlation_detector::CorrelationDetector;
use binoc_stdlib::transformers::folder_move_detector::FolderMoveDetector;
use binoc_stdlib::transformers::tabular_analyzer::TabularAnalyzer;

fn da() -> LocalDataAccess {
    LocalDataAccess::new()
}

fn null_cfg() -> serde_json::Value {
    serde_json::Value::Null
}

// ── Correlation detector (leaf-level move + copy) ─────────────────────

#[test]
fn correlation_detector_collapses_matching_add_remove() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![
        DiffNode::new("remove", "file", "old.bin")
            .with_detail("hash_left", serde_json::json!("abc123")),
        DiffNode::new("add", "file", "new.bin")
            .with_detail("hash_right", serde_json::json!("abc123")),
    ]);

    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
    match result {
        TransformResult::Replace(node) => {
            assert_eq!(node.children.len(), 1);
            assert_eq!(node.children[0].action, "move");
            assert_eq!(node.children[0].path, "new.bin");
            assert_eq!(node.children[0].source_path.as_deref(), Some("old.bin"));
            assert!(node.children[0].tags.contains("binoc.move"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn correlation_detector_crosses_container_boundaries() {
    // Leaves correlated across distinct parent containers.
    let root = DiffNode::new("modify", "directory", "").with_children(vec![
        // Root-level remove.
        DiffNode::new("remove", "file", "alpha.txt")
            .with_detail("hash_left", serde_json::json!("h_alpha")),
        // Sub-container with an add at a different path.
        DiffNode::new("modify", "directory", "sub").with_children(vec![DiffNode::new(
            "add",
            "file",
            "sub/alpha-renamed.txt",
        )
        .with_detail("hash_right", serde_json::json!("h_alpha"))]),
    ]);

    let result = CorrelationDetector.transform(root, &da(), &null_cfg());
    match result {
        TransformResult::Replace(root) => {
            // Root no longer has alpha.txt as a remove.
            assert!(!root.children.iter().any(|c| c.action == "remove"));
            // The sub container has the move.
            let sub = root
                .children
                .iter()
                .find(|c| c.path == "sub")
                .expect("sub survives");
            let move_node = sub
                .children
                .iter()
                .find(|c| c.action == "move")
                .expect("move node inserted under sub");
            assert_eq!(move_node.path, "sub/alpha-renamed.txt");
            assert_eq!(move_node.source_path.as_deref(), Some("alpha.txt"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn correlation_detector_ignores_non_matching_hashes() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![
        DiffNode::new("remove", "file", "old.bin")
            .with_detail("hash_left", serde_json::json!("aaa")),
        DiffNode::new("add", "file", "new.bin").with_detail("hash_right", serde_json::json!("bbb")),
    ]);

    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn correlation_detector_unchanged_without_adds_and_removes() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![DiffNode::new(
        "modify",
        "file",
        "changed.txt",
    )]);
    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn correlation_detector_preserves_non_moved_children() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![
        DiffNode::new("remove", "file", "moved_old.bin")
            .with_detail("hash_left", serde_json::json!("abc")),
        DiffNode::new("add", "file", "moved_new.bin")
            .with_detail("hash_right", serde_json::json!("abc")),
        DiffNode::new("modify", "file", "untouched.txt"),
        DiffNode::new("add", "file", "truly_new.bin")
            .with_detail("hash_right", serde_json::json!("xyz")),
    ]);

    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
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
fn correlation_detector_copy_from_identical() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![
        DiffNode::new("identical", "file", "original.bin")
            .with_detail("hash", serde_json::json!("abc123")),
        DiffNode::new("add", "file", "duplicate.bin")
            .with_detail("hash_right", serde_json::json!("abc123")),
    ]);

    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
    match result {
        TransformResult::Replace(node) => {
            let copy = node
                .children
                .iter()
                .find(|c| c.action == "copy")
                .expect("copy node emitted");
            assert_eq!(copy.path, "duplicate.bin");
            assert_eq!(copy.source_path.as_deref(), Some("original.bin"));
            assert!(copy.tags.contains("binoc.copy"));
            assert!(
                node.children.iter().any(|c| c.action == "identical"),
                "identical source retained"
            );
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn correlation_detector_aggregates_one_to_many_copy() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![
        DiffNode::new("identical", "file", "source.bin")
            .with_detail("hash", serde_json::json!("H")),
        DiffNode::new("add", "file", "copy_a.bin")
            .with_detail("hash_right", serde_json::json!("H")),
        DiffNode::new("add", "file", "copy_b.bin")
            .with_detail("hash_right", serde_json::json!("H")),
    ]);
    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
    match result {
        TransformResult::Replace(node) => {
            let copy = node
                .children
                .iter()
                .find(|c| c.action == "copy")
                .expect("one aggregated copy node");
            let dests = copy
                .details
                .get("destinations")
                .expect("destinations present");
            assert_eq!(dests.as_array().unwrap().len(), 2);
            assert!(copy.summary.as_deref().unwrap().contains("copy_a.bin"));
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn correlation_detector_aggregates_many_to_one_move() {
    let container = DiffNode::new("modify", "directory", "").with_children(vec![
        DiffNode::new("remove", "file", "a.bin").with_detail("hash_left", serde_json::json!("H")),
        DiffNode::new("remove", "file", "b.bin").with_detail("hash_left", serde_json::json!("H")),
        DiffNode::new("add", "file", "merged.bin")
            .with_detail("hash_right", serde_json::json!("H")),
    ]);
    let result = CorrelationDetector.transform(container, &da(), &null_cfg());
    match result {
        TransformResult::Replace(node) => {
            let mv = node
                .children
                .iter()
                .find(|c| c.action == "move")
                .expect("single aggregated move");
            assert_eq!(mv.path, "merged.bin");
            let sources = mv.details.get("sources").expect("sources present");
            assert_eq!(sources.as_array().unwrap().len(), 2);
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn correlation_detector_descriptor() {
    let desc = CorrelationDetector.descriptor();
    assert_eq!(desc.name, "binoc.correlation_detector");
    assert_eq!(desc.node_shape, NodeShapeFilter::Root);
}

// ── Hydration invariant: hash is resolved from source_items if missing ─

#[test]
fn correlation_detector_hydrates_missing_hashes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let phys_left = tmp.path().join("old_name.bin");
    let phys_right = tmp.path().join("new_name.bin");
    let bytes = b"identical payload across rename".to_vec();
    std::fs::write(&phys_left, &bytes).unwrap();
    std::fs::write(&phys_right, &bytes).unwrap();

    let data = LocalDataAccess::new();
    let left = data
        .register_local(&phys_left, "old_name.bin")
        .expect("register left");
    let right = data
        .register_local(&phys_right, "new_name.bin")
        .expect("register right");

    assert!(
        left.content_hash.is_none() && right.content_hash.is_none(),
        "fixture precondition: register_local must leave content_hash None"
    );

    let remove =
        DiffNode::new("remove", "file", "old_name.bin").with_source_items(ItemPair::removed(left));
    let add =
        DiffNode::new("add", "file", "new_name.bin").with_source_items(ItemPair::added(right));
    let container = DiffNode::new("modify", "directory", "").with_children(vec![remove, add]);

    let result = CorrelationDetector.transform(container, &data, &null_cfg());
    let TransformResult::Replace(node) = result else {
        panic!("expected Replace variant");
    };
    let moved = node
        .children
        .iter()
        .find(|c| c.action == "move")
        .expect("move node");
    assert_eq!(moved.path, "new_name.bin");
    assert_eq!(moved.source_path.as_deref(), Some("old_name.bin"));
}

#[test]
fn correlation_detector_details_take_precedence_over_hydration() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let phys_left = tmp.path().join("old.bin");
    let phys_right = tmp.path().join("new.bin");
    std::fs::write(&phys_left, b"X").unwrap();
    std::fs::write(&phys_right, b"X").unwrap();

    let data = LocalDataAccess::new();
    let left = data.register_local(&phys_left, "old.bin").unwrap();
    let right = data.register_local(&phys_right, "new.bin").unwrap();

    let mut remove =
        DiffNode::new("remove", "file", "old.bin").with_source_items(ItemPair::removed(left));
    remove
        .details
        .insert("hash_left".into(), serde_json::json!("AAA"));
    let mut add = DiffNode::new("add", "file", "new.bin").with_source_items(ItemPair::added(right));
    add.details
        .insert("hash_right".into(), serde_json::json!("BBB"));
    let container = DiffNode::new("modify", "directory", "").with_children(vec![remove, add]);

    let result = CorrelationDetector.transform(container, &data, &null_cfg());
    // Mismatched hashes ⇒ no correlation ⇒ Unchanged.
    assert!(
        matches!(result, TransformResult::Unchanged),
        "expected Unchanged when details contain mismatched hashes"
    );
}

// ── Folder-move detector ───────────────────────────────────────────────

#[test]
fn folder_move_rolls_up_whole_directory_rename() {
    // Pre-correlated tree: "docs/" (removed) and "documentation/" (added
    // with everything as move nodes from docs/).
    let dst = DiffNode::new("add", "directory", "documentation").with_children(vec![
        DiffNode::new("move", "file", "documentation/a.txt")
            .with_source_path("docs/a.txt")
            .with_tag("binoc.move"),
        DiffNode::new("move", "file", "documentation/b.txt")
            .with_source_path("docs/b.txt")
            .with_tag("binoc.move"),
    ]);
    let src_empty = DiffNode::new("remove", "directory", "docs");

    let root = DiffNode::new("modify", "directory", "").with_children(vec![src_empty, dst]);
    let result = FolderMoveDetector.transform(root, &da(), &null_cfg());
    match result {
        TransformResult::Replace(root) => {
            assert_eq!(root.children.len(), 1, "source container removed");
            let folded = &root.children[0];
            assert_eq!(folded.path, "documentation");
            assert_eq!(folded.action, "move");
            assert_eq!(folded.source_path.as_deref(), Some("docs"));
            assert!(folded.tags.contains("binoc.move"));
            assert!(folded.tags.contains("binoc.folder-move"));
            assert!(
                folded.children.is_empty(),
                "strict rollup collapses children"
            );
        }
        _ => panic!("Expected Replace"),
    }
}

#[test]
fn folder_move_handles_nested_subfolders() {
    // docs/reports/quarterly/q1.txt → documentation/reports/quarterly/q1.txt
    let quarterly =
        DiffNode::new("add", "directory", "documentation/reports/quarterly").with_children(vec![
            DiffNode::new("move", "file", "documentation/reports/quarterly/q1.txt")
                .with_source_path("docs/reports/quarterly/q1.txt")
                .with_tag("binoc.move"),
        ]);
    let reports =
        DiffNode::new("add", "directory", "documentation/reports").with_children(vec![quarterly]);
    let dst = DiffNode::new("add", "directory", "documentation").with_children(vec![reports]);
    let src = DiffNode::new("remove", "directory", "docs");

    let root = DiffNode::new("modify", "directory", "").with_children(vec![src, dst]);
    let TransformResult::Replace(root) = FolderMoveDetector.transform(root, &da(), &null_cfg())
    else {
        panic!("Expected Replace");
    };
    assert_eq!(root.children.len(), 1, "outermost rollup wins");
    let folded = &root.children[0];
    assert_eq!(folded.path, "documentation");
    assert_eq!(folded.source_path.as_deref(), Some("docs"));
    assert!(folded.tags.contains("binoc.folder-move"));
}

#[test]
fn folder_move_rejects_partial_under_strict_threshold() {
    let dst = DiffNode::new("add", "directory", "newdir").with_children(vec![
        DiffNode::new("move", "file", "newdir/a.txt")
            .with_source_path("olddir/a.txt")
            .with_tag("binoc.move"),
        // Unrelated add — breaks strict rollup.
        DiffNode::new("add", "file", "newdir/b.txt"),
    ]);
    let src = DiffNode::new("remove", "directory", "olddir");
    let root = DiffNode::new("modify", "directory", "").with_children(vec![src, dst]);
    let strict_cfg = serde_json::json!({ "threshold": 1.0 });
    let result = FolderMoveDetector.transform(root, &da(), &strict_cfg);
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn folder_move_rejects_inconsistent_sources() {
    let dst = DiffNode::new("add", "directory", "newdir").with_children(vec![
        DiffNode::new("move", "file", "newdir/a.txt")
            .with_source_path("olddir/a.txt")
            .with_tag("binoc.move"),
        DiffNode::new("move", "file", "newdir/b.txt")
            .with_source_path("otherdir/b.txt")
            .with_tag("binoc.move"),
    ]);
    let root = DiffNode::new("modify", "directory", "").with_children(vec![dst]);
    let result = FolderMoveDetector.transform(root, &da(), &null_cfg());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn folder_move_rolls_up_partial_rename_and_keeps_remainders() {
    let mut moved_modified = DiffNode::new("move", "file", "newdir/changed.txt")
        .with_source_path("olddir/changed.txt")
        .with_summary("Moved from changed.txt (modified)")
        .with_tag("binoc.move")
        .with_tag("binoc.move.modified")
        .with_tag("binoc.content-changed");
    moved_modified
        .annotations
        .insert("content_summary".into(), serde_json::json!("2 lines added"));

    let dst = DiffNode::new("add", "directory", "newdir").with_children(vec![
        DiffNode::new("move", "file", "newdir/a.txt")
            .with_source_path("olddir/a.txt")
            .with_tag("binoc.move"),
        DiffNode::new("move", "file", "newdir/keep/b.txt")
            .with_source_path("olddir/keep/b.txt")
            .with_tag("binoc.move"),
        moved_modified,
        DiffNode::new("add", "file", "newdir/added.txt")
            .with_summary("New file")
            .with_tag("binoc.content-changed"),
    ]);
    let src = DiffNode::new("remove", "directory", "olddir").with_children(vec![DiffNode::new(
        "remove",
        "file",
        "olddir/removed.txt",
    )
    .with_summary("File removed")
    .with_tag("binoc.content-changed")]);
    let root = DiffNode::new("modify", "directory", "").with_children(vec![src, dst]);

    let cfg = serde_json::json!({ "threshold": 0.5 });
    let TransformResult::Replace(root) = FolderMoveDetector.transform(root, &da(), &cfg) else {
        panic!("Expected Replace");
    };

    assert_eq!(root.children.len(), 1, "source container removed");
    let folded = &root.children[0];
    assert_eq!(folded.action, "move");
    assert_eq!(folded.path, "newdir");
    assert_eq!(folded.source_path.as_deref(), Some("olddir"));
    assert_eq!(folded.children.len(), 3, "only remainder nodes survive");

    let added = folded
        .children
        .iter()
        .find(|c| c.path == "newdir/added.txt")
        .expect("added remainder");
    assert_eq!(added.action, "add");

    let modified = folded
        .children
        .iter()
        .find(|c| c.path == "newdir/changed.txt")
        .expect("modified remainder");
    assert_eq!(modified.action, "modify");
    assert_eq!(modified.source_path, None);
    assert_eq!(modified.summary.as_deref(), Some("2 lines added"));
    assert!(!modified.tags.contains("binoc.move"));

    let removed = folded
        .children
        .iter()
        .find(|c| c.path == "newdir/removed.txt")
        .expect("relocated remove remainder");
    assert_eq!(removed.action, "remove");

    assert!(
        folded
            .children
            .iter()
            .all(|c| c.path != "newdir/a.txt" && c.path != "newdir/keep/b.txt"),
        "clean moved descendants are suppressed beneath the folder move"
    );
}

#[test]
fn folder_move_descriptor() {
    let desc = FolderMoveDetector.descriptor();
    assert_eq!(desc.name, "binoc.folder_move_detector");
    assert_eq!(desc.node_shape, NodeShapeFilter::Root);
}

// ── Column reorder detector ────────────────────────────────────────────

#[test]
fn column_reorder_unchanged_without_artifacts() {
    let node = DiffNode::new("modify", "tabular", "data.csv").with_tag("binoc.column-reorder");

    let result = ColumnReorderDetector.transform(node, &da(), &null_cfg());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn column_reorder_descriptor() {
    let desc = ColumnReorderDetector.descriptor();
    assert!(desc.match_types.is_empty());
    assert_eq!(desc.match_tags, vec!["binoc.column-reorder".to_string()]);
    assert_eq!(desc.match_artifacts, vec![tabular_v1()]);
}

// ── Tabular analyzer ───────────────────────────────────────────────────

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

    match TabularAnalyzer.transform(node, &data, &null_cfg()) {
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

    match TabularAnalyzer.transform(node, &data, &null_cfg()) {
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

    match TabularAnalyzer.transform(node, &data, &null_cfg()) {
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

    match TabularAnalyzer.transform(node, &data, &null_cfg()) {
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

    match TabularAnalyzer.transform(node, &data, &null_cfg()) {
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
    let result = TabularAnalyzer.transform(node, &da(), &null_cfg());
    assert!(matches!(result, TransformResult::Unchanged));
}

#[test]
fn tabular_analyzer_descriptor() {
    let desc = TabularAnalyzer.descriptor();
    assert_eq!(desc.name, "binoc.tabular_analyzer");
    assert_eq!(desc.match_artifacts, vec![tabular_v1()]);
}
