use std::collections::BTreeSet;

use binoc_sdk::{
    decompose_child, structured_document_v1, tabular_v1, BinocError, BinocResult, CsvDialectConfig,
    DataAccess, ItemRef, NodeMatch, ParseDescriptor, ParseOutput, ParseRule, ParsedArtifact,
    ParsedChild, ProjectionHint, StructuredDocument, TabularData, TabularParseConfig,
};
use serde::{de, de::DeserializeSeed, Deserialize, Deserializer, Serialize};

const CSV_SNIFF_BYTES: usize = 16 * 1024;
const CSV_DELIMITER_CANDIDATES: &[u8] = b",\t|;:";

pub struct CsvParse {
    pub large_tabular_threshold_bytes: u64,
}
pub struct CsvMediaParse {
    pub large_tabular_threshold_bytes: u64,
}
pub struct JsonParse;
pub struct JsonMediaParse;
/// Routes JSON / JSONL whose top level is a consistently-shaped record
/// collection (array of objects, array of arrays, or like-shaped JSONL) to a
/// `tabular_v1` artifact. Non-record JSON is declined here and handled by
/// [`JsonParse`] as a `structured_document`.
pub struct JsonRecordsParse;
/// Media-typed analogue of [`JsonRecordsParse`] (whole-document JSON only).
pub struct JsonMediaRecordsParse;
/// Transcodes YAML documents into a `structured_document` (matched by extension).
pub struct YamlParse;
/// Media-typed analogue of [`YamlParse`].
pub struct YamlMediaParse;
/// Transcodes TOML documents into a `structured_document`.
pub struct TomlParse;
/// Transcodes INI / cfg / properties files into a `structured_document`.
pub struct IniParse;

/// Default byte threshold above which stdlib tabular rules switch from
/// in-memory `tabular_v1` materialization to the streaming keyed-writer path.
/// The runtime value is configurable via
/// `dataset.correspondence.large_tabular_threshold_bytes`.
pub(crate) const LARGE_TABULAR_THRESHOLD_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonSourceFacts {
    pub byte_len: usize,
    pub trailing_newline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indentation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_key_orders: Vec<JsonObjectKeyOrder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonObjectKeyOrder {
    pub path: String,
    pub keys: Vec<String>,
}

impl ParseRule for CsvParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.csv".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".csv".into(), ".tsv".into()],
                ..NodeMatch::default()
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        if item.resolve_size(data)? > self.large_tabular_threshold_bytes {
            return Ok(ParseOutput::default());
        }
        let bytes = data.read_bytes(item)?;
        let dialect = resolve_csv_dialect(item, &bytes)?;
        let records = parse_csv_records(&bytes, &dialect)?;
        let tabular =
            table_from_csv_records_with_config(records.clone(), item.tabular_parse.as_ref());
        let sections = detect_stacked_sections_from_rows(&records);
        let projection = if dialect.should_disclose_provenance() {
            ProjectionHint::default()
                .tag("binoc.dialect-inferred")
                .annotate(
                    "binoc",
                    "dialect_provenance",
                    serde_json::json!(dialect_provenance_summary(&dialect)),
                )
        } else {
            ProjectionHint::default()
        };

        // Fewer than two qualifying regions: a plain CSV is a single table,
        // emitted as a LEAF `tabular_v1` artifact with no children.
        if sections.len() < 2 {
            let bytes = serde_json::to_vec(&tabular)
                .map_err(|err| BinocError::Other(format!("serialize tabular artifact: {err}")))?;
            return Ok(ParseOutput {
                bytes,
                diagnostics: Vec::new(),
                children: Vec::new(),
                artifacts: Vec::new(),
                projection,
            });
        }

        // Two or more qualifying stacked regions: a CONTAINER parse — no parent
        // artifact, one `tabular_v1` child node per detected section.
        let children = children_from_sections(&item.logical_path, &sections);
        Ok(ParseOutput {
            bytes: Vec::new(),
            diagnostics: Vec::new(),
            children,
            artifacts: Vec::new(),
            projection: projection.item_type("stacked tables"),
        })
    }
}

impl ParseRule for CsvMediaParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.csv_media".into(),
            input: NodeMatch {
                is_dir: Some(false),
                media_types: vec!["text/csv".into(), "text/tab-separated-values".into()],
                ..NodeMatch::default()
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        CsvParse {
            large_tabular_threshold_bytes: self.large_tabular_threshold_bytes,
        }
        .parse(item, data)
    }
}

impl ParseRule for JsonParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.json".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![
                    ".json".into(),
                    ".geojson".into(),
                    ".jsonld".into(),
                    ".json-ld".into(),
                ],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        parse_json_item(item, data)
    }
}

impl ParseRule for JsonMediaParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.json_media".into(),
            input: NodeMatch {
                is_dir: Some(false),
                media_types: vec![
                    "application/json".into(),
                    "application/ld+json".into(),
                    "application/geo+json".into(),
                ],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        parse_json_item(item, data)
    }
}

impl ParseRule for JsonRecordsParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.json_records".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![
                    ".json".into(),
                    ".jsonl".into(),
                    ".ndjson".into(),
                    ".geojson".into(),
                    ".jsonld".into(),
                    ".json-ld".into(),
                ],
                ..NodeMatch::default()
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let table = match records_path_config(item.tabular_parse.as_ref()) {
            Some(_) => serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|err| BinocError::Other(format!("parse JSON: {err}")))
                .and_then(|value| json_records(&value, item.tabular_parse.as_ref()))?,
            None => match item.extension().as_deref() {
                Some(".jsonl") | Some(".ndjson") => jsonl_records(&bytes),
                _ => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(value) => json_records(&value, item.tabular_parse.as_ref())?,
                    Err(_) => None,
                },
            },
        };
        match table {
            // Declining (empty output) leaves the node for `JsonParse`.
            None => Ok(ParseOutput::default()),
            Some(table) => serde_json::to_vec(&table)
                .map(Into::into)
                .map_err(|err| BinocError::Other(format!("serialize tabular artifact: {err}"))),
        }
    }
}

impl ParseRule for JsonMediaRecordsParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.json_media_records".into(),
            input: NodeMatch {
                is_dir: Some(false),
                media_types: vec![
                    "application/json".into(),
                    "application/ld+json".into(),
                    "application/geo+json".into(),
                ],
                ..NodeMatch::default()
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let table = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => json_records(&value, item.tabular_parse.as_ref())?,
            Err(err) if records_path_config(item.tabular_parse.as_ref()).is_some() => {
                return Err(BinocError::Other(format!("parse JSON: {err}")));
            }
            Err(_) => None,
        };
        match table {
            None => Ok(ParseOutput::default()),
            Some(table) => serde_json::to_vec(&table)
                .map(Into::into)
                .map_err(|err| BinocError::Other(format!("serialize tabular artifact: {err}"))),
        }
    }
}

fn records_path_config(config: Option<&TabularParseConfig>) -> Option<&str> {
    config.and_then(|config| config.records_path.as_deref())
}

fn json_records(
    value: &serde_json::Value,
    config: Option<&TabularParseConfig>,
) -> BinocResult<Option<TabularData>> {
    if let Some(records_path) = records_path_config(config) {
        if records_path.trim().is_empty() {
            return Err(BinocError::Config(
                "records_path must be a non-empty JSON path".into(),
            ));
        }
        return records_from_path(value, records_path).map(Some);
    }

    Ok(detect_json_records(value))
}

/// Detect a consistently-shaped record collection in a parsed JSON value.
///
/// An array whose elements are all objects becomes a named table (columns are
/// the union of keys in first-seen order; missing keys are `Null`). An array
/// whose elements are all arrays becomes a headerless, positional table.
/// Anything else returns `None` (the document is not record-shaped).
fn detect_json_records(value: &serde_json::Value) -> Option<TabularData> {
    if let Some(table) = geojson_records(value) {
        return Some(table);
    }
    let array = value.as_array()?;
    if array.is_empty() {
        return None;
    }
    if array.iter().all(serde_json::Value::is_object) {
        Some(table_from_objects(array))
    } else if array.iter().all(serde_json::Value::is_array) {
        Some(table_from_arrays(array))
    } else {
        None
    }
}

fn records_from_path(value: &serde_json::Value, path: &str) -> BinocResult<TabularData> {
    let records = resolve_simple_json_path(value, path)?;
    let Some(array) = records.as_array() else {
        return Err(BinocError::Config(format!(
            "records_path {path:?} did not resolve to an array"
        )));
    };
    if array.is_empty() {
        return Err(BinocError::Config(format!(
            "records_path {path:?} resolved to an empty array"
        )));
    }
    if array.iter().all(serde_json::Value::is_object) {
        Ok(table_from_objects(array))
    } else if array.iter().all(serde_json::Value::is_array) {
        Ok(table_from_arrays(array))
    } else {
        Err(BinocError::Config(format!(
            "records_path {path:?} resolved to an array that is not consistently objects or arrays"
        )))
    }
}

fn resolve_simple_json_path<'a>(
    mut value: &'a serde_json::Value,
    path: &str,
) -> BinocResult<&'a serde_json::Value> {
    let Some(rest) = path.strip_prefix('$') else {
        return Err(BinocError::Config(format!(
            "records_path {path:?} must start with '$'"
        )));
    };
    if rest.is_empty() {
        return Ok(value);
    }
    let Some(rest) = rest.strip_prefix('.') else {
        return Err(BinocError::Config(format!(
            "records_path {path:?} must use simple dotted form like '$.objects'"
        )));
    };
    for key in rest.split('.') {
        if key.is_empty() {
            return Err(BinocError::Config(format!(
                "records_path {path:?} contains an empty path segment"
            )));
        }
        let Some(object) = value.as_object() else {
            return Err(BinocError::Config(format!(
                "records_path {path:?} cannot descend through non-object segment {key:?}"
            )));
        };
        let Some(next) = object.get(key) else {
            return Err(BinocError::Config(format!(
                "records_path {path:?} did not resolve; missing segment {key:?}"
            )));
        };
        value = next;
    }
    Ok(value)
}

/// Detect a GeoJSON `FeatureCollection` and build a table from its features.
///
/// Recognizes a top-level object `{"type":"FeatureCollection","features":[...]}`
/// whose `features` is a non-empty array of objects, and returns a table over
/// the features (each feature's `geometry`/`properties` become columns; nested
/// values such as `geometry` land as `Value::Nested` cells). Any other JSON
/// object returns `None` so it stays a `structured_document`.
fn geojson_records(value: &serde_json::Value) -> Option<TabularData> {
    let object = value.as_object()?;
    if object.get("type").and_then(serde_json::Value::as_str) != Some("FeatureCollection") {
        return None;
    }
    let features = object
        .get("features")
        .and_then(serde_json::Value::as_array)?;
    if features.is_empty() || !features.iter().all(serde_json::Value::is_object) {
        return None;
    }
    Some(table_from_objects(features))
}

/// Detect a like-shaped JSONL/NDJSON record stream: every non-blank line is a
/// JSON object.
fn jsonl_records(bytes: &[u8]) -> Option<TabularData> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut values = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        values.push(serde_json::from_str::<serde_json::Value>(line).ok()?);
    }
    if values.is_empty() || !values.iter().all(serde_json::Value::is_object) {
        return None;
    }
    Some(table_from_objects(&values))
}

fn table_from_objects(items: &[serde_json::Value]) -> TabularData {
    let mut headers: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for item in items {
        if let Some(obj) = item.as_object() {
            for key in obj.keys() {
                if seen.insert(key.clone()) {
                    headers.push(key.clone());
                }
            }
        }
    }
    let rows = items
        .iter()
        .map(|item| {
            let obj = item.as_object();
            headers
                .iter()
                .map(|header| {
                    obj.and_then(|o| o.get(header))
                        .cloned()
                        .map(binoc_sdk::Value::from_json)
                        .unwrap_or(binoc_sdk::Value::Null)
                })
                .collect()
        })
        .collect();
    TabularData::new(headers, rows)
}

fn table_from_arrays(items: &[serde_json::Value]) -> TabularData {
    let width = items
        .iter()
        .filter_map(serde_json::Value::as_array)
        .map(|a| a.len())
        .max()
        .unwrap_or(0);
    let headers = (1..=width).map(|i| format!("column_{i}")).collect();
    let rows = items
        .iter()
        .map(|item| {
            let arr = item.as_array();
            (0..width)
                .map(|i| {
                    arr.and_then(|a| a.get(i))
                        .cloned()
                        .map(binoc_sdk::Value::from_json)
                        .unwrap_or(binoc_sdk::Value::Null)
                })
                .collect()
        })
        .collect();
    let mut table = TabularData::new(headers, rows);
    table.has_header = false;
    table
}

/// Serialize a [`StructuredDocument`] to bytes and wrap it in a [`ParseOutput`].
///
/// Shared by every `structured_document_v1()` parser (JSON, YAML, TOML, INI);
/// each supplies its already-transcoded `value`, a `format` tag, and
/// format-specific `source` facts.
fn structured_document_output(
    value: serde_json::Value,
    format: &str,
    source: serde_json::Value,
) -> BinocResult<ParseOutput> {
    serde_json::to_vec(&StructuredDocument {
        value,
        format: format.into(),
        source,
    })
    .map(Into::into)
    .map_err(|err| BinocError::Other(format!("serialize structured document artifact: {err}")))
}

/// True when an item is JSON-LD, by `.jsonld` / `.json-ld` extension or the
/// `application/ld+json` media type. JSON-LD is structurally plain JSON, so it
/// flows through the same parser; this only affects the `format` tag so future
/// rewrite rules can target JSON-LD specifically.
fn is_json_ld(item: &ItemRef) -> bool {
    matches!(
        item.extension().as_deref(),
        Some(".jsonld") | Some(".json-ld")
    ) || item.media_type.as_deref() == Some("application/ld+json")
}

fn parse_json_item(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
    let bytes = data.read_bytes(item)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|err| BinocError::Other(format!("parse JSON: {err}")))?;
    // Record collections are handled as `tabular` by JsonRecordsParse; decline
    // here so the same document is not also published as a structured_document.
    if json_records(&value, item.tabular_parse.as_ref())?.is_some() {
        return Ok(ParseOutput::default());
    }
    let source = json_source_facts(&bytes)?;
    let source = serde_json::to_value(&source)
        .map_err(|err| BinocError::Other(format!("serialize JSON source facts: {err}")))?;
    // JSON-LD is plain JSON structurally but gets a distinct tag so rewrite
    // rules can target it; everything else is plain `json`.
    let format = if is_json_ld(item) { "jsonld" } else { "json" };
    structured_document_output(value, format, source)
}

fn parse_yaml_item(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
    let bytes = data.read_bytes(item)?;
    let value: serde_json::Value = serde_yaml::from_slice(&bytes)
        .map_err(|err| BinocError::Other(format!("parse YAML: {err}")))?;
    let source = serde_json::json!({ "byte_len": bytes.len() });
    structured_document_output(value, "yaml", source)
}

impl ParseRule for YamlParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.yaml".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".yaml".into(), ".yml".into()],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        parse_yaml_item(item, data)
    }
}

impl ParseRule for YamlMediaParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.yaml_media".into(),
            input: NodeMatch {
                is_dir: Some(false),
                media_types: vec![
                    "application/yaml".into(),
                    "application/x-yaml".into(),
                    "text/yaml".into(),
                ],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        parse_yaml_item(item, data)
    }
}

impl ParseRule for TomlParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.toml".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".toml".into()],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|err| BinocError::Other(format!("decode TOML as UTF-8: {err}")))?;
        let value: serde_json::Value =
            toml::from_str(text).map_err(|err| BinocError::Other(format!("parse TOML: {err}")))?;
        let source = serde_json::json!({ "byte_len": bytes.len() });
        structured_document_output(value, "toml", source)
    }
}

impl ParseRule for IniParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.ini".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".ini".into(), ".cfg".into(), ".properties".into()],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|err| BinocError::Other(format!("decode INI as UTF-8: {err}")))?;
        let value = ini_to_json(text)?;
        let source = serde_json::json!({ "byte_len": bytes.len() });
        structured_document_output(value, "ini", source)
    }
}

/// Transcode an INI document into a `serde_json::Value::Object`.
///
/// Keys in the default (unnamed) section become top-level fields; each named
/// `[section]` becomes a nested object under its name.
fn ini_to_json(text: &str) -> BinocResult<serde_json::Value> {
    let ini = ini::Ini::load_from_str(text)
        .map_err(|err| BinocError::Other(format!("parse INI: {err}")))?;
    let mut root = serde_json::Map::new();
    for (section, properties) in ini.iter() {
        let mut entries = serde_json::Map::new();
        for (key, value) in properties.iter() {
            entries.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
        match section {
            None => {
                for (key, value) in entries {
                    root.insert(key, value);
                }
            }
            Some(name) => {
                root.insert(name.to_string(), serde_json::Value::Object(entries));
            }
        }
    }
    Ok(serde_json::Value::Object(root))
}

fn json_source_facts(bytes: &[u8]) -> BinocResult<JsonSourceFacts> {
    let text = std::str::from_utf8(bytes)
        .map_err(|err| BinocError::Other(format!("decode JSON as UTF-8: {err}")))?;
    let mut object_key_orders = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(text);
    JsonKeyOrderSeed {
        path: "$".into(),
        object_key_orders: &mut object_key_orders,
    }
    .deserialize(&mut deserializer)
    .map_err(|err| BinocError::Other(format!("scan JSON source facts: {err}")))?;
    deserializer
        .end()
        .map_err(|err| BinocError::Other(format!("scan JSON source facts: {err}")))?;
    Ok(JsonSourceFacts {
        byte_len: bytes.len(),
        trailing_newline: text.ends_with('\n') || text.ends_with('\r'),
        line_ending: line_ending(text),
        indentation: indentation(text),
        object_key_orders,
    })
}

fn line_ending(text: &str) -> Option<String> {
    if text.contains("\r\n") {
        Some("crlf".into())
    } else if text.contains('\n') {
        Some("lf".into())
    } else {
        None
    }
}

fn indentation(text: &str) -> Option<String> {
    text.lines().skip(1).find_map(|line| {
        let prefix: String = line
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        if prefix.is_empty() && line.trim().is_empty() {
            None
        } else if prefix.contains('\t') {
            Some("tabs".into())
        } else if !prefix.is_empty() {
            Some(format!("{} spaces", prefix.len()))
        } else {
            Some("none".into())
        }
    })
}

struct JsonKeyOrderSeed<'a> {
    path: String,
    object_key_orders: &'a mut Vec<JsonObjectKeyOrder>,
}

impl<'de> de::DeserializeSeed<'de> for JsonKeyOrderSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> de::Visitor<'de> for JsonKeyOrderSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_none<E>(self) -> Result<(), E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        self.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut index = 0usize;
        while seq
            .next_element_seed(JsonKeyOrderSeed {
                path: format!("{}[{index}]", self.path),
                object_key_orders: self.object_key_orders,
            })?
            .is_some()
        {
            index += 1;
        }
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<(), A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let path = self.path;
        let object_key_orders = self.object_key_orders;
        let mut keys = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            keys.push(key.clone());
            map.next_value_seed(JsonKeyOrderSeed {
                path: json_child_path(&path, &key),
                object_key_orders,
            })?;
        }
        object_key_orders.push(JsonObjectKeyOrder { path, keys });
        Ok(())
    }
}

fn json_child_path(parent: &str, key: &str) -> String {
    if key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        format!("{parent}.{key}")
    } else {
        format!("{parent}[{}]", serde_json::to_string(key).expect("string"))
    }
}

fn default_delimiter_for(item: &ItemRef) -> u8 {
    if item.media_type.as_deref() == Some("text/tab-separated-values") {
        return b'\t';
    }
    match item.extension().as_deref() {
        Some(".tsv") => b'\t',
        _ => b',',
    }
}

#[cfg(test)]
fn parse_csv_bytes(bytes: &[u8], delimiter: u8) -> BinocResult<TabularData> {
    parse_csv_records(bytes, &ResolvedCsvDialect::for_tests(delimiter)).map(table_from_csv_records)
}

fn parse_csv_records(bytes: &[u8], dialect: &ResolvedCsvDialect) -> BinocResult<Vec<Vec<String>>> {
    let mut builder = csv::ReaderBuilder::new();
    builder
        .delimiter(dialect.delimiter)
        .has_headers(false)
        .flexible(true);
    if let Some(quote) = dialect.quote {
        builder.quote(quote).quoting(true);
    } else {
        builder.quoting(false);
    }
    builder.double_quote(dialect.double_quote);
    if let Some(escape) = dialect.escape {
        builder.escape(Some(escape));
    }
    if let Some(terminator) = dialect.terminator() {
        builder.terminator(terminator);
    }
    let parse_bytes = if dialect.bom && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    let mut reader = builder.from_reader(parse_bytes);
    let mut records = Vec::new();
    let mut record = csv::ByteRecord::new();
    while reader
        .read_byte_record(&mut record)
        .map_err(|err| BinocError::Csv(err.to_string()))?
    {
        records.push(
            record
                .iter()
                .map(|field| String::from_utf8_lossy(field).into_owned())
                .collect(),
        );
    }
    Ok(records)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedCsvDialect {
    delimiter: u8,
    default_delimiter: u8,
    quote: Option<u8>,
    escape: Option<u8>,
    double_quote: bool,
    bom: bool,
    newline: Option<String>,
    inferred: bool,
}

impl ResolvedCsvDialect {
    #[cfg(test)]
    fn for_tests(delimiter: u8) -> Self {
        Self {
            delimiter,
            default_delimiter: delimiter,
            quote: Some(b'"'),
            escape: None,
            double_quote: true,
            bom: false,
            newline: Some("\n".into()),
            inferred: false,
        }
    }

    fn terminator(&self) -> Option<csv::Terminator> {
        match self.newline.as_deref() {
            Some("\n") => Some(csv::Terminator::Any(b'\n')),
            Some("\r") => Some(csv::Terminator::Any(b'\r')),
            Some("\r\n") => Some(csv::Terminator::CRLF),
            _ => None,
        }
    }

    fn should_disclose_provenance(&self) -> bool {
        self.inferred && !self.matches_boring_default()
    }

    fn matches_boring_default(&self) -> bool {
        self.delimiter == self.default_delimiter
            && self.default_delimiter == b','
            && matches!(self.quote, None | Some(b'"'))
            && self.escape.is_none()
            && !self.bom
            && self.newline.as_deref() == Some("\n")
    }
}

fn resolve_csv_dialect(item: &ItemRef, bytes: &[u8]) -> BinocResult<ResolvedCsvDialect> {
    let mut resolved = sniff_csv_dialect(bytes, default_delimiter_for(item));
    let Some(parse) = item.tabular_parse.as_ref() else {
        resolved.inferred = true;
        return Ok(resolved);
    };
    let declared = parse.delimiter.is_some() || parse.dialect.is_some();
    resolved.inferred = !declared;
    if let Some(delimiter) = parse.delimiter.as_deref() {
        resolved.delimiter = single_byte(delimiter, "delimiter")?;
    }
    if let Some(dialect) = parse.dialect.as_ref() {
        apply_declared_dialect(&mut resolved, dialect)?;
    }
    Ok(resolved)
}

fn apply_declared_dialect(
    resolved: &mut ResolvedCsvDialect,
    dialect: &CsvDialectConfig,
) -> BinocResult<()> {
    if let Some(delimiter) = dialect.delimiter.as_deref() {
        resolved.delimiter = single_byte(delimiter, "delimiter")?;
    }
    if let Some(quote) = dialect.quote.as_deref() {
        resolved.quote = Some(single_byte(quote, "quote")?);
    }
    if let Some(escape) = dialect.escape.as_deref() {
        resolved.escape = Some(single_byte(escape, "escape")?);
        resolved.double_quote = false;
    }
    if let Some(bom) = dialect.bom {
        resolved.bom = bom;
    }
    if let Some(newline) = dialect.newline.as_ref() {
        resolved.newline = Some(normalize_newline(newline)?);
    }
    Ok(())
}

fn single_byte(value: &str, field: &str) -> BinocResult<u8> {
    let bytes = value.as_bytes();
    if bytes.len() == 1 {
        Ok(bytes[0])
    } else {
        Err(BinocError::Other(format!(
            "CSV dialect {field} must be exactly one byte, got {value:?}"
        )))
    }
}

fn normalize_newline(value: &str) -> BinocResult<String> {
    match value {
        "\\n" => Ok("\n".into()),
        "\\r" => Ok("\r".into()),
        "\\r\\n" => Ok("\r\n".into()),
        "\n" | "\r" | "\r\n" => Ok(value.into()),
        _ => Err(BinocError::Other(format!(
            "CSV dialect newline must be one of \\n, \\r, or \\r\\n, got {value:?}"
        ))),
    }
}

fn sniff_csv_dialect(bytes: &[u8], fallback_delimiter: u8) -> ResolvedCsvDialect {
    let sample_end = bytes.len().min(CSV_SNIFF_BYTES);
    let sample = &bytes[..sample_end];
    let bom = sample.starts_with(&[0xEF, 0xBB, 0xBF]);
    let sample = if bom && sample.len() >= 3 {
        &sample[3..]
    } else {
        sample
    };
    let delimiter = sniff_delimiter(sample, fallback_delimiter).unwrap_or(fallback_delimiter);
    let quote = sniff_quote(sample, delimiter);
    let escape = quote.and_then(|quote| sniff_escape(sample, quote));
    let double_quote = escape.is_none();
    let newline = sniff_newline(sample);
    ResolvedCsvDialect {
        delimiter,
        default_delimiter: fallback_delimiter,
        quote,
        escape,
        double_quote,
        bom,
        newline,
        inferred: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DelimiterScore {
    candidate: u8,
    mode_rows: usize,
    row_count: usize,
    occurrences: usize,
    numeric_occurrences: usize,
    non_numeric_occurrences: usize,
    is_default: bool,
}

fn sniff_delimiter(sample: &[u8], default_delimiter: u8) -> Option<u8> {
    let mut best: Option<DelimiterScore> = None;
    for &candidate in CSV_DELIMITER_CANDIDATES {
        let Some(score) = delimiter_score(sample, candidate, default_delimiter) else {
            continue;
        };
        if best.as_ref().is_none_or(|best| score.beats(*best)) {
            best = Some(score);
        }
    }
    best.map(|score| score.candidate)
}

impl DelimiterScore {
    fn beats(self, other: Self) -> bool {
        if self.beats_decimal_comma_default(other) {
            return true;
        }
        if other.beats_decimal_comma_default(self) {
            return false;
        }
        (
            self.mode_rows,
            self.row_count,
            self.non_numeric_occurrences,
            self.is_default,
        ) > (
            other.mode_rows,
            other.row_count,
            other.non_numeric_occurrences,
            other.is_default,
        )
    }

    fn beats_decimal_comma_default(self, other: Self) -> bool {
        matches!(self.candidate, b'\t' | b'|' | b';')
            && other.candidate == b','
            && other.is_default
            && other.occurrences > 0
            && other.occurrences == other.numeric_occurrences
            && self.mode_rows == other.mode_rows
            && self.row_count == other.row_count
    }
}

fn delimiter_score(sample: &[u8], candidate: u8, default_delimiter: u8) -> Option<DelimiterScore> {
    let mut widths = Vec::new();
    let mut occurrences = 0;
    let mut numeric_occurrences = 0;
    for line in sample.split(|byte| *byte == b'\n' || *byte == b'\r') {
        if line.is_empty() {
            continue;
        }
        let stats = line_delimiter_stats(line, candidate);
        widths.push(stats.width);
        occurrences += stats.occurrences;
        numeric_occurrences += stats.numeric_occurrences;
    }
    if widths.len() < 2 {
        return None;
    }
    let (mode_width, mode_rows) = mode_width(&widths);
    if mode_width < 2 {
        return None;
    }
    Some(DelimiterScore {
        candidate,
        mode_rows,
        row_count: widths.len(),
        occurrences,
        numeric_occurrences,
        non_numeric_occurrences: occurrences.saturating_sub(numeric_occurrences),
        is_default: candidate == default_delimiter,
    })
}

fn mode_width(widths: &[usize]) -> (usize, usize) {
    let mut sorted = widths.to_vec();
    sorted.sort_unstable();
    let mut best_width = sorted[0];
    let mut best_count = 1;
    let mut current_width = sorted[0];
    let mut current_count = 1;
    for width in sorted.into_iter().skip(1) {
        if width == current_width {
            current_count += 1;
            continue;
        }
        if current_count > best_count {
            best_width = current_width;
            best_count = current_count;
        }
        current_width = width;
        current_count = 1;
    }
    if current_count > best_count {
        best_width = current_width;
        best_count = current_count;
    }
    (best_width, best_count)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineDelimiterStats {
    width: usize,
    occurrences: usize,
    numeric_occurrences: usize,
}

#[cfg(test)]
fn sample_widths(sample: &[u8], delimiter: u8) -> Vec<usize> {
    let mut widths = Vec::new();
    for line in sample.split(|byte| *byte == b'\n' || *byte == b'\r') {
        if line.is_empty() {
            continue;
        }
        widths.push(line_delimiter_stats(line, delimiter).width);
    }
    widths
}

fn line_delimiter_stats(line: &[u8], delimiter: u8) -> LineDelimiterStats {
    let mut stats = LineDelimiterStats {
        width: 1,
        occurrences: 0,
        numeric_occurrences: 0,
    };
    let mut active_quote = None;
    let mut at_field_start = true;
    let mut index = 0;
    while index < line.len() {
        let byte = line[index];
        if let Some(quote) = active_quote {
            if byte == quote {
                if index + 1 < line.len() && line[index + 1] == quote {
                    index += 2;
                    continue;
                }
                active_quote = None;
            }
            index += 1;
            continue;
        }
        if at_field_start && matches!(byte, b'"' | b'\'') {
            active_quote = Some(byte);
            at_field_start = false;
            index += 1;
            continue;
        }
        if byte == delimiter {
            stats.width += 1;
            stats.occurrences += 1;
            if index > 0
                && index + 1 < line.len()
                && line[index - 1].is_ascii_digit()
                && line[index + 1].is_ascii_digit()
            {
                stats.numeric_occurrences += 1;
            }
            at_field_start = true;
            index += 1;
            continue;
        }
        at_field_start = false;
        index += 1;
    }
    stats
}

fn sniff_quote(sample: &[u8], delimiter: u8) -> Option<u8> {
    [b'"', b'\''].into_iter().find(|quote| {
        sample
            .split(|byte| *byte == b'\n' || *byte == b'\r')
            .filter(|line| !line.is_empty())
            .any(|line| line_contains_quoted_field(line, delimiter, *quote))
    })
}

fn line_contains_quoted_field(line: &[u8], delimiter: u8, quote: u8) -> bool {
    let mut at_field_start = true;
    let mut in_quotes = false;
    let mut saw_quote = false;
    for &byte in line {
        if at_field_start && byte == quote {
            in_quotes = !in_quotes;
            saw_quote = true;
        } else if in_quotes && byte == quote {
            in_quotes = false;
        } else if !in_quotes && byte == delimiter {
            at_field_start = true;
            continue;
        }
        at_field_start = false;
    }
    saw_quote && !in_quotes
}

fn sniff_escape(sample: &[u8], quote: u8) -> Option<u8> {
    if plausible_backslash_escaped_quotes(sample, quote) >= 2 {
        Some(b'\\')
    } else {
        None
    }
}

fn plausible_backslash_escaped_quotes(sample: &[u8], quote: u8) -> usize {
    let mut count = 0;
    for line in sample.split(|byte| *byte == b'\n' || *byte == b'\r') {
        let mut in_quotes = false;
        let mut index = 0;
        while index < line.len() {
            let byte = line[index];
            if byte == quote {
                in_quotes = !in_quotes;
                index += 1;
                continue;
            }
            if in_quotes && byte == b'\\' && index + 1 < line.len() && line[index + 1] == quote {
                let after_quote = line.get(index + 2).copied();
                if !matches!(after_quote, None | Some(b',' | b'\t' | b'|' | b';' | b':')) {
                    count += 1;
                }
                index += 2;
                continue;
            }
            index += 1;
        }
    }
    count
}

fn sniff_newline(sample: &[u8]) -> Option<String> {
    let crlf = sample.windows(2).filter(|pair| *pair == b"\r\n").count();
    let lf = sample.iter().filter(|byte| **byte == b'\n').count();
    let cr = sample.iter().filter(|byte| **byte == b'\r').count();
    if crlf > 0 && crlf * 2 >= lf.max(cr) {
        Some("\r\n".into())
    } else if lf > 0 {
        Some("\n".into())
    } else if cr > 0 {
        Some("\r".into())
    } else {
        None
    }
}

fn dialect_provenance_summary(dialect: &ResolvedCsvDialect) -> String {
    let mut parts = vec![format!(
        "detected {}-delimited",
        describe_byte(dialect.delimiter)
    )];
    match dialect.quote {
        Some(quote) => parts.push(format!("quote {}", describe_byte(quote))),
        None => parts.push("no quoting".into()),
    }
    if let Some(escape) = dialect.escape {
        parts.push(format!("escape {}", describe_byte(escape)));
    }
    if dialect.bom {
        parts.push("UTF-8 BOM".into());
    }
    if let Some(newline) = dialect.newline.as_deref() {
        parts.push(format!("newline {}", describe_newline(newline)));
    }
    parts.join(", ")
}

fn describe_byte(byte: u8) -> String {
    match byte {
        b'\t' => "tab".into(),
        b'|' => "`|`".into(),
        b',' => "comma".into(),
        b';' => "semicolon".into(),
        b':' => "colon".into(),
        b'"' => "`\"`".into(),
        b'\'' => "`'`".into(),
        b'\\' => "`\\\\`".into(),
        other => format!("`{}`", char::from(other)),
    }
}

fn describe_newline(newline: &str) -> &'static str {
    match newline {
        "\r\n" => "CRLF",
        "\r" => "CR",
        _ => "LF",
    }
}
#[cfg(test)]
fn table_from_csv_records(records: Vec<Vec<String>>) -> TabularData {
    table_from_csv_records_with_config(records, None)
}

fn table_from_csv_records_with_config(
    records: Vec<Vec<String>>,
    config: Option<&TabularParseConfig>,
) -> TabularData {
    let Some(first) = records.first() else {
        return TabularData::from_string_rows(Vec::new(), Vec::new());
    };
    let has_header = config.is_none_or(|config| config.header);
    let skip_lines = config.and_then(|config| config.skip_lines).unwrap_or(0);
    let header_index = config
        .and_then(|config| config.header_line)
        .map(|line| line.saturating_sub(1))
        .unwrap_or(skip_lines);

    if has_header {
        let header = records.get(header_index).unwrap_or(first);
        let width = records
            .iter()
            .skip(header_index)
            .map(Vec::len)
            .max()
            .unwrap_or(header.len());
        let headers = complete_csv_headers(header, width);
        let rows = records.into_iter().skip(header_index + 1).collect();
        TabularData::from_string_rows(headers, rows)
    } else {
        let width = records
            .iter()
            .skip(skip_lines)
            .map(Vec::len)
            .max()
            .unwrap_or(first.len());
        let headers = complete_csv_headers(&[], width);
        let rows = records.into_iter().skip(skip_lines).collect();
        let mut table = TabularData::from_string_rows(headers, rows);
        table.has_header = false;
        table
    }
}

fn complete_csv_headers(first: &[String], width: usize) -> Vec<String> {
    let mut headers = Vec::with_capacity(width);
    let mut seen = BTreeSet::new();
    for index in 0..width {
        let raw = first
            .get(index)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("column_{}", index + 1));
        let mut candidate = raw.clone();
        let mut suffix = 2usize;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{raw}_{suffix}");
            suffix += 1;
        }
        headers.push(candidate);
    }
    headers
}

#[derive(Debug, Clone)]
struct StackedSection {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

/// Split a CSV into stacked sub-tables, or report it as a single flat table.
///
/// This is an intentionally **conservative placeholder**, not a real
/// table-discovery algorithm. Its sole job is to stop the previous heuristic
/// from shredding flat tables: it fires only on an obvious, unambiguous stack
/// and otherwise leaves the file as one table. A proper implementation (handling
/// preamble, footnotes, ragged real-world data, fixed-width layouts, etc.) is
/// future work — see `docs/core-developers/research/csv-table-extraction.md`
/// (the ExtracTable / Pytheas survey) for the planned approach.
///
/// The rule: partition the rows into **regions** of consecutive rows sharing the
/// same [`normalized_width`]. Blank rows (`normalized_width == 0`) are
/// transparent — they neither start, extend, nor break a region. The file is a
/// stack only if there are between 2 and 5 regions inclusive and every region
/// has more than 10 rows (≥ 11, counting its header). When it qualifies, each
/// region's first row is the header and the rest are data rows, trimmed to the
/// region width. Otherwise an empty `Vec` is returned (a single flat table).
fn detect_stacked_sections_from_rows(rows: &[Vec<String>]) -> Vec<StackedSection> {
    // Partition into regions of consecutive same-width rows, skipping blanks.
    let mut regions: Vec<Vec<Vec<String>>> = Vec::new();
    let mut current_width: Option<usize> = None;
    for row in rows {
        let width = normalized_width(row);
        if width == 0 {
            // Blank rows are transparent.
            continue;
        }
        if current_width == Some(width) {
            regions
                .last_mut()
                .expect("current_width set implies a region exists")
                .push(trim_to_width(row, width));
        } else {
            current_width = Some(width);
            regions.push(vec![trim_to_width(row, width)]);
        }
    }

    // Qualify: 2..=5 regions, each with more than 10 rows (header included).
    if !(2..=5).contains(&regions.len()) || regions.iter().any(|region| region.len() <= 10) {
        return Vec::new();
    }

    regions
        .into_iter()
        .map(|mut region| {
            let headers = region.remove(0);
            StackedSection {
                headers,
                rows: region,
            }
        })
        .collect()
}

/// Build one `tabular_v1` child node per detected stacked section, named
/// positionally `table_1`, `table_2`, ….
fn children_from_sections(parent_path: &str, sections: &[StackedSection]) -> Vec<ParsedChild> {
    let mut children = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        let name = format!("table_{}", index + 1);
        let logical_path = decompose_child(parent_path, &name);
        let table = TabularData::from_string_rows(section.headers.clone(), section.rows.clone());
        let bytes = serde_json::to_vec(&table)
            .expect("serializing parse-owned tabular section should not fail");
        children.push(ParsedChild {
            item: ItemRef {
                logical_path: logical_path.clone(),
                is_dir: false,
                content_hash: Some(blake3::hash(&bytes).to_hex().to_string()),
                size: Some(bytes.len() as u64),
                media_type: Some("application/vnd.binoc.tabular+json".into()),
                projection_hint: ProjectionHint::default().item_type("tabular"),
                tabular_parse: None,
                handle: logical_path,
            },
            artifacts: vec![ParsedArtifact {
                format: tabular_v1(),
                bytes,
            }],
        });
    }
    children
}

fn normalized_width(row: &[String]) -> usize {
    row.iter()
        .rposition(|cell| !cell.trim().is_empty())
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn trim_to_width(row: &[String], width: usize) -> Vec<String> {
    (0..width)
        .map(|index| {
            row.get(index)
                .map(|cell| cell.trim().to_string())
                .unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(csv: &str) -> Vec<StackedSection> {
        let records = parse_csv_records(csv.as_bytes(), &ResolvedCsvDialect::for_tests(b','))
            .expect("parse csv");
        detect_stacked_sections_from_rows(&records)
    }

    /// Build a CSV body of `count` rows, each `width` comma-separated cells,
    /// the first row a header. Cell values are unique enough to be valid rows.
    fn region(prefix: &str, width: usize, count: usize) -> String {
        let mut out = String::new();
        let header: Vec<String> = (0..width).map(|c| format!("{prefix}_h{c}")).collect();
        out.push_str(&header.join(","));
        out.push('\n');
        for r in 0..count.saturating_sub(1) {
            let cells: Vec<String> = (0..width).map(|c| format!("{prefix}_{r}_{c}")).collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        out
    }

    #[test]
    fn csv_parse_preserves_fields_after_single_cell_banner() {
        let csv = "Land-Ocean: Global Means\n\
                   Year,Jan,Feb\n\
                   1880,-.18,-.24\n";
        let table = parse_csv_bytes(csv.as_bytes(), b',').expect("parse csv");
        assert_eq!(
            table.headers,
            vec![
                "Land-Ocean: Global Means".to_string(),
                "column_2".to_string(),
                "column_3".to_string()
            ]
        );
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][0].as_text(), "Year");
        assert_eq!(table.rows[0][1].as_text(), "Jan");
        assert_eq!(table.rows[0][2].as_text(), "Feb");
        assert_eq!(table.rows[1][2].as_text(), "-.24");
    }

    #[test]
    fn sniff_csv_dialect_detects_pipe_without_quotes() {
        let dialect = sniff_csv_dialect(b"id|value\n1|old\n2|same\n", b',');
        assert_eq!(dialect.delimiter, b'|');
        assert_eq!(dialect.quote, None);
        assert_eq!(dialect.newline.as_deref(), Some("\n"));
        assert_eq!(
            dialect_provenance_summary(&dialect),
            "detected `|`-delimited, no quoting, newline LF"
        );
    }

    #[test]
    fn default_inferred_csv_without_quoted_fields_is_silent() {
        let dialect = sniff_csv_dialect(b"id,value\n1,old\n2,same\n", b',');
        let dialect = ResolvedCsvDialect {
            inferred: true,
            ..dialect
        };
        assert!(dialect.matches_boring_default());
        assert!(!dialect.should_disclose_provenance());
    }

    #[test]
    fn default_inferred_csv_with_double_quotes_is_silent() {
        let dialect = sniff_csv_dialect(b"id,value\n1,\"old\"\n2,\"same\"\n", b',');
        let dialect = ResolvedCsvDialect {
            inferred: true,
            ..dialect
        };
        assert_eq!(dialect.quote, Some(b'"'));
        assert!(dialect.matches_boring_default());
        assert!(!dialect.should_disclose_provenance());
    }

    #[test]
    fn inferred_non_default_csv_keeps_provenance() {
        let dialect = sniff_csv_dialect(b"id\tvalue\n1\told\n2\tsame\n", b',');
        let dialect = ResolvedCsvDialect {
            inferred: true,
            ..dialect
        };
        assert!(!dialect.matches_boring_default());
        assert!(dialect.should_disclose_provenance());
    }

    #[test]
    fn inferred_comma_delimiter_for_tsv_keeps_provenance() {
        let dialect = sniff_csv_dialect(b"id,value\n1,old\n2,same\n", b'\t');
        let dialect = ResolvedCsvDialect {
            inferred: true,
            ..dialect
        };
        assert_eq!(dialect.delimiter, b',');
        assert!(!dialect.matches_boring_default());
        assert!(dialect.should_disclose_provenance());
    }

    #[test]
    fn sniff_csv_dialect_uses_extension_prior_for_headerless_time_csv() {
        let dialect = sniff_csv_dialect(b"12:30:00,5\n13:45:00,6\n", b',');
        assert_eq!(dialect.delimiter, b',');
    }

    #[test]
    fn sniff_csv_dialect_uses_extension_prior_for_headerless_decimal_comma_tsv() {
        let dialect = sniff_csv_dialect(b"foo\t1,5\nbar\t2,5\n", b'\t');
        assert_eq!(dialect.delimiter, b'\t');
    }

    #[test]
    fn sniff_csv_dialect_detects_headerless_semicolon_decimal_comma_csv() {
        let dialect = sniff_csv_dialect(b"foo;1,5\nbar;2,5\n", b',');
        assert_eq!(dialect.delimiter, b';');
    }

    #[test]
    fn sniff_csv_dialect_detects_all_numeric_headerless_semicolon_decimal_comma_csv() {
        let dialect = sniff_csv_dialect(b"1,5;2,5\n3,5;4,5\n", b',');
        assert_eq!(dialect.delimiter, b';');
    }

    #[test]
    fn sniff_csv_dialect_ignores_quoted_commas_for_pipe_file() {
        let dialect = sniff_csv_dialect(b"\"1,5\"|x\n\"2,5\"|y\n", b',');
        assert_eq!(dialect.delimiter, b'|');
    }

    #[test]
    fn sample_widths_ignores_quoted_delimiters() {
        assert_eq!(sample_widths(b"\"1,5\"|x\n\"2,5\"|y\n", b','), vec![1, 1]);
        assert_eq!(sample_widths(b"\"1,5\"|x\n\"2,5\"|y\n", b'|'), vec![2, 2]);
    }

    #[test]
    fn sniff_escape_ignores_single_backslash_before_closing_quote() {
        let dialect = sniff_csv_dialect(
            br#"id,path
1,"C:\temp\"
2,"C:\logs\"
"#,
            b',',
        );
        assert_eq!(dialect.escape, None);
        assert!(dialect.double_quote);
    }

    #[test]
    fn parse_csv_records_respects_backslash_escape() {
        let dialect = ResolvedCsvDialect {
            delimiter: b'|',
            default_delimiter: b'|',
            quote: Some(b'"'),
            escape: Some(b'\\'),
            double_quote: false,
            bom: false,
            newline: Some("\n".into()),
            inferred: false,
        };
        let rows = parse_csv_records(
            br#"id|note
1|"said \"hi\""
"#,
            &dialect,
        )
        .expect("parse escaped csv");
        assert_eq!(rows[1][1], "said \"hi\"");
    }

    #[test]
    fn flat_ragged_csv_is_not_stacked() {
        // A plain flat table with a few ragged rows (brfss / fda shape). Width
        // varies row-to-row, so it never forms 2..=5 fat uniform regions: a
        // single flat table, no splitting.
        let csv = "State,Topic,Response,Break_Out,Sample_Size\n\
                   Alabama,Health Status,Excellent,Overall,1234\n\
                   Alaska,Diabetes,Yes,Age 18-24,extra,cell\n\
                   Arizona,Smoking,Current,Female,Male,Other,More\n\
                   Arkansas,Health Status,Good,Overall,2345\n\
                   California,Diabetes,No,Age 25-34,3456\n";
        assert!(detect(csv).is_empty());
    }

    #[test]
    fn flat_rectangular_csv_is_not_stacked() {
        // A uniform-width table (with an internal blank line) is one region, so
        // it is not a stack even though it is large.
        let mut csv = region("a", 4, 30);
        csv.push('\n'); // transparent blank line inside the single region
        csv.push_str(
            &region("b", 4, 5)
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let sections = detect(&csv);
        assert!(sections.is_empty(), "single-width file must stay flat");
    }

    #[test]
    fn genuine_two_region_stack_splits() {
        // Region A: width 3, 12 rows. Blank line. Region B: width 5, 14 rows.
        let mut csv = region("a", 3, 12);
        csv.push('\n');
        csv.push_str(&region("b", 5, 14));
        let sections = detect(&csv);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].headers, vec!["a_h0", "a_h1", "a_h2"]);
        assert_eq!(sections[0].rows.len(), 11);
        assert_eq!(
            sections[1].headers,
            vec!["b_h0", "b_h1", "b_h2", "b_h3", "b_h4"]
        );
        assert_eq!(sections[1].rows.len(), 13);
    }

    #[test]
    fn regions_with_too_few_rows_are_not_stacked() {
        // Two regions of different widths but each only 8 rows (≤ 10): below the
        // size floor, so not a stack.
        let mut csv = region("a", 3, 8);
        csv.push('\n');
        csv.push_str(&region("b", 5, 8));
        assert!(detect(&csv).is_empty());
    }

    #[test]
    fn more_than_five_regions_is_not_stacked() {
        // Six fat regions of alternating widths exceeds the 5-region cap.
        let widths = [3usize, 4, 3, 4, 3, 4];
        let mut csv = String::new();
        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                csv.push('\n');
            }
            csv.push_str(&region(&format!("r{i}"), *w, 12));
        }
        assert!(detect(&csv).is_empty());
    }
}
