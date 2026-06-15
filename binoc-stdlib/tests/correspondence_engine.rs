use std::fs;
use std::io::Write;
use std::sync::Arc;

use binoc_core::controller::Controller;
use binoc_core::correspondence::{self, ActionLine, CorrespondenceRunResult, Projection};
use binoc_sdk::{
    BinocResult, CoreRule, CorrespondenceEngineConfig, DataAccess, DiffNode, Edit, EditListWriter,
    EngineView, LinkCtx, LinkProposal, NodeMatch, PairDescriptor, PairOutput, PairRule,
    ProjectionHint, ShapeFilter, TreeSide, WriteOutput, WriterDescriptor,
};
use binoc_stdlib::correspondence::{default_engine_config, expand, pair, parse, writers};
use binoc_stdlib::test_vectors::{
    check_changeset_invariants, materialize_snapshots, stdlib_materializers, VectorMaterializer,
};

fn diff_with_correspondence(left: &std::path::Path, right: &std::path::Path) -> DiffNode {
    let controller = Controller::new(default_engine_config());
    controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff")
        .root
        .expect("root")
}

fn diff_with_config(
    left: &std::path::Path,
    right: &std::path::Path,
    config: CorrespondenceEngineConfig,
) -> binoc_sdk::Changeset {
    let controller = Controller::new(config);
    controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff")
}

fn materialized_vector(name: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let vectors_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-vectors");
    let vector = vectors_root.join(name);
    let temp = tempfile::tempdir().expect("tempdir");
    let materialized = temp.path().join(name);
    let materializers = stdlib_materializers();
    let materializer_refs: Vec<&dyn VectorMaterializer> = materializers
        .iter()
        .map(|materializer| &**materializer)
        .collect();
    materialize_snapshots(&vector, &materialized, &materializer_refs);
    let left = materialized.join("snapshot-a");
    let right = materialized.join("snapshot-b");
    (temp, left, right)
}

fn run_engine(
    left: &std::path::Path,
    right: &std::path::Path,
    config: &CorrespondenceEngineConfig,
) -> CorrespondenceRunResult {
    let data = binoc_sdk::LocalDataAccess::new();
    let left_root = data.register_local(left, "").expect("left root");
    let right_root = data.register_local(right, "").expect("right root");
    correspondence::driver::run(config, left_root, right_root, &data).expect("engine run")
}

fn find<'a>(node: &'a DiffNode, path: &str) -> Option<&'a DiffNode> {
    if node.path == path {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, path))
}

fn changed(projection: &Projection) -> Vec<&ActionLine> {
    projection.changed().collect()
}

fn find_line<'a>(projection: &'a Projection, action: &str, path: &str) -> &'a ActionLine {
    projection
        .lines
        .iter()
        .find(|line| line.action == action && line.path == path)
        .unwrap_or_else(|| {
            panic!(
                "expected `{action} {path}` in projection:\n{}",
                projection.render_text()
            )
        })
}

#[test]
fn settled_archive_link_short_circuits_expansion_and_parse() {
    let (_guard, left, right) = materialized_vector("zip-rename-identical");
    let config = binoc_stdlib::correspondence::engine_config_with_options(
        binoc_stdlib::correspondence::CorrespondenceOptions {
            expand_renamed_unchanged_collections: false,
        },
    );

    let result = run_engine(&left, &right, &config);
    let projection = result.project();
    let lines = changed(&projection);

    assert_eq!(lines.len(), 1, "projection:\n{}", projection.render_text());
    let archive = find_line(&projection, "move", "archive.zip");
    assert_eq!(archive.source_path.as_deref(), Some("data.zip"));
    assert_eq!(archive.evidence.as_deref(), Some("binoc.pair.hash"));

    assert_eq!(result.stats.fires_of("binoc.expand.zip"), 0);
    assert_eq!(result.stats.invocations_of("binoc.parse.csv"), 0);
    assert!(result.stats.suppressed_of("binoc.expand.zip") >= 2);
    assert!(
        result.stats.fires_beneath_settled.is_empty(),
        "no rule should fire beneath settled links: {:?}",
        result.stats.fires_beneath_settled
    );
}

#[test]
fn late_fuzzy_link_triggers_parse_without_add_remove_fallout() {
    let (_guard, left, right) = materialized_vector("zip-rename-inner-rename-edit");
    let result = run_engine(&left, &right, &default_engine_config());
    let projection = result.project();

    let lines = changed(&projection);
    assert!(
        lines.iter().all(|line| line.action == "move"),
        "projection should contain only move claims:\n{}",
        projection.render_text()
    );

    let archive = find_line(&projection, "move", "archive.zip");
    assert_eq!(archive.source_path.as_deref(), Some("data.zip"));
    assert_eq!(
        archive.evidence.as_deref(),
        Some("binoc.pair.container_from_children")
    );

    let csv = find_line(&projection, "move", "archive.zip/new.csv");
    assert_eq!(csv.source_path.as_deref(), Some("data.zip/old.csv"));
    assert_eq!(csv.evidence.as_deref(), Some("binoc.pair.fuzzy"));
    assert_eq!(csv.verbs, vec!["tabular.edit_cell"]);
    assert_eq!(csv.edits[0].params["column"], serde_json::json!("score"));

    let fuzzy_at = result
        .stats
        .events
        .iter()
        .position(|event| event.kind == "link-add" && event.rule == "binoc.pair.fuzzy")
        .expect("fuzzy link event");
    let parse_positions = result
        .stats
        .events
        .iter()
        .enumerate()
        .filter(|(_, event)| event.kind == "parse" && event.rule == "binoc.parse.csv")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(parse_positions.len(), 2);
    assert!(
        parse_positions.iter().all(|position| *position > fuzzy_at),
        "parse events should follow fuzzy link event: fuzzy={fuzzy_at}, parses={parse_positions:?}"
    );
}

#[test]
fn config_injected_pair_rule_improves_output_without_engine_awareness() {
    let (_guard, left, right) = materialized_vector("zip-rename-inner-rename-edit");

    let mut without = default_engine_config();
    without
        .rules
        .retain(|rule| rule.name() != "binoc.pair.container_from_children");
    let without_projection = run_engine(&left, &right, &without).project();
    let moved_file = find_line(&without_projection, "move", "archive.zip/new.csv");
    assert_eq!(moved_file.verbs, vec!["tabular.edit_cell"]);
    find_line(&without_projection, "remove", "data.zip");
    find_line(&without_projection, "add", "archive.zip");

    let mut with = without.clone();
    let fuzzy_position = with
        .rules
        .iter()
        .position(|rule| rule.name() == "binoc.pair.fuzzy")
        .expect("fuzzy rule");
    with.rules.insert(
        fuzzy_position + 1,
        CoreRule::Pair(Arc::new(ExtrasContainerFromChildEvidence)),
    );
    let with_projection = run_engine(&left, &right, &with).project();
    let archive = find_line(&with_projection, "move", "archive.zip");
    assert_eq!(
        archive.evidence.as_deref(),
        Some("extras.container_from_children")
    );
    assert!(changed(&with_projection).len() < changed(&without_projection).len());
}

#[test]
fn unknown_writer_verbs_flow_through_projection() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("notes.txt"), "hello\nworld\n").unwrap();
    fs::write(right.join("notes.txt"), "hello\nthere\ngeneral\n").unwrap();

    let mut config = default_engine_config();
    config.writers.insert(0, Arc::new(ExtrasFrobnicateWriter));
    let root = diff_with_config(&left, &right, config).root.expect("root");
    let node = find(&root, "notes.txt").expect("notes.txt");
    assert_eq!(node.action, "modify");
    assert_eq!(node.details["edits"][0]["verb"], "extras.frobnicate");
    assert_eq!(node.details["edits"][0]["params"]["left_lines"], 2);
    assert_eq!(node.details["edits"][0]["params"]["right_lines"], 3);
}

#[test]
fn pair_rule_order_controls_priority_and_output_deterministically() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("a.csv"), "id,word\n1,apple\n2,banana\n3,cherry\n").unwrap();
    fs::write(right.join("a.csv"), "k1,k2\n9,zulu\n8,yankee\n7,xray\n").unwrap();
    fs::write(right.join("b.csv"), "id,word\n1,apple\n2,banana\n3,grape\n").unwrap();

    let default_config = default_engine_config();
    let default_first = run_engine(&left, &right, &default_config);
    let default_second = run_engine(&left, &right, &default_config);
    assert_eq!(
        default_first.project().render_text(),
        default_second.project().render_text()
    );
    assert_eq!(default_first.stats.fires, default_second.stats.fires);
    assert_eq!(default_first.stats.rounds, default_second.stats.rounds);

    let default_projection = default_first.project();
    let same_name = find_line(&default_projection, "modify", "a.csv");
    assert_eq!(same_name.evidence.as_deref(), Some("binoc.pair.name"));
    find_line(&default_projection, "add", "b.csv");

    let mut permuted = default_engine_config();
    let name_idx = position_of_rule(&permuted, "binoc.pair.name");
    let fuzzy_idx = position_of_rule(&permuted, "binoc.pair.fuzzy");
    permuted.rules.swap(name_idx, fuzzy_idx);
    let permuted_run = run_engine(&left, &right, &permuted);
    let permuted_projection = permuted_run.project();
    let cross = find_line(&permuted_projection, "move", "b.csv");
    assert_eq!(cross.source_path.as_deref(), Some("a.csv"));
    assert_eq!(cross.evidence.as_deref(), Some("binoc.pair.fuzzy"));
    find_line(&permuted_projection, "add", "a.csv");

    assert!(
        default_first.stats.priorities["binoc.pair.name"]
            > default_first.stats.priorities["binoc.pair.fuzzy"]
    );
    assert!(
        permuted_run.stats.priorities["binoc.pair.fuzzy"]
            > permuted_run.stats.priorities["binoc.pair.name"]
    );
}

#[test]
fn correspondence_engine_reports_csv_cell_change() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("data.csv"), "id,value\n1,old\n").unwrap();
    fs::write(right.join("data.csv"), "id,value\n1,new\n").unwrap();

    let root = diff_with_correspondence(&left, &right);
    let node = find(&root, "data.csv").expect("data.csv node");
    assert_eq!(node.action, "modify");
    assert_eq!(node.item_type, "tabular");
    assert!(node.tags.contains("binoc.cell-change"));
    assert_eq!(
        node.details["edits"][0]["verb"],
        serde_json::json!("tabular.edit_cell")
    );
}

#[test]
fn lcs_row_alignment_compacts_mid_table_insertion_with_column_changes() {
    let vectors_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-vectors");
    let vector = vectors_root.join("csv-mid-row-insertion");
    let temp = tempfile::tempdir().expect("tempdir");
    let materialized = temp.path().join("csv-mid-row-insertion");
    let materializers = stdlib_materializers();
    let materializer_refs: Vec<&dyn VectorMaterializer> = materializers
        .iter()
        .map(|materializer| &**materializer)
        .collect();
    materialize_snapshots(&vector, &materialized, &materializer_refs);

    let root = diff_with_correspondence(
        &materialized.join("snapshot-a"),
        &materialized.join("snapshot-b"),
    );
    let node = find(&root, "data.csv").expect("data.csv node");
    assert!(node.tags.contains("binoc.row-addition"));
    assert!(node.tags.contains("binoc.column-reorder"));
    assert!(node.tags.contains("binoc.column-addition"));
    assert!(!node.tags.contains("binoc.cell-change"));

    let edits = node
        .details
        .get("edits")
        .and_then(|value| value.as_array())
        .expect("edits");
    let verbs: Vec<&str> = edits
        .iter()
        .map(|edit| edit["verb"].as_str().expect("verb"))
        .collect();
    assert_eq!(
        verbs,
        vec![
            "tabular.reorder_columns",
            "tabular.add_column",
            "tabular.add_row"
        ]
    );
    assert_eq!(edits[2]["params"]["index"], serde_json::json!(1));
}

#[test]
fn correspondence_engine_reports_copy_without_double_counting_container_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("original.txt"), "same\n").unwrap();
    fs::write(right.join("original.txt"), "same\n").unwrap();
    fs::write(right.join("duplicate.txt"), "same\n").unwrap();

    let root = diff_with_correspondence(&left, &right);
    let node = find(&root, "duplicate.txt").expect("copy node");
    assert_eq!(node.action, "copy");
    assert_eq!(node.source_path.as_deref(), Some("original.txt"));
    assert!(node.tags.contains("binoc.copy"));
    assert!(
        root.details.is_empty(),
        "hidden container add/remove edits should not appear as root details"
    );
}

#[test]
fn correspondence_engine_does_not_treat_root_metadata_as_projection_collision() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(right.join("added.txt"), "new\n").unwrap();

    let root = diff_with_correspondence(&left, &right);

    assert!(
        !root.details.contains_key("projection_line_count"),
        "root's own projected line should not be handled as a merge collision"
    );
    let added = find(&root, "added.txt").expect("added child");
    assert_eq!(added.action, "add");
}

#[test]
fn correspondence_engine_rolls_up_container_move_from_child_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(left.join("docs")).unwrap();
    fs::create_dir_all(right.join("documentation")).unwrap();
    fs::write(left.join("docs/readme.txt"), "same\n").unwrap();
    fs::write(right.join("documentation/readme.txt"), "same\n").unwrap();

    let root = diff_with_correspondence(&left, &right);
    let folder = find(&root, "documentation").expect("folder move");
    assert_eq!(folder.action, "move");
    assert_eq!(folder.source_path.as_deref(), Some("docs"));
    assert!(folder.tags.contains("binoc.move"));
    assert!(
        find(&root, "documentation/readme.txt").is_none(),
        "child path change should be carried by the container move"
    );
}

#[test]
fn correspondence_engine_default_expands_renamed_collection_and_keeps_copy_out() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(left.join("docs")).unwrap();
    fs::create_dir_all(right.join("documentation")).unwrap();
    fs::write(left.join("docs/original.txt"), "same\n").unwrap();
    fs::write(right.join("documentation/original.txt"), "same\n").unwrap();
    fs::write(right.join("duplicate.txt"), "same\n").unwrap();

    let root = diff_with_correspondence(&left, &right);
    let copy = find(&root, "duplicate.txt").expect("copy out");
    assert_eq!(copy.action, "copy");
    assert_eq!(copy.source_path.as_deref(), Some("docs/original.txt"));
}

#[test]
fn correspondence_engine_dataset_config_can_skip_renamed_unchanged_collection_expansion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    write_zip_with_entries(&left.join("data.zip"), &[("original.txt", b"same\n")]);
    write_zip_with_entries(&right.join("archive.zip"), &[("original.txt", b"same\n")]);

    let config =
        binoc_stdlib::correspondence::engine_config_for_dataset_config(&serde_json::json!({
            "correspondence": {
                "expand_renamed_unchanged_collections": false
            }
        }));
    let data = binoc_sdk::LocalDataAccess::new();
    let left_root = data.register_local(&left, "").expect("left root");
    let right_root = data.register_local(&right, "").expect("right root");
    let result = binoc_core::correspondence::driver::run(&config, left_root, right_root, &data)
        .expect("run");

    let settled_folder_link = result.store.links.iter().any(|(_, link)| {
        result.store.left.node(link.left).item.logical_path == "data.zip"
            && result.store.right.node(link.right).item.logical_path == "archive.zip"
            && link.settled
    });
    assert!(
        settled_folder_link,
        "fast config should settle renamed unchanged archive links"
    );
}

#[test]
fn fuzzy_candidate_limit_surfaces_diagnostic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("a.txt"), "alpha one\n").unwrap();
    fs::write(left.join("b.txt"), "beta two\n").unwrap();
    fs::write(right.join("c.txt"), "gamma three\n").unwrap();
    fs::write(right.join("d.txt"), "delta four\n").unwrap();

    let config = CorrespondenceEngineConfig {
        rules: vec![
            CoreRule::Expand(Arc::new(expand::DirectoryExpand)),
            CoreRule::Pair(Arc::new(pair::FuzzyPair {
                rename_limit: 1,
                ..Default::default()
            })),
            CoreRule::Pair(Arc::new(pair::RootPair)),
        ],
        writers: vec![Arc::new(writers::FallbackWriter)],
        compaction: vec![],
        annotators: vec![],
        row_keys: Default::default(),
        row_identity_policies: Default::default(),
        root_projection: ProjectionHint::default().item_type("directory"),
        dataset_configurator: None,
    };
    let changeset = diff_with_config(&left, &right, config);
    assert!(changeset
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "binoc.fuzzy_pair_limit"));
}

#[test]
fn unsafe_zip_entry_skip_surfaces_diagnostic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    write_zip_with_entries(&left.join("archive.zip"), &[("../evil.txt", b"bad")]);
    write_zip_with_entries(&right.join("archive.zip"), &[("../evil.txt", b"bad")]);

    let config = CorrespondenceEngineConfig {
        rules: vec![
            CoreRule::Expand(Arc::new(expand::DirectoryExpand)),
            CoreRule::Expand(Arc::new(expand::ZipExpand)),
            CoreRule::Pair(Arc::new(pair::RootPair)),
        ],
        writers: vec![Arc::new(writers::FallbackWriter)],
        compaction: vec![],
        annotators: vec![],
        row_keys: Default::default(),
        row_identity_policies: Default::default(),
        root_projection: ProjectionHint::default().item_type("directory"),
        dataset_configurator: None,
    };
    let changeset = diff_with_config(&left, &right, config);
    assert!(changeset
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "binoc.archive_entry_skipped"));
}

#[test]
fn keyed_row_exclusion_degrades_with_diagnostic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("data.csv"), "id,value\n1,a\n1,b\n").unwrap();
    fs::write(right.join("data.csv"), "id,value\n1,a\n1,c\n").unwrap();

    let mut row_keys = std::collections::BTreeMap::new();
    row_keys.insert("data.csv".to_string(), vec!["id".to_string()]);
    let config = CorrespondenceEngineConfig {
        rules: vec![
            CoreRule::Expand(Arc::new(expand::DirectoryExpand)),
            CoreRule::Pair(Arc::new(pair::RootPair)),
            CoreRule::Pair(Arc::new(pair::NameUnderPairedParent)),
            CoreRule::Parse(Arc::new(parse::CsvParse)),
        ],
        writers: vec![
            Arc::new(writers::TabularWriter),
            Arc::new(writers::FallbackWriter),
        ],
        compaction: vec![],
        annotators: vec![],
        row_keys,
        row_identity_policies: Default::default(),
        root_projection: ProjectionHint::default().item_type("directory"),
        dataset_configurator: None,
    };
    let changeset = diff_with_config(&left, &right, config);
    assert!(changeset
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "binoc.keyed_row_identity_degraded"));
    let root = changeset.root.expect("root");
    let node = find(&root, "data.csv").expect("data.csv");
    assert_eq!(node.action, "modify");
}

#[test]
fn keyed_row_degradation_tags_only_observed_failures() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("data.csv"), "id,value\n1,a\n1,b\n").unwrap();
    fs::write(right.join("data.csv"), "id,value\n1,a\n1,c\n").unwrap();

    let controller =
        Controller::new(default_engine_config()).with_dataset_config(serde_json::json!({
            "tables": [{ "path_regex": "^data\\.csv$", "columns": ["id"] }]
        }));
    let root = controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff")
        .root
        .expect("root");
    let node = find(&root, "data.csv").expect("data.csv");
    assert!(node.tags.contains("binoc.duplicate-key"));
    assert!(!node.tags.contains("binoc.null-key"));
}

#[test]
fn keyed_row_degradation_honors_failure_policies() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("data.csv"), "id,value\n,a\n").unwrap();
    fs::write(right.join("data.csv"), "id,value\n,b\n").unwrap();

    let ignored = Controller::new(default_engine_config())
        .with_dataset_config(serde_json::json!({
            "tables": [{
                "path_regex": "^data\\.csv$",
                "columns": ["id"],
                "on_null_key": "ignore"
            }]
        }))
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff");
    assert!(!ignored
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "binoc.keyed_row_identity_degraded"));
    let ignored_root = ignored.root.expect("root");
    let ignored_node = find(&ignored_root, "data.csv").expect("data.csv");
    assert!(!ignored_node.tags.contains("binoc.null-key"));

    let errored = Controller::new(default_engine_config())
        .with_dataset_config(serde_json::json!({
            "tables": [{
                "path_regex": "^data\\.csv$",
                "columns": ["id"],
                "on_null_key": "error"
            }]
        }))
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff");
    assert!(errored.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "binoc.keyed_row_identity_degraded"
            && diagnostic.severity == binoc_sdk::DiagnosticSeverity::Error
    }));
}

#[test]
fn correspondence_engine_uses_dataset_row_keys_on_production_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("data.csv"), "id,value\n1,a\n2,b\n").unwrap();
    fs::write(right.join("data.csv"), "id,value\n2,b\n1,c\n").unwrap();

    let controller =
        Controller::new(default_engine_config()).with_dataset_config(serde_json::json!({
            "tables": [{ "path_regex": "^data\\.csv$", "columns": ["id"] }]
        }));
    let root = controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff")
        .root
        .expect("root");
    let node = find(&root, "data.csv").expect("data.csv");
    assert_eq!(node.details["edits"][0]["params"]["key"]["id"], "1");
}

#[test]
fn correspondence_engine_resolves_declared_pairs_against_live_archive_view() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    write_zip_with_entries(
        &left.join("data.zip"),
        &[("state_old.csv", b"id,value\n1,old\n")],
    );
    write_zip_with_entries(
        &right.join("archive.zip"),
        &[("records.csv", b"id,value\n1,new\n")],
    );

    let controller =
        Controller::new(default_engine_config()).with_dataset_config(serde_json::json!({
            "files": {
                "correspondences": [{
                    "name": "archive-inner",
                    "key": "records",
                    "left": { "path_regex": "^data\\.zip/state_old\\.csv$" },
                    "right": { "path_regex": "^archive\\.zip/records\\.csv$" }
                }]
            }
        }));
    let root = controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff")
        .root
        .expect("root");
    let node = find(&root, "archive.zip/records.csv").expect("declared pair node");
    assert!(node.tags.contains("binoc.declared-correspondence"));
    assert_eq!(node.source_path.as_deref(), Some("data.zip/state_old.csv"));
}

#[test]
fn correspondence_engine_reports_malformed_dataset_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();

    let changeset = Controller::new(default_engine_config())
        .with_dataset_config(serde_json::json!({ "tables": "not a table config" }))
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("correspondence diff");
    assert!(changeset
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "binoc.dataset_config.invalid"));
}

fn write_zip_with_entries(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, contents) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(contents).unwrap();
    }
    zip.finish().unwrap();
}

fn position_of_rule(config: &CorrespondenceEngineConfig, name: &str) -> usize {
    config
        .rules
        .iter()
        .position(|rule| rule.name() == name)
        .unwrap_or_else(|| panic!("rule {name} not found"))
}

struct ExtrasContainerFromChildEvidence;

impl PairRule for ExtrasContainerFromChildEvidence {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "extras.container_from_children".into(),
            emits: vec!["extras.container_from_children".into()],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, _data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut proposals = Vec::new();
        for link in view.links() {
            let Some(left_parent) = view.parent(link.left) else {
                continue;
            };
            let Some(right_parent) = view.parent(link.right) else {
                continue;
            };
            if view.is_linked(left_parent) || view.is_linked(right_parent) {
                continue;
            }
            if left_parent == view.root(TreeSide::Left)
                || right_parent == view.root(TreeSide::Right)
            {
                continue;
            }
            if !view.has_children(left_parent) || !view.has_children(right_parent) {
                continue;
            }
            proposals.push(LinkProposal {
                left: left_parent.index,
                right: right_parent.index,
                evidence: "extras.container_from_children".into(),
                settled: false,
                projection: ProjectionHint::default().tag("binoc.move"),
            });
        }
        proposals.sort_by_key(|proposal| (proposal.left, proposal.right));
        proposals.dedup_by_key(|proposal| (proposal.left, proposal.right));
        Ok(proposals.into())
    }
}

struct ExtrasFrobnicateWriter;

impl EditListWriter for ExtrasFrobnicateWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "extras.write.frobnicate".into(),
            formats: vec![],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let path = &ctx.view.item(ctx.link.right).logical_path;
        if path != "notes.txt" {
            return Ok(None);
        }
        let left = data.read_bytes(ctx.view.item(ctx.link.left))?;
        let right = data.read_bytes(ctx.view.item(ctx.link.right))?;
        let line_count = |bytes: &[u8]| {
            String::from_utf8_lossy(bytes)
                .lines()
                .filter(|line| !line.is_empty())
                .count()
        };
        Ok(Some(
            vec![Edit::new(
                "extras.frobnicate",
                serde_json::json!({
                    "left_lines": line_count(&left),
                    "right_lines": line_count(&right),
                }),
            )
            .with_item_type("text")
            .with_summary("Frobnicated")]
            .into(),
        ))
    }
}

#[test]
fn representative_vectors_route_through_correspondence_engine() {
    let vectors_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-vectors");
    let vectors = [
        "zip-simple",
        "tar-simple",
        "gzip-inner-dispatch",
        "directory-nested",
        "single-file-modify-binary",
        "single-file-modify-text",
        "single-file-modify-csv",
        "csv-column-reorder",
        "directory-file-copy",
        "folder-move-nested",
    ];
    let materializers = stdlib_materializers();
    let materializer_refs: Vec<&dyn VectorMaterializer> = materializers
        .iter()
        .map(|materializer| &**materializer)
        .collect();
    for vector in vectors {
        let source = vectors_root.join(vector);
        let temp = tempfile::tempdir().expect("tempdir");
        let materialized = temp.path().join(vector);
        materialize_snapshots(&source, &materialized, &materializer_refs);
        let changeset = diff_with_config(
            &materialized.join("snapshot-a"),
            &materialized.join("snapshot-b"),
            default_engine_config(),
        );
        check_changeset_invariants(vector, &changeset);
    }
}
