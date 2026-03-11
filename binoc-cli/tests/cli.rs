use assert_cmd::Command;

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
        .stdout(predicates::str::contains("lines added"));
}

#[test]
fn diff_format_json_outputs_raw_migration() {
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
        .stdout(predicates::str::contains("\"kind\""));
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
        .stdout(predicates::str::contains("Column added: 'email'"));
}

#[test]
fn diff_output_json_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("migration.json");
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
    assert!(md.contains("Substantive Changes"));
}

#[test]
fn diff_multiple_outputs() {
    let tmp = tempfile::tempdir().unwrap();
    let json_path = tmp.path().join("migration.json");
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
fn diff_quiet_suppresses_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("migration.json");
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
fn diff_with_config_file() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
comparators:
  - binoc.directory
  - binoc.csv
  - binoc.text
  - binoc.binary
transformers: []
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
        .stdout(predicates::str::contains("rows added"));
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
fn changelog_from_migration_file() {
    let tmp = tempfile::tempdir().unwrap();
    let migration_path = tmp.path().join("migration.json");
    let dir = vectors_dir().join("csv-column-addition");

    // Generate migration JSON
    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&migration_path)
        .arg("-q")
        .assert()
        .success();

    // Generate changelog from saved migration
    binoc()
        .arg("changelog")
        .arg(&migration_path)
        .assert()
        .success()
        .stdout(predicates::str::contains("Changelog:"))
        .stdout(predicates::str::contains("data.csv"));
}

#[test]
fn changelog_output_to_file() {
    let tmp = tempfile::tempdir().unwrap();
    let migration_path = tmp.path().join("migration.json");
    let changelog_path = tmp.path().join("CHANGELOG.md");
    let dir = vectors_dir().join("csv-column-addition");

    binoc()
        .arg("diff")
        .arg(dir.join("snapshot-a"))
        .arg(dir.join("snapshot-b"))
        .arg("-o")
        .arg(&migration_path)
        .arg("-q")
        .assert()
        .success();

    binoc()
        .arg("changelog")
        .arg(&migration_path)
        .arg("-o")
        .arg(&changelog_path)
        .arg("-q")
        .assert()
        .success();

    let md = std::fs::read_to_string(&changelog_path).unwrap();
    assert!(md.contains("Changelog:"));
    assert!(md.contains("data.csv"));
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
