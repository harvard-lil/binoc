use std::collections::BTreeSet;

use binoc_sdk::{
    decompose_child, structured_document_v1, tabular_v1, BinocError, BinocResult, DataAccess,
    Diagnostic, ItemRef, NodeMatch, ParseDescriptor, ParseOutput, ParseRule, ParsedArtifact,
    ParsedChild, ProjectionHint, StructuredDocument, Summary, TabularData,
};
use serde::{de, de::DeserializeSeed, Deserialize, Deserializer, Serialize};

pub struct CsvParse;
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
        let bytes = data.read_bytes(item)?;
        let tabular = parse_csv_bytes(&bytes, delimiter_for(item))?;
        let detection = detect_stacked_sections(&tabular);
        let is_ambiguous = detection.ambiguous_reason.is_some();
        let diagnostics: Vec<Diagnostic> = detection
            .ambiguous_reason
            .into_iter()
            .map(|reason| Diagnostic::suggestion("binoc.table_splitter.ambiguous", reason))
            .collect();

        // Ambiguous or fewer than two clean sections: a plain CSV is a single
        // table, emitted as a LEAF `tabular_v1` artifact with no children.
        if is_ambiguous || detection.sections.len() < 2 {
            let bytes = serde_json::to_vec(&tabular)
                .map_err(|err| BinocError::Other(format!("serialize tabular artifact: {err}")))?;
            return Ok(ParseOutput {
                bytes,
                diagnostics,
                children: Vec::new(),
                artifacts: Vec::new(),
                projection: ProjectionHint::default(),
            });
        }

        // Two or more clean stacked tables: a CONTAINER parse — no parent
        // artifact, one `tabular_v1` child node per detected section.
        let children = children_from_sections(&item.logical_path, &detection.sections);
        Ok(ParseOutput {
            bytes: Vec::new(),
            diagnostics,
            children,
            artifacts: Vec::new(),
            projection: ProjectionHint::default().item_type("stacked tables"),
        })
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
        let table = match item.extension().as_deref() {
            Some(".jsonl") | Some(".ndjson") => jsonl_records(&bytes),
            _ => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) => json_records(&value),
                Err(_) => None,
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
            Ok(value) => json_records(&value),
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

/// Detect a consistently-shaped record collection in a parsed JSON value.
///
/// An array whose elements are all objects becomes a named table (columns are
/// the union of keys in first-seen order; missing keys are `Null`). An array
/// whose elements are all arrays becomes a headerless, positional table.
/// Anything else returns `None` (the document is not record-shaped).
fn json_records(value: &serde_json::Value) -> Option<TabularData> {
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
    let headers = (1..=width).map(|i| i.to_string()).collect();
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
    if json_records(&value).is_some() {
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

fn delimiter_for(item: &ItemRef) -> u8 {
    match item.extension().as_deref() {
        Some(".tsv") => b'\t',
        _ => b',',
    }
}

fn parse_csv_bytes(bytes: &[u8], delimiter: u8) -> BinocResult<TabularData> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(bytes);
    let headers = reader
        .byte_headers()
        .map_err(|err| BinocError::Csv(err.to_string()))?
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    let mut rows = Vec::new();
    let mut record = csv::ByteRecord::new();
    while reader
        .read_byte_record(&mut record)
        .map_err(|err| BinocError::Csv(err.to_string()))?
    {
        rows.push(
            record
                .iter()
                .map(|field| String::from_utf8_lossy(field).into_owned())
                .collect(),
        );
    }
    Ok(TabularData::from_string_rows(headers, rows))
}

#[derive(Debug, Clone)]
struct StackedSection {
    title: Option<String>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
struct StackedDetection {
    sections: Vec<StackedSection>,
    ambiguous_reason: Option<Summary>,
}

fn detect_stacked_sections(table: &TabularData) -> StackedDetection {
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
        let mut section_rows = Vec::new();
        let mut j = i + 1;
        while j < rows.len() {
            let row_width = normalized_width(&rows[j]);
            if row_width == 0 || row_width != width {
                break;
            }
            let row = trim_to_width(&rows[j], width);
            if row != headers {
                section_rows.push(row);
            }
            j += 1;
        }

        if section_rows.is_empty() {
            wide_unclaimed.push(header_row + 1);
            i = header_row + 1;
            continue;
        }

        sections.push(StackedSection {
            title: title_from_rows(&rows, &title_rows),
            headers,
            rows: section_rows,
        });
        i = j;
    }

    // Only treat unclaimed wide rows as a genuine ambiguous *stacked* layout
    // when there is positive evidence of stacking beyond the row-width
    // heuristic: at least one section is introduced by a banner/title row (a
    // width-1 caption above its header). A plain flat table with a few ragged
    // rows can otherwise be chopped into header-led "sections" whose "headers"
    // are really data rows — that is not a stacked layout and must not surface
    // the splitter suggestion (false positive on showcase brfss-prevalence /
    // fda-purple-book flat tables, neither of which carries banner rows).
    let looks_genuinely_stacked =
        sections.len() >= 2 && sections.iter().any(|section| section.title.is_some());
    let ambiguous_reason = if looks_genuinely_stacked && !wide_unclaimed.is_empty() {
        let plural = if wide_unclaimed.len() == 1 { "" } else { "s" };
        let mut summary = Summary::new().text(format!(
            "The CSV has stacked table-like regions, but row{plural} "
        ));
        summary.extend(bounded_index_list(&wide_unclaimed));
        Some(summary.text(" outside any clear rectangle; leaving it as one table."))
    } else {
        None
    };

    StackedDetection {
        sections,
        ambiguous_reason,
    }
}

/// Maximum number of concrete row indices listed inline in a diagnostic
/// message before the rest are summarized as a remaining count, so a large
/// input can never produce an unbounded message string.
const MAX_INLINE_INDICES: usize = 5;

/// Render a list of 1-based row indices as typed [`Summary`] segments for inline
/// use in a diagnostic message, capped at [`MAX_INLINE_INDICES`] concrete
/// entries (each a digit-grouped [`Segment::Uint`]) plus an `and N more` suffix.
/// Keeps per-row diagnostic messages bounded on large inputs.
fn bounded_index_list(indices: &[usize]) -> Summary {
    let mut summary = Summary::new();
    for (position, index) in indices.iter().take(MAX_INLINE_INDICES).enumerate() {
        if position > 0 {
            summary = summary.text(", ");
        }
        summary = summary.uint(*index as u64);
    }
    if indices.len() > MAX_INLINE_INDICES {
        summary = summary
            .text(", and ")
            .uint((indices.len() - MAX_INLINE_INDICES) as u64)
            .text(" more");
    }
    summary
}

/// Build one `tabular_v1` child node per detected stacked section.
///
/// Child name is the section title where one was detected (sanitized to a stable
/// token), else a positional `table_N` fallback. Names are de-duplicated so the
/// resulting `/>`-joined logical paths stay unique.
fn children_from_sections(parent_path: &str, sections: &[StackedSection]) -> Vec<ParsedChild> {
    let mut children = Vec::new();
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    for (index, section) in sections.iter().enumerate() {
        let base = section
            .title
            .as_deref()
            .map(sanitize_name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("table_{}", index + 1));
        let name = unique_name(base, &mut used_names);
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

/// Reduce a detected section title to a stable, path-safe token: ASCII
/// alphanumerics kept, every other run collapsed to a single `_`.
fn sanitize_name(title: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// Ensure `base` is unique within `used`, appending `_2`, `_3`, … on collision.
fn unique_name(base: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}_{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn raw_rows(table: &TabularData) -> Vec<Vec<String>> {
    std::iter::once(table.headers.clone())
        .chain(
            table
                .rows
                .iter()
                .map(|row| row.iter().map(|cell| cell.as_text().into_owned()).collect()),
        )
        .collect()
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
        .filter_map(|index| rows.get(*index)?.first())
        .map(|cell| cell.trim())
        .filter(|cell| !cell.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" / "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(csv: &str) -> StackedDetection {
        let table = parse_csv_bytes(csv.as_bytes(), b',').expect("parse csv");
        detect_stacked_sections(&table)
    }

    #[test]
    fn flat_wide_csv_does_not_emit_ambiguous_suggestion() {
        // A plain flat table whose mostly-unique text rows let the width
        // heuristic chop it into header-led "sections" (the second "header" is
        // really a data row) and strand a couple of ragged rows. With no banner
        // rows it is not a stacked layout, so no splitter suggestion fires.
        let csv = "State,Topic,Response,Break_Out,Sample_Size\n\
                   Alabama,Health Status,Excellent,Overall,1234\n\
                   Alaska,Diabetes,Yes,Age 18-24,extra,cell\n\
                   Arizona,Smoking,Current,Female,Male,Other,More\n\
                   Arkansas,Health Status,Good,Overall,2345\n\
                   California,Diabetes,No,Age 25-34,3456\n";
        let detection = detect(csv);
        // The heuristic still chops it (multiple sections, stray wide rows) ...
        assert!(detection.sections.len() >= 2);
        // ... but with no banner rows it must not surface as ambiguous.
        assert!(
            detection.ambiguous_reason.is_none(),
            "flat table wrongly flagged ambiguous: {:?}",
            detection.ambiguous_reason
        );
    }

    #[test]
    fn genuinely_stacked_csv_still_emits_ambiguous_suggestion() {
        // Banner row ("Report") above the first table is positive evidence of a
        // stacked layout; with a stray wide row it stays genuinely ambiguous.
        let csv = "Report\nA,B\n1,2\n\n100,200,300\nC,D\n3,4\n";
        let detection = detect(csv);
        let reason = detection
            .ambiguous_reason
            .expect("genuinely stacked CSV should stay ambiguous");
        assert!(
            reason.plain_text().contains("outside any clear rectangle"),
            "{reason}"
        );
    }

    #[test]
    fn bounded_index_list_caps_inline_indices() {
        assert_eq!(bounded_index_list(&[3]).plain_text(), "3");
        assert_eq!(bounded_index_list(&[3, 4, 5]).plain_text(), "3, 4, 5");
        // More than the cap collapses the tail into a remaining count so the
        // message can never grow unbounded on large inputs.
        let many: Vec<usize> = (1..=12).collect();
        assert_eq!(
            bounded_index_list(&many).plain_text(),
            "1, 2, 3, 4, 5, and 7 more",
            "expected a capped list with a remaining count"
        );
        // Indices are typed `Uint` segments, so a renderer digit-groups them
        // rather than reparsing prose — a five-digit row index stays a count.
        let big = bounded_index_list(&[12345]);
        assert_eq!(big.segments(), &[binoc_sdk::Segment::Uint(12345)]);
    }
}
