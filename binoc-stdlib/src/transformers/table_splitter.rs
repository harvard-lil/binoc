use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;

const PRODUCER: &str = "binoc.table_splitter";

/// Splits a single messy tabular artifact into a collection of logical
/// tables when the file clearly contains stacked rectangular regions.
pub struct TableSplitter;

#[derive(Debug, Clone)]
struct Section {
    logical_name: String,
    title: Option<String>,
    header_row: usize,
    end_row: usize,
    data: TabularData,
}

#[derive(Debug, Clone)]
struct Detection {
    sections: Vec<Section>,
    ambiguous_reason: Option<String>,
}

impl Transformer for TableSplitter {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new(PRODUCER)
            .with_match_artifacts(vec![tabular_v1()])
            .with_emits_tags(vec!["binoc.tabular-collection".into()])
            .with_emits_actions(vec!["add".into(), "remove".into(), "modify".into()])
            .with_emits_item_types(vec!["tabular_collection".into(), "tabular".into()])
            .with_publishes_artifacts(vec![tabular_v1(), tabular_collection_v1()])
    }

    fn transform(
        &self,
        mut node: DiffNode,
        data: &dyn DataAccess,
        _config: &serde_json::Value,
    ) -> TransformResult {
        if node.item_type == "tabular_collection" {
            return TransformResult::Unchanged;
        }

        let Some(pair) = TabularDataPair::from_artifacts(&node, data) else {
            return TransformResult::Unchanged;
        };

        let left_detection = pair.left.as_ref().map(detect_sections);
        let right_detection = pair.right.as_ref().map(detect_sections);

        if let Some(reason) = ambiguous_reason(&left_detection, &right_detection) {
            node.push_diagnostic(Diagnostic::suggestion(
                "binoc.table_splitter.ambiguous",
                reason,
            ));
            return TransformResult::Replace(Box::new(node));
        }

        if !should_split(&left_detection, &right_detection) {
            return TransformResult::Unchanged;
        }

        let original_path = node.path.clone();
        match split_node(node, data, left_detection, right_detection) {
            Ok(Some(node)) => TransformResult::Replace(Box::new(node)),
            Ok(None) => TransformResult::Remove,
            Err(err) => {
                let mut fallback = DiffNode::new("modify", "tabular", original_path);
                fallback.push_diagnostic(Diagnostic::warning(
                    "binoc.table_splitter.failed",
                    format!("Could not split logical tables: {err}"),
                ));
                TransformResult::Replace(Box::new(fallback))
            }
        }
    }
}

fn should_split(left: &Option<Detection>, right: &Option<Detection>) -> bool {
    left.as_ref()
        .is_some_and(|d| d.ambiguous_reason.is_none() && d.sections.len() >= 2)
        || right
            .as_ref()
            .is_some_and(|d| d.ambiguous_reason.is_none() && d.sections.len() >= 2)
}

fn ambiguous_reason(left: &Option<Detection>, right: &Option<Detection>) -> Option<String> {
    for detection in [left.as_ref(), right.as_ref()].into_iter().flatten() {
        if let Some(reason) = &detection.ambiguous_reason {
            return Some(reason.clone());
        }
    }

    match (left, right) {
        (Some(left), Some(right))
            if (left.sections.len() >= 2) ^ (right.sections.len() >= 2) =>
        {
            Some(
                "One side looks like stacked logical tables, but the other side does not; leaving the CSV as one table."
                    .into(),
            )
        }
        _ => None,
    }
}

fn split_node(
    mut node: DiffNode,
    data: &dyn DataAccess,
    left: Option<Detection>,
    right: Option<Detection>,
) -> BinocResult<Option<DiffNode>> {
    let left_sections = left.map(|d| d.sections).unwrap_or_default();
    let right_sections = right.map(|d| d.sections).unwrap_or_default();

    let left_by_name = section_map(&left_sections);
    let right_by_name = section_map(&right_sections);
    let names = left_by_name
        .keys()
        .chain(right_by_name.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut children = Vec::new();
    for name in names {
        match (left_by_name.get(&name), right_by_name.get(&name)) {
            (Some(left_section), Some(right_section)) => {
                if left_section.data == right_section.data {
                    continue;
                }
                let left_artifact =
                    publish_tabular(data, &left_section.data, ArtifactSubject::Left)?;
                let right_artifact =
                    publish_tabular(data, &right_section.data, ArtifactSubject::Right)?;
                children.push(
                    table_node(&node, "modify", left_section, Some(right_section))
                        .with_artifact(left_artifact)
                        .with_artifact(right_artifact),
                );
            }
            (Some(left_section), None) => {
                let artifact = publish_tabular(data, &left_section.data, ArtifactSubject::Left)?;
                children
                    .push(table_node(&node, "remove", left_section, None).with_artifact(artifact));
            }
            (None, Some(right_section)) => {
                let artifact = publish_tabular(data, &right_section.data, ArtifactSubject::Right)?;
                children
                    .push(table_node(&node, "add", right_section, None).with_artifact(artifact));
            }
            (None, None) => {}
        }
    }

    if children.is_empty() {
        return Ok(None);
    }

    let left_collection = collection_from_sections(&node.path, &left_sections);
    let right_collection = collection_from_sections(&node.path, &right_sections);

    node.item_type = "tabular_collection".into();
    node.children = children;
    node.summary = None;
    node.tags.insert("binoc.tabular-collection".into());
    node.details.clear();
    node.artifacts.clear();
    if !left_sections.is_empty() {
        node.details.insert(
            "tables_left".into(),
            serde_json::json!(table_names(&left_sections)),
        );
        node.artifacts.push(publish_collection(
            data,
            &left_collection,
            ArtifactSubject::Left,
        )?);
    }
    if !right_sections.is_empty() {
        node.details.insert(
            "tables_right".into(),
            serde_json::json!(table_names(&right_sections)),
        );
        node.artifacts.push(publish_collection(
            data,
            &right_collection,
            ArtifactSubject::Right,
        )?);
    }

    Ok(Some(node))
}

fn detect_sections(table: &TabularData) -> Detection {
    let rows = raw_rows(table);
    let mut sections = Vec::new();
    let mut wide_unclaimed = Vec::new();
    let mut i = 0;

    while i < rows.len() {
        let mut title_rows = Vec::new();
        while i < rows.len() {
            let width = normalized_width(&rows[i]);
            if width == 0 {
                i += 1;
            } else if width == 1 {
                title_rows.push(i);
                i += 1;
            } else {
                break;
            }
        }
        if i >= rows.len() {
            break;
        }

        let width = normalized_width(&rows[i]);
        if width < 2 || !looks_like_header(&rows[i]) {
            if width > 1 {
                wide_unclaimed.push(i + 1);
            }
            i += 1;
            continue;
        }

        let header_row = i;
        let headers = trim_to_width(&rows[header_row], width);
        let mut data_rows = Vec::new();
        let mut j = i + 1;
        while j < rows.len() {
            let row_width = normalized_width(&rows[j]);
            if row_width == 0 || row_width != width {
                break;
            }
            let row = trim_to_width(&rows[j], width);
            if row != headers {
                data_rows.push(row);
            }
            j += 1;
        }

        if data_rows.is_empty() {
            wide_unclaimed.push(header_row + 1);
            i = header_row + 1;
            continue;
        }

        let logical_name = format!("table_{}", sections.len() + 1);
        let title = title_from_rows(&rows, &title_rows);
        sections.push(Section {
            logical_name,
            title,
            header_row,
            end_row: j,
            data: TabularData {
                headers,
                rows: data_rows,
            },
        });
        i = j;
    }

    let ambiguous_reason = if sections.len() >= 2 && !wide_unclaimed.is_empty() {
        Some(format!(
            "The CSV has stacked table-like regions, but {} {} outside any clear rectangle; leaving it as one table.",
            summarize_unclaimed(&wide_unclaimed),
            if wide_unclaimed.len() == 1 { "falls" } else { "fall" }
        ))
    } else {
        None
    };

    Detection {
        sections,
        ambiguous_reason,
    }
}

/// Summarize the unclaimed row indices as a count plus a small sample, rather
/// than enumerating every index (which bloats the suggestion on large tables).
fn summarize_unclaimed(rows: &[usize]) -> String {
    const SAMPLE: usize = 5;

    let count = rows.len();
    let noun = if count == 1 { "row" } else { "rows" };

    if count <= SAMPLE {
        let list = rows
            .iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return format!("{count} {noun} ({list})");
    }

    let sample = rows[..SAMPLE]
        .iter()
        .map(|row| row.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = count - SAMPLE;
    format!("{count} {noun} (e.g. {sample}, … and {remaining} more)")
}

fn raw_rows(table: &TabularData) -> Vec<Vec<String>> {
    std::iter::once(table.headers.clone())
        .chain(table.rows.clone())
        .collect()
}

fn normalized_width(row: &[String]) -> usize {
    row.iter()
        .rposition(|cell| !cell.trim().is_empty())
        .map_or(0, |idx| idx + 1)
}

fn trim_to_width(row: &[String], width: usize) -> Vec<String> {
    (0..width)
        .map(|idx| {
            row.get(idx)
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        })
        .collect()
}

fn looks_like_header(row: &[String]) -> bool {
    let width = normalized_width(row);
    if width < 2 {
        return false;
    }
    let cells = trim_to_width(row, width);
    let non_empty = cells.iter().filter(|cell| !cell.is_empty()).count();
    if non_empty < 2 {
        return false;
    }
    let unique = cells
        .iter()
        .filter(|cell| !cell.is_empty())
        .collect::<BTreeSet<_>>();
    if unique.len() != non_empty {
        return false;
    }
    let numericish = cells.iter().filter(|cell| is_numericish(cell)).count();
    numericish * 2 < non_empty
}

fn is_numericish(cell: &str) -> bool {
    let trimmed = cell.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | ',' | '$' | '%'))
}

fn title_from_rows(rows: &[Vec<String>], title_rows: &[usize]) -> Option<String> {
    let parts = title_rows
        .iter()
        .filter_map(|idx| rows.get(*idx)?.first())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" / "))
    }
}

fn section_map(sections: &[Section]) -> BTreeMap<String, &Section> {
    sections
        .iter()
        .map(|section| (section.logical_name.clone(), section))
        .collect()
}

fn table_names(sections: &[Section]) -> Vec<String> {
    sections
        .iter()
        .map(|section| section.logical_name.clone())
        .collect()
}

fn table_node(
    parent: &DiffNode,
    action: &str,
    primary: &Section,
    secondary: Option<&Section>,
) -> DiffNode {
    let table_path = table_node_path(&parent.path, &primary.logical_name);
    let mut node = DiffNode::new(action, "tabular", table_path)
        .with_detail("logical_name", serde_json::json!(primary.logical_name))
        .with_detail("header_row", serde_json::json!(primary.header_row + 1))
        .with_detail("end_row", serde_json::json!(primary.end_row))
        .with_detail("columns", serde_json::json!(primary.data.headers));
    if let Some(title) = primary
        .title
        .as_ref()
        .or_else(|| secondary.and_then(|s| s.title.as_ref()))
    {
        node = node.with_detail("title", serde_json::json!(title));
    }
    node.comparator.clone_from(&parent.comparator);
    node.source_items.clone_from(&parent.source_items);
    node
}

fn table_node_path(parent_path: &str, logical_name: &str) -> String {
    format!("{parent_path}#{logical_name}")
}

fn collection_from_sections(parent_path: &str, sections: &[Section]) -> TabularCollectionData {
    TabularCollectionData {
        tables: sections
            .iter()
            .map(|section| TableMember {
                logical_name: section.logical_name.clone(),
                node_path: table_node_path(parent_path, &section.logical_name),
                source: TableSourceLocation {
                    item_path: parent_path.into(),
                    kind: "csv_region".into(),
                    locator: BTreeMap::from([
                        (
                            "header_row".into(),
                            serde_json::json!(section.header_row + 1),
                        ),
                        ("end_row".into(), serde_json::json!(section.end_row)),
                    ]),
                },
                shape: TableShape {
                    columns: section.data.headers.clone(),
                    row_count: Some(section.data.rows.len() as u64),
                },
                metadata: section
                    .title
                    .as_ref()
                    .map(|title| BTreeMap::from([("title".into(), serde_json::json!(title))]))
                    .unwrap_or_default(),
            })
            .collect(),
    }
}

fn publish_tabular(
    data: &dyn DataAccess,
    tabular: &TabularData,
    subject: ArtifactSubject,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(tabular)
        .map_err(|e| BinocError::Other(format!("serialize split tabular artifact: {e}")))?;
    data.publish_artifact(&tabular_v1(), subject, PRODUCER, &bytes)
}

fn publish_collection(
    data: &dyn DataAccess,
    collection: &TabularCollectionData,
    subject: ArtifactSubject,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(collection)
        .map_err(|e| BinocError::Other(format!("serialize split collection artifact: {e}")))?;
    data.publish_artifact(&tabular_collection_v1(), subject, PRODUCER, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[&[&str]]) -> TabularData {
        let mut iter = rows.iter();
        let headers = iter.next().unwrap().iter().map(|s| s.to_string()).collect();
        let rows = iter
            .map(|row| row.iter().map(|s| s.to_string()).collect())
            .collect();
        TabularData { headers, rows }
    }

    #[test]
    fn detects_stacked_rectangles_with_titles_and_blank_spacers() {
        let data = table(&[
            &["Purple Book"],
            &["Monthly changes"],
            &["Appl No", "Product", "Change"],
            &["1", "Alpha", "Added"],
            &[""],
            &["Appl No", "Product", "Applicant", "Approval Date"],
            &["1", "Alpha", "Acme", "2026-01-01"],
            &["2", "Beta", "Bravo", "2026-02-01"],
        ]);

        let detection = detect_sections(&data);

        assert!(detection.ambiguous_reason.is_none());
        assert_eq!(detection.sections.len(), 2);
        assert_eq!(detection.sections[0].logical_name, "table_1");
        assert_eq!(
            detection.sections[0].data.headers,
            vec!["Appl No", "Product", "Change"]
        );
        assert_eq!(
            detection.sections[0].title.as_deref(),
            Some("Purple Book / Monthly changes")
        );
        assert_eq!(
            detection.sections[1].data.headers,
            vec!["Appl No", "Product", "Applicant", "Approval Date"]
        );
    }

    #[test]
    fn ignores_single_clean_table() {
        let data = table(&[&["id", "name"], &["1", "Alice"], &["2", "Bob"]]);

        let detection = detect_sections(&data);

        assert_eq!(detection.sections.len(), 1);
        assert!(detection.ambiguous_reason.is_none());
    }

    #[test]
    fn marks_wide_unclaimed_rows_ambiguous() {
        let data = table(&[
            &["First title"],
            &["id", "name"],
            &["1", "Alice"],
            &["wide", "note", "outside"],
            &["Second title"],
            &["code", "value"],
            &["A", "10"],
        ]);

        let detection = detect_sections(&data);

        assert_eq!(detection.sections.len(), 2);
        assert!(detection.ambiguous_reason.is_some());
    }

    #[test]
    fn summarizes_unclaimed_rows_without_enumerating_all() {
        let few = summarize_unclaimed(&[1, 2, 3]);
        assert_eq!(few, "3 rows (1, 2, 3)");

        let one = summarize_unclaimed(&[7]);
        assert_eq!(one, "1 row (7)");

        let many: Vec<usize> = (1..=10_000).collect();
        let summary = summarize_unclaimed(&many);
        assert_eq!(summary, "10000 rows (e.g. 1, 2, 3, 4, 5, … and 9995 more)");
        // The whole point: the summary stays short regardless of row count.
        assert!(summary.len() < 80);
    }
}
