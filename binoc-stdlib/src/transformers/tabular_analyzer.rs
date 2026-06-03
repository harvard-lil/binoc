use std::collections::BTreeSet;

use binoc_sdk::*;

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
        _config: &serde_json::Value,
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
            "modify" | "move" => transform_modify(node, &pair),
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

fn transform_modify(mut node: DiffNode, pair: &TabularDataPair) -> TransformResult {
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
