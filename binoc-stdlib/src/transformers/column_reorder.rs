use binoc_core::ir::DiffNode;
use binoc_core::traits::{CompareContext, Transformer};
use binoc_core::types::*;

use crate::comparators::csv_compare::tabular_extract;

/// Detects pure column reordering in tabular diffs.
/// When the only difference between two CSVs is the column order (same columns,
/// same data), replaces the modify node with a reorder node.
///
/// Uses cached tabular data from the CompareContext when available, falling
/// back to details inspection for migrations loaded from JSON.
pub struct ColumnReorderDetector;

impl Transformer for ColumnReorderDetector {
    fn name(&self) -> &str {
        "binoc.column_reorder_detector"
    }

    fn match_types(&self) -> &[&str] {
        &["tabular"]
    }

    fn scope(&self) -> TransformScope {
        TransformScope::Node
    }

    fn transform(&self, mut node: DiffNode, ctx: &CompareContext) -> TransformResult {
        let has_reorder_tag = node.tags.contains("binoc.column-reorder");
        if !has_reorder_tag {
            return TransformResult::Unchanged;
        }

        let is_pure_reorder =
            if let Some(ReopenedData::Tabular(pair)) = ctx.get_cached_data(&node.path) {
                check_pure_reorder_from_data(&pair)
            } else {
                check_pure_reorder_from_details(&node)
            };

        if is_pure_reorder {
            node.kind = "reorder".into();
            node.summary = Some("Columns reordered (content unchanged)".into());
            node.tags.clear();
            node.tags.insert("binoc.column-reorder".into());
            TransformResult::Replace(node)
        } else {
            TransformResult::Unchanged
        }
    }

    fn extract(&self, data: &ReopenedData, node: &DiffNode, aspect: &str) -> Option<ExtractResult> {
        match aspect {
            "column_order" => {
                let ReopenedData::Tabular(pair) = data else {
                    return None;
                };
                let mut out = String::new();
                if let Some(left) = &pair.left {
                    out.push_str("before: ");
                    out.push_str(&left.headers.join(", "));
                    out.push('\n');
                }
                if let Some(right) = &pair.right {
                    out.push_str("after:  ");
                    out.push_str(&right.headers.join(", "));
                    out.push('\n');
                }
                Some(ExtractResult::Text(out))
            }
            _ => {
                let ReopenedData::Tabular(pair) = data else {
                    return None;
                };
                tabular_extract(pair, node, aspect)
            }
        }
    }
}

/// Check for pure reorder using actual tabular data.
fn check_pure_reorder_from_data(pair: &TabularDataPair) -> bool {
    let (Some(left), Some(right)) = (&pair.left, &pair.right) else {
        return false;
    };

    if left.rows.len() != right.rows.len() {
        return false;
    }

    use std::collections::BTreeSet;
    let left_cols: BTreeSet<&str> = left.headers.iter().map(|s| s.as_str()).collect();
    let right_cols: BTreeSet<&str> = right.headers.iter().map(|s| s.as_str()).collect();
    if left_cols != right_cols {
        return false;
    }

    // Verify all cell values match when indexed by column name
    for (i, left_row) in left.rows.iter().enumerate() {
        let right_row = &right.rows[i];
        for col in &left.headers {
            let li = left.column_index(col).unwrap();
            let ri = right.column_index(col).unwrap();
            let lv = left_row.get(li).map(|s| s.as_str()).unwrap_or("");
            let rv = right_row.get(ri).map(|s| s.as_str()).unwrap_or("");
            if lv != rv {
                return false;
            }
        }
    }

    true
}

/// Fallback: check using details metadata (for loaded migrations without cache).
fn check_pure_reorder_from_details(node: &DiffNode) -> bool {
    let no_col_adds = node
        .details
        .get("columns_added")
        .and_then(|v| v.as_array())
        .is_none_or(|a| a.is_empty());
    let no_col_removes = node
        .details
        .get("columns_removed")
        .and_then(|v| v.as_array())
        .is_none_or(|a| a.is_empty());
    let no_row_adds = node
        .details
        .get("rows_added")
        .and_then(|v| v.as_u64())
        .is_none_or(|n| n == 0);
    let no_row_removes = node
        .details
        .get("rows_removed")
        .and_then(|v| v.as_u64())
        .is_none_or(|n| n == 0);
    let no_cell_changes = node
        .details
        .get("cells_changed")
        .and_then(|v| v.as_u64())
        .is_none_or(|n| n == 0);

    no_col_adds && no_col_removes && no_row_adds && no_row_removes && no_cell_changes
}
