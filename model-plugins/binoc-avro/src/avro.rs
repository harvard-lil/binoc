//! Avro Object Container File parse rule.
//!
//! Reads an `.avro` container, takes the writer schema (which must be a record
//! schema), and emits a typed [`TabularData`]: one column per record field, one
//! row per record in the file.

use apache_avro::schema::{RecordField, Schema};
use apache_avro::types::Value as AvroValue;
use apache_avro::Reader;
use binoc_sdk::*;

#[derive(Default)]
pub struct AvroParseRule;

impl ParseRule for AvroParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-avro.parse.avro".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".avro".into()],
                media_types: Vec::new(),
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let tabular = read_avro_tabular(&bytes)?;
        serde_json::to_vec(&tabular)
            .map(ParseOutput::from)
            .map_err(|e| BinocError::Other(format!("serialize avro tabular artifact: {e}")))
    }
}

/// Read an Avro Object Container File into a typed [`TabularData`].
///
/// The writer schema embedded in the container must be a record schema; its
/// fields define the table columns (and their declared types). Each record in
/// the file becomes a row.
fn read_avro_tabular(bytes: &[u8]) -> BinocResult<TabularData> {
    let reader = Reader::new(bytes).map_err(|e| BinocError::Other(format!("avro: {e}")))?;

    let Schema::Record(record_schema) = reader.writer_schema() else {
        return Err(BinocError::Other(
            "avro: writer schema is not a record schema".into(),
        ));
    };

    let headers: Vec<String> = record_schema
        .fields
        .iter()
        .map(|f| f.name.clone())
        .collect();
    let column_types: Vec<Option<String>> = record_schema
        .fields
        .iter()
        .map(|f| Some(field_type_name(f)))
        .collect();

    let mut rows: Vec<Vec<Value>> = Vec::new();
    for record in reader {
        let record = record.map_err(|e| BinocError::Other(format!("avro: {e}")))?;
        rows.push(record_to_row(record, &headers)?);
    }

    Ok(TabularData {
        headers,
        rows,
        has_header: true,
        key: Vec::new(),
        column_types,
        column_metadata: Vec::new(),
        table_metadata: Default::default(),
    })
}

/// Convert one Avro record value into a row aligned to `headers`.
fn record_to_row(value: AvroValue, headers: &[String]) -> BinocResult<Vec<Value>> {
    let AvroValue::Record(fields) = value else {
        return Err(BinocError::Other(format!(
            "avro: expected a record value, found {value:?}"
        )));
    };

    // Index the record's (name, value) pairs so we can emit cells in schema
    // field order regardless of the order the decoder yields them in.
    let mut by_name: std::collections::HashMap<String, AvroValue> = fields.into_iter().collect();
    let mut row = Vec::with_capacity(headers.len());
    for header in headers {
        let cell = by_name.remove(header).unwrap_or(AvroValue::Null);
        row.push(avro_value_to_cell(cell));
    }
    Ok(row)
}

/// Map an Avro value to a Binoc cell [`Value`].
///
/// Scalars map to their typed `Value` variants; a union unwraps to its held
/// value; records/arrays/maps round-trip through JSON so they land as
/// [`Value::Nested`].
fn avro_value_to_cell(value: AvroValue) -> Value {
    match value {
        AvroValue::Null => Value::Null,
        AvroValue::Boolean(b) => Value::Bool(b),
        AvroValue::Int(n) => Value::Number(n.into()),
        AvroValue::Long(n) => Value::Number(n.into()),
        AvroValue::Float(f) => f64_to_cell(f as f64),
        AvroValue::Double(f) => f64_to_cell(f),
        AvroValue::String(s) | AvroValue::Enum(_, s) => Value::String(s),
        AvroValue::Union(_, inner) => avro_value_to_cell(*inner),
        other => Value::from_json(avro_value_to_json(other)),
    }
}

fn f64_to_cell(f: f64) -> Value {
    serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
}

/// Render an Avro value (record/array/map and other composites) as JSON for
/// storage in a [`Value::Nested`] cell.
fn avro_value_to_json(value: AvroValue) -> serde_json::Value {
    match value {
        AvroValue::Null => serde_json::Value::Null,
        AvroValue::Boolean(b) => serde_json::Value::Bool(b),
        AvroValue::Int(n) => serde_json::Value::from(n),
        AvroValue::Long(n) => serde_json::Value::from(n),
        AvroValue::Float(f) => serde_json::Value::from(f),
        AvroValue::Double(f) => serde_json::Value::from(f),
        AvroValue::String(s) | AvroValue::Enum(_, s) => serde_json::Value::String(s),
        AvroValue::Bytes(b) | AvroValue::Fixed(_, b) => {
            serde_json::Value::Array(b.into_iter().map(serde_json::Value::from).collect())
        }
        AvroValue::Union(_, inner) => avro_value_to_json(*inner),
        AvroValue::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(avro_value_to_json).collect())
        }
        AvroValue::Map(entries) => serde_json::Value::Object(
            entries
                .into_iter()
                .map(|(k, v)| (k, avro_value_to_json(v)))
                .collect(),
        ),
        AvroValue::Record(fields) => serde_json::Value::Object(
            fields
                .into_iter()
                .map(|(k, v)| (k, avro_value_to_json(v)))
                .collect(),
        ),
        // Logical/temporal types: render their textual debug form. These do not
        // appear in our vectors but keep the mapping total.
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

/// The Avro type name to record in `column_types` for a record field. Unions
/// report as `"union"`; named composites report their kind.
fn field_type_name(field: &RecordField) -> String {
    schema_type_name(&field.schema).to_string()
}

fn schema_type_name(schema: &Schema) -> &'static str {
    match schema {
        Schema::Null => "null",
        Schema::Boolean => "boolean",
        Schema::Int => "int",
        Schema::Long => "long",
        Schema::Float => "float",
        Schema::Double => "double",
        Schema::Bytes => "bytes",
        Schema::String => "string",
        Schema::Array(_) => "array",
        Schema::Map(_) => "map",
        Schema::Union(_) => "union",
        Schema::Record(_) => "record",
        Schema::Enum(_) => "enum",
        Schema::Fixed(_) => "fixed",
        Schema::Decimal(_) => "decimal",
        Schema::BigDecimal => "big-decimal",
        Schema::Uuid => "uuid",
        Schema::Date => "date",
        Schema::TimeMillis => "time-millis",
        Schema::TimeMicros => "time-micros",
        Schema::TimestampMillis => "timestamp-millis",
        Schema::TimestampMicros => "timestamp-micros",
        Schema::TimestampNanos => "timestamp-nanos",
        Schema::LocalTimestampMillis => "local-timestamp-millis",
        Schema::LocalTimestampMicros => "local-timestamp-micros",
        Schema::LocalTimestampNanos => "local-timestamp-nanos",
        Schema::Duration => "duration",
        Schema::Ref { .. } => "ref",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apache_avro::types::Record;
    use apache_avro::{Schema, Writer};

    /// Round-trip a small record file through the writer and `read_avro_tabular`,
    /// then assert the declared `column_types` and typed cell `Value` variants.
    #[test]
    fn reads_typed_columns_and_cells() {
        let schema_json = r#"{
            "type": "record",
            "name": "Row",
            "fields": [
                {"name": "id", "type": "long"},
                {"name": "label", "type": "string"},
                {"name": "score", "type": "double"},
                {"name": "active", "type": "boolean"},
                {"name": "note", "type": ["null", "string"], "default": null}
            ]
        }"#;
        let schema = Schema::parse_str(schema_json).unwrap();
        let mut writer = Writer::new(&schema, Vec::new());
        let mut record = Record::new(&schema).unwrap();
        record.put("id", 7i64);
        record.put("label", "alpha");
        record.put("score", 88.5f64);
        record.put("active", true);
        record.put("note", Some("hi".to_string()));
        writer.append(record).unwrap();
        let bytes = writer.into_inner().unwrap();

        let tabular = read_avro_tabular(&bytes).unwrap();

        assert_eq!(tabular.headers, ["id", "label", "score", "active", "note"]);
        assert!(tabular.has_header);
        assert_eq!(
            tabular.column_types,
            [
                Some("long".to_string()),
                Some("string".to_string()),
                Some("double".to_string()),
                Some("boolean".to_string()),
                Some("union".to_string()),
            ]
        );
        assert_eq!(tabular.rows.len(), 1);
        let row = &tabular.rows[0];
        assert_eq!(row[0], Value::Number(7i64.into()));
        assert_eq!(row[1], Value::String("alpha".into()));
        assert_eq!(
            row[2],
            Value::Number(serde_json::Number::from_f64(88.5).unwrap())
        );
        assert_eq!(row[3], Value::Bool(true));
        // Nullable union unwraps to the held string value.
        assert_eq!(row[4], Value::String("hi".into()));
    }
}
