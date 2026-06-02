use std::collections::BTreeMap;
use std::path::Path;

use binoc_sdk::*;
use dta::stata::dta::dta_reader::DtaReader;
use dta::stata::dta::value::Value as DtaValue;
use dta::stata::dta::value_label::ValueLabelSet;
use dta::stata::stata_byte::StataByte;
use dta::stata::stata_double::StataDouble;
use dta::stata::stata_float::StataFloat;
use dta::stata::stata_int::StataInt;
use dta::stata::stata_long::StataLong;
use sas7bdat::cell::{CellValue, MissingValue as SasMissingValue};
use sas_xport::sas::xport::{XportReader, XportValue};

#[derive(Default)]
pub struct StataComparator;

#[derive(Default)]
pub struct Sas7bdatComparator;

#[derive(Default)]
pub struct XptComparator;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTable {
    tabular: TabularData,
    metadata: serde_json::Value,
}

trait StatBinaryFormat {
    const NAME: &'static str;
    const PRODUCER: &'static str;
    const EXTENSIONS: &'static [&'static str];

    fn parse(path: &Path) -> BinocResult<ParsedTable>;
}

struct StataFormat;
struct Sas7bdatFormat;

impl StatBinaryFormat for StataFormat {
    const NAME: &'static str = "binoc-stat-binary.stata";
    const PRODUCER: &'static str = "binoc-stat-binary.stata";
    const EXTENSIONS: &'static [&'static str] = &[".dta"];

    fn parse(path: &Path) -> BinocResult<ParsedTable> {
        parse_stata(path)
    }
}

impl StatBinaryFormat for Sas7bdatFormat {
    const NAME: &'static str = "binoc-stat-binary.sas7bdat";
    const PRODUCER: &'static str = "binoc-stat-binary.sas7bdat";
    const EXTENSIONS: &'static [&'static str] = &[".sas7bdat"];

    fn parse(path: &Path) -> BinocResult<ParsedTable> {
        parse_sas7bdat(path)
    }
}

macro_rules! impl_comparator {
    ($comparator:ty, $format:ty) => {
        impl Comparator for $comparator {
            fn descriptor(&self) -> ComparatorDescriptor {
                ComparatorDescriptor::new(<$format>::NAME).with_extensions(
                    <$format>::EXTENSIONS
                        .iter()
                        .map(|ext| (*ext).to_string())
                        .collect(),
                )
            }

            fn compare(
                &self,
                pair: &ItemPair,
                data: &dyn DataAccess,
            ) -> BinocResult<CompareResult> {
                compare_as_tabular::<$format>(pair, data)
            }

            fn extract(
                &self,
                node: &DiffNode,
                aspect: &str,
                data: &dyn DataAccess,
            ) -> Option<ExtractResult> {
                let pair = TabularDataPair::from_artifacts(node, data)?;
                tabular_extract(&pair, node, aspect)
            }
        }
    };
}

impl_comparator!(StataComparator, StataFormat);
impl_comparator!(Sas7bdatComparator, Sas7bdatFormat);

impl Comparator for XptComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc-stat-binary.xpt").with_extensions(vec![".xpt".to_string()])
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        compare_xpt(pair, data)
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let pair = TabularDataPair::from_artifacts(node, data)?;
        tabular_extract(&pair, node, aspect)
    }
}

fn compare_as_tabular<F: StatBinaryFormat>(
    pair: &ItemPair,
    data: &dyn DataAccess,
) -> BinocResult<CompareResult> {
    match (&pair.left, &pair.right) {
        (Some(left), Some(right)) => {
            let left_table = parse_via_data::<F>(left, data)?;
            let right_table = parse_via_data::<F>(right, data)?;

            if left_table.tabular == right_table.tabular {
                return Ok(CompareResult::Identical);
            }

            let left_artifact = publish_tabular(
                data,
                &left_table.tabular,
                ArtifactSubject::Left,
                F::PRODUCER,
            )?;
            let right_artifact = publish_tabular(
                data,
                &right_table.tabular,
                ArtifactSubject::Right,
                F::PRODUCER,
            )?;

            let node = DiffNode::new("modify", "tabular", pair.logical_path())
                .with_detail("left_metadata", left_table.metadata)
                .with_detail("right_metadata", right_table.metadata)
                .with_artifact(left_artifact)
                .with_artifact(right_artifact);

            Ok(CompareResult::Leaf(node))
        }
        (None, Some(right)) => {
            let table = parse_via_data::<F>(right, data)?;
            let artifact =
                publish_tabular(data, &table.tabular, ArtifactSubject::Right, F::PRODUCER)?;
            let node = DiffNode::new("add", "tabular", &right.logical_path)
                .with_detail("metadata", table.metadata)
                .with_artifact(artifact);
            Ok(CompareResult::Leaf(node))
        }
        (Some(left), None) => {
            let table = parse_via_data::<F>(left, data)?;
            let artifact =
                publish_tabular(data, &table.tabular, ArtifactSubject::Left, F::PRODUCER)?;
            let node = DiffNode::new("remove", "tabular", &left.logical_path)
                .with_detail("metadata", table.metadata)
                .with_artifact(artifact);
            Ok(CompareResult::Leaf(node))
        }
        (None, None) => Ok(CompareResult::Identical),
    }
}

fn parse_via_data<F: StatBinaryFormat>(
    item: &ItemRef,
    data: &dyn DataAccess,
) -> BinocResult<ParsedTable> {
    let path = data.local_path(item)?;
    F::parse(&path)
}

fn publish_tabular(
    data: &dyn DataAccess,
    tabular: &TabularData,
    subject: ArtifactSubject,
    producer: &str,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(tabular)
        .map_err(|e| BinocError::Other(format!("serialize tabular artifact: {e}")))?;
    data.publish_artifact(&tabular_v1(), subject, producer, &bytes)
}

fn publish_tabular_collection(
    data: &dyn DataAccess,
    collection: &TabularCollectionData,
    subject: ArtifactSubject,
    producer: &str,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(collection)
        .map_err(|e| BinocError::Other(format!("serialize tabular collection artifact: {e}")))?;
    data.publish_artifact(&tabular_collection_v1(), subject, producer, &bytes)
}

fn parse_stata(path: &Path) -> BinocResult<ParsedTable> {
    let header_reader = DtaReader::new()
        .from_path(path)
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;
    let schema_reader = header_reader
        .read_header()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;
    let header = schema_reader.header().clone();
    let mut characteristic_reader = schema_reader
        .read_schema()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;
    characteristic_reader
        .skip_to_end()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;

    let mut record_reader = characteristic_reader
        .into_record_reader()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;
    let schema = record_reader.schema().clone();
    let headers = schema
        .variables()
        .iter()
        .map(|variable| variable.name().to_string())
        .collect();

    let mut rows = Vec::new();
    while let Some(record) = record_reader
        .read_record()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?
    {
        rows.push(record.values().iter().map(stata_value_to_string).collect());
    }

    let mut long_string_reader = record_reader
        .into_long_string_reader()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;
    long_string_reader
        .skip_to_end()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;

    let mut value_label_reader = long_string_reader
        .into_value_label_reader()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?;
    let mut value_labels = BTreeMap::new();
    while let Some(set) = value_label_reader
        .read_value_label_set()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?
    {
        value_labels.insert(set.name().to_string(), value_label_set_json(&set));
    }

    let variable_labels: BTreeMap<String, serde_json::Value> = schema
        .variables()
        .iter()
        .filter_map(|variable| {
            let has_metadata = !variable.label().is_empty()
                || !variable.format().is_empty()
                || !variable.value_label_name().is_empty();
            has_metadata.then(|| {
                (
                    variable.name().to_string(),
                    serde_json::json!({
                        "label": empty_to_null(variable.label()),
                        "format": empty_to_null(variable.format()),
                        "value_label_set": empty_to_null(variable.value_label_name()),
                    }),
                )
            })
        })
        .collect();

    Ok(ParsedTable {
        tabular: TabularData { headers, rows },
        metadata: serde_json::json!({
            "format": "stata_dta",
            "release": header.release().to_string(),
            "dataset_label": empty_to_null(header.dataset_label()),
            "columns": variable_labels,
            "value_labels": value_labels,
            "cell_encoding": "values flattened to display strings; Stata missing values use '.', '.a' ... '.z'; value labels are metadata only",
        }),
    })
}

fn parse_sas7bdat(path: &Path) -> BinocResult<ParsedTable> {
    let mut reader =
        sas7bdat::SasReader::open(path).map_err(|e| BinocError::Other(format!("sas7bdat: {e}")))?;
    let metadata = reader.metadata().clone();
    let headers: Vec<String> = metadata
        .variables
        .iter()
        .map(|variable| variable.name.trim_end().to_string())
        .collect();
    let columns: BTreeMap<String, serde_json::Value> = metadata
        .variables
        .iter()
        .filter_map(|variable| {
            let has_metadata = variable.label.is_some()
                || variable.format.is_some()
                || variable.value_labels.is_some();
            has_metadata.then(|| {
                (
                    variable.name.trim_end().to_string(),
                    serde_json::json!({
                        "label": variable.label,
                        "format": variable.format.as_ref().map(|format| &format.name),
                        "value_label_set": variable.value_labels,
                    }),
                )
            })
        })
        .collect();

    let mut rows = Vec::new();
    let mut row_iter = reader
        .rows_named()
        .map_err(|e| BinocError::Other(format!("sas7bdat: {e}")))?;
    while let Some(row) = row_iter
        .try_next()
        .map_err(|e| BinocError::Other(format!("sas7bdat: {e}")))?
    {
        rows.push(row.values().iter().map(sas7_cell_to_string).collect());
    }

    Ok(ParsedTable {
        tabular: TabularData { headers, rows },
        metadata: serde_json::json!({
            "format": "sas7bdat",
            "dataset_name": metadata.table_name,
            "dataset_label": metadata.file_label,
            "columns": columns,
            "cell_encoding": "values flattened to display strings; SAS missing values use '.', '.A' ... where available; value labels are metadata only",
        }),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedXptDataset {
    logical_name: String,
    node_name: String,
    tabular: TabularData,
    metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedXptFile {
    datasets: Vec<ParsedXptDataset>,
    metadata: serde_json::Value,
}

fn compare_xpt(pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
    match (&pair.left, &pair.right) {
        (Some(left), Some(right)) => {
            let left_file = parse_xpt_via_data(left, data)?;
            let right_file = parse_xpt_via_data(right, data)?;
            if xpt_uses_collection(Some(&left_file), Some(&right_file)) {
                compare_xpt_collection(
                    Some(&left_file),
                    Some(&right_file),
                    pair.logical_path(),
                    data,
                )
            } else {
                compare_single_tables(
                    pair.logical_path(),
                    &left_file.datasets[0],
                    &right_file.datasets[0],
                    data,
                    "binoc-stat-binary.xpt",
                )
            }
        }
        (None, Some(right)) => {
            let right_file = parse_xpt_via_data(right, data)?;
            if xpt_uses_collection(None, Some(&right_file)) {
                compare_xpt_collection(None, Some(&right_file), &right.logical_path, data)
            } else {
                add_single_table(
                    &right.logical_path,
                    &right_file.datasets[0],
                    data,
                    ArtifactSubject::Right,
                    "binoc-stat-binary.xpt",
                )
            }
        }
        (Some(left), None) => {
            let left_file = parse_xpt_via_data(left, data)?;
            if xpt_uses_collection(Some(&left_file), None) {
                compare_xpt_collection(Some(&left_file), None, &left.logical_path, data)
            } else {
                add_single_table(
                    &left.logical_path,
                    &left_file.datasets[0],
                    data,
                    ArtifactSubject::Left,
                    "binoc-stat-binary.xpt",
                )
            }
        }
        (None, None) => Ok(CompareResult::Identical),
    }
}

fn compare_single_tables(
    logical_path: &str,
    left: &ParsedXptDataset,
    right: &ParsedXptDataset,
    data: &dyn DataAccess,
    producer: &str,
) -> BinocResult<CompareResult> {
    if left.tabular == right.tabular {
        return Ok(CompareResult::Identical);
    }

    let left_artifact = publish_tabular(data, &left.tabular, ArtifactSubject::Left, producer)?;
    let right_artifact = publish_tabular(data, &right.tabular, ArtifactSubject::Right, producer)?;

    let node = DiffNode::new("modify", "tabular", logical_path)
        .with_detail("left_metadata", left.metadata.clone())
        .with_detail("right_metadata", right.metadata.clone())
        .with_artifact(left_artifact)
        .with_artifact(right_artifact);

    Ok(CompareResult::Leaf(node))
}

fn add_single_table(
    logical_path: &str,
    table: &ParsedXptDataset,
    data: &dyn DataAccess,
    subject: ArtifactSubject,
    producer: &str,
) -> BinocResult<CompareResult> {
    let artifact = publish_tabular(data, &table.tabular, subject, producer)?;
    let action = match subject {
        ArtifactSubject::Left => "remove",
        ArtifactSubject::Right => "add",
        ArtifactSubject::Pair => unreachable!("single table node never uses pair subject"),
    };
    let detail_key = match subject {
        ArtifactSubject::Left | ArtifactSubject::Right => "metadata",
        ArtifactSubject::Pair => unreachable!("single table node never uses pair subject"),
    };
    let node = DiffNode::new(action, "tabular", logical_path)
        .with_detail(detail_key, table.metadata.clone())
        .with_artifact(artifact);
    Ok(CompareResult::Leaf(node))
}

fn parse_xpt_via_data(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParsedXptFile> {
    let path = data.local_path(item)?;
    parse_xpt(path.as_path())
}

fn xpt_uses_collection(left: Option<&ParsedXptFile>, right: Option<&ParsedXptFile>) -> bool {
    [left, right]
        .into_iter()
        .flatten()
        .any(|file| file.datasets.len() != 1)
}

fn compare_xpt_collection(
    left_file: Option<&ParsedXptFile>,
    right_file: Option<&ParsedXptFile>,
    logical_path: &str,
    data: &dyn DataAccess,
) -> BinocResult<CompareResult> {
    if left_file == right_file {
        return Ok(CompareResult::Identical);
    }

    let left_collection = left_file.map(|file| xpt_collection_from_file(logical_path, file));
    let right_collection = right_file.map(|file| xpt_collection_from_file(logical_path, file));

    let mut children = Vec::new();
    let mut tables_added = Vec::new();
    let mut tables_removed = Vec::new();
    let mut tables_changed = Vec::new();
    let mut summary_parts = Vec::new();

    let left_groups = xpt_dataset_groups(left_file);
    let right_groups = xpt_dataset_groups(right_file);
    let all_names: std::collections::BTreeSet<String> = left_groups
        .keys()
        .chain(right_groups.keys())
        .cloned()
        .collect();

    for name in all_names {
        let left_group = left_groups.get(&name).cloned().unwrap_or_default();
        let right_group = right_groups.get(&name).cloned().unwrap_or_default();

        match (left_group.as_slice(), right_group.as_slice()) {
            ([left_dataset], [right_dataset]) => {
                if left_dataset.tabular != right_dataset.tabular {
                    let child_summary =
                        xpt_table_change_summary(&left_dataset.tabular, &right_dataset.tabular);
                    let left_artifact = publish_tabular(
                        data,
                        &left_dataset.tabular,
                        ArtifactSubject::Left,
                        "binoc-stat-binary.xpt",
                    )?;
                    let right_artifact = publish_tabular(
                        data,
                        &right_dataset.tabular,
                        ArtifactSubject::Right,
                        "binoc-stat-binary.xpt",
                    )?;
                    let node = DiffNode::new(
                        "modify",
                        "tabular",
                        xpt_table_node_path(logical_path, &right_dataset.node_name),
                    )
                    .with_summary(child_summary.clone())
                    .with_tag("binoc.table-change")
                    .with_detail("left_metadata", left_dataset.metadata.clone())
                    .with_detail("right_metadata", right_dataset.metadata.clone())
                    .with_detail("logical_name", serde_json::json!(name))
                    .with_artifact(left_artifact)
                    .with_artifact(right_artifact);
                    children.push(node);
                    tables_changed.push(name.clone());
                    summary_parts.push(format!(
                        "Table {name} changed: {}",
                        lower_first(&child_summary)
                    ));
                }
            }
            _ => {
                for dataset in left_group {
                    tables_removed.push(dataset.logical_name.clone());
                    summary_parts.push(format!(
                        "Table {} removed: {}",
                        dataset.logical_name,
                        lower_first(&removed_table_summary(dataset))
                    ));
                    children.push(xpt_table_add_remove_node(
                        logical_path,
                        dataset,
                        ArtifactSubject::Left,
                        data,
                    )?);
                }
                for dataset in right_group {
                    tables_added.push(dataset.logical_name.clone());
                    summary_parts.push(format!(
                        "Table {} added: {}",
                        dataset.logical_name,
                        lower_first(&new_table_summary(dataset))
                    ));
                    children.push(xpt_table_add_remove_node(
                        logical_path,
                        dataset,
                        ArtifactSubject::Right,
                        data,
                    )?);
                }
            }
        }
    }

    if children.is_empty() {
        return Ok(CompareResult::Identical);
    }

    let mut node = DiffNode::new(
        xpt_collection_action(left_file.is_some(), right_file.is_some()),
        "tabular_collection",
        logical_path,
    )
    .with_children(children);

    if !tables_added.is_empty() || !tables_removed.is_empty() || !tables_changed.is_empty() {
        node = node
            .with_tag("binoc.tabular-collection-change")
            .with_summary(summary_parts.join("; "));
    }
    if !tables_added.is_empty() {
        node = node
            .with_tag("binoc.table-addition")
            .with_detail("tables_added", serde_json::json!(tables_added));
    }
    if !tables_removed.is_empty() {
        node = node
            .with_tag("binoc.table-removal")
            .with_detail("tables_removed", serde_json::json!(tables_removed));
    }
    if !tables_changed.is_empty() {
        node = node
            .with_tag("binoc.table-change")
            .with_detail("tables_changed", serde_json::json!(tables_changed));
    }

    if let Some(file) = left_file {
        node = node.with_detail("left_metadata", file.metadata.clone());
    }
    if let Some(file) = right_file {
        node = node.with_detail("right_metadata", file.metadata.clone());
    }
    if let Some(collection) = &left_collection {
        node = node.with_artifact(publish_tabular_collection(
            data,
            collection,
            ArtifactSubject::Left,
            "binoc-stat-binary.xpt",
        )?);
    }
    if let Some(collection) = &right_collection {
        node = node.with_artifact(publish_tabular_collection(
            data,
            collection,
            ArtifactSubject::Right,
            "binoc-stat-binary.xpt",
        )?);
    }

    Ok(CompareResult::Leaf(node))
}

fn xpt_collection_action(has_left: bool, has_right: bool) -> &'static str {
    match (has_left, has_right) {
        (true, true) => "modify",
        (false, true) => "add",
        (true, false) => "remove",
        (false, false) => "identical",
    }
}

fn xpt_collection_from_file(logical_path: &str, file: &ParsedXptFile) -> TabularCollectionData {
    TabularCollectionData {
        tables: file
            .datasets
            .iter()
            .map(|dataset| {
                let dataset_name = dataset.metadata["dataset_name"]
                    .as_str()
                    .unwrap_or(dataset.logical_name.as_str())
                    .to_string();
                let dataset_index = dataset.metadata["dataset_index"]
                    .as_u64()
                    .expect("xpt dataset index is present");
                let dataset_label = dataset.metadata["dataset_label"].clone();
                TableMember {
                    logical_name: dataset.logical_name.clone(),
                    node_path: xpt_table_node_path(logical_path, &dataset.node_name),
                    source: TableSourceLocation {
                        item_path: logical_path.into(),
                        kind: "sas_xport_dataset".into(),
                        locator: BTreeMap::from([
                            ("dataset_name".into(), serde_json::json!(dataset_name)),
                            ("dataset_index".into(), serde_json::json!(dataset_index)),
                        ]),
                    },
                    shape: TableShape {
                        columns: dataset.tabular.headers.clone(),
                        row_count: Some(dataset.tabular.rows.len() as u64),
                    },
                    metadata: BTreeMap::from([
                        ("dataset_label".into(), dataset_label),
                        ("node_name".into(), serde_json::json!(dataset.node_name)),
                    ]),
                }
            })
            .collect(),
    }
}

fn xpt_dataset_groups(file: Option<&ParsedXptFile>) -> BTreeMap<String, Vec<&ParsedXptDataset>> {
    let mut groups = BTreeMap::new();
    if let Some(file) = file {
        for dataset in &file.datasets {
            groups
                .entry(dataset.logical_name.clone())
                .or_insert_with(Vec::new)
                .push(dataset);
        }
    }
    groups
}

fn xpt_table_add_remove_node(
    logical_path: &str,
    dataset: &ParsedXptDataset,
    subject: ArtifactSubject,
    data: &dyn DataAccess,
) -> BinocResult<DiffNode> {
    let action = match subject {
        ArtifactSubject::Left => "remove",
        ArtifactSubject::Right => "add",
        ArtifactSubject::Pair => unreachable!("collection child nodes are single-sided"),
    };
    let detail_key = match subject {
        ArtifactSubject::Left => "metadata",
        ArtifactSubject::Right => "metadata",
        ArtifactSubject::Pair => unreachable!("collection child nodes are single-sided"),
    };
    let artifact = publish_tabular(data, &dataset.tabular, subject, "binoc-stat-binary.xpt")?;
    let summary = match subject {
        ArtifactSubject::Left => removed_table_summary(dataset),
        ArtifactSubject::Right => new_table_summary(dataset),
        ArtifactSubject::Pair => unreachable!("collection child nodes are single-sided"),
    };
    let tag = match subject {
        ArtifactSubject::Left => "binoc.table-removal",
        ArtifactSubject::Right => "binoc.table-addition",
        ArtifactSubject::Pair => unreachable!("collection child nodes are single-sided"),
    };
    Ok(DiffNode::new(
        action,
        "tabular",
        xpt_table_node_path(logical_path, &dataset.node_name),
    )
    .with_summary(summary)
    .with_tag(tag)
    .with_detail("logical_name", serde_json::json!(dataset.logical_name))
    .with_detail(detail_key, dataset.metadata.clone())
    .with_artifact(artifact))
}

fn xpt_table_node_path(logical_path: &str, node_name: &str) -> String {
    format!("{logical_path}::{node_name}")
}

fn xpt_table_change_summary(left: &TabularData, right: &TabularData) -> String {
    let left_headers: std::collections::BTreeSet<&str> =
        left.headers.iter().map(String::as_str).collect();
    let right_headers: std::collections::BTreeSet<&str> =
        right.headers.iter().map(String::as_str).collect();

    let columns_added: Vec<&str> = right_headers.difference(&left_headers).copied().collect();
    let columns_removed: Vec<&str> = left_headers.difference(&right_headers).copied().collect();
    let rows_added = right.rows.len().saturating_sub(left.rows.len());
    let rows_removed = left.rows.len().saturating_sub(right.rows.len());

    let common_headers: Vec<&str> = left_headers.intersection(&right_headers).copied().collect();
    let mut cells_changed = 0usize;
    for row_index in 0..left.rows.len().min(right.rows.len()) {
        for header in &common_headers {
            let left_index = left
                .column_index(header)
                .expect("common header exists on left");
            let right_index = right
                .column_index(header)
                .expect("common header exists on right");
            let left_value = left.rows[row_index]
                .get(left_index)
                .map(String::as_str)
                .unwrap_or("");
            let right_value = right.rows[row_index]
                .get(right_index)
                .map(String::as_str)
                .unwrap_or("");
            if left_value != right_value {
                cells_changed += 1;
            }
        }
    }

    let mut parts = Vec::new();
    if !columns_added.is_empty() {
        parts.push(match columns_added.as_slice() {
            [name] => format!("Column added: '{name}'"),
            names => format!("{} columns added", names.len()),
        });
    }
    if !columns_removed.is_empty() {
        parts.push(match columns_removed.as_slice() {
            [name] => format!("Column removed: '{name}'"),
            names => format!("{} columns removed", names.len()),
        });
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
        "Table changed".to_string()
    } else {
        parts.join("; ")
    }
}

fn new_table_summary(dataset: &ParsedXptDataset) -> String {
    format!(
        "New table ({} column{}, {} row{})",
        dataset.tabular.headers.len(),
        if dataset.tabular.headers.len() == 1 {
            ""
        } else {
            "s"
        },
        dataset.tabular.rows.len(),
        if dataset.tabular.rows.len() == 1 {
            ""
        } else {
            "s"
        },
    )
}

fn removed_table_summary(dataset: &ParsedXptDataset) -> String {
    format!(
        "Table removed ({} column{}, {} row{})",
        dataset.tabular.headers.len(),
        if dataset.tabular.headers.len() == 1 {
            ""
        } else {
            "s"
        },
        dataset.tabular.rows.len(),
        if dataset.tabular.rows.len() == 1 {
            ""
        } else {
            "s"
        },
    )
}

fn lower_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().to_string() + chars.as_str(),
    }
}

fn parse_xpt(path: &Path) -> BinocResult<ParsedXptFile> {
    let file = std::fs::File::open(path).map_err(BinocError::Io)?;
    let reader =
        XportReader::from_file(file).map_err(|e| BinocError::Other(format!("xpt: {e}")))?;

    let mut datasets = Vec::new();
    let mut dataset_names = Vec::new();
    let mut next_dataset = reader
        .next_dataset()
        .map_err(|e| BinocError::Other(format!("xpt: {e}")))?;
    let mut index = 0usize;

    while let Some(mut dataset) = next_dataset {
        let schema = dataset.schema().clone();
        let headers = schema
            .variables()
            .iter()
            .map(|variable| variable.full_name().to_string())
            .collect();
        let columns: BTreeMap<String, serde_json::Value> = schema
            .variables()
            .iter()
            .filter_map(|variable| {
                let has_metadata =
                    !variable.full_label().is_empty() || !variable.full_format().is_empty();
                has_metadata.then(|| {
                    (
                        variable.full_name().to_string(),
                        serde_json::json!({
                            "label": empty_to_null(variable.full_label()),
                            "format": empty_to_null(variable.full_format()),
                        }),
                    )
                })
            })
            .collect();

        let mut rows = Vec::new();
        for record in dataset.records() {
            let record = record.map_err(|e| BinocError::Other(format!("xpt: {e}")))?;
            rows.push(record.iter().map(xpt_value_to_string).collect());
        }

        let dataset_name = schema.dataset_name().to_string();
        let logical_name = if dataset_name.is_empty() {
            format!("dataset_{}", index + 1)
        } else {
            dataset_name.clone()
        };
        dataset_names.push(logical_name.clone());
        datasets.push(ParsedXptDataset {
            logical_name,
            node_name: String::new(),
            tabular: TabularData { headers, rows },
            metadata: serde_json::json!({
                "format": "sas_xport",
                "dataset_index": index + 1,
                "dataset_name": dataset_name,
                "dataset_label": empty_to_null(schema.dataset_label()),
                "columns": columns,
                "cell_encoding": "values flattened to display strings; numeric NaN is treated as SAS missing '.'",
            }),
        });

        next_dataset = dataset
            .next_dataset()
            .map_err(|e| BinocError::Other(format!("xpt: {e}")))?;
        index += 1;
    }

    let mut name_counts = BTreeMap::new();
    for name in &dataset_names {
        *name_counts.entry(name.clone()).or_insert(0usize) += 1;
    }
    let mut seen_names = BTreeMap::new();
    for dataset in &mut datasets {
        let seen = seen_names
            .entry(dataset.logical_name.clone())
            .or_insert(0usize);
        *seen += 1;
        dataset.node_name = if name_counts[&dataset.logical_name] == 1 {
            dataset.logical_name.clone()
        } else {
            format!("{}#{}", dataset.logical_name, *seen)
        };
    }

    let dataset_inventory = datasets
        .iter()
        .map(|dataset| {
            serde_json::json!({
                "dataset_index": dataset.metadata["dataset_index"].clone(),
                "dataset_name": dataset.metadata["dataset_name"].clone(),
                "dataset_label": dataset.metadata["dataset_label"].clone(),
                "logical_name": dataset.logical_name.clone(),
                "node_name": dataset.node_name.clone(),
                "columns": dataset.tabular.headers.clone(),
                "rows": dataset.tabular.rows.len(),
            })
        })
        .collect::<Vec<_>>();

    for dataset in &mut datasets {
        let metadata = dataset
            .metadata
            .as_object_mut()
            .expect("xpt dataset metadata is an object");
        metadata.insert(
            "logical_name".into(),
            serde_json::json!(dataset.logical_name.clone()),
        );
        metadata.insert(
            "node_name".into(),
            serde_json::json!(dataset.node_name.clone()),
        );
        metadata.insert(
            "datasets".into(),
            serde_json::json!(dataset_inventory.clone()),
        );
    }

    Ok(ParsedXptFile {
        metadata: serde_json::json!({
            "format": "sas_xport",
            "datasets": dataset_inventory,
            "cell_encoding": "values flattened to display strings; numeric NaN is treated as SAS missing '.'",
        }),
        datasets,
    })
}

fn stata_value_to_string(value: &DtaValue<'_>) -> String {
    match value {
        DtaValue::Byte(StataByte::Present(value)) => value.to_string(),
        DtaValue::Byte(StataByte::Missing(missing)) => missing.to_string(),
        DtaValue::Int(StataInt::Present(value)) => value.to_string(),
        DtaValue::Int(StataInt::Missing(missing)) => missing.to_string(),
        DtaValue::Long(StataLong::Present(value)) => value.to_string(),
        DtaValue::Long(StataLong::Missing(missing)) => missing.to_string(),
        DtaValue::Float(StataFloat::Present(value)) => format_float(f64::from(*value)),
        DtaValue::Float(StataFloat::Missing(missing)) => missing.to_string(),
        DtaValue::Double(StataDouble::Present(value)) => format_float(*value),
        DtaValue::Double(StataDouble::Missing(missing)) => missing.to_string(),
        DtaValue::String(value) => value.to_string(),
        DtaValue::LongStringRef(reference) => {
            format!(
                "<strL:{}:{}>",
                reference.variable(),
                reference.observation()
            )
        }
    }
}

fn sas7_cell_to_string(value: &CellValue<'_>) -> String {
    match value {
        CellValue::Float(value) => format_float(*value),
        CellValue::Int32(value) => value.to_string(),
        CellValue::Int64(value) => value.to_string(),
        CellValue::NumericString(value) | CellValue::Str(value) => value.to_string(),
        CellValue::Bytes(bytes) => bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(""),
        CellValue::DateTime(value) | CellValue::Date(value) => value.to_string(),
        CellValue::Time(value) => value.to_string(),
        CellValue::Missing(missing) => sas_missing_to_string(missing),
    }
}

fn xpt_value_to_string(value: &XportValue<'_>) -> String {
    match value {
        XportValue::Character(value) => value.trim_end().to_string(),
        XportValue::Number(value) if value.is_nan() => ".".to_string(),
        XportValue::Number(value) => format_float(*value),
    }
}

fn sas_missing_to_string(missing: &SasMissingValue) -> String {
    match missing {
        SasMissingValue::System => ".".to_string(),
        SasMissingValue::Tagged(tagged) => tagged
            .tag
            .map(|tag| format!(".{tag}"))
            .unwrap_or_else(|| ".".to_string()),
        SasMissingValue::Range { .. } => ".".to_string(),
    }
}

fn format_float(value: f64) -> String {
    if value.is_nan() {
        ".".to_string()
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn value_label_set_json(set: &ValueLabelSet) -> serde_json::Value {
    let entries: BTreeMap<String, String> = set
        .entries()
        .iter()
        .map(|entry| (entry.value().to_string(), entry.label().to_string()))
        .collect();
    serde_json::json!(entries)
}

fn empty_to_null(value: &str) -> serde_json::Value {
    if value.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::LocalDataAccess;
    use dta::stata::dta::byte_order::ByteOrder;
    use dta::stata::dta::dta_writer::DtaWriter;
    use dta::stata::dta::header::Header;
    use dta::stata::dta::release::Release;
    use dta::stata::dta::schema::Schema;
    use dta::stata::dta::value::Value;
    use dta::stata::dta::variable::Variable;
    use dta::stata::dta::variable_type::VariableType;
    use sas_xport::sas::xport::{
        XportMetadata, XportSchema, XportValue, XportVariable, XportWriter,
    };
    use sas_xport::sas::SasVariableType;

    fn write_simple_dta(path: &Path, extra_column: bool) {
        let header = Header::builder(Release::V118, ByteOrder::LittleEndian)
            .dataset_label("test table")
            .build();
        let mut schema_builder = Schema::builder()
            .add_variable(Variable::builder(VariableType::Long, "id").label("Identifier"))
            .add_variable(Variable::builder(VariableType::FixedString(16), "name").label("Name"));
        if extra_column {
            schema_builder =
                schema_builder.add_variable(Variable::builder(VariableType::Long, "score"));
        }
        let schema = schema_builder.build().unwrap();

        let mut record_writer = DtaWriter::new()
            .from_path(path)
            .unwrap()
            .write_header(header)
            .unwrap()
            .write_schema(schema)
            .unwrap()
            .into_record_writer()
            .unwrap();
        if extra_column {
            record_writer
                .write_record(&[
                    Value::Long(StataLong::Present(1)),
                    Value::string("Alice"),
                    Value::Long(StataLong::Present(9)),
                ])
                .unwrap();
        } else {
            record_writer
                .write_record(&[Value::Long(StataLong::Present(1)), Value::string("Alice")])
                .unwrap();
        }
        record_writer
            .into_long_string_writer()
            .unwrap()
            .into_value_label_writer()
            .unwrap()
            .finish()
            .unwrap();
    }

    fn write_multi_xpt(path: &Path) {
        let dm = XportSchema::builder()
            .dataset_name("DM")
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("id")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("name")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .try_build()
            .unwrap();
        let ae = XportSchema::builder()
            .dataset_name("AE")
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("id")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("event")
                    .value_type(SasVariableType::Character)
                    .value_length(16);
                variable
            })
            .try_build()
            .unwrap();

        let file = std::fs::File::create(path).unwrap();
        let writer = XportWriter::from_file(file, XportMetadata::builder().build()).unwrap();
        let mut writer = writer.write_schema(dm).unwrap();
        writer
            .write_record(&[XportValue::from("1"), XportValue::from("Alice")])
            .unwrap();
        let writer = writer.next_dataset().unwrap();
        let mut writer = writer.write_schema(ae).unwrap();
        writer
            .write_record(&[XportValue::from("1"), XportValue::from("Headache")])
            .unwrap();
        writer.finish().unwrap();
    }

    fn write_multi_xpt_with_extra_member(path: &Path) {
        let dm = XportSchema::builder()
            .dataset_name("DM")
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("id")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("name")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .try_build()
            .unwrap();
        let ae = XportSchema::builder()
            .dataset_name("AE")
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("id")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("event")
                    .value_type(SasVariableType::Character)
                    .value_length(16);
                variable
            })
            .try_build()
            .unwrap();
        let lb = XportSchema::builder()
            .dataset_name("LB")
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("id")
                    .value_type(SasVariableType::Character)
                    .value_length(8);
                variable
            })
            .add_variable({
                let mut variable = XportVariable::builder();
                variable
                    .short_name("lab")
                    .value_type(SasVariableType::Character)
                    .value_length(16);
                variable
            })
            .try_build()
            .unwrap();

        let file = std::fs::File::create(path).unwrap();
        let writer = XportWriter::from_file(file, XportMetadata::builder().build()).unwrap();
        let mut writer = writer.write_schema(dm).unwrap();
        writer
            .write_record(&[XportValue::from("1"), XportValue::from("Alice")])
            .unwrap();
        writer
            .write_record(&[XportValue::from("2"), XportValue::from("Bob")])
            .unwrap();
        let writer = writer.next_dataset().unwrap();
        let mut writer = writer.write_schema(ae).unwrap();
        writer
            .write_record(&[XportValue::from("1"), XportValue::from("Headache")])
            .unwrap();
        let writer = writer.next_dataset().unwrap();
        let mut writer = writer.write_schema(lb).unwrap();
        writer
            .write_record(&[XportValue::from("1"), XportValue::from("ALT")])
            .unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn parses_stata_as_tabular() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.dta");
        write_simple_dta(&path, false);

        let parsed = parse_stata(&path).unwrap();
        assert_eq!(parsed.tabular.headers, vec!["id", "name"]);
        assert_eq!(parsed.tabular.rows, vec![vec!["1", "Alice"]]);
        assert_eq!(
            parsed.metadata["columns"]["id"]["label"],
            serde_json::json!("Identifier")
        );
    }

    #[test]
    fn stata_comparator_publishes_tabular_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.dta");
        let right = dir.path().join("right.dta");
        write_simple_dta(&left, false);
        write_simple_dta(&right, true);

        let data = LocalDataAccess::new();
        let pair = ItemPair::both(
            data.register_local(&left, "data.dta").unwrap(),
            data.register_local(&right, "data.dta").unwrap(),
        );
        let result = StataComparator.compare(&pair, &data).unwrap();
        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.item_type, "tabular");
                assert_eq!(node.artifacts.len(), 2);
                assert!(node
                    .artifacts
                    .iter()
                    .all(|artifact| artifact.format == tabular_v1()));
            }
            _ => panic!("expected changed leaf"),
        }
    }

    #[test]
    fn parses_multi_dataset_xpt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.xpt");
        write_multi_xpt(&path);

        let parsed = parse_xpt(&path).unwrap();
        assert_eq!(parsed.datasets.len(), 2);
        assert_eq!(parsed.datasets[0].logical_name, "DM");
        assert_eq!(parsed.datasets[1].logical_name, "AE");
    }

    #[test]
    fn xpt_comparator_uses_collection_for_multi_dataset_files() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.xpt");
        let right = dir.path().join("right.xpt");
        write_multi_xpt(&left);
        write_multi_xpt_with_extra_member(&right);

        let data = LocalDataAccess::new();
        let pair = ItemPair::both(
            data.register_local(&left, "data.xpt").unwrap(),
            data.register_local(&right, "data.xpt").unwrap(),
        );

        let result = XptComparator.compare(&pair, &data).unwrap();
        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.item_type, "tabular_collection");
                assert_eq!(node.children.len(), 2);
                assert!(node
                    .artifacts
                    .iter()
                    .all(|artifact| artifact.format == tabular_collection_v1()));
            }
            _ => panic!("expected changed leaf"),
        }
    }
}
