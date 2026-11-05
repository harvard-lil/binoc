//! Runs the bundled `binoc-cli` on one test vector from each first-party
//! format pack and checks that the pack actually handled it.
//!
//! Each pack's own tests register it directly into an engine; this test covers
//! the other half: that the `bundled` feature set wires every pack into the CLI
//! people run. A pack missing from `register_bundled_correspondence_rules` (or
//! from `bundled`) falls back to a byte comparison and fails the tag check.
//! Run with `cargo test -p binoc-cli --features bundled --test bundled_formats`.
#![cfg(all(
    feature = "bundled",
    feature = "sqlite",
    feature = "excel",
    feature = "parquet",
    feature = "avro",
    feature = "dbf",
    feature = "xml",
    feature = "shapefile",
    feature = "binformats",
    feature = "stat-binary",
    feature = "row-reorder"
))]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use binoc_stdlib::test_vectors::{materialize_snapshots, stdlib_materializers, VectorMaterializer};
use serde_json::Value;

/// (plugin directory under model-plugins/, vector name, text only that pack
/// emits). The manifest's `expected.has_tags` is checked for every vector; the
/// text is for packs whose tags the standard library also produces, such as
/// row-reorder, whose `binoc.row-reorder` tag the stdlib tabular writer also
/// emits for a pure reorder.
const VECTORS: &[(&str, &str, Option<&str>)] = &[
    ("binoc-avro", "avro-cell-change", None),
    ("binoc-binformats", "cbor-value-change", None),
    ("binoc-dbf", "dbf-cell-change", None),
    ("binoc-excel", "excel-single-sheet-cell-change", None),
    ("binoc-parquet", "parquet-cell-change", None),
    (
        "binoc-row-reorder",
        "csv-row-reorder",
        Some("Rows reordered (same data, different sort order)"),
    ),
    ("binoc-shapefile", "shapefile-feature-addition", None),
    ("binoc-sqlite", "sqlite-row-addition", None),
    ("binoc-stat-binary", "stata-column-addition", None),
    ("binoc-xml", "xml-socrata-rows-row-addition", None),
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn materializers() -> Vec<Box<dyn VectorMaterializer>> {
    let mut all = stdlib_materializers();
    all.push(Box::new(binoc_sqlite::test_support::SqliteMaterializer));
    all.push(Box::new(binoc_excel::test_support::ExcelMaterializer));
    all.push(Box::new(binoc_parquet::test_support::ParquetMaterializer));
    all.push(Box::new(binoc_avro::test_support::AvroMaterializer));
    all.push(Box::new(binoc_dbf::test_support::DbfMaterializer));
    all.push(Box::new(
        binoc_shapefile::test_support::ShapefileMaterializer,
    ));
    all.push(Box::new(
        binoc_binformats::test_support::BinFormatsMaterializer,
    ));
    all.push(Box::new(binoc_stat_binary::test_support::DtaMaterializer));
    all.push(Box::new(binoc_stat_binary::test_support::XptMaterializer));
    all
}

fn expected_tags(manifest: &Path) -> Vec<String> {
    let manifest: toml::Value = toml::from_str(&std::fs::read_to_string(manifest).unwrap())
        .unwrap_or_else(|e| panic!("parse {}: {e}", manifest.display()));
    manifest["expected"]["has_tags"]
        .as_array()
        .unwrap_or_else(|| panic!("{} has no expected.has_tags", manifest_name(&manifest)))
        .iter()
        .map(|tag| tag.as_str().unwrap().to_string())
        .collect()
}

fn manifest_name(manifest: &toml::Value) -> String {
    manifest["vector"]["name"]
        .as_str()
        .unwrap_or("?")
        .to_string()
}

fn collect_tags(node: &Value, tags: &mut BTreeSet<String>) {
    if let Some(node_tags) = node["tags"].as_array() {
        tags.extend(
            node_tags
                .iter()
                .filter_map(|t| t.as_str().map(String::from)),
        );
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            collect_tags(child, tags);
        }
    }
}

#[test]
fn bundled_cli_handles_each_format_pack() {
    let materializers = materializers();
    let materializer_refs: Vec<&dyn VectorMaterializer> =
        materializers.iter().map(|m| m.as_ref()).collect();
    let tmp = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();

    for (plugin, vector, marker) in VECTORS {
        let source = workspace_root()
            .join("model-plugins")
            .join(plugin)
            .join("test-vectors")
            .join(vector);
        let dest = tmp.path().join(vector);
        materialize_snapshots(&source, &dest, &materializer_refs);

        let output = Command::new(assert_cmd::cargo_bin!("binoc-cli"))
            .arg("diff")
            .arg(dest.join("snapshot-a"))
            .arg(dest.join("snapshot-b"))
            .arg("--format")
            .arg("json")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{plugin}/{vector}: binoc-cli failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let changeset: Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut tags = BTreeSet::new();
        collect_tags(&changeset["root"], &mut tags);
        let missing: Vec<String> = expected_tags(&source.join("manifest.toml"))
            .into_iter()
            .filter(|tag| !tags.contains(tag))
            .collect();
        if !missing.is_empty() {
            failures.push(format!(
                "{plugin}/{vector}: missing {missing:?}; got {tags:?}"
            ));
        }
        if let Some(marker) = marker {
            if !String::from_utf8_lossy(&output.stdout).contains(marker) {
                failures.push(format!("{plugin}/{vector}: output lacks {marker:?}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "bundled CLI did not handle these format packs:\n{}",
        failures.join("\n")
    );
}
