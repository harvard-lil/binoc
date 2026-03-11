use std::collections::{BTreeMap, BTreeSet};
use std::io::BufReader;

use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;

/// Column-level and row-level aware CSV comparator.
/// Emits `item_type: tabular` with the standard tabular detail schema.
/// Caches parsed tabular data in CompareContext for downstream transformers.
pub struct CsvComparator;

fn parse_csv(path: &std::path::Path) -> BinocResult<TabularData> {
    let file = std::fs::File::open(path).map_err(BinocError::Io)?;
    let reader = BufReader::new(file);
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(reader);

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| BinocError::Csv(e.to_string()))?
        .iter()
        .map(|s| s.to_string())
        .collect();

    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result.map_err(|e| BinocError::Csv(e.to_string()))?;
        rows.push(record.iter().map(|s| s.to_string()).collect());
    }

    Ok(TabularData { headers, rows })
}

impl Comparator for CsvComparator {
    fn name(&self) -> &str {
        "binoc.csv"
    }

    fn handles_extensions(&self) -> &[&str] {
        &[".csv", ".tsv"]
    }

    fn compare(&self, pair: &ItemPair, ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => self.compare_both(left, right, pair.logical_path(), ctx),
            (None, Some(right)) => {
                let csv = parse_csv(&right.physical_path)?;
                let summary = format!(
                    "New table ({} columns, {} rows)",
                    csv.headers.len(),
                    csv.rows.len()
                );
                let node = DiffNode::new("add", "tabular", &right.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc.content-changed")
                    .with_detail("columns", serde_json::json!(csv.headers))
                    .with_detail("rows", serde_json::json!(csv.rows.len()));

                ctx.cache_data(
                    &right.logical_path,
                    ReopenedData::Tabular(TabularDataPair {
                        left: None,
                        right: Some(csv),
                    }),
                );

                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let csv = parse_csv(&left.physical_path)?;
                let summary = format!(
                    "Table removed ({} columns, {} rows)",
                    csv.headers.len(),
                    csv.rows.len()
                );
                let node = DiffNode::new("remove", "tabular", &left.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc.content-changed")
                    .with_detail("columns", serde_json::json!(csv.headers))
                    .with_detail("rows", serde_json::json!(csv.rows.len()));

                ctx.cache_data(
                    &left.logical_path,
                    ReopenedData::Tabular(TabularDataPair {
                        left: Some(csv),
                        right: None,
                    }),
                );

                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }

    fn reopen_data(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<ReopenedData> {
        let left = pair
            .left
            .as_ref()
            .map(|item| parse_csv(&item.physical_path))
            .transpose()?;
        let right = pair
            .right
            .as_ref()
            .map(|item| parse_csv(&item.physical_path))
            .transpose()?;
        Ok(ReopenedData::Tabular(TabularDataPair { left, right }))
    }

    fn extract(&self, data: &ReopenedData, node: &DiffNode, aspect: &str) -> Option<ExtractResult> {
        let ReopenedData::Tabular(pair) = data else {
            return None;
        };
        tabular_extract(pair, node, aspect)
    }
}

/// Shared tabular extraction logic. Used by both the CSV comparator (when it's
/// the last toucher) and tabular transformers.
pub fn tabular_extract(
    pair: &TabularDataPair,
    _node: &DiffNode,
    aspect: &str,
) -> Option<ExtractResult> {
    match aspect {
        "rows_added" => {
            let right = pair.right.as_ref()?;
            let left_len = pair.left.as_ref().map_or(0, |l| l.rows.len());
            if left_len >= right.rows.len() {
                return Some(ExtractResult::Text("No rows added.\n".into()));
            }
            let added = TabularData {
                headers: right.headers.clone(),
                rows: right.rows[left_len..].to_vec(),
            };
            Some(ExtractResult::Text(added.to_csv()))
        }
        "rows_removed" => {
            let left = pair.left.as_ref()?;
            let right_len = pair.right.as_ref().map_or(0, |r| r.rows.len());
            if right_len >= left.rows.len() {
                return Some(ExtractResult::Text("No rows removed.\n".into()));
            }
            let removed = TabularData {
                headers: left.headers.clone(),
                rows: left.rows[right_len..].to_vec(),
            };
            Some(ExtractResult::Text(removed.to_csv()))
        }
        "cells_changed" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let common_cols = columns_in_common(left, right);
            let min_rows = left.rows.len().min(right.rows.len());

            let mut out = String::from("row,column,old_value,new_value\n");
            for i in 0..min_rows {
                for col in &common_cols {
                    let li = left.column_index(col)?;
                    let ri = right.column_index(col)?;
                    let lv = left.rows[i].get(li).map(|s| s.as_str()).unwrap_or("");
                    let rv = right.rows[i].get(ri).map(|s| s.as_str()).unwrap_or("");
                    if lv != rv {
                        out.push_str(&format!("{i},{col},{lv},{rv}\n"));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "columns_added" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let left_set: BTreeSet<&str> = left.headers.iter().map(|s| s.as_str()).collect();
            let added: Vec<&str> = right
                .headers
                .iter()
                .filter(|h| !left_set.contains(h.as_str()))
                .map(|h| h.as_str())
                .collect();
            if added.is_empty() {
                return Some(ExtractResult::Text("No columns added.\n".into()));
            }
            let mut out = String::new();
            for col in &added {
                out.push_str(&format!("{col}\n"));
                if let Some(vals) = right.column_values(col) {
                    for val in vals {
                        out.push_str(&format!("  {val}\n"));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "columns_removed" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let right_set: BTreeSet<&str> = right.headers.iter().map(|s| s.as_str()).collect();
            let removed: Vec<&str> = left
                .headers
                .iter()
                .filter(|h| !right_set.contains(h.as_str()))
                .map(|h| h.as_str())
                .collect();
            if removed.is_empty() {
                return Some(ExtractResult::Text("No columns removed.\n".into()));
            }
            let mut out = String::new();
            for col in &removed {
                out.push_str(&format!("{col}\n"));
                if let Some(vals) = left.column_values(col) {
                    for val in vals {
                        out.push_str(&format!("  {val}\n"));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "content" | "full" => {
            let mut out = String::new();
            if let Some(left) = &pair.left {
                out.push_str("--- left\n");
                out.push_str(&left.to_csv());
            }
            if let Some(right) = &pair.right {
                out.push_str("+++ right\n");
                out.push_str(&right.to_csv());
            }
            Some(ExtractResult::Text(out))
        }
        _ => None,
    }
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
        // Capitalize first letter
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

fn columns_in_common(left: &TabularData, right: &TabularData) -> Vec<String> {
    let left_set: BTreeSet<&str> = left.headers.iter().map(|s| s.as_str()).collect();
    right
        .headers
        .iter()
        .filter(|h| left_set.contains(h.as_str()))
        .cloned()
        .collect()
}

impl CsvComparator {
    fn compare_both(
        &self,
        left: &Item,
        right: &Item,
        logical_path: &str,
        ctx: &CompareContext,
    ) -> BinocResult<CompareResult> {
        let csv_l = parse_csv(&left.physical_path)?;
        let csv_r = parse_csv(&right.physical_path)?;

        let headers_l: BTreeSet<&str> = csv_l.headers.iter().map(|s| s.as_str()).collect();
        let headers_r: BTreeSet<&str> = csv_r.headers.iter().map(|s| s.as_str()).collect();

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
            let common_order_l: Vec<&str> = csv_l
                .headers
                .iter()
                .filter(|h| columns_common.contains(h))
                .map(|s| s.as_str())
                .collect();
            let common_order_r: Vec<&str> = csv_r
                .headers
                .iter()
                .filter(|h| columns_common.contains(h))
                .map(|s| s.as_str())
                .collect();
            common_order_l != common_order_r
        };

        let col_idx_l: BTreeMap<&str, usize> = csv_l
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| (h.as_str(), i))
            .collect();
        let col_idx_r: BTreeMap<&str, usize> = csv_r
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| (h.as_str(), i))
            .collect();

        let min_rows = csv_l.rows.len().min(csv_r.rows.len());
        let mut cells_changed: u64 = 0;

        for i in 0..min_rows {
            let row_l = &csv_l.rows[i];
            let row_r = &csv_r.rows[i];
            for col in &columns_common {
                let val_l = col_idx_l
                    .get(col.as_str())
                    .and_then(|&j| row_l.get(j))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let val_r = col_idx_r
                    .get(col.as_str())
                    .and_then(|&j| row_r.get(j))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                if val_l != val_r {
                    cells_changed += 1;
                }
            }
        }

        let rows_added = csv_r.rows.len().saturating_sub(csv_l.rows.len()) as u64;
        let rows_removed = csv_l.rows.len().saturating_sub(csv_r.rows.len()) as u64;

        if columns_added.is_empty()
            && columns_removed.is_empty()
            && !order_changed
            && rows_added == 0
            && rows_removed == 0
            && cells_changed == 0
        {
            return Ok(CompareResult::Identical);
        }

        let mut node = DiffNode::new("modify", "tabular", logical_path)
            .with_detail("columns_left", serde_json::json!(csv_l.headers))
            .with_detail("columns_right", serde_json::json!(csv_r.headers))
            .with_detail("columns_added", serde_json::json!(columns_added))
            .with_detail("columns_removed", serde_json::json!(columns_removed))
            .with_detail("rows_left", serde_json::json!(csv_l.rows.len()))
            .with_detail("rows_right", serde_json::json!(csv_r.rows.len()))
            .with_detail("rows_added", serde_json::json!(rows_added))
            .with_detail("rows_removed", serde_json::json!(rows_removed))
            .with_detail("cells_changed", serde_json::json!(cells_changed));

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

        node.summary = Some(tabular_summary(
            &columns_added,
            &columns_removed,
            order_changed,
            rows_added,
            rows_removed,
            cells_changed,
        ));

        // Cache the parsed data for downstream transformers and extract
        ctx.cache_data(
            logical_path,
            ReopenedData::Tabular(TabularDataPair {
                left: Some(csv_l),
                right: Some(csv_r),
            }),
        );

        Ok(CompareResult::Leaf(node))
    }
}
