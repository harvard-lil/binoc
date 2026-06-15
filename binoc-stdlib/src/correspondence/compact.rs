use binoc_sdk::{CompactionRule, Edit};
use serde_json::json;

const MAX_ROW_ALIGNMENT_ROWS: usize = 512;

pub struct ColumnReorder;

impl CompactionRule for ColumnReorder {
    fn name(&self) -> &str {
        "binoc.compact.column_reorder"
    }

    fn rewrite(&self, edits: &[Edit]) -> Option<Vec<Edit>> {
        let header = edits
            .iter()
            .find(|edit| edit.verb == "tabular.set_headers")?;
        let from: Vec<String> = header
            .params
            .get("from")?
            .as_array()?
            .iter()
            .map(|value| value.as_str().map(str::to_string))
            .collect::<Option<_>>()?;
        let to: Vec<String> = header
            .params
            .get("to")?
            .as_array()?
            .iter()
            .map(|value| value.as_str().map(str::to_string))
            .collect::<Option<_>>()?;
        let common_from: Vec<&String> = from.iter().filter(|header| to.contains(header)).collect();
        let common_to: Vec<&String> = to.iter().filter(|header| from.contains(header)).collect();
        if common_from.len() < 2 || common_from == common_to {
            return None;
        }
        let moved = common_from.iter().any(|name| {
            from.iter().position(|header| header == *name)
                != to.iter().position(|header| header == *name)
        });
        if !moved {
            return None;
        }
        let mut out = Vec::new();
        for edit in edits {
            if edit.verb == "tabular.set_headers" {
                out.push(
                    Edit::new("tabular.reorder_columns", json!({ "order": common_to }))
                        .with_item_type("tabular")
                        .with_tag("binoc.column-reorder")
                        .with_tag("binoc.schema-change"),
                );
            } else {
                out.push(edit.clone());
            }
        }
        Some(out)
    }
}

pub struct RowAdditionConsolidation;

pub struct RowAlignment;

impl CompactionRule for RowAlignment {
    fn name(&self) -> &str {
        "binoc.compact.row_alignment"
    }

    fn rewrite(&self, edits: &[Edit]) -> Option<Vec<Edit>> {
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
                    && edit.verb != "tabular.edit_cell"
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
                            && (edit.verb == "tabular.edit_cell"
                                || edit.verb == "tabular.add_row"
                                || edit.verb == "tabular.remove_row")
                    })
                    .map(|(_, edit)| edit.clone()),
            );
        }

        Some(out)
    }
}

struct RowAlignmentBasis {
    left: Vec<String>,
    right: Vec<String>,
    right_rows: Vec<serde_json::Value>,
}

impl RowAlignmentBasis {
    fn from_edit(edit: &Edit) -> Option<Self> {
        let left = string_array(edit.params.get("left")?)?;
        let right = string_array(edit.params.get("right")?)?;
        if left.len() > MAX_ROW_ALIGNMENT_ROWS || right.len() > MAX_ROW_ALIGNMENT_ROWS {
            return None;
        }
        let right_rows = edit.params.get("right_rows")?.as_array()?.clone();
        Some(Self {
            left,
            right,
            right_rows,
        })
    }
}

fn string_array(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn lcs_pairs(left: &[String], right: &[String]) -> Vec<(usize, usize)> {
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

    fn rewrite(&self, edits: &[Edit]) -> Option<Vec<Edit>> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basis(left: &[&str], right: &[&str], right_rows: Vec<serde_json::Value>) -> Edit {
        Edit::new(
            "tabular.row_alignment_basis",
            json!({
                "left": left,
                "right": right,
                "right_rows": right_rows,
            }),
        )
        .hidden()
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

        let rewritten = RowAlignment.rewrite(&edits).expect("rewrite");

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

        let rewritten = RowAlignment.rewrite(&edits).expect("basis cleanup");

        assert_eq!(rewritten, vec![cell]);
    }
}
