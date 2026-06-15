//! Parse rules for binary / value-tree serialization formats.
//!
//! Each rule reads the source bytes, transcodes them into a single
//! [`serde_json::Value`] tree, and emits the standard `structured_document_v1`
//! artifact (mirroring the JSON/YAML/TOML parsers in `binoc-stdlib`). The
//! generic structured-document writer then handles diffing, summaries, and tags
//! without knowing the origin format.

use binoc_sdk::*;

/// Serialize a [`StructuredDocument`] to bytes and wrap it in a [`ParseOutput`].
///
/// Shared by every rule in this pack: each supplies its already-transcoded
/// `value` and a `format` tag. We use a minimal `source` (just the byte length),
/// matching the YAML/TOML parsers in the stdlib.
fn structured_document_output(
    value: serde_json::Value,
    format: &str,
    byte_len: usize,
) -> BinocResult<ParseOutput> {
    let source = serde_json::json!({ "byte_len": byte_len });
    serde_json::to_vec(&StructuredDocument {
        value,
        format: format.into(),
        source,
    })
    .map(ParseOutput::from)
    .map_err(|err| BinocError::Other(format!("serialize structured document artifact: {err}")))
}

// ── CBOR ────────────────────────────────────────────────────────────

/// Parses Concise Binary Object Representation (`.cbor`) files.
#[derive(Default)]
pub struct CborParseRule;

impl ParseRule for CborParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-binformats.parse.cbor".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".cbor".into()],
                media_types: Vec::new(),
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let value: serde_json::Value = ciborium::de::from_reader(bytes.as_slice())
            .map_err(|err| BinocError::Other(format!("parse CBOR: {err}")))?;
        structured_document_output(value, "cbor", bytes.len())
    }
}

// ── MessagePack ─────────────────────────────────────────────────────

/// Parses MessagePack (`.msgpack`, `.mp`) files.
#[derive(Default)]
pub struct MsgpackParseRule;

impl ParseRule for MsgpackParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-binformats.parse.msgpack".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".msgpack".into(), ".mp".into()],
                media_types: Vec::new(),
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let value: serde_json::Value = rmp_serde::from_slice(&bytes)
            .map_err(|err| BinocError::Other(format!("parse MessagePack: {err}")))?;
        structured_document_output(value, "msgpack", bytes.len())
    }
}

// ── BSON ────────────────────────────────────────────────────────────

/// Parses a single BSON document (`.bson`).
#[derive(Default)]
pub struct BsonParseRule;

impl ParseRule for BsonParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-binformats.parse.bson".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".bson".into()],
                media_types: Vec::new(),
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let doc: bson::Document = bson::deserialize_from_slice(&bytes)
            .map_err(|err| BinocError::Other(format!("parse BSON: {err}")))?;
        // `bson::Document` serializes to JSON in MongoDB Extended JSON form for
        // its special types (ObjectId -> {"$oid": ...}, DateTime -> {"$date":
        // ...}, Binary -> {"$binary": ...}). Scalars round-trip directly.
        let value = serde_json::to_value(&doc)
            .map_err(|err| BinocError::Other(format!("transcode BSON to JSON: {err}")))?;
        structured_document_output(value, "bson", bytes.len())
    }
}

// ── Plist ───────────────────────────────────────────────────────────

/// Parses Apple property lists (`.plist`), either XML or binary form.
#[derive(Default)]
pub struct PlistParseRule;

impl ParseRule for PlistParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-binformats.parse.plist".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".plist".into()],
                media_types: Vec::new(),
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let plist_value = plist::Value::from_reader(std::io::Cursor::new(&bytes))
            .map_err(|err| BinocError::Other(format!("parse plist: {err}")))?;
        let value = plist_value_to_json(plist_value);
        structured_document_output(value, "plist", bytes.len())
    }
}

/// Convert a [`plist::Value`] into a [`serde_json::Value`].
///
/// Scalars map to their JSON counterparts. Plist-specific types that have no
/// JSON equivalent (`Date`, `Data`, `Uid`) render as strings so they remain
/// diffable; the date is emitted as its XML/RFC 3339 form and data as base64.
fn plist_value_to_json(value: plist::Value) -> serde_json::Value {
    match value {
        plist::Value::String(s) => serde_json::Value::String(s),
        plist::Value::Boolean(b) => serde_json::Value::Bool(b),
        plist::Value::Integer(i) => {
            if let Some(n) = i.as_signed() {
                serde_json::Value::Number(n.into())
            } else if let Some(n) = i.as_unsigned() {
                serde_json::Value::Number(n.into())
            } else {
                serde_json::Value::String(i.to_string())
            }
        }
        plist::Value::Real(f) => serde_json::Number::from_f64(f)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        plist::Value::Date(d) => serde_json::Value::String(d.to_xml_format()),
        plist::Value::Data(bytes) => serde_json::Value::String(plist_data_to_string(&bytes)),
        plist::Value::Uid(uid) => serde_json::Value::String(uid.get().to_string()),
        plist::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(plist_value_to_json).collect())
        }
        plist::Value::Dictionary(dict) => serde_json::Value::Object(
            dict.into_iter()
                .map(|(k, v)| (k, plist_value_to_json(v)))
                .collect(),
        ),
        // `plist::Value` is non-exhaustive; fall back to a debug rendering.
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

/// Render plist `<data>` bytes as a stable hex string so binary blobs stay
/// diffable in the value tree.
fn plist_data_to_string(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ── Ion ─────────────────────────────────────────────────────────────

/// Parses Amazon Ion text (`.ion`) documents.
#[derive(Default)]
pub struct IonParseRule;

impl ParseRule for IonParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-binformats.parse.ion".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".ion".into()],
                media_types: Vec::new(),
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let element = ion_rs::element::Element::read_one(&bytes)
            .map_err(|err| BinocError::Other(format!("parse Ion: {err}")))?;
        let value = ion_value_to_json(element.value());
        structured_document_output(value, "ion", bytes.len())
    }
}

/// Convert an Ion [`Value`](ion_rs::element::Value) into a
/// [`serde_json::Value`].
///
/// Scalars map directly. Ion-specific types without a JSON equivalent
/// (`Decimal`, `Timestamp`, `Symbol`, `Clob`, `Blob`) render as strings so they
/// remain diffable. `SExp` is treated like a list.
fn ion_value_to_json(value: &ion_rs::element::Value) -> serde_json::Value {
    use ion_rs::element::Value as IonValue;
    use ion_rs::Int;

    match value {
        IonValue::Null(_) => serde_json::Value::Null,
        IonValue::Bool(b) => serde_json::Value::Bool(*b),
        IonValue::Int(Int::I64(n)) => serde_json::Value::Number((*n).into()),
        IonValue::Int(Int::BigInt(n)) => serde_json::Value::String(n.to_string()),
        IonValue::Float(f) => serde_json::Number::from_f64(*f)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        IonValue::Decimal(d) => serde_json::Value::String(d.to_string()),
        IonValue::Timestamp(t) => serde_json::Value::String(t.to_string()),
        IonValue::Symbol(s) => serde_json::Value::String(s.text().unwrap_or_default().to_string()),
        IonValue::String(s) => serde_json::Value::String(s.text().to_string()),
        IonValue::Clob(b) | IonValue::Blob(b) => {
            serde_json::Value::String(plist_data_to_string(b.as_ref()))
        }
        IonValue::List(seq) | IonValue::SExp(seq) => serde_json::Value::Array(
            seq.elements()
                .map(|e| ion_value_to_json(e.value()))
                .collect(),
        ),
        IonValue::Struct(s) => serde_json::Value::Object(
            s.iter()
                .map(|(name, elem)| {
                    (
                        name.text().unwrap_or_default().to_string(),
                        ion_value_to_json(elem.value()),
                    )
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbor_round_trips_scalar_tree() {
        let json = serde_json::json!({ "name": "svc", "replicas": 3 });
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&json, &mut bytes).unwrap();
        let decoded: serde_json::Value = ciborium::de::from_reader(bytes.as_slice()).unwrap();
        assert_eq!(decoded, json);
    }

    #[test]
    fn plist_data_renders_as_hex() {
        assert_eq!(plist_data_to_string(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }

    #[test]
    fn ion_struct_transcodes_to_json_object() {
        let element = ion_rs::element::Element::read_one(b"{name: \"svc\", replicas: 3}").unwrap();
        let value = ion_value_to_json(element.value());
        assert_eq!(value, serde_json::json!({ "name": "svc", "replicas": 3 }));
    }
}
