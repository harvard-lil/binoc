use assert_cmd::Command;
use predicates::prelude::Predicate;
use predicates::prelude::PredicateBooleanExt;
use serde_json::Value;

fn vectors_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-vectors")
}

fn binoc() -> Command {
    Command::new(assert_cmd::cargo_bin!("binoc-cli"))
}

// ── diff subcommand ────────────────────────────────────────────────

#[test]
fn diff_identical_directories() {
    let dir = vectors_dir().join("trivial-identical");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .assert()
        .success();
}

#[test]
fn diff_default_stdout_is_markdown() {
    let dir = vectors_dir().join("single-file-modify-text");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .assert()
        .success()
        .stdout(predicates::str::contains("# Changelog:"))
        .stdout(predicates::str::contains("story.txt"))
        .stdout(predicates::str::contains("2 lines added; 1 line removed"))
        .stdout(predicates::str::contains("Claims: none").not())
        .stdout(predicates::str::contains("Sources").not());
}

#[test]
fn diff_format_json_outputs_raw_changeset() {
    let dir = vectors_dir().join("single-file-modify-text");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicates::str::contains("\"from_snapshot\""))
        .stdout(predicates::str::contains("\"action\""));
}

#[test]
fn diff_three_snapshots_outputs_pairwise_sequence() {
    let tmp = tempfile::tempdir().unwrap();
    let snap_a = tmp.path().join("snapshot-a");
    let snap_b = tmp.path().join("snapshot-b");
    let snap_c = tmp.path().join("snapshot-c");
    std::fs::create_dir(&snap_a).unwrap();
    std::fs::create_dir(&snap_b).unwrap();
    std::fs::create_dir(&snap_c).unwrap();
    std::fs::write(snap_a.join("story.txt"), "alpha\n").unwrap();
    std::fs::write(snap_b.join("story.txt"), "alpha\nbeta\n").unwrap();
    std::fs::write(snap_c.join("story.txt"), "alpha\nbeta\ngamma\n").unwrap();

    binoc()
        .arg("diff")
        .arg(&snap_a)
        .arg(&snap_b)
        .arg(&snap_c)
        .assert()
        .success()
        .stdout(predicates::str::contains("# Changelog:"))
        .stdout(predicates::str::contains("snapshot-a"))
        .stdout(predicates::str::contains("snapshot-b"))
        .stdout(predicates::str::contains("snapshot-c"));
}

#[test]
fn diff_csv_column_addition_markdown() {
    let dir = vectors_dir().join("csv-column-addition");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .assert()
        .success()
        .stdout(predicates::str::contains("data.csv"))
        .stdout(predicates::str::contains("Column added: 'email'"))
        .stdout(predicates::str::contains("Claims: none").not())
        .stdout(predicates::str::contains("Sources").not());
}

#[test]
fn diff_output_json_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("changeset.json");
    let dir = vectors_dir().join("single-file-add");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&out_path)
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(content.contains("from_snapshot"));
    assert!(content.contains("to_snapshot"));
}

#[test]
fn diff_output_md_file() {
    let tmp = tempfile::tempdir().unwrap();
    let md_path = tmp.path().join("changelog.md");
    let dir = vectors_dir().join("csv-row-addition");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&md_path)
        .assert()
        .success();

    let md = std::fs::read_to_string(&md_path).unwrap();
    assert!(md.contains("# Changelog:"));
    assert!(!md.contains("## "));
}

#[test]
fn diff_multiple_outputs() {
    let tmp = tempfile::tempdir().unwrap();
    let json_path = tmp.path().join("changeset.json");
    let md_path = tmp.path().join("changelog.md");
    let dir = vectors_dir().join("csv-row-addition");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&json_path)
        .arg("-o")
        .arg(&md_path)
        .assert()
        .success();

    let json = std::fs::read_to_string(&json_path).unwrap();
    assert!(json.contains("from_snapshot"));

    let md = std::fs::read_to_string(&md_path).unwrap();
    assert!(md.contains("# Changelog:"));
}

#[test]
fn extract_rows_added_from_saved_changeset() {
    let tmp = tempfile::tempdir().unwrap();
    let changeset_path = tmp.path().join("changeset.json");
    let dir = vectors_dir().join("csv-row-addition");

    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&changeset_path)
        .arg("-q")
        .assert()
        .success();

    binoc()
        .arg("extract")
        .arg(&changeset_path)
        .arg("data.csv")
        .arg("rows_added")
        .assert()
        .success()
        .stdout(predicates::str::contains("Charlie"));
}

#[test]
fn diff_two_snapshots_json_stdout_remains_single_object() {
    let dir = vectors_dir().join("single-file-modify-text");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("{"));
}

#[test]
fn diff_quiet_suppresses_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("changeset.json");
    let dir = vectors_dir().join("single-file-add");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&out_path)
        .arg("-q")
        .assert()
        .success()
        .stdout(predicates::str::is_empty());

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(content.contains("from_snapshot"));
}

#[test]
fn diff_explicit_format_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("output.dat");
    let dir = vectors_dir().join("single-file-add");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(format!("json:{}", out_path.display()))
        .arg("-q")
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(content.contains("from_snapshot"));
}

#[test]
fn report_bundle_copies_snapshots_and_reproduces_changeset() {
    let tmp = tempfile::tempdir().unwrap();
    let bundle_dir = tmp.path().join("report");
    let rerun_changeset = tmp.path().join("rerun.json");
    let dir = vectors_dir().join("single-file-modify-text");

    binoc()
        .arg("report")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--output-dir")
        .arg(&bundle_dir)
        .assert()
        .success()
        .stderr(predicates::str::contains("Wrote report bundle"));

    assert!(bundle_dir.join("dataset-config.yaml").exists());
    assert!(bundle_dir.join("changeset.json").exists());
    assert!(bundle_dir.join("changelog.md").exists());
    assert!(bundle_dir.join("run.trace.json").exists());
    assert!(bundle_dir.join("metadata.json").exists());
    assert!(bundle_dir.join("README.md").exists());
    assert!(bundle_dir.join("snapshots/snapshot-a/story.txt").exists());
    assert!(bundle_dir.join("snapshots/snapshot-b/story.txt").exists());

    let metadata = std::fs::read_to_string(bundle_dir.join("metadata.json")).unwrap();
    assert!(metadata.contains("\"snapshot_mode\": \"copy\""));
    assert!(metadata.contains("\"bundled_path\": \"snapshots/snapshot-a\""));

    let original_changeset = std::fs::read_to_string(bundle_dir.join("changeset.json")).unwrap();
    binoc()
        .arg("diff")
        .arg(bundle_dir.join("snapshots/snapshot-a"))
        .arg(bundle_dir.join("snapshots/snapshot-b"))
        .arg("--format")
        .arg("json")
        .arg("-q")
        .arg("-o")
        .arg(&rerun_changeset)
        .assert()
        .success();
    let mut original: Value = serde_json::from_str(&original_changeset).unwrap();
    let mut rerun: Value =
        serde_json::from_str(&std::fs::read_to_string(&rerun_changeset).unwrap()).unwrap();
    original["from_snapshot"] = Value::String("<normalized>".into());
    original["to_snapshot"] = Value::String("<normalized>".into());
    rerun["from_snapshot"] = Value::String("<normalized>".into());
    rerun["to_snapshot"] = Value::String("<normalized>".into());
    assert_eq!(original, rerun);
}

#[test]
fn report_reference_mode_skips_snapshot_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let bundle_dir = tmp.path().join("report");
    let dir = vectors_dir().join("single-file-modify-text");

    binoc()
        .arg("report")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--output-dir")
        .arg(&bundle_dir)
        .arg("--snapshot-mode")
        .arg("reference")
        .assert()
        .success();

    assert!(!bundle_dir.join("snapshots").exists());
    let metadata = std::fs::read_to_string(bundle_dir.join("metadata.json")).unwrap();
    assert!(metadata.contains("\"snapshot_mode\": \"reference\""));
    assert!(predicates::str::contains("\"bundled_path\": null").eval(&metadata));
}

#[test]
fn diff_with_config_file() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
dataset:
  tables:
    defaults:
      row_identity:
        columns:
          - id
"#,
    )
    .unwrap();

    let dir = vectors_dir().join("csv-row-addition");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicates::str::contains("data.csv"))
        .stdout(predicates::str::contains("2 rows added"));
}

#[test]
fn diff_rejects_removed_pipeline_lists_in_config() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
comparators:
  - example.no_longer_loaded
transformers:
  - example.no_longer_loaded
"#,
    )
    .unwrap();

    let dir = vectors_dir().join("csv-row-addition");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--config")
        .arg(&config_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("comparators"));
}

// ── Error cases ────────────────────────────────────────────────────

#[test]
fn diff_missing_snapshot_fails() {
    binoc()
        .arg("diff")
        .arg("/nonexistent/path/a")
        .arg("/nonexistent/path/b")
        .assert()
        .failure();
}

#[test]
fn diff_invalid_config_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("bad.yaml");
    std::fs::write(&config, "not: [valid: config: {{{}}}").unwrap();
    let dir = vectors_dir().join("trivial-identical");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("--config")
        .arg(&config)
        .assert()
        .failure();
}

#[test]
fn diff_unknown_extension_without_prefix_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("output.xyz");
    let dir = vectors_dir().join("trivial-identical");
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&out_path)
        .assert()
        .failure();
}

// ── changelog subcommand ───────────────────────────────────────────

#[test]
fn changelog_from_changeset_file() {
    let tmp = tempfile::tempdir().unwrap();
    let changeset_path = tmp.path().join("changeset.json");
    let dir = vectors_dir().join("csv-column-addition");

    // Generate changeset JSON
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&changeset_path)
        .arg("-q")
        .assert()
        .success();

    // Generate changelog from saved changeset
    binoc()
        .arg("changelog")
        .arg(&changeset_path)
        .assert()
        .success()
        .stdout(predicates::str::contains("Changelog:"))
        .stdout(predicates::str::contains("data.csv"));
}

#[test]
fn changelog_output_to_file() {
    let tmp = tempfile::tempdir().unwrap();
    let changeset_path = tmp.path().join("changeset.json");
    let changelog_path = tmp.path().join("CHANGELOG.md");
    let dir = vectors_dir().join("csv-column-addition");

    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&changeset_path)
        .arg("-q")
        .assert()
        .success();

    binoc()
        .arg("changelog")
        .arg(&changeset_path)
        .arg("-o")
        .arg(&changelog_path)
        .arg("-q")
        .assert()
        .success();

    let md = std::fs::read_to_string(&changelog_path).unwrap();
    assert!(md.contains("Changelog:"));
    assert!(md.contains("data.csv"));
}

#[test]
fn changelog_accepts_multi_snapshot_changeset_file() {
    let tmp = tempfile::tempdir().unwrap();
    let changeset_path = tmp.path().join("changesets.json");
    let changelog_path = tmp.path().join("CHANGELOG.md");
    let snap_a = tmp.path().join("snapshot-a");
    let snap_b = tmp.path().join("snapshot-b");
    let snap_c = tmp.path().join("snapshot-c");
    std::fs::create_dir(&snap_a).unwrap();
    std::fs::create_dir(&snap_b).unwrap();
    std::fs::create_dir(&snap_c).unwrap();
    std::fs::write(snap_a.join("story.txt"), "alpha\n").unwrap();
    std::fs::write(snap_b.join("story.txt"), "alpha\nbeta\n").unwrap();
    std::fs::write(snap_c.join("story.txt"), "alpha\nbeta\ngamma\n").unwrap();

    binoc()
        .arg("diff")
        .arg(&snap_a)
        .arg(&snap_b)
        .arg(&snap_c)
        .arg("-o")
        .arg(&changeset_path)
        .arg("-q")
        .assert()
        .success();

    binoc()
        .arg("changelog")
        .arg(&changeset_path)
        .arg("-o")
        .arg(&changelog_path)
        .arg("-q")
        .assert()
        .success();

    let md = std::fs::read_to_string(&changelog_path).unwrap();
    assert!(md.contains("snapshot-a"));
    assert!(md.contains("snapshot-b"));
    assert!(md.contains("snapshot-c"));
}

// ── help and version ───────────────────────────────────────────────

#[test]
fn help_flag_works() {
    binoc()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("changelog for datasets"));
}

#[test]
fn diff_help_flag_works() {
    binoc()
        .arg("diff")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("snapshot"));
}
