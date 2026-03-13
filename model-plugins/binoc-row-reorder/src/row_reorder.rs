use std::collections::BTreeMap;
use std::io::BufReader;

use binoc_sdk::*;

/// Detects when a tabular dataset has been re-sorted: same rows as a
/// multiset, but in a different order. Parses source CSV files directly
/// via `node.source_items` rather than relying on a cache.
#[derive(Default)]
pub struct RowReorderDetector;

impl Transformer for RowReorderDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.row_reorder_detector")
            .with_match_types(vec!["tabular".into()])
    }

    fn transform(&self, node: DiffNode, data: &dyn DataAccess) -> TransformResult {
        let Some(pair) = tabular_pair_from_source(&node, data) else {
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

fn parse_csv(path: &std::path::Path) -> Option<TabularData> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(reader);

    let headers: Vec<String> = rdr.headers().ok()?.iter().map(|s| s.to_string()).collect();

    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result.ok()?;
        rows.push(record.iter().map(|s| s.to_string()).collect());
    }

    Some(TabularData { headers, rows })
}

fn tabular_pair_from_source(node: &DiffNode, data: &dyn DataAccess) -> Option<TabularDataPair> {
    let pair = node.source_items.as_ref()?;
    let left = pair
        .left
        .as_ref()
        .and_then(|item| data.local_path(item).ok())
        .and_then(|p| parse_csv(&p));
    let right = pair
        .right
        .as_ref()
        .and_then(|item| data.local_path(item).ok())
        .and_then(|p| parse_csv(&p));
    if left.is_none() && right.is_none() {
        return None;
    }
    Some(TabularDataPair { left, right })
}

fn row_multiset(rows: &[Vec<String>]) -> BTreeMap<Vec<String>, usize> {
    let mut bag = BTreeMap::new();
    for row in rows {
        *bag.entry(row.clone()).or_insert(0) += 1;
    }
    bag
}
