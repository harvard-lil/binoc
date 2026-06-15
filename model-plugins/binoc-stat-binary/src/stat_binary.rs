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
pub struct StataParseRule;

#[derive(Default)]
pub struct Sas7bdatParseRule;

#[derive(Default)]
pub struct XptParseRule;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTable {
    tabular: TabularData,
    metadata: serde_json::Value,
}

trait StatBinaryFormat {
    const NAME: &'static str;
    const EXTENSIONS: &'static [&'static str];

    fn parse(path: &Path) -> BinocResult<ParsedTable>;
}

struct StataFormat;
struct Sas7bdatFormat;

impl StatBinaryFormat for StataFormat {
    const NAME: &'static str = "binoc-stat-binary.stata";
    const EXTENSIONS: &'static [&'static str] = &[".dta"];

    fn parse(path: &Path) -> BinocResult<ParsedTable> {
        parse_stata(path)
    }
}

impl StatBinaryFormat for Sas7bdatFormat {
    const NAME: &'static str = "binoc-stat-binary.sas7bdat";
    const EXTENSIONS: &'static [&'static str] = &[".sas7bdat"];

    fn parse(path: &Path) -> BinocResult<ParsedTable> {
        parse_sas7bdat(path)
    }
}

macro_rules! impl_collection_parse_rule {
    ($rule:ty, $format:ty) => {
        impl ParseRule for $rule {
            fn descriptor(&self) -> ParseDescriptor {
                ParseDescriptor {
                    name: format!("{}.parse", <$format>::NAME),
                    input: NodeMatch {
                        is_dir: Some(false),
                        extensions: <$format>::EXTENSIONS
                            .iter()
                            .map(|extension| (*extension).to_string())
                            .collect(),
                        media_types: Vec::new(),
                    },
                    output: tabular_collection_v1(),
                    requires_link: true,
                    fires_beneath_settled: false,
                }
            }

            fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
                let path = data.local_path(item)?;
                let parsed = <$format>::parse(&path)?;
                let collection = single_table_collection(&item.logical_path, parsed);
                serde_json::to_vec(&collection)
                    .map(ParseOutput::from)
                    .map_err(|e| {
                        BinocError::Other(format!("serialize stat-binary collection artifact: {e}"))
                    })
            }
        }
    };
}

impl_collection_parse_rule!(StataParseRule, StataFormat);
impl_collection_parse_rule!(Sas7bdatParseRule, Sas7bdatFormat);

impl ParseRule for XptParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-stat-binary.xpt.parse".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".xpt".into()],
                media_types: Vec::new(),
            },
            output: tabular_collection_v1(),
            requires_link: true,
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let path = data.local_path(item)?;
        let parsed = parse_xpt(&path)?;
        serde_json::to_vec(&xpt_collection_from_file(&item.logical_path, &parsed))
            .map(ParseOutput::from)
            .map_err(|e| BinocError::Other(format!("serialize xpt collection artifact: {e}")))
    }
}

fn single_table_collection(logical_path: &str, parsed: ParsedTable) -> TabularCollectionData {
    let logical_name = std::path::Path::new(logical_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "table".into());
    TabularCollectionData {
        tables: vec![TableMember {
            logical_name,
            node_path: logical_path.into(),
            source: TableSourceLocation {
                item_path: logical_path.into(),
                kind: "stat_binary_table".into(),
                locator: BTreeMap::new(),
            },
            shape: TableShape {
                columns: parsed.tabular.headers,
                row_count: Some(parsed.tabular.rows.len() as u64),
            },
            metadata: BTreeMap::from([("metadata".into(), parsed.metadata)]),
        }],
    }
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

fn xpt_table_node_path(logical_path: &str, node_name: &str) -> String {
    format!("{logical_path}::{node_name}")
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
    fn parses_multi_dataset_xpt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.xpt");
        write_multi_xpt(&path);

        let parsed = parse_xpt(&path).unwrap();
        assert_eq!(parsed.datasets.len(), 2);
        assert_eq!(parsed.datasets[0].logical_name, "DM");
        assert_eq!(parsed.datasets[1].logical_name, "AE");
    }
}
