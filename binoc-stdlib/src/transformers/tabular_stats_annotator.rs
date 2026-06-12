use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;
use serde::Deserialize;

const DISTRIBUTION_ANNOTATION_KEY: &str = "distribution_shifts";
const EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, Deserialize, Default)]
struct TabularStatsAnnotatorConfig {
    #[serde(default)]
    enabled: bool,
}

/// Opt-in statistical annotation for changed tabular numeric columns.
///
/// This transformer consumes `tabular_v1` artifacts and publishes a structured
/// distribution-shift annotation for numeric columns. It prefers keyed row
/// matching when row identity has already been configured, but falls back to
/// positional pairing.
pub struct TabularStatsAnnotator;

impl Transformer for TabularStatsAnnotator {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.tabular_stats_annotator")
            .with_match_artifacts(vec![tabular_v1()])
            .with_match_actions(vec!["modify".into(), "move".into()])
            // Writes only annotations, which are outside the write-set model.
            .with_emits_tags(vec![])
            .with_emits_actions(vec![])
            .with_emits_item_types(vec![])
            .with_publishes_artifacts(vec![])
    }

    fn transform(
        &self,
        mut node: DiffNode,
        data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let config: TabularStatsAnnotatorConfig =
            serde_json::from_value(config.clone()).unwrap_or_default();
        if !config.enabled {
            return TransformResult::Unchanged;
        }

        let Some(pair) = TabularDataPair::from_artifacts(&node, data) else {
            return TransformResult::Unchanged;
        };
        let (Some(left), Some(right)) = (&pair.left, &pair.right) else {
            return TransformResult::Unchanged;
        };

        let pairing = Pairing::for_node(&node, left, right);
        let candidate_columns = candidate_columns(left, right, &pairing);
        if candidate_columns.is_empty() {
            return TransformResult::Unchanged;
        }

        let lines: Vec<String> = candidate_columns
            .into_iter()
            .filter_map(|column| distribution_change_for_column(&column, left, right, &pairing))
            .map(|change| change.to_annotation_line())
            .collect();

        if lines.is_empty() {
            return TransformResult::Unchanged;
        }

        node.annotate_from(
            "binoc",
            DISTRIBUTION_ANNOTATION_KEY,
            serde_json::json!(lines),
        );
        TransformResult::Replace(Box::new(node))
    }
}

#[derive(Debug, Clone)]
struct Pairing {
    mode: &'static str,
    matched: Vec<(usize, usize)>,
    row_set_changed: bool,
}

impl Pairing {
    fn for_node(node: &DiffNode, left: &TabularData, right: &TabularData) -> Self {
        if let Some(key_columns) = row_identity_columns(node, left, right) {
            return keyed_pairing(left, right, &key_columns);
        }
        positional_pairing(left, right)
    }
}

fn positional_pairing(left: &TabularData, right: &TabularData) -> Pairing {
    let matched_len = left.rows.len().min(right.rows.len());
    Pairing {
        mode: "position",
        matched: (0..matched_len).map(|idx| (idx, idx)).collect(),
        row_set_changed: left.rows.len() != right.rows.len(),
    }
}

fn keyed_pairing(left: &TabularData, right: &TabularData, key_columns: &[String]) -> Pairing {
    let left_index = index_rows_by_key(left, key_columns);
    let right_index = index_rows_by_key(right, key_columns);
    let keys: BTreeSet<Vec<String>> = left_index
        .keys()
        .chain(right_index.keys())
        .cloned()
        .collect();

    let mut matched = Vec::new();
    let mut row_set_changed = false;

    for key in keys {
        let left_rows = left_index.get(&key).cloned().unwrap_or_default();
        let right_rows = right_index.get(&key).cloned().unwrap_or_default();
        match (left_rows.as_slice(), right_rows.as_slice()) {
            ([left_idx], [right_idx]) => matched.push((*left_idx, *right_idx)),
            _ => row_set_changed = true,
        }
    }

    Pairing {
        mode: "keyed",
        matched,
        row_set_changed,
    }
}

fn row_identity_columns(
    node: &DiffNode,
    left: &TabularData,
    right: &TabularData,
) -> Option<Vec<String>> {
    let columns: Vec<String> = node
        .details
        .get("row_identity")
        .and_then(|value| value.get("columns"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())?;
    if columns.is_empty() {
        return None;
    }
    if columns
        .iter()
        .all(|column| left.column_index(column).is_some() && right.column_index(column).is_some())
    {
        Some(columns)
    } else {
        None
    }
}

fn index_rows_by_key(
    table: &TabularData,
    key_columns: &[String],
) -> BTreeMap<Vec<String>, Vec<usize>> {
    let indices: Vec<usize> = key_columns
        .iter()
        .filter_map(|column| table.column_index(column))
        .collect();
    let mut by_key: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut key = Vec::with_capacity(indices.len());
        let mut has_null = false;
        for index in &indices {
            let value = row.get(*index).map(|value| value.trim()).unwrap_or("");
            if value.is_empty() {
                has_null = true;
                break;
            }
            key.push(value.to_string());
        }
        if !has_null {
            by_key.entry(key).or_default().push(row_index);
        }
    }

    by_key
}

fn candidate_columns(left: &TabularData, right: &TabularData, pairing: &Pairing) -> Vec<String> {
    let left_set: BTreeSet<&str> = left.headers.iter().map(String::as_str).collect();
    let common_columns: Vec<String> = right
        .headers
        .iter()
        .filter(|column| left_set.contains(column.as_str()))
        .cloned()
        .collect();

    if pairing.row_set_changed {
        return common_columns;
    }

    common_columns
        .into_iter()
        .filter(|column| {
            let Some(left_index) = left.column_index(column) else {
                return false;
            };
            let Some(right_index) = right.column_index(column) else {
                return false;
            };
            pairing.matched.iter().any(|(left_row, right_row)| {
                left.rows[*left_row]
                    .get(left_index)
                    .map(String::as_str)
                    .unwrap_or("")
                    != right.rows[*right_row]
                        .get(right_index)
                        .map(String::as_str)
                        .unwrap_or("")
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
struct ColumnNumbers {
    values: Vec<f64>,
    null_count: u64,
}

fn distribution_change_for_column(
    column: &str,
    left: &TabularData,
    right: &TabularData,
    pairing: &Pairing,
) -> Option<NumericColumnDistributionChange> {
    let left_numbers = numeric_column(left, column)?;
    let right_numbers = numeric_column(right, column)?;
    if left_numbers.values.is_empty() || right_numbers.values.is_empty() {
        return None;
    }

    let left_stats = stats_for_numbers(&left_numbers);
    let right_stats = stats_for_numbers(&right_numbers);
    let delta = NumericDistributionDelta {
        null_count: right_stats.null_count as i64 - left_stats.null_count as i64,
        min: right_stats.min - left_stats.min,
        max: right_stats.max - left_stats.max,
        mean: right_stats.mean - left_stats.mean,
        median: right_stats.median - left_stats.median,
        q1: right_stats.q1 - left_stats.q1,
        q3: right_stats.q3 - left_stats.q3,
    };
    let paired = paired_magnitude(left, right, column, pairing);

    if !distribution_changed(&left_stats, &right_stats, paired.as_ref()) {
        return None;
    }

    Some(NumericColumnDistributionChange {
        column: column.to_string(),
        pairing_mode: pairing.mode.to_string(),
        left: left_stats,
        right: right_stats,
        delta,
        paired,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct NumericDistributionStats {
    count: u64,
    null_count: u64,
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    q1: f64,
    q3: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct NumericDistributionDelta {
    null_count: i64,
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    q1: f64,
    q3: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct NumericPairMagnitude {
    compared_rows: u64,
    changed_rows: u64,
    mean_absolute_delta: f64,
    max_absolute_delta: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct NumericColumnDistributionChange {
    column: String,
    pairing_mode: String,
    left: NumericDistributionStats,
    right: NumericDistributionStats,
    delta: NumericDistributionDelta,
    paired: Option<NumericPairMagnitude>,
}

impl NumericColumnDistributionChange {
    fn to_annotation_line(&self) -> String {
        let mut line = format!(
            "column '{}': mean {} -> {}, median {} -> {}, range {}-{} -> {}-{}",
            self.column,
            format_number(self.left.mean),
            format_number(self.right.mean),
            format_number(self.left.median),
            format_number(self.right.median),
            format_number(self.left.min),
            format_number(self.left.max),
            format_number(self.right.min),
            format_number(self.right.max),
        );
        if self.left.null_count != self.right.null_count {
            line.push_str(&format!(
                ", nulls {} -> {}",
                self.left.null_count, self.right.null_count
            ));
        }
        if let Some(paired) = &self.paired {
            if paired.changed_rows > 0 {
                line.push_str(&format!(
                    ", mean abs delta {} across {} paired row{}",
                    format_number(paired.mean_absolute_delta),
                    paired.changed_rows,
                    if paired.changed_rows == 1 { "" } else { "s" }
                ));
            }
        }
        line
    }
}

fn format_number(value: f64) -> String {
    let rounded = if value.abs() < 1_000_000.0 {
        format!("{value:.3}")
    } else {
        format!("{value:.6e}")
    };
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn numeric_column(table: &TabularData, column: &str) -> Option<ColumnNumbers> {
    let index = table.column_index(column)?;
    let mut values = Vec::new();
    let mut null_count = 0_u64;

    for row in &table.rows {
        let raw = row.get(index).map(String::as_str).unwrap_or("").trim();
        if raw.is_empty() {
            null_count += 1;
            continue;
        }
        let value = raw.parse::<f64>().ok()?;
        if !value.is_finite() {
            return None;
        }
        values.push(value);
    }

    Some(ColumnNumbers { values, null_count })
}

fn stats_for_numbers(numbers: &ColumnNumbers) -> NumericDistributionStats {
    let mut values = numbers.values.clone();
    values.sort_by(|left, right| left.total_cmp(right));
    let count = values.len() as u64;
    NumericDistributionStats {
        count,
        null_count: numbers.null_count,
        min: values[0],
        max: values[count as usize - 1],
        mean: values.iter().sum::<f64>() / count as f64,
        median: quantile(&values, 0.5),
        q1: quantile(&values, 0.25),
        q3: quantile(&values, 0.75),
    }
}

fn quantile(values: &[f64], p: f64) -> f64 {
    if values.len() == 1 {
        return values[0];
    }
    let position = p * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        values[lower]
    } else {
        let weight = position - lower as f64;
        values[lower] + (values[upper] - values[lower]) * weight
    }
}

fn paired_magnitude(
    left: &TabularData,
    right: &TabularData,
    column: &str,
    pairing: &Pairing,
) -> Option<NumericPairMagnitude> {
    let left_index = left.column_index(column)?;
    let right_index = right.column_index(column)?;

    let mut compared_rows = 0_u64;
    let mut changed_rows = 0_u64;
    let mut sum_abs_delta = 0.0_f64;
    let mut max_abs_delta = 0.0_f64;

    for (left_row, right_row) in &pairing.matched {
        let left_value = parse_numeric_cell(&left.rows[*left_row], left_index);
        let right_value = parse_numeric_cell(&right.rows[*right_row], right_index);
        let (Some(left_value), Some(right_value)) = (left_value, right_value) else {
            continue;
        };
        compared_rows += 1;
        let abs_delta = (right_value - left_value).abs();
        if abs_delta > EPSILON {
            changed_rows += 1;
            sum_abs_delta += abs_delta;
            if abs_delta > max_abs_delta {
                max_abs_delta = abs_delta;
            }
        }
    }

    if compared_rows == 0 {
        return None;
    }

    Some(NumericPairMagnitude {
        compared_rows,
        changed_rows,
        mean_absolute_delta: if changed_rows == 0 {
            0.0
        } else {
            sum_abs_delta / changed_rows as f64
        },
        max_absolute_delta: max_abs_delta,
    })
}

fn parse_numeric_cell(row: &[String], index: usize) -> Option<f64> {
    let raw = row.get(index).map(String::as_str).unwrap_or("").trim();
    if raw.is_empty() {
        return None;
    }
    let value = raw.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn distribution_changed(
    left: &NumericDistributionStats,
    right: &NumericDistributionStats,
    paired: Option<&NumericPairMagnitude>,
) -> bool {
    left.null_count != right.null_count
        || (left.min - right.min).abs() > EPSILON
        || (left.max - right.max).abs() > EPSILON
        || (left.mean - right.mean).abs() > EPSILON
        || (left.median - right.median).abs() > EPSILON
        || (left.q1 - right.q1).abs() > EPSILON
        || (left.q3 - right.q3).abs() > EPSILON
        || paired.is_some_and(|paired| paired.changed_rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_core::data_access::LocalDataAccess;

    fn publish_pair(
        data: &LocalDataAccess,
        node: &mut DiffNode,
        left: &TabularData,
        right: &TabularData,
    ) {
        let left_bytes = serde_json::to_vec(left).unwrap();
        let right_bytes = serde_json::to_vec(right).unwrap();
        node.artifacts.push(
            data.publish_artifact(&tabular_v1(), ArtifactSubject::Left, "test", &left_bytes)
                .unwrap(),
        );
        node.artifacts.push(
            data.publish_artifact(&tabular_v1(), ArtifactSubject::Right, "test", &right_bytes)
                .unwrap(),
        );
    }

    #[test]
    fn annotates_numeric_distribution_shift_for_changed_column() {
        let data = LocalDataAccess::new();
        let mut node = DiffNode::new("modify", "tabular", "data.csv");
        node.details.insert(
            "row_identity".into(),
            serde_json::json!({ "columns": ["id"] }),
        );
        publish_pair(
            &data,
            &mut node,
            &TabularData {
                headers: vec!["id".into(), "score".into(), "label".into()],
                rows: vec![
                    vec!["1".into(), "10".into(), "a".into()],
                    vec!["2".into(), "20".into(), "b".into()],
                    vec!["3".into(), "".into(), "c".into()],
                ],
            },
            &TabularData {
                headers: vec!["id".into(), "score".into(), "label".into()],
                rows: vec![
                    vec!["1".into(), "15".into(), "a".into()],
                    vec!["2".into(), "35".into(), "b2".into()],
                    vec!["3".into(), "45".into(), "c".into()],
                ],
            },
        );

        let result =
            TabularStatsAnnotator.transform(node, &data, &serde_json::json!({ "enabled": true }));

        let TransformResult::Replace(node) = result else {
            panic!("expected node replacement");
        };
        let annotation = node.binoc_annotation(DISTRIBUTION_ANNOTATION_KEY).unwrap();
        assert_eq!(annotation.package, "binoc");
        assert_eq!(annotation.key, "distribution_shifts");
        let lines: Vec<String> = serde_json::from_value(annotation.value.clone()).unwrap();
        assert_eq!(
            lines,
            vec![
                "column 'score': mean 15 -> 31.667, median 15 -> 35, range 10-20 -> 15-45, nulls 1 -> 0, mean abs delta 10 across 2 paired rows"
            ]
        );
    }

    #[test]
    fn stays_disabled_without_explicit_config() {
        let data = LocalDataAccess::new();
        let mut node = DiffNode::new("modify", "tabular", "data.csv");
        publish_pair(
            &data,
            &mut node,
            &TabularData {
                headers: vec!["score".into()],
                rows: vec![vec!["1".into()]],
            },
            &TabularData {
                headers: vec!["score".into()],
                rows: vec![vec!["2".into()]],
            },
        );

        let result = TabularStatsAnnotator.transform(node, &data, &serde_json::Value::Null);
        assert!(matches!(result, TransformResult::Unchanged));
    }
}
