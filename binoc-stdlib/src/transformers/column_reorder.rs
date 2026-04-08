use binoc_sdk::*;

/// Detects pure column reordering in tabular diffs.
///
/// Matches any node with [`tabular_v1`] artifacts and checks whether
/// the columns are reordered with no other data changes. Source-format-
/// agnostic — works with any comparator that publishes tabular artifacts.
pub struct ColumnReorderDetector;

impl Transformer for ColumnReorderDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.column_reorder_detector")
            .with_match_artifacts(vec![tabular_v1()])
            .with_match_tags(vec!["binoc.column-reorder".into()])
    }

    fn transform(&self, mut node: DiffNode, data: &dyn DataAccess) -> TransformResult {
        let is_pure_reorder = if let Some(pair) = TabularDataPair::from_artifacts(&node, data) {
            check_pure_reorder_from_data(&pair)
        } else {
            return TransformResult::Unchanged;
        };

        if is_pure_reorder {
            node.action = "reorder".into();
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
        let pair = TabularDataPair::from_artifacts(node, data)?;
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
