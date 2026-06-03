use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;
use serde::{Deserialize, Serialize};

const MAX_CAPTURED_CELL_EXAMPLES: usize = 8;
const MAX_CAPTURED_VALUE_CHARS: usize = 256;

/// Analyzes tabular data artifacts to detect schema changes, row changes,
/// and cell-level changes. Source-format-agnostic — works with any
/// comparator that publishes [`tabular_v1`] artifacts (CSV, Parquet,
/// Excel, etc.).
///
/// Should run before refinement transformers like `ColumnReorderDetector`
/// and `RowReorderDetector` so they can build on the tags this sets.
pub struct TabularAnalyzer;

impl Transformer for TabularAnalyzer {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.tabular_analyzer")
            .with_match_artifacts(vec![tabular_v1()])
    }

    fn transform(
        &self,
        node: DiffNode,
        data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let Some(pair) = TabularDataPair::from_artifacts(&node, data) else {
            return TransformResult::Unchanged;
        };

        match node.action.as_str() {
            "add" => transform_add(node, &pair),
            "remove" => transform_remove(node, &pair),
            // `move` nodes from fuzzy correlation carry tabular artifacts
            // from the re-dispatched content diff; analyze them the same
            // way as a `modify`, but preserve the move summary.
            "modify" | "move" => transform_modify(node, &pair, config),
            _ => TransformResult::Unchanged,
        }
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let pair = TabularDataPair::from_artifacts(node, data)?;
        if let Some(result) = keyed_tabular_extract(&pair, node, aspect) {
            return Some(result);
        }
        tabular_extract(&pair, node, aspect)
    }
}

fn transform_add(mut node: DiffNode, pair: &TabularDataPair) -> TransformResult {
    let Some(right) = &pair.right else {
        return TransformResult::Unchanged;
    };
    node.summary = Some(format!(
        "New table ({} column{}, {} row{})",
        right.headers.len(),
        if right.headers.len() == 1 { "" } else { "s" },
        right.rows.len(),
        if right.rows.len() == 1 { "" } else { "s" }
    ));
    node.tags.insert("binoc.content-changed".into());
    node.details
        .insert("columns".into(), serde_json::json!(right.headers));
    node.details
        .insert("rows".into(), serde_json::json!(right.rows.len()));
    TransformResult::Replace(Box::new(node))
}

fn transform_remove(mut node: DiffNode, pair: &TabularDataPair) -> TransformResult {
    let Some(left) = &pair.left else {
        return TransformResult::Unchanged;
    };
    node.summary = Some(format!(
        "Table removed ({} column{}, {} row{})",
        left.headers.len(),
        if left.headers.len() == 1 { "" } else { "s" },
        left.rows.len(),
        if left.rows.len() == 1 { "" } else { "s" }
    ));
    node.tags.insert("binoc.content-changed".into());
    node.details
        .insert("columns".into(), serde_json::json!(left.headers));
    node.details
        .insert("rows".into(), serde_json::json!(left.rows.len()));
    TransformResult::Replace(Box::new(node))
}

fn transform_modify(
    mut node: DiffNode,
    pair: &TabularDataPair,
    config: &serde_json::Value,
) -> TransformResult {
    let (Some(left), Some(right)) = (&pair.left, &pair.right) else {
        return TransformResult::Unchanged;
    };

    let headers_l: BTreeSet<&str> = left.headers.iter().map(|s| s.as_str()).collect();
    let headers_r: BTreeSet<&str> = right.headers.iter().map(|s| s.as_str()).collect();

    let columns_added: Vec<String> = headers_r
        .difference(&headers_l)
        .map(|s| s.to_string())
        .collect();
    let columns_removed: Vec<String> = headers_l
        .difference(&headers_r)
        .map(|s| s.to_string())
        .collect();
    let columns_common: Vec<String> = headers_l
        .intersection(&headers_r)
        .map(|s| s.to_string())
        .collect();

    let order_changed = {
        let common_order_l: Vec<&str> = left
            .headers
            .iter()
            .filter(|h| columns_common.contains(h))
            .map(|s| s.as_str())
            .collect();
        let common_order_r: Vec<&str> = right
            .headers
            .iter()
            .filter(|h| columns_common.contains(h))
            .map(|s| s.as_str())
            .collect();
        common_order_l != common_order_r
    };

    if let Some(row_identity) = row_identity_for_node(&node, config) {
        if row_identity
            .columns
            .iter()
            .all(|col| left.column_index(col).is_some() && right.column_index(col).is_some())
        {
            return transform_modify_keyed(
                node,
                left,
                right,
                TabularColumnChange {
                    columns_added,
                    columns_removed,
                    columns_common,
                    order_changed,
                },
                row_identity,
            );
        }

        let missing_columns: Vec<&str> = row_identity
            .columns
            .iter()
            .filter(|col| left.column_index(col).is_none() || right.column_index(col).is_none())
            .map(|col| col.as_str())
            .collect();
        node.tags.insert("binoc.identity-diagnostic".into());
        node.tags.insert("binoc.row-identity-ambiguous".into());
        node.details.insert(
            "row_identity_missing_columns".into(),
            serde_json::json!(missing_columns),
        );
        node.diagnostics.push(
            Diagnostic::warning(
                "binoc.row-identity-missing-columns",
                format!(
                    "Configured row identity references missing column{}: {}",
                    if missing_columns.len() == 1 { "" } else { "s" },
                    fmt_quoted_list(&missing_columns)
                ),
            )
            .with_location(node.path.clone()),
        );
    }

    let min_rows = left.rows.len().min(right.rows.len());
    let mut cells_changed: u64 = 0;
    let mut cell_examples = Vec::new();
    for i in 0..min_rows {
        let row_l = &left.rows[i];
        let row_r = &right.rows[i];
        for col in &columns_common {
            let val_l = left
                .column_index(col)
                .and_then(|j| row_l.get(j))
                .map(|s| s.as_str())
                .unwrap_or("");
            let val_r = right
                .column_index(col)
                .and_then(|j| row_r.get(j))
                .map(|s| s.as_str())
                .unwrap_or("");
            if val_l != val_r {
                cells_changed += 1;
                if cell_examples.len() < MAX_CAPTURED_CELL_EXAMPLES {
                    cell_examples.push(changed_cell_example(i, col, val_l, val_r));
                }
            }
        }
    }

    let rows_added = right.rows.len().saturating_sub(left.rows.len()) as u64;
    let rows_removed = left.rows.len().saturating_sub(right.rows.len()) as u64;

    node.details
        .insert("columns_left".into(), serde_json::json!(left.headers));
    node.details
        .insert("columns_right".into(), serde_json::json!(right.headers));
    node.details
        .insert("columns_added".into(), serde_json::json!(columns_added));
    node.details
        .insert("columns_removed".into(), serde_json::json!(columns_removed));
    node.details
        .insert("rows_left".into(), serde_json::json!(left.rows.len()));
    node.details
        .insert("rows_right".into(), serde_json::json!(right.rows.len()));
    node.details
        .insert("rows_added".into(), serde_json::json!(rows_added));
    node.details
        .insert("rows_removed".into(), serde_json::json!(rows_removed));
    node.details
        .insert("cells_changed".into(), serde_json::json!(cells_changed));

    if cells_changed > 0 {
        node.detail_blocks.push(
            DetailBlock::new("cells_changed", "binoc.tabular.cell_changes.v1")
                .with_label("Changed cells")
                .with_total_count(cells_changed)
                .with_extract_hint(
                    ExtractHint::new("cells_changed").with_label("All changed cells"),
                ),
        );
        if let Some(block) = node.detail_blocks.last_mut() {
            block.examples = cell_examples;
            block.truncated = cells_changed as usize > block.examples.len();
        }
    }

    if !columns_added.is_empty() {
        node.tags.insert("binoc.column-addition".into());
    }
    if !columns_removed.is_empty() {
        node.tags.insert("binoc.column-removal".into());
    }
    if order_changed {
        node.tags.insert("binoc.column-reorder".into());
    }
    if rows_added > 0 {
        node.tags.insert("binoc.row-addition".into());
    }
    if rows_removed > 0 {
        node.tags.insert("binoc.row-removal".into());
    }
    if cells_changed > 0 {
        node.tags.insert("binoc.cell-change".into());
    }
    if !columns_added.is_empty() || !columns_removed.is_empty() {
        node.tags.insert("binoc.schema-change".into());
    }

    let tabular_desc = tabular_summary(
        &columns_added,
        &columns_removed,
        order_changed,
        rows_added,
        rows_removed,
        cells_changed,
    );

    // For `move` nodes, the move summary already describes the rename;
    // stash the tabular description as an annotation so renderers can
    // surface it if they want without overwriting "Moved from ...".
    if node.action == "move" {
        node.annotations
            .insert("tabular_summary".into(), serde_json::json!(tabular_desc));
    } else {
        if tabular_desc != "Table modified" || node.summary.is_none() {
            node.summary = Some(tabular_desc);
        }
    }

    TransformResult::Replace(Box::new(node))
}

struct TabularColumnChange {
    columns_added: Vec<String>,
    columns_removed: Vec<String>,
    columns_common: Vec<String>,
    order_changed: bool,
}

fn transform_modify_keyed(
    mut node: DiffNode,
    left: &TabularData,
    right: &TabularData,
    column_change: TabularColumnChange,
    row_identity: EffectiveRowIdentity,
) -> TransformResult {
    let key_columns = row_identity.columns.clone();
    let key_match = build_key_match(left, right, &row_identity);

    let mut cells_changed: u64 = 0;
    let mut rows_modified: u64 = 0;
    let mut cell_examples = Vec::new();

    for matched in &key_match.matched {
        let row_l = &left.rows[matched.left_index];
        let row_r = &right.rows[matched.right_index];
        let mut row_changed = false;
        for col in &column_change.columns_common {
            let val_l = left
                .column_index(col)
                .and_then(|j| row_l.get(j))
                .map(|s| s.as_str())
                .unwrap_or("");
            let val_r = right
                .column_index(col)
                .and_then(|j| row_r.get(j))
                .map(|s| s.as_str())
                .unwrap_or("");
            if val_l != val_r {
                row_changed = true;
                cells_changed += 1;
                if cell_examples.len() < MAX_CAPTURED_CELL_EXAMPLES {
                    cell_examples.push(changed_cell_example_with_key(
                        &key_columns,
                        &matched.key,
                        col,
                        val_l,
                        val_r,
                    ));
                }
            }
        }
        if row_changed {
            rows_modified += 1;
        }
    }

    let rows_added = key_match.unmatched_right.len() as u64;
    let rows_removed = key_match.unmatched_left.len() as u64;

    node.details
        .insert("columns_left".into(), serde_json::json!(left.headers));
    node.details
        .insert("columns_right".into(), serde_json::json!(right.headers));
    node.details.insert(
        "columns_added".into(),
        serde_json::json!(&column_change.columns_added),
    );
    node.details.insert(
        "columns_removed".into(),
        serde_json::json!(&column_change.columns_removed),
    );
    node.details
        .insert("rows_left".into(), serde_json::json!(left.rows.len()));
    node.details
        .insert("rows_right".into(), serde_json::json!(right.rows.len()));
    node.details
        .insert("rows_added".into(), serde_json::json!(rows_added));
    node.details
        .insert("rows_removed".into(), serde_json::json!(rows_removed));
    node.details
        .insert("rows_modified".into(), serde_json::json!(rows_modified));
    node.details
        .insert("cells_changed".into(), serde_json::json!(cells_changed));
    node.details.insert(
        "row_identity".into(),
        serde_json::json!({
            "columns": &key_columns,
            "matched_rows": key_match.matched.len(),
            "mode": "keyed",
            "on_null_key": row_identity.on_null_key,
            "on_duplicate_key": row_identity.on_duplicate_key
        }),
    );
    if !key_match.unmatched_left.is_empty() {
        node.details.insert(
            "rows_removed_keys".into(),
            serde_json::json!(keyed_rows_for_detail(
                &key_columns,
                &key_match.unmatched_left
            )),
        );
    }
    if !key_match.unmatched_right.is_empty() {
        node.details.insert(
            "rows_added_keys".into(),
            serde_json::json!(keyed_rows_for_detail(
                &key_columns,
                &key_match.unmatched_right
            )),
        );
    }

    if cells_changed > 0 {
        node.detail_blocks.push(
            DetailBlock::new("cells_changed", "binoc.tabular.cell_changes.v1")
                .with_label("Changed cells")
                .with_total_count(cells_changed)
                .with_extract_hint(
                    ExtractHint::new("cells_changed").with_label("All changed cells"),
                ),
        );
        if let Some(block) = node.detail_blocks.last_mut() {
            block.examples = cell_examples;
            block.truncated = cells_changed as usize > block.examples.len();
        }
    }

    apply_tabular_tags(
        &mut node,
        &column_change.columns_added,
        &column_change.columns_removed,
        column_change.order_changed,
        rows_added,
        rows_removed,
        cells_changed,
    );
    apply_key_diagnostics(&mut node, &key_match.diagnostics);

    let tabular_desc = keyed_tabular_summary(
        &column_change.columns_added,
        &column_change.columns_removed,
        column_change.order_changed,
        rows_added,
        rows_removed,
        rows_modified,
        cells_changed,
    );

    if node.action == "move" {
        node.annotations
            .insert("tabular_summary".into(), serde_json::json!(tabular_desc));
    } else if tabular_desc != "Table modified" || node.summary.is_none() {
        node.summary = Some(tabular_desc);
    }

    TransformResult::Replace(Box::new(node))
}

fn tabular_summary(
    columns_added: &[String],
    columns_removed: &[String],
    order_changed: bool,
    rows_added: u64,
    rows_removed: u64,
    cells_changed: u64,
) -> String {
    let mut parts = Vec::new();

    if !columns_added.is_empty() {
        let names: Vec<&str> = columns_added.iter().map(|s| s.as_str()).collect();
        if names.len() == 1 {
            parts.push(format!("column added: '{}'", names[0]));
        } else {
            parts.push(format!("columns added: {}", fmt_quoted_list(&names)));
        }
    }
    if !columns_removed.is_empty() {
        let names: Vec<&str> = columns_removed.iter().map(|s| s.as_str()).collect();
        if names.len() == 1 {
            parts.push(format!("column removed: '{}'", names[0]));
        } else {
            parts.push(format!("columns removed: {}", fmt_quoted_list(&names)));
        }
    }
    if order_changed {
        parts.push("columns reordered".into());
    }
    if rows_added > 0 {
        parts.push(format!(
            "{rows_added} row{} added",
            if rows_added == 1 { "" } else { "s" }
        ));
    }
    if rows_removed > 0 {
        parts.push(format!(
            "{rows_removed} row{} removed",
            if rows_removed == 1 { "" } else { "s" }
        ));
    }
    if cells_changed > 0 {
        parts.push(format!(
            "{cells_changed} cell{} changed",
            if cells_changed == 1 { "" } else { "s" }
        ));
    }

    if parts.is_empty() {
        "Table modified".into()
    } else {
        let mut s = parts.join("; ");
        if let Some(first) = s.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        s
    }
}

fn fmt_quoted_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn changed_cell_example(row: usize, column: &str, before: &str, after: &str) -> DetailExample {
    let mut example = DetailExample::new();
    example.locator.insert("row".into(), serde_json::json!(row));
    example
        .locator
        .insert("column".into(), serde_json::json!(column));
    example.before = Some(capture_value_preview(before));
    example.after = Some(capture_value_preview(after));
    example
}

fn changed_cell_example_with_key(
    key_columns: &[String],
    key: &[String],
    column: &str,
    before: &str,
    after: &str,
) -> DetailExample {
    let mut example = DetailExample::new();
    example.locator.insert(
        "key".into(),
        serde_json::json!(key_to_map(key_columns, key)),
    );
    example
        .locator
        .insert("column".into(), serde_json::json!(column));
    example.before = Some(capture_value_preview(before));
    example.after = Some(capture_value_preview(after));
    example
}

fn capture_value_preview(value: &str) -> ValuePreview {
    let truncated = value.chars().count() > MAX_CAPTURED_VALUE_CHARS;
    let value = if truncated {
        value
            .chars()
            .take(MAX_CAPTURED_VALUE_CHARS)
            .collect::<String>()
    } else {
        value.to_string()
    };
    ValuePreview {
        value: serde_json::json!(value),
        media_type: Some("text/plain".into()),
        truncated,
    }
}

fn apply_tabular_tags(
    node: &mut DiffNode,
    columns_added: &[String],
    columns_removed: &[String],
    order_changed: bool,
    rows_added: u64,
    rows_removed: u64,
    cells_changed: u64,
) {
    if !columns_added.is_empty() {
        node.tags.insert("binoc.column-addition".into());
    }
    if !columns_removed.is_empty() {
        node.tags.insert("binoc.column-removal".into());
    }
    if order_changed {
        node.tags.insert("binoc.column-reorder".into());
    }
    if rows_added > 0 {
        node.tags.insert("binoc.row-addition".into());
    }
    if rows_removed > 0 {
        node.tags.insert("binoc.row-removal".into());
    }
    if cells_changed > 0 {
        node.tags.insert("binoc.cell-change".into());
    }
    if !columns_added.is_empty() || !columns_removed.is_empty() {
        node.tags.insert("binoc.schema-change".into());
    }
}

fn keyed_tabular_summary(
    columns_added: &[String],
    columns_removed: &[String],
    order_changed: bool,
    rows_added: u64,
    rows_removed: u64,
    rows_modified: u64,
    cells_changed: u64,
) -> String {
    let mut parts = Vec::new();

    if !columns_added.is_empty() {
        let names: Vec<&str> = columns_added.iter().map(|s| s.as_str()).collect();
        if names.len() == 1 {
            parts.push(format!("column added: '{}'", names[0]));
        } else {
            parts.push(format!("columns added: {}", fmt_quoted_list(&names)));
        }
    }
    if !columns_removed.is_empty() {
        let names: Vec<&str> = columns_removed.iter().map(|s| s.as_str()).collect();
        if names.len() == 1 {
            parts.push(format!("column removed: '{}'", names[0]));
        } else {
            parts.push(format!("columns removed: {}", fmt_quoted_list(&names)));
        }
    }
    if order_changed {
        parts.push("columns reordered".into());
    }
    if rows_added > 0 {
        parts.push(format!(
            "{rows_added} row{} added by key",
            if rows_added == 1 { "" } else { "s" }
        ));
    }
    if rows_removed > 0 {
        parts.push(format!(
            "{rows_removed} row{} removed by key",
            if rows_removed == 1 { "" } else { "s" }
        ));
    }
    if rows_modified > 0 {
        parts.push(format!(
            "{rows_modified} row{} modified by key",
            if rows_modified == 1 { "" } else { "s" }
        ));
    } else if cells_changed > 0 {
        parts.push(format!(
            "{cells_changed} cell{} changed",
            if cells_changed == 1 { "" } else { "s" }
        ));
    }

    if parts.is_empty() {
        "Table modified".into()
    } else {
        let mut s = parts.join("; ");
        if let Some(first) = s.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        s
    }
}

#[derive(Debug, Clone)]
struct EffectiveRowIdentity {
    columns: Vec<String>,
    on_null_key: IdentityFailurePolicy,
    on_duplicate_key: IdentityFailurePolicy,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum IdentityFailurePolicy {
    #[default]
    Diagnostic,
    Error,
    Ignore,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DatasetSemanticsConfig {
    #[serde(default)]
    tables: TableConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TableConfig {
    #[serde(default)]
    defaults: TableDefaults,
    #[serde(default)]
    entries: BTreeMap<String, TableEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TableDefaults {
    #[serde(default)]
    row_identity: RowIdentityConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TableEntry {
    #[serde(default, rename = "match")]
    match_: TableSelector,
    #[serde(default)]
    row_identity: RowIdentityConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TableSelector {
    #[serde(default)]
    logical_name: Option<String>,
    #[serde(default)]
    source: Option<TableSourceSelector>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TableSourceSelector {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    path_regex: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RowIdentityConfig {
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default)]
    on_null_key: Option<IdentityFailurePolicy>,
    #[serde(default)]
    on_duplicate_key: Option<IdentityFailurePolicy>,
}

fn row_identity_for_node(
    node: &DiffNode,
    config: &serde_json::Value,
) -> Option<EffectiveRowIdentity> {
    let dataset = config
        .get("dataset")
        .filter(|value| !value.is_null())
        .unwrap_or(config);
    let semantics: DatasetSemanticsConfig = serde_json::from_value(dataset.clone()).ok()?;
    let defaults = semantics.tables.defaults.row_identity;

    for (entry_name, entry) in &semantics.tables.entries {
        if table_entry_matches(entry_name, entry, node) {
            let columns = if entry.row_identity.columns.is_empty() {
                defaults.columns.clone()
            } else {
                entry.row_identity.columns.clone()
            };
            if columns.is_empty() {
                return None;
            }
            return Some(EffectiveRowIdentity {
                columns,
                on_null_key: entry
                    .row_identity
                    .on_null_key
                    .unwrap_or(defaults.on_null_key.unwrap_or_default()),
                on_duplicate_key: entry
                    .row_identity
                    .on_duplicate_key
                    .unwrap_or(defaults.on_duplicate_key.unwrap_or_default()),
            });
        }
    }

    if defaults.columns.is_empty() {
        None
    } else {
        Some(EffectiveRowIdentity {
            columns: defaults.columns,
            on_null_key: defaults.on_null_key.unwrap_or_default(),
            on_duplicate_key: defaults.on_duplicate_key.unwrap_or_default(),
        })
    }
}

fn table_entry_matches(entry_name: &str, entry: &TableEntry, node: &DiffNode) -> bool {
    if let Some(expected) = &entry.match_.logical_name {
        return node_logical_name(node).is_some_and(|logical| logical == expected);
    }

    if let Some(source) = &entry.match_.source {
        let paths = node_source_paths(node);
        if let Some(expected) = &source.path {
            if paths.iter().any(|path| path == expected) {
                return true;
            }
        }
        if let Some(pattern) = &source.path_regex {
            if let Ok(regex) = regex::Regex::new(pattern) {
                return paths.iter().any(|path| regex.is_match(path));
            }
        }
        return false;
    }

    node_logical_name(node).is_some_and(|logical| logical == entry_name)
        || node.path == entry_name
        || std::path::Path::new(&node.path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem == entry_name)
}

fn node_logical_name(node: &DiffNode) -> Option<&str> {
    node.details.get("logical_name")?.as_str()
}

fn node_source_paths(node: &DiffNode) -> Vec<&str> {
    let mut paths = vec![node.path.as_str()];
    if let Some(pair) = &node.source_items {
        if let Some(left) = &pair.left {
            paths.push(left.logical_path.as_str());
        }
        if let Some(right) = &pair.right {
            paths.push(right.logical_path.as_str());
        }
    }
    paths
}

#[derive(Debug, Clone)]
struct MatchedRow {
    left_index: usize,
    right_index: usize,
    key: Vec<String>,
}

#[derive(Debug, Clone)]
struct UnmatchedRow {
    row_index: usize,
    key: Option<Vec<String>>,
}

#[derive(Debug, Default)]
struct KeyDiagnostics {
    null_left: usize,
    null_right: usize,
    duplicate_left: usize,
    duplicate_right: usize,
    ambiguous_keys: usize,
    duplicate_examples: Vec<Vec<String>>,
}

#[derive(Debug)]
struct KeyMatch {
    matched: Vec<MatchedRow>,
    unmatched_left: Vec<UnmatchedRow>,
    unmatched_right: Vec<UnmatchedRow>,
    diagnostics: KeyDiagnostics,
}

fn build_key_match(
    left: &TabularData,
    right: &TabularData,
    identity: &EffectiveRowIdentity,
) -> KeyMatch {
    let left_index = index_rows_by_key(left, &identity.columns);
    let right_index = index_rows_by_key(right, &identity.columns);

    let mut matched = Vec::new();
    let mut unmatched_left: Vec<UnmatchedRow> = left_index
        .null_rows
        .iter()
        .map(|row_index| UnmatchedRow {
            row_index: *row_index,
            key: None,
        })
        .collect();
    let mut unmatched_right: Vec<UnmatchedRow> = right_index
        .null_rows
        .iter()
        .map(|row_index| UnmatchedRow {
            row_index: *row_index,
            key: None,
        })
        .collect();
    let mut diagnostics = KeyDiagnostics {
        null_left: left_index.null_rows.len(),
        null_right: right_index.null_rows.len(),
        ..Default::default()
    };

    let keys: BTreeSet<Vec<String>> = left_index
        .rows_by_key
        .keys()
        .chain(right_index.rows_by_key.keys())
        .cloned()
        .collect();

    for key in keys {
        let left_rows = left_index
            .rows_by_key
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let right_rows = right_index
            .rows_by_key
            .get(&key)
            .cloned()
            .unwrap_or_default();

        match (left_rows.len(), right_rows.len()) {
            (1, 1) => matched.push(MatchedRow {
                left_index: left_rows[0],
                right_index: right_rows[0],
                key,
            }),
            (1, 0) => unmatched_left.push(UnmatchedRow {
                row_index: left_rows[0],
                key: Some(key),
            }),
            (0, 1) => unmatched_right.push(UnmatchedRow {
                row_index: right_rows[0],
                key: Some(key),
            }),
            _ => {
                if left_rows.len() > 1 {
                    diagnostics.duplicate_left += left_rows.len();
                }
                if right_rows.len() > 1 {
                    diagnostics.duplicate_right += right_rows.len();
                }
                if !left_rows.is_empty() && !right_rows.is_empty() {
                    diagnostics.ambiguous_keys += 1;
                }
                if diagnostics.duplicate_examples.len() < MAX_CAPTURED_CELL_EXAMPLES {
                    diagnostics.duplicate_examples.push(key.clone());
                }
                unmatched_left.extend(left_rows.into_iter().map(|row_index| UnmatchedRow {
                    row_index,
                    key: Some(key.clone()),
                }));
                unmatched_right.extend(right_rows.into_iter().map(|row_index| UnmatchedRow {
                    row_index,
                    key: Some(key.clone()),
                }));
            }
        }
    }

    KeyMatch {
        matched,
        unmatched_left,
        unmatched_right,
        diagnostics,
    }
}

#[derive(Debug)]
struct RowKeyIndex {
    rows_by_key: BTreeMap<Vec<String>, Vec<usize>>,
    null_rows: Vec<usize>,
}

fn index_rows_by_key(table: &TabularData, key_columns: &[String]) -> RowKeyIndex {
    let key_indices: Vec<usize> = key_columns
        .iter()
        .filter_map(|column| table.column_index(column))
        .collect();
    let mut rows_by_key: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
    let mut null_rows = Vec::new();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut key = Vec::with_capacity(key_indices.len());
        let mut is_null = false;
        for idx in &key_indices {
            let value = row.get(*idx).map(|s| s.trim()).unwrap_or("");
            if value.is_empty() {
                is_null = true;
            }
            key.push(value.to_string());
        }
        if is_null {
            null_rows.push(row_index);
        } else {
            rows_by_key.entry(key).or_default().push(row_index);
        }
    }

    RowKeyIndex {
        rows_by_key,
        null_rows,
    }
}

fn apply_key_diagnostics(node: &mut DiffNode, diagnostics: &KeyDiagnostics) {
    let Some(row_identity) = node.details.get("row_identity").cloned() else {
        return;
    };
    let policies = row_identity_policy_from_details(&row_identity);

    if diagnostics.null_left + diagnostics.null_right > 0 {
        node.tags.insert("binoc.identity-diagnostic".into());
        node.tags.insert("binoc.null-key".into());
        node.tags.insert("binoc.row-identity-ambiguous".into());
        if let Some(diagnostic) = diagnostic_for_identity_policy(
            policies.on_null_key,
            "binoc.null-key",
            format!(
                "{} row{} had null configured key values",
                diagnostics.null_left + diagnostics.null_right,
                if diagnostics.null_left + diagnostics.null_right == 1 {
                    ""
                } else {
                    "s"
                }
            ),
        ) {
            node.diagnostics
                .push(diagnostic.with_location(node.path.clone()));
        }
    }

    if diagnostics.duplicate_left + diagnostics.duplicate_right > 0 {
        node.tags.insert("binoc.identity-diagnostic".into());
        node.tags.insert("binoc.duplicate-key".into());
        node.tags.insert("binoc.row-identity-ambiguous".into());
        if diagnostics.ambiguous_keys > 0 {
            node.tags.insert("binoc.ambiguous-key".into());
        }
        if let Some(diagnostic) = diagnostic_for_identity_policy(
            policies.on_duplicate_key,
            "binoc.duplicate-key",
            format!(
                "{} configured row key{} appeared more than once",
                diagnostics
                    .ambiguous_keys
                    .max(diagnostics.duplicate_examples.len()),
                if diagnostics
                    .ambiguous_keys
                    .max(diagnostics.duplicate_examples.len())
                    == 1
                {
                    ""
                } else {
                    "s"
                }
            ),
        ) {
            node.diagnostics
                .push(diagnostic.with_location(node.path.clone()));
        }
    }
}

fn diagnostic_for_identity_policy(
    policy: IdentityFailurePolicy,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Option<Diagnostic> {
    match policy {
        IdentityFailurePolicy::Error => Some(Diagnostic::error(code, message)),
        IdentityFailurePolicy::Diagnostic => Some(Diagnostic::warning(code, message)),
        IdentityFailurePolicy::Ignore => None,
    }
}

fn row_identity_policy_from_details(row_identity: &serde_json::Value) -> EffectiveRowIdentity {
    let columns: Vec<String> = row_identity
        .get("columns")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let on_null_key = row_identity
        .get("on_null_key")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let on_duplicate_key = row_identity
        .get("on_duplicate_key")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    EffectiveRowIdentity {
        columns,
        on_null_key,
        on_duplicate_key,
    }
}

fn key_to_map(key_columns: &[String], key: &[String]) -> BTreeMap<String, String> {
    key_columns
        .iter()
        .cloned()
        .zip(key.iter().cloned())
        .collect()
}

fn keyed_rows_for_detail(key_columns: &[String], rows: &[UnmatchedRow]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|row| {
            let mut value = serde_json::Map::new();
            value.insert("row".into(), serde_json::json!(row.row_index));
            if let Some(key) = &row.key {
                value.insert(
                    "key".into(),
                    serde_json::json!(key_to_map(key_columns, key)),
                );
            } else {
                value.insert("key".into(), serde_json::Value::Null);
            }
            serde_json::Value::Object(value)
        })
        .collect()
}

fn keyed_tabular_extract(
    pair: &TabularDataPair,
    node: &DiffNode,
    aspect: &str,
) -> Option<ExtractResult> {
    let row_identity = row_identity_from_node_details(node)?;
    let left = pair.left.as_ref()?;
    let right = pair.right.as_ref()?;
    if !row_identity
        .columns
        .iter()
        .all(|col| left.column_index(col).is_some() && right.column_index(col).is_some())
    {
        return None;
    }
    let key_match = build_key_match(left, right, &row_identity);

    match aspect {
        "cells_changed" => Some(ExtractResult::Text(keyed_cells_changed_csv(
            left,
            right,
            &row_identity.columns,
            &key_match.matched,
        )?)),
        "rows_added" => Some(ExtractResult::Text(keyed_rows_csv(
            right,
            &key_match.unmatched_right,
            "No rows added.\n",
        )?)),
        "rows_removed" => Some(ExtractResult::Text(keyed_rows_csv(
            left,
            &key_match.unmatched_left,
            "No rows removed.\n",
        )?)),
        _ => None,
    }
}

fn row_identity_from_node_details(node: &DiffNode) -> Option<EffectiveRowIdentity> {
    let value = node.details.get("row_identity")?;
    let columns: Vec<String> = serde_json::from_value(value.get("columns")?.clone()).ok()?;
    if columns.is_empty() {
        return None;
    }
    Some(EffectiveRowIdentity {
        columns,
        on_null_key: value
            .get("on_null_key")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        on_duplicate_key: value
            .get("on_duplicate_key")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
    })
}

fn keyed_cells_changed_csv(
    left: &TabularData,
    right: &TabularData,
    key_columns: &[String],
    matched_rows: &[MatchedRow],
) -> Option<String> {
    let common_cols = tabular_columns_in_common_local(left, right);
    let mut writer = csv::Writer::from_writer(Vec::new());
    let mut header = key_columns.to_vec();
    header.push("column".into());
    header.push("old_value".into());
    header.push("new_value".into());
    writer.write_record(&header).ok()?;

    for matched in matched_rows {
        let row_l = &left.rows[matched.left_index];
        let row_r = &right.rows[matched.right_index];
        for col in &common_cols {
            let li = left.column_index(col)?;
            let ri = right.column_index(col)?;
            let lv = row_l.get(li).map(|s| s.as_str()).unwrap_or("");
            let rv = row_r.get(ri).map(|s| s.as_str()).unwrap_or("");
            if lv != rv {
                let mut record = matched.key.clone();
                record.push(col.clone());
                record.push(lv.to_string());
                record.push(rv.to_string());
                writer.write_record(&record).ok()?;
            }
        }
    }

    String::from_utf8(writer.into_inner().ok()?).ok()
}

fn keyed_rows_csv(
    table: &TabularData,
    rows: &[UnmatchedRow],
    empty_message: &str,
) -> Option<String> {
    if rows.is_empty() {
        return Some(empty_message.into());
    }
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(&table.headers).ok()?;
    for row in rows {
        writer.write_record(&table.rows[row.row_index]).ok()?;
    }
    String::from_utf8(writer.into_inner().ok()?).ok()
}

fn tabular_columns_in_common_local(left: &TabularData, right: &TabularData) -> Vec<String> {
    let left_set: BTreeSet<&str> = left.headers.iter().map(|s| s.as_str()).collect();
    right
        .headers
        .iter()
        .filter(|h| left_set.contains(h.as_str()))
        .cloned()
        .collect()
}
