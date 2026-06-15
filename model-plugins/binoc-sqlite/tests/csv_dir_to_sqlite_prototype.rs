//! DIAGNOSTIC PROTOTYPE — not a shipped feature.
//!
//! Question: when snapshot A is a DIRECTORY OF CSV FILES and snapshot B is a
//! SQLITE DATABASE holding the corresponding tables, does binoc's projection
//! read honestly, or produce mush?
//!
//! This test builds the smallest real reproduction and prints the rendered
//! changelog (JSON changeset + Markdown) against current `pre-refactor`
//! behavior. It is intentionally NOT asserting a "correct" answer — it captures
//! the baseline so the finding doc can quote real output. Run with:
//!
//!   cargo test -p binoc-sqlite --test csv_dir_to_sqlite_prototype -- --nocapture
//!
//! See docs/prototypes/csv-dir-to-sqlite-finding.md for the write-up.

use std::fs;
use std::path::Path;

use std::sync::Arc;

use binoc_core::controller::Controller;
use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};
use binoc_stdlib::correspondence::{default_engine_config, pair::DeclaredPair};
use binoc_stdlib::renderers::markdown::{render_markdown, MarkdownRendererConfig};
use rusqlite::Connection;

fn write_csv(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
}

/// Left snapshot: a directory containing `customers.csv` and `orders.csv`.
fn build_csv_dir(root: &Path) {
    fs::create_dir_all(root).unwrap();
    write_csv(
        &root.join("customers.csv"),
        "id,name,state\n1,Alice,MA\n2,Bob,CA\n",
    );
    write_csv(
        &root.join("orders.csv"),
        "id,customer_id,total\n100,1,42\n101,2,17\n",
    );
}

/// Right snapshot: a directory containing a single `data.sqlite` with
/// `customers` and `orders` tables holding the corresponding rows, plus two
/// deliberate differences: an added customer row (Carol) and a changed order
/// total (101: 17 -> 99).
fn build_sqlite_dir(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let db = root.join("data.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE customers (id INTEGER, name TEXT, state TEXT);
         INSERT INTO customers VALUES (1, 'Alice', 'MA');
         INSERT INTO customers VALUES (2, 'Bob', 'CA');
         INSERT INTO customers VALUES (3, 'Carol', 'NY');
         CREATE TABLE orders (id INTEGER, customer_id INTEGER, total INTEGER);
         INSERT INTO orders VALUES (100, 1, 42);
         INSERT INTO orders VALUES (101, 2, 99);",
    )
    .unwrap();
}

#[test]
fn csv_dir_to_sqlite_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let left = tmp.path().join("snapshot-a");
    let right = tmp.path().join("snapshot-b");
    build_csv_dir(&left);
    build_sqlite_dir(&right);

    let mut config = default_engine_config();
    binoc_sqlite::register_correspondence_rules(&mut config);

    let controller = Controller::new(config);
    let mut changeset = controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("diff should not error");

    // Stabilize snapshot labels for readability.
    changeset.from_snapshot = "snapshot-a (dir of CSVs)".into();
    changeset.to_snapshot = "snapshot-b (data.sqlite)".into();

    let json = serde_json::to_string_pretty(&changeset).unwrap();
    let md = render_markdown(
        std::slice::from_ref(&changeset),
        &MarkdownRendererConfig::default(),
    );

    println!("\n===== CSV-DIR -> SQLITE :: JSON CHANGESET =====\n{json}");
    println!("\n===== CSV-DIR -> SQLITE :: MARKDOWN CHANGELOG =====\n{md}");

    // No correctness assertion: this is a diagnostic capture. We only assert the
    // engine produced *something* so the test fails loudly if the pipeline
    // panics or returns an empty root.
    assert!(changeset.root.is_some(), "expected a root diff node");
}

/// Second probe: FORCE the is_dir-crossing container link `"" -> "data.sqlite"`
/// (the root directory linked to the single sqlite file) via a declared pair,
/// to characterize what the projection does when the container-type-change link
/// actually resolves. This is the link a `ContainerFromChildEvidence`-style rule
/// would eventually vote for IF table-level child links existed.
#[test]
fn csv_dir_to_sqlite_forced_container_link() {
    let tmp = tempfile::tempdir().unwrap();
    let left = tmp.path().join("snapshot-a");
    let right = tmp.path().join("snapshot-b");
    build_csv_dir(&left);
    build_sqlite_dir(&right);

    let mut config = default_engine_config();
    binoc_sqlite::register_correspondence_rules(&mut config);
    force_root_to_sqlite_link(&mut config);

    let controller = Controller::new(config);
    let mut changeset = controller
        .diff(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("diff should not error");
    changeset.from_snapshot = "snapshot-a (dir of CSVs)".into();
    changeset.to_snapshot = "snapshot-b (data.sqlite)".into();

    let json = serde_json::to_string_pretty(&changeset).unwrap();
    let md = render_markdown(
        std::slice::from_ref(&changeset),
        &MarkdownRendererConfig::default(),
    );

    println!("\n===== FORCED CONTAINER LINK (root dir -> data.sqlite) :: JSON =====\n{json}");
    println!("\n===== FORCED CONTAINER LINK (root dir -> data.sqlite) :: MARKDOWN =====\n{md}");

    assert!(changeset.root.is_some(), "expected a root diff node");
}

/// Insert a `DeclaredPair` that links the left root directory (logical path "")
/// to the right `data.sqlite` file. DeclaredPair is keyed on exact logical
/// paths, so this is a precise, hand-built container link.
fn force_root_to_sqlite_link(config: &mut CorrespondenceEngineConfig) {
    config.rules.insert(
        0,
        CoreRule::Pair(Arc::new(DeclaredPair {
            pairs: vec![(String::new(), "data.sqlite".to_string())],
            rules: Vec::new(),
        })),
    );
}
