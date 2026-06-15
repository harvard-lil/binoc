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

/// A parsed single-table file: the `tabular_v1` payload (carrying tier-1 column
/// metadata and tier-2 table metadata) plus the tier-3 [`ParserMetadata`] bag
/// that rides as a second artifact on the same node.
struct ParsedLeaf {
    tabular: TabularData,
    parser_metadata: ParserMetadata,
}

trait StatBinaryFormat {
    const NAME: &'static str;
    const EXTENSIONS: &'static [&'static str];

    fn parse(path: &Path) -> BinocResult<ParsedLeaf>;
}

struct StataFormat;
struct Sas7bdatFormat;

impl StatBinaryFormat for StataFormat {
    const NAME: &'static str = "binoc-stat-binary.stata";
    const EXTENSIONS: &'static [&'static str] = &[".dta"];

    fn parse(path: &Path) -> BinocResult<ParsedLeaf> {
        parse_stata(path)
    }
}

impl StatBinaryFormat for Sas7bdatFormat {
    const NAME: &'static str = "binoc-stat-binary.sas7bdat";
    const EXTENSIONS: &'static [&'static str] = &[".sas7bdat"];

    fn parse(path: &Path) -> BinocResult<ParsedLeaf> {
        parse_sas7bdat(path)
    }
}

macro_rules! impl_leaf_parse_rule {
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
                    output: tabular_v1(),
                    fires_beneath_settled: false,
                }
            }

            fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
                let path = data.local_path(item)?;
                let parsed = <$format>::parse(&path)?;
                // A `.dta`/`.sas7bdat` file is a single dataset: emit a LEAF
                // `tabular_v1` on the file node (tier-1/tier-2 metadata ride
                // inside it), with the tier-3 `parser_metadata_v1` bag as a
                // second artifact on the same node.
                let bytes = serde_json::to_vec(&parsed.tabular).map_err(|e| {
                    BinocError::Other(format!("serialize stat-binary tabular artifact: {e}"))
                })?;
                let metadata_bytes = serde_json::to_vec(&parsed.parser_metadata).map_err(|e| {
                    BinocError::Other(format!("serialize stat-binary parser metadata: {e}"))
                })?;
                Ok(ParseOutput {
                    bytes,
                    diagnostics: Vec::new(),
                    children: Vec::new(),
                    artifacts: vec![ParsedArtifact {
                        format: parser_metadata_v1(),
                        bytes: metadata_bytes,
                    }],
                    projection: ProjectionHint::default(),
                })
            }
        }
    };
}

impl_leaf_parse_rule!(StataParseRule, StataFormat);
impl_leaf_parse_rule!(Sas7bdatParseRule, Sas7bdatFormat);

impl ParseRule for XptParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-stat-binary.xpt.parse".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".xpt".into()],
                media_types: Vec::new(),
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let path = data.local_path(item)?;
        let parsed = parse_xpt(&path)?;
        // A SAS transport file is a container of one or more named datasets:
        // emit one `tabular_v1` CHILD per dataset (each carrying tier-1/tier-2
        // metadata) and no primary artifact. File-level facts ride as a tier-3
        // `parser_metadata_v1` bag on the container node itself.
        let children = xpt_children(&item.logical_path, &parsed)?;
        let metadata_bytes = serde_json::to_vec(&parsed.parser_metadata)
            .map_err(|e| BinocError::Other(format!("serialize xpt parser metadata: {e}")))?;
        Ok(ParseOutput {
            bytes: Vec::new(),
            diagnostics: Vec::new(),
            children,
            artifacts: vec![ParsedArtifact {
                format: parser_metadata_v1(),
                bytes: metadata_bytes,
            }],
            projection: ProjectionHint::default().item_type("SAS transport file"),
        })
    }
}

fn parse_stata(path: &Path) -> BinocResult<ParsedLeaf> {
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
    let headers: Vec<String> = schema
        .variables()
        .iter()
        .map(|variable| variable.name().to_string())
        .collect();
    // Tier 1: per-column label / display format / value-label set name.
    let column_metadata: Vec<serde_json::Value> = schema
        .variables()
        .iter()
        .map(|variable| {
            column_meta_object([
                ("label", Some(variable.label())),
                ("format", Some(variable.format())),
                ("value_label_set", Some(variable.value_label_name())),
            ])
        })
        .collect();

    let mut rows = Vec::new();
    while let Some(record) = record_reader
        .read_record()
        .map_err(|e| BinocError::Other(format!("stata: {e}")))?
    {
        rows.push(record.values().iter().map(stata_value_to_string).collect());
    }

    // Drain the remaining reader stages to reach the value-label dictionaries,
    // which are file-level (tier 3): referenced by columns but stored once.
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

    let tabular = TabularData::from_string_rows(headers, rows)
        .with_column_metadata(column_metadata)
        // Tier 2: this single table's own label.
        .with_table_metadata(serde_json::json!({
            "dataset_label": empty_to_null(header.dataset_label()),
        }));
    // Tier 3: how the file is encoded plus its value-label dictionaries.
    let parser_metadata = ParserMetadata::new(
        "stata_dta",
        serde_json::json!({
            "release": header.release().to_string(),
            "value_labels": value_labels,
            "cell_encoding": "values flattened to display strings; Stata missing values use '.', '.a' ... '.z'; value labels are metadata only",
        }),
    );
    Ok(ParsedLeaf {
        tabular,
        parser_metadata,
    })
}

fn parse_sas7bdat(path: &Path) -> BinocResult<ParsedLeaf> {
    let mut reader =
        sas7bdat::SasReader::open(path).map_err(|e| BinocError::Other(format!("sas7bdat: {e}")))?;
    let metadata = reader.metadata().clone();
    let headers: Vec<String> = metadata
        .variables
        .iter()
        .map(|variable| variable.name.trim_end().to_string())
        .collect();
    // Tier 1: per-column label / display format / value-label set name.
    let column_metadata: Vec<serde_json::Value> = metadata
        .variables
        .iter()
        .map(|variable| {
            column_meta_object([
                ("label", variable.label.as_deref()),
                ("format", variable.format.as_ref().map(|f| f.name.as_str())),
                ("value_label_set", variable.value_labels.as_deref()),
            ])
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

    let version = &metadata.version;
    let tabular = TabularData::from_string_rows(headers, rows)
        .with_column_metadata(column_metadata)
        // Tier 2: this table's name and label.
        .with_table_metadata(serde_json::json!({
            "dataset_name": metadata.table_name,
            "dataset_label": metadata.file_label,
        }));
    // Tier 3: physical file-format facts.
    let parser_metadata = ParserMetadata::new(
        "sas7bdat",
        serde_json::json!({
            "version": format!("{}.{}.{}", version.major, version.minor, version.revision),
            "compression": format!("{:?}", metadata.compression),
            "endianness": format!("{:?}", metadata.endianness),
            "file_encoding": metadata.file_encoding,
            "vendor": format!("{:?}", metadata.vendor),
            "cell_encoding": "values flattened to display strings; SAS missing values use '.', '.A' ... where available; value labels are metadata only",
        }),
    );
    Ok(ParsedLeaf {
        tabular,
        parser_metadata,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedXptDataset {
    logical_name: String,
    node_name: String,
    tabular: TabularData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedXptFile {
    datasets: Vec<ParsedXptDataset>,
    parser_metadata: ParserMetadata,
}

/// Build one `tabular_v1` child node per dataset in the SAS transport file.
///
/// The dataset name (de-duplicated into `node_name`) is the stable logical name;
/// child paths join the file node with `/>` via [`decompose_child`].
fn xpt_children(logical_path: &str, file: &ParsedXptFile) -> BinocResult<Vec<ParsedChild>> {
    let mut children = Vec::with_capacity(file.datasets.len());
    for dataset in &file.datasets {
        let child_path = decompose_child(logical_path, &dataset.node_name);
        let bytes = serde_json::to_vec(&dataset.tabular)
            .map_err(|e| BinocError::Other(format!("serialize xpt dataset artifact: {e}")))?;
        children.push(ParsedChild {
            item: ItemRef {
                logical_path: child_path.clone(),
                is_dir: false,
                content_hash: Some(blake3::hash(&bytes).to_hex().to_string()),
                size: Some(bytes.len() as u64),
                media_type: Some("application/vnd.binoc.tabular+json".into()),
                projection_hint: ProjectionHint::default().item_type("tabular"),
                handle: child_path,
            },
            artifacts: vec![ParsedArtifact {
                format: tabular_v1(),
                bytes,
            }],
        });
    }
    Ok(children)
}

fn parse_xpt(path: &Path) -> BinocResult<ParsedXptFile> {
    let file = std::fs::File::open(path).map_err(BinocError::Io)?;
    let reader =
        XportReader::from_file(file).map_err(|e| BinocError::Other(format!("xpt: {e}")))?;
    // File-level (tier 3): captured before `next_dataset` consumes the reader.
    let sas_version = reader.metadata().sas_version().to_string();

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
        // Tier 1: per-column label / display format.
        let column_metadata: Vec<serde_json::Value> = schema
            .variables()
            .iter()
            .map(|variable| {
                column_meta_object([
                    ("label", Some(variable.full_label())),
                    ("format", Some(variable.full_format())),
                ])
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
        // Tier 2: this dataset's own identity within the transport file.
        let table_metadata = serde_json::json!({
            "dataset_name": empty_to_null(&dataset_name),
            "dataset_label": empty_to_null(schema.dataset_label()),
            "dataset_index": index + 1,
        });
        datasets.push(ParsedXptDataset {
            logical_name,
            node_name: String::new(),
            tabular: TabularData::from_string_rows(headers, rows)
                .with_column_metadata(column_metadata)
                .with_table_metadata(table_metadata),
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

    // Tier 3: file-format identity plus a light inventory of member datasets.
    let inventory: Vec<serde_json::Value> = datasets
        .iter()
        .map(|dataset| {
            serde_json::json!({
                "logical_name": dataset.logical_name,
                "node_name": dataset.node_name,
            })
        })
        .collect();
    let parser_metadata = ParserMetadata::new(
        "sas_xport",
        serde_json::json!({
            "sas_version": empty_to_null(&sas_version),
            "datasets": inventory,
            "cell_encoding": "values flattened to display strings; numeric NaN is treated as SAS missing '.'",
        }),
    );

    Ok(ParsedXptFile {
        datasets,
        parser_metadata,
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

/// Build one column's tier-1 metadata object from `(key, value)` pairs. A pair
/// whose value is `None` or empty becomes a JSON `null`; when every value is
/// null the whole column collapses to `Null`, so columns with no metadata stay
/// cheap and a generic consumer sees a parallel-to-`headers` array of objects.
fn column_meta_object<const N: usize>(entries: [(&str, Option<&str>); N]) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = entries
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Some(text) if !text.is_empty() => serde_json::Value::String(text.to_string()),
                _ => serde_json::Value::Null,
            };
            (key.to_string(), value)
        })
        .collect();
    if map.values().all(serde_json::Value::is_null) {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(map)
    }
}

/// Flatten a Stata value-label set into a `{ value: label }` JSON object.
fn value_label_set_json(set: &ValueLabelSet) -> serde_json::Value {
    let entries: BTreeMap<String, String> = set
        .entries()
        .iter()
        .map(|entry| (entry.value().to_string(), entry.label().to_string()))
        .collect();
    serde_json::json!(entries)
}

/// Map an empty string to JSON `null`, otherwise a JSON string.
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
        let tabular = &parsed.tabular;
        assert_eq!(tabular.headers, vec!["id", "name"]);
        assert_eq!(
            tabular.rows,
            vec![vec![
                binoc_sdk::Value::String("1".into()),
                binoc_sdk::Value::String("Alice".into())
            ]]
        );
        // Tier 1: the `id` column's label rides on the tabular artifact.
        assert_eq!(
            tabular.column_metadata[0]["label"],
            serde_json::json!("Identifier")
        );
        // Tier 2: the dataset label rides as table metadata.
        assert_eq!(
            tabular.table_metadata["dataset_label"],
            serde_json::json!("test table")
        );
        // Tier 3: the source-format identity rides as a separate artifact.
        assert_eq!(parsed.parser_metadata.format, "stata_dta");
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
