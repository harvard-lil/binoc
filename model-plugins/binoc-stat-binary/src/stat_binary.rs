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
struct XptFormat;

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

impl StatBinaryFormat for XptFormat {
    const NAME: &'static str = "binoc-stat-binary.xpt";
    const PRODUCER: &'static str = "binoc-stat-binary.xpt";
    const EXTENSIONS: &'static [&'static str] = &[".xpt"];

    fn parse(path: &Path) -> BinocResult<ParsedTable> {
        parse_xpt(path)
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
impl_comparator!(XptComparator, XptFormat);

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

fn parse_xpt(path: &Path) -> BinocResult<ParsedTable> {
    let file = std::fs::File::open(path).map_err(BinocError::Io)?;
    let reader =
        XportReader::from_file(file).map_err(|e| BinocError::Other(format!("xpt: {e}")))?;
    let Some(mut dataset) = reader
        .next_dataset()
        .map_err(|e| BinocError::Other(format!("xpt: {e}")))?
    else {
        return Ok(ParsedTable {
            tabular: TabularData {
                headers: Vec::new(),
                rows: Vec::new(),
            },
            metadata: serde_json::json!({
                "format": "sas_xport",
                "datasets": [],
                "cell_encoding": "values flattened to display strings; numeric NaN is treated as SAS missing '.'",
            }),
        });
    };

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

    Ok(ParsedTable {
        tabular: TabularData { headers, rows },
        metadata: serde_json::json!({
            "format": "sas_xport",
            "dataset_name": schema.dataset_name(),
            "dataset_label": empty_to_null(schema.dataset_label()),
            "columns": columns,
            "cell_encoding": "values flattened to display strings; numeric NaN is treated as SAS missing '.'",
        }),
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
}
