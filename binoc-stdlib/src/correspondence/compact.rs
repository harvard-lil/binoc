use binoc_sdk::{
    tabular_v1, BinocResult, CompactionRule, DataAccess, Edit, LinkCtx, TabularData, Value,
};
use serde_json::json;

use super::tabular::load_tabular;

const MAX_ROW_ALIGNMENT_ROWS: usize = 512;
const COLUMN_RENAME_MIN_MATCHES: usize = 2;
const COLUMN_RENAME_MIN_MATCH_RATIO: f64 = 0.10;

pub struct ColumnReorder;

pub struct ColumnRename;

impl CompactionRule for ColumnReorder {
    fn name(&self) -> &str {
        "binoc.compact.column_reorder"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        let Some(header) = edits.iter().find(|edit| edit.verb == "tabular.set_headers") else {
            return Ok(None);
        };
        let Some(from) = header.params.get("from").and_then(string_array) else {
            return Ok(None);
        };
        let Some(to) = header.params.get("to").and_then(string_array) else {
            return Ok(None);
        };
        let common_from: Vec<&String> = from.iter().filter(|header| to.contains(header)).collect();
        let common_to: Vec<&String> = to.iter().filter(|header| from.contains(header)).collect();
        if common_from.len() < 2 || common_from == common_to {
            return Ok(None);
        }
        let moved = common_from.iter().any(|name| {
            from.iter().position(|header| header == *name)
                != to.iter().position(|header| header == *name)
        });
        if !moved {
            return Ok(None);
        }
        let mut out = Vec::new();
        for edit in edits {
            if edit.verb == "tabular.set_headers" {
                out.push(
                    Edit::new("tabular.reorder_columns", json!({ "order": common_to }))
                        .with_item_type("tabular")
                        .with_tag("binoc.column-reorder")
                        .with_tag("binoc.schema-change")
                        .with_summary("Columns reordered"),
                );
            } else {
                out.push(edit.clone());
            }
        }
        Ok(Some(out))
    }
}

impl CompactionRule for ColumnRename {
    fn name(&self) -> &str {
        "binoc.compact.column_rename"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        ctx: &LinkCtx<'_>,
        edits: &[Edit],
        data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        let (Some(left), Some(right)) = (
            load_tabular(ctx, ctx.link.left, data)?,
            load_tabular(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };
        Ok(rewrite_column_renames(edits, &left, &right))
    }
}

fn rewrite_column_renames(
    edits: &[Edit],
    left: &TabularData,
    right: &TabularData,
) -> Option<Vec<Edit>> {
    let matches = column_rename_matches(edits, left, right);
    if matches.is_empty() {
        return None;
    }
    let redundant_headers = redundant_header_edits(edits, &matches);

    let mut out = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        if redundant_headers.contains(&index) {
            continue;
        }
        if let Some(rename) = matches.iter().find(|rename| rename.first_index() == index) {
            out.push(
                Edit::new(
                    "tabular.rename_column",
                    json!({ "from": rename.from, "to": rename.to }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.column-rename")
                .with_tag("binoc.schema-change")
                .with_summary(
                    binoc_sdk::Summary::new()
                        .text("Column renamed: '")
                        .text(rename.from.clone())
                        .text("' -> '")
                        .text(rename.to.clone())
                        .text("'"),
                ),
            );
            out.extend(rename.value_edits.clone());
            continue;
        }
        if matches.iter().any(|rename| rename.contains(index)) {
            continue;
        }
        out.push(edit.clone());
    }

    Some(out)
}

#[derive(Debug, Clone)]
struct ColumnRenameMatch {
    remove_index: usize,
    add_index: usize,
    from: String,
    to: String,
    score: usize,
    value_edits: Vec<Edit>,
}

impl ColumnRenameMatch {
    fn contains(&self, index: usize) -> bool {
        self.remove_index == index || self.add_index == index
    }

    fn first_index(&self) -> usize {
        self.remove_index.min(self.add_index)
    }
}

#[derive(Debug, Clone)]
struct ColumnEdit {
    index: usize,
    name: String,
    values: Vec<Value>,
}

fn column_rename_matches(
    edits: &[Edit],
    left: &TabularData,
    right: &TabularData,
) -> Vec<ColumnRenameMatch> {
    let removes = removed_columns(edits, left, right);
    let adds = added_columns(edits, left, right);
    if removes.is_empty() || adds.is_empty() {
        return Vec::new();
    }
    let row_alignment = edits
        .iter()
        .find(|edit| edit.verb == "tabular.row_alignment_basis")
        .and_then(RowAlignmentBasis::from_edit)
        .map(|basis| lcs_pairs(&basis.left, &basis.right));

    let mut candidates = Vec::new();
    for remove in &removes {
        for add in &adds {
            if let Some((score, value_edits)) = column_rename_score(
                &add.name,
                &remove.values,
                &add.values,
                row_alignment.as_deref(),
            ) {
                candidates.push(ColumnRenameMatch {
                    remove_index: remove.index,
                    add_index: add.index,
                    from: remove.name.clone(),
                    to: add.name.clone(),
                    score,
                    value_edits,
                });
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| a.remove_index.cmp(&b.remove_index))
            .then_with(|| a.add_index.cmp(&b.add_index))
    });

    let mut used_removes = Vec::new();
    let mut used_adds = Vec::new();
    let mut matches = Vec::new();
    for candidate in candidates {
        if used_removes.contains(&candidate.remove_index)
            || used_adds.contains(&candidate.add_index)
        {
            continue;
        }
        used_removes.push(candidate.remove_index);
        used_adds.push(candidate.add_index);
        matches.push(candidate);
    }
    matches.sort_by_key(ColumnRenameMatch::first_index);
    matches
}

fn removed_columns(edits: &[Edit], left: &TabularData, right: &TabularData) -> Vec<ColumnEdit> {
    column_edits(edits, "tabular.remove_column")
        .into_iter()
        .filter_map(|(index, name)| {
            if right.headers.contains(&name) {
                return None;
            }
            let values = left.column_values(&name)?.into_iter().cloned().collect();
            Some(ColumnEdit {
                index,
                name,
                values,
            })
        })
        .collect()
}

fn added_columns(edits: &[Edit], left: &TabularData, right: &TabularData) -> Vec<ColumnEdit> {
    column_edits(edits, "tabular.add_column")
        .into_iter()
        .filter_map(|(index, name)| {
            if left.headers.contains(&name) {
                return None;
            }
            let values = right.column_values(&name)?.into_iter().cloned().collect();
            Some(ColumnEdit {
                index,
                name,
                values,
            })
        })
        .collect()
}

fn redundant_header_edits(edits: &[Edit], matches: &[ColumnRenameMatch]) -> Vec<usize> {
    edits
        .iter()
        .enumerate()
        .filter_map(|(index, edit)| {
            (edit.verb == "tabular.set_headers"
                && header_change_is_explained_by_renames(edit, matches))
            .then_some(index)
        })
        .collect()
}

fn header_change_is_explained_by_renames(edit: &Edit, matches: &[ColumnRenameMatch]) -> bool {
    let Some(from) = edit.params.get("from") else {
        return false;
    };
    let Some(to_value) = edit.params.get("to") else {
        return false;
    };
    let Some(mut renamed) = string_array(from) else {
        return false;
    };
    let Some(to) = string_array(to_value) else {
        return false;
    };
    for header in &mut renamed {
        if let Some(rename) = matches.iter().find(|rename| rename.from == *header) {
            *header = rename.to.clone();
        }
    }
    renamed == to
}

fn column_edits(edits: &[Edit], verb: &str) -> Vec<(usize, String)> {
    edits
        .iter()
        .enumerate()
        .filter_map(|(index, edit)| {
            if edit.verb != verb {
                return None;
            }
            let name = edit.params.get("name")?.as_str()?.to_string();
            Some((index, name))
        })
        .collect()
}

fn column_rename_score(
    to_name: &str,
    left: &[Value],
    right: &[Value],
    row_alignment: Option<&[(usize, usize)]>,
) -> Option<(usize, Vec<Edit>)> {
    if let Some(row_alignment) = row_alignment {
        return column_rename_score_for_aligned_rows(to_name, left, right, row_alignment);
    }

    let value_alignment = lcs_pairs(left, right);
    let matches = value_alignment
        .iter()
        .filter(|(left_index, right_index)| {
            let left_value = &left[*left_index];
            let right_value = &right[*right_index];
            !left_value.is_blank() && left_value == right_value
        })
        .count();
    let comparable = left
        .iter()
        .filter(|value| !value.is_blank())
        .count()
        .min(right.iter().filter(|value| !value.is_blank()).count());
    if comparable == 0
        || matches < COLUMN_RENAME_MIN_MATCHES
        || (matches as f64 / comparable as f64) < COLUMN_RENAME_MIN_MATCH_RATIO
    {
        return None;
    }

    if left.len() == right.len() {
        let value_edits = left
            .iter()
            .zip(right)
            .enumerate()
            .filter(|(_, (from, to))| from != to)
            .map(|(row, (from, to))| column_value_edit(row, to_name, from, to))
            .collect();
        return Some((matches, value_edits));
    }

    let shorter = left.len().min(right.len());
    (value_alignment.len() == shorter).then_some((matches, Vec::new()))
}

fn column_rename_score_for_aligned_rows(
    to_name: &str,
    left: &[Value],
    right: &[Value],
    row_alignment: &[(usize, usize)],
) -> Option<(usize, Vec<Edit>)> {
    let comparable = row_alignment
        .iter()
        .filter(|(left_index, right_index)| {
            left.get(*left_index).is_some_and(|value| !value.is_blank())
                && right
                    .get(*right_index)
                    .is_some_and(|value| !value.is_blank())
        })
        .count();
    let matches = row_alignment
        .iter()
        .filter(|(left_index, right_index)| {
            let Some(left_value) = left.get(*left_index) else {
                return false;
            };
            let Some(right_value) = right.get(*right_index) else {
                return false;
            };
            !left_value.is_blank() && left_value == right_value
        })
        .count();
    if comparable == 0
        || matches < COLUMN_RENAME_MIN_MATCHES
        || (matches as f64 / comparable as f64) < COLUMN_RENAME_MIN_MATCH_RATIO
    {
        return None;
    }

    let value_edits = row_alignment
        .iter()
        .filter_map(|(left_index, right_index)| {
            let from = left.get(*left_index)?;
            let to = right.get(*right_index)?;
            (from != to).then(|| column_value_edit(*right_index, to_name, from, to))
        })
        .collect();
    Some((matches, value_edits))
}

fn column_value_edit(row: usize, column: &str, from: &Value, to: &Value) -> Edit {
    Edit::new(
        "tabular.edit_cell",
        json!({
            "row": row,
            "column": column,
            "from": from.to_json(),
            "to": to.to_json(),
        }),
    )
    .with_item_type("tabular")
    .with_tag("binoc.cell-change")
}

pub struct RowAdditionConsolidation;

pub struct RowAlignment;

impl CompactionRule for RowAlignment {
    fn name(&self) -> &str {
        "binoc.compact.row_alignment"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        Ok(rewrite_row_alignment(edits))
    }
}

fn rewrite_row_alignment(edits: &[Edit]) -> Option<Vec<Edit>> {
    let basis_index = edits
        .iter()
        .position(|edit| edit.verb == "tabular.row_alignment_basis")?;
    let basis = RowAlignmentBasis::from_edit(&edits[basis_index])?;
    let alignment = lcs_pairs(&basis.left, &basis.right);
    let mut matched_left = vec![false; basis.left.len()];
    let mut matched_right = vec![false; basis.right.len()];
    for (left, right) in alignment {
        matched_left[left] = true;
        matched_right[right] = true;
    }

    let unmatched_left = matched_left.iter().filter(|matched| !**matched).count();
    let unmatched_right: Vec<usize> = matched_right
        .iter()
        .enumerate()
        .filter_map(|(index, matched)| (!*matched).then_some(index))
        .collect();

    let mut out: Vec<Edit> = edits
        .iter()
        .enumerate()
        .filter(|(index, edit)| {
            *index != basis_index
                && !basis.owns_cell_edit(edit)
                && edit.verb != "tabular.add_row"
                && edit.verb != "tabular.remove_row"
        })
        .map(|(_, edit)| edit.clone())
        .collect();

    if unmatched_left == 0 && !unmatched_right.is_empty() {
        for index in unmatched_right {
            out.push(
                Edit::new(
                    "tabular.add_row",
                    json!({
                        "index": index,
                        "values": basis.right_rows.get(index).cloned().unwrap_or_else(|| json!([])),
                    }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.row-addition"),
            );
        }
    } else {
        out.extend(
            edits
                .iter()
                .enumerate()
                .filter(|(index, edit)| {
                    *index != basis_index
                        && (basis.owns_cell_edit(edit)
                            || edit.verb == "tabular.add_row"
                            || edit.verb == "tabular.remove_row")
                })
                .map(|(_, edit)| edit.clone()),
        );
    }

    Some(out)
}

struct RowAlignmentBasis {
    columns: Vec<String>,
    left: Vec<String>,
    right: Vec<String>,
    right_rows: Vec<serde_json::Value>,
}

impl RowAlignmentBasis {
    fn from_edit(edit: &Edit) -> Option<Self> {
        let columns = string_array(edit.params.get("columns")?)?;
        let left = string_array(edit.params.get("left")?)?;
        let right = string_array(edit.params.get("right")?)?;
        if left.len() > MAX_ROW_ALIGNMENT_ROWS || right.len() > MAX_ROW_ALIGNMENT_ROWS {
            return None;
        }
        let right_rows = edit.params.get("right_rows")?.as_array()?.clone();
        Some(Self {
            columns,
            left,
            right,
            right_rows,
        })
    }

    fn owns_cell_edit(&self, edit: &Edit) -> bool {
        edit.verb == "tabular.edit_cell"
            && edit
                .params
                .get("column")
                .and_then(|value| value.as_str())
                .is_some_and(|column| self.columns.iter().any(|basis| basis == column))
    }
}

fn string_array(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn lcs_pairs<T: PartialEq>(left: &[T], right: &[T]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            lengths[left_index][right_index] = if left[left_index] == right[right_index] {
                lengths[left_index + 1][right_index + 1] + 1
            } else {
                lengths[left_index + 1][right_index].max(lengths[left_index][right_index + 1])
            };
        }
    }

    let mut pairs = Vec::new();
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            pairs.push((left_index, right_index));
            left_index += 1;
            right_index += 1;
        } else if lengths[left_index + 1][right_index] >= lengths[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    pairs
}

impl CompactionRule for RowAdditionConsolidation {
    fn name(&self) -> &str {
        "binoc.compact.row_addition"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        Ok(rewrite_row_addition_consolidation(edits))
    }
}

fn rewrite_row_addition_consolidation(edits: &[Edit]) -> Option<Vec<Edit>> {
    let mut out = Vec::new();
    let mut run = Vec::new();
    let mut rewrote = false;

    fn flush(out: &mut Vec<Edit>, run: &mut Vec<(u64, serde_json::Value)>, rewrote: &mut bool) {
        if run.len() >= 2 {
            let start = run[0].0;
            let rows: Vec<serde_json::Value> =
                run.iter().map(|(_, values)| values.clone()).collect();
            out.push(
                Edit::new(
                    "tabular.append_rows",
                    json!({ "start": start, "rows": rows }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.row-addition"),
            );
            *rewrote = true;
        } else {
            for (index, values) in run.drain(..) {
                out.push(
                    Edit::new(
                        "tabular.add_row",
                        json!({ "index": index, "values": values }),
                    )
                    .with_item_type("tabular")
                    .with_tag("binoc.row-addition"),
                );
            }
        }
        run.clear();
    }

    for edit in edits {
        let row = (edit.verb == "tabular.add_row")
            .then(|| {
                Some((
                    edit.params.get("index")?.as_u64()?,
                    edit.params.get("values")?.clone(),
                ))
            })
            .flatten();
        match row {
            Some((index, values)) => {
                if let Some((last, _)) = run.last() {
                    if index != last + 1 {
                        flush(&mut out, &mut run, &mut rewrote);
                    }
                }
                run.push((index, values));
            }
            None => {
                flush(&mut out, &mut run, &mut rewrote);
                out.push(edit.clone());
            }
        }
    }
    flush(&mut out, &mut run, &mut rewrote);

    rewrote.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_column(name: &str, values: &[&str]) -> Edit {
        Edit::new(
            "tabular.add_column",
            json!({
                "name": name,
                "values": {
                    "total_values": values.len(),
                    "truncated": false,
                    "values": values,
                }
            }),
        )
        .with_item_type("tabular")
        .with_tag("binoc.column-addition")
        .with_tag("binoc.schema-change")
    }

    fn remove_column(name: &str, values: &[&str]) -> Edit {
        Edit::new(
            "tabular.remove_column",
            json!({
                "name": name,
                "values": {
                    "total_values": values.len(),
                    "truncated": false,
                    "values": values,
                }
            }),
        )
        .with_item_type("tabular")
        .with_tag("binoc.column-removal")
        .with_tag("binoc.schema-change")
    }

    fn table(headers: &[&str], rows: &[&[&str]]) -> TabularData {
        TabularData::from_string_rows(
            headers.iter().map(|header| (*header).to_string()).collect(),
            rows.iter()
                .map(|row| row.iter().map(|value| (*value).to_string()).collect())
                .collect(),
        )
    }

    fn basis(left: &[&str], right: &[&str], right_rows: Vec<serde_json::Value>) -> Edit {
        Edit::new(
            "tabular.row_alignment_basis",
            json!({
                "columns": ["name", "age"],
                "left": left,
                "right": right,
                "right_rows": right_rows,
            }),
        )
        .hidden()
    }

    #[test]
    fn column_rename_rewrites_matching_add_remove_columns() {
        let edits = vec![
            Edit::new(
                "tabular.set_headers",
                json!({"from": ["id", "a", "score"], "to": ["id", "b", "score"]}),
            )
            .with_item_type("tabular"),
            add_column("b", &["alpha", "beta", "gamma", "delta"]),
            remove_column("a", &["alpha", "beta", "delta"]),
            Edit::new("tabular.add_row", json!({"index": 2})).with_item_type("tabular"),
        ];

        let left = table(
            &["id", "a", "score"],
            &[
                &["1", "alpha", "10"],
                &["2", "beta", "20"],
                &["4", "delta", "40"],
            ],
        );
        let right = table(
            &["id", "b", "score"],
            &[
                &["1", "alpha", "10"],
                &["2", "beta", "20"],
                &["3", "gamma", "30"],
                &["4", "delta", "40"],
            ],
        );

        let rewritten = rewrite_column_renames(&edits, &left, &right).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.rename_column");
        assert_eq!(rewritten[0].params, json!({"from": "a", "to": "b"}));
        assert!(rewritten[0]
            .projection
            .hint
            .tags
            .contains(&"binoc.column-rename".into()));
        assert!(!rewritten
            .iter()
            .any(|edit| edit.verb == "tabular.add_column" || edit.verb == "tabular.remove_column"));
    }

    #[test]
    fn column_rename_keeps_changed_values_as_cell_edits() {
        let edits = vec![
            Edit::new(
                "tabular.set_headers",
                json!({"from": ["id", "status"], "to": ["id", "state"]}),
            )
            .with_item_type("tabular"),
            add_column("state", &["active", "paused", "closed", "archived"]),
            remove_column("status", &["active", "pending", "closed", "archived"]),
        ];

        let left = table(
            &["id", "status"],
            &[
                &["1", "active"],
                &["2", "pending"],
                &["3", "closed"],
                &["4", "archived"],
            ],
        );
        let right = table(
            &["id", "state"],
            &[
                &["1", "active"],
                &["2", "paused"],
                &["3", "closed"],
                &["4", "archived"],
            ],
        );

        let rewritten = rewrite_column_renames(&edits, &left, &right).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.rename_column");
        assert_eq!(
            rewritten[0].params,
            json!({"from": "status", "to": "state"})
        );
        assert_eq!(rewritten[1].verb, "tabular.edit_cell");
        assert_eq!(
            rewritten[1].params,
            json!({"row": 1, "column": "state", "from": "pending", "to": "paused"})
        );
    }

    #[test]
    fn column_rename_ignores_unrelated_add_remove_columns() {
        let edits = vec![
            add_column("email", &["a@example.test", "b@example.test"]),
            remove_column("legacy", &["alpha", "beta"]),
        ];

        let left = table(&["legacy"], &[&["alpha"], &["beta"]]);
        let right = table(&["email"], &[&["a@example.test"], &["b@example.test"]]);

        assert!(rewrite_column_renames(&edits, &left, &right).is_none());
    }

    #[test]
    fn column_rename_requires_more_than_one_matching_value() {
        let edits = vec![
            add_column("status", &["active", "draft", "hold", "closed"]),
            remove_column("legacy", &["active", "x", "y", "z"]),
        ];

        let left = table(&["legacy"], &[&["active"], &["x"], &["y"], &["z"]]);
        let right = table(
            &["status"],
            &[&["active"], &["draft"], &["hold"], &["closed"]],
        );

        assert!(rewrite_column_renames(&edits, &left, &right).is_none());
    }

    #[test]
    fn row_alignment_rewrites_mid_table_insertion() {
        let edits = vec![
            Edit::new(
                "tabular.reorder_columns",
                json!({"order": ["city", "name", "age"]}),
            )
            .with_item_type("tabular")
            .with_tag("binoc.column-reorder"),
            Edit::new(
                "tabular.add_column",
                json!({"name": "email", "values": {"values": ["alice@example.test", "bob@example.test", "charlie@example.test"]}}),
            )
            .with_item_type("tabular")
            .with_tag("binoc.column-addition"),
            basis(
                &["alice", "charlie"],
                &["alice", "bob", "charlie"],
                vec![
                    json!({"values": ["Alice", "30"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Bob", "25"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Charlie", "35"], "total_values": 2, "truncated": false}),
                ],
            ),
            Edit::new(
                "tabular.edit_cell",
                json!({"row": 1, "column": "name", "from": "Charlie", "to": "Bob"}),
            ),
            Edit::new(
                "tabular.edit_cell",
                json!({"row": 1, "column": "age", "from": "35", "to": "25"}),
            ),
            Edit::new(
                "tabular.add_row",
                json!({
                    "index": 2,
                    "values": {"values": ["Charlie", "35"], "total_values": 2, "truncated": false}
                }),
            ),
        ];

        let rewritten = rewrite_row_alignment(&edits).expect("rewrite");

        assert_eq!(rewritten.len(), 3);
        assert_eq!(rewritten[0].verb, "tabular.reorder_columns");
        assert_eq!(rewritten[1].verb, "tabular.add_column");
        assert_eq!(rewritten[2].verb, "tabular.add_row");
        assert_eq!(rewritten[2].params["index"], json!(1));
        assert_eq!(
            rewritten[2].params["values"],
            json!({"values": ["Bob", "25"], "total_values": 2, "truncated": false})
        );
    }

    #[test]
    fn row_alignment_keeps_visible_edits_when_left_rows_are_unmatched() {
        let cell = Edit::new(
            "tabular.edit_cell",
            json!({"row": 0, "column": "name", "from": "Alice", "to": "Alicia"}),
        );
        let edits = vec![
            basis(
                &["alice"],
                &["alicia"],
                vec![json!({"values": ["Alicia"], "total_values": 1, "truncated": false})],
            ),
            cell.clone(),
        ];

        let rewritten = rewrite_row_alignment(&edits).expect("basis cleanup");

        assert_eq!(rewritten, vec![cell]);
    }
}
