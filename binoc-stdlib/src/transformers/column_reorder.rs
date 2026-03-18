use binoc_sdk::*;

use crate::comparators::csv_compare::{tabular_extract, tabular_pair_from_source};

/// Detects pure column reordering in tabular diffs.
pub struct ColumnReorderDetector;

impl Transformer for ColumnReorderDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.column_reorder_detector")
            .with_match_types(vec!["tabular".into()])
    }

    fn transform(&self, mut node: DiffNode, data: &dyn DataAccess) -> TransformResult {
        let has_reorder_tag = node.tags.contains("binoc.column-reorder");
        if !has_reorder_tag {
            return TransformResult::Unchanged;
        }

        let is_pure_reorder = if let Some(pair) = tabular_pair_from_source(&node, data) {
            check_pure_reorder_from_data(&pair)
        } else {
            check_pure_reorder_from_details(&node)
        };

        if is_pure_reorder {
            node.kind = "reorder".into();
            node.summary = Some("Columns reordered (content unchanged)".into());
            node.tags.clear();
            node.tags.insert("binoc.column-reorder".into());
            TransformResult::Replace(Box::new(node))
        } else {
            TransformResult::Unchanged
        }
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let pair = tabular_pair_from_source(node, data)?;
        match aspect {
            "column_order" => {
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
            _ => tabular_extract(&pair, node, aspect),
        }
    }
}

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
