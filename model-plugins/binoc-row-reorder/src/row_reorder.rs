use std::collections::BTreeMap;

use binoc_sdk::*;

/// Detects when a tabular dataset has been re-sorted: same rows as a
/// multiset, but in a different order.
///
/// Consumes [`TABULAR_V1`] artifacts — source-format-agnostic.
#[derive(Default)]
pub struct RowReorderDetector;

impl Transformer for RowReorderDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.row_reorder_detector")
            .with_match_artifacts(vec![tabular_v1()])
            .with_match_tags(vec!["binoc.cell-change".into()])
    }

    fn transform(&self, node: DiffNode, data: &dyn DataAccess) -> TransformResult {
        let Some(pair) = TabularDataPair::from_artifacts(&node, data) else {
            return TransformResult::Unchanged;
        };
        let (Some(left), Some(right)) = (&pair.left, &pair.right) else {
            return TransformResult::Unchanged;
        };

        if left.headers != right.headers {
            return TransformResult::Unchanged;
        }

        if left.rows.len() != right.rows.len() {
            return TransformResult::Unchanged;
        }

        let left_bag = row_multiset(&left.rows);
        let right_bag = row_multiset(&right.rows);
        if left_bag != right_bag {
            return TransformResult::Unchanged;
        }

        if left.rows == right.rows {
            return TransformResult::Unchanged;
        }

        let new_node = node
            .with_tag("binoc.row-reorder")
            .with_summary("Rows reordered (same data, different sort order)".to_string());
        TransformResult::Replace(Box::new(new_node))
    }
}

fn row_multiset(rows: &[Vec<String>]) -> BTreeMap<Vec<String>, usize> {
    let mut bag = BTreeMap::new();
    for row in rows {
        *bag.entry(row.clone()).or_insert(0) += 1;
    }
    bag
}
