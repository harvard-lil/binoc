//! Test-vector helpers for `binoc-avro`. Enabled by the `test-support` feature.
//! Provides [`AvroMaterializer`], a [`VectorMaterializer`] that builds real
//! `.avro` Object Container Files from `.avro.d` staging directories.
//!
//! Staging layout (one `.avro.d` directory per source file):
//!
//! ```text
//! data.avro.d/
//!   schema.avsc     # the writer schema, as Avro JSON (a record schema)
//!   records.jsonl   # one JSON object per line, each matching the schema
//! ```
//!
//! `schema.avsc` is parsed with `apache_avro::Schema::parse_str` (it must be a
//! record schema). Each non-empty line of `records.jsonl` is deserialized as a
//! `serde_json::Value` object and converted, field by field against the record
//! schema, into an `apache_avro::types::Value::Record` (resolving union branches
//! such as `["null", "string"]` by value). The records are appended through the
//! writer (which validates them against the schema) and the encoded container is
//! written to the sibling artifact path (`data.avro`).

use std::path::Path;

use apache_avro::schema::{RecordSchema, Schema, UnionSchema};
use apache_avro::types::Value as AvroValue;
use apache_avro::Writer;
use binoc_stdlib::test_vectors::VectorMaterializer;

/// Builds `.avro` containers from `.avro.d` staging directories.
pub struct AvroMaterializer;

impl VectorMaterializer for AvroMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".avro.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let schema_path = staging_dir.join("schema.avsc");
        let records_path = staging_dir.join("records.jsonl");

        let schema_json = std::fs::read_to_string(&schema_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", schema_path.display()));
        let schema = Schema::parse_str(&schema_json)
            .unwrap_or_else(|e| panic!("parse {}: {e}", schema_path.display()));
        let Schema::Record(record_schema) = &schema else {
            panic!("{} is not a record schema", schema_path.display());
        };

        let records = std::fs::read_to_string(&records_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", records_path.display()));

        let mut writer = Writer::new(&schema, Vec::new());
        for (lineno, line) in records.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let json: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("parse {} line {}: {e}", records_path.display(), lineno + 1)
            });
            let value = json_record_to_avro(record_schema, json).unwrap_or_else(|e| {
                panic!(
                    "convert {} line {}: {e}",
                    records_path.display(),
                    lineno + 1
                )
            });
            writer.append(value).unwrap_or_else(|e| {
                panic!(
                    "append {} line {} to {}: {e}",
                    records_path.display(),
                    lineno + 1,
                    out_path.display()
                )
            });
        }
        let bytes = writer
            .into_inner()
            .unwrap_or_else(|e| panic!("finalize {}: {e}", out_path.display()));

        std::fs::write(out_path, bytes)
            .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
    }
}

/// Convert a JSON object into an `AvroValue::Record` against `record_schema`,
/// emitting fields in schema order and filling missing fields with `null`.
fn json_record_to_avro(
    record_schema: &RecordSchema,
    json: serde_json::Value,
) -> Result<AvroValue, String> {
    let serde_json::Value::Object(mut map) = json else {
        return Err(format!("expected a JSON object, found {json}"));
    };
    let mut fields = Vec::with_capacity(record_schema.fields.len());
    for field in &record_schema.fields {
        let raw = map.remove(&field.name).unwrap_or(serde_json::Value::Null);
        let value =
            json_to_avro(&field.schema, raw).map_err(|e| format!("field `{}`: {e}", field.name))?;
        fields.push((field.name.clone(), value));
    }
    Ok(AvroValue::Record(fields))
}

/// Convert a JSON value into an `AvroValue` against the given field schema.
/// Handles the scalar and union cases exercised by this plugin's vectors.
fn json_to_avro(schema: &Schema, json: serde_json::Value) -> Result<AvroValue, String> {
    match schema {
        Schema::Null => Ok(AvroValue::Null),
        Schema::Boolean => json
            .as_bool()
            .map(AvroValue::Boolean)
            .ok_or_else(|| format!("expected boolean, found {json}")),
        Schema::Int => json
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .map(AvroValue::Int)
            .ok_or_else(|| format!("expected int, found {json}")),
        Schema::Long => json
            .as_i64()
            .map(AvroValue::Long)
            .ok_or_else(|| format!("expected long, found {json}")),
        Schema::Float => json
            .as_f64()
            .map(|f| AvroValue::Float(f as f32))
            .ok_or_else(|| format!("expected float, found {json}")),
        Schema::Double => json
            .as_f64()
            .map(AvroValue::Double)
            .ok_or_else(|| format!("expected double, found {json}")),
        Schema::String => match json {
            serde_json::Value::String(s) => Ok(AvroValue::String(s)),
            other => Err(format!("expected string, found {other}")),
        },
        Schema::Record(inner) => json_record_to_avro(inner, json),
        Schema::Union(union) => json_to_avro_union(union, json),
        other => Err(format!("unsupported field schema {other:?}")),
    }
}

/// Resolve a JSON value against a union schema, picking the branch by value:
/// `null` selects a `Null` branch; anything else takes the first non-null branch
/// it successfully converts against.
fn json_to_avro_union(union: &UnionSchema, json: serde_json::Value) -> Result<AvroValue, String> {
    if json.is_null() {
        if let Some(pos) = union
            .variants()
            .iter()
            .position(|s| matches!(s, Schema::Null))
        {
            return Ok(AvroValue::Union(pos as u32, Box::new(AvroValue::Null)));
        }
        return Err("null value but union has no null branch".into());
    }
    for (pos, branch) in union.variants().iter().enumerate() {
        if matches!(branch, Schema::Null) {
            continue;
        }
        if let Ok(value) = json_to_avro(branch, json.clone()) {
            return Ok(AvroValue::Union(pos as u32, Box::new(value)));
        }
    }
    Err(format!("no union branch matched {json}"))
}
