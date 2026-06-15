//! dBASE / `.dbf` table parse rule.
//!
//! Reads a `.dbf` file, takes its field descriptors as the column headers (and
//! their declared dBASE field types), and emits a typed [`TabularData`]: one
//! column per field, one row per record in the file.

use binoc_sdk::*;
use dbase::{FieldType, FieldValue, Reader};

#[derive(Default)]
pub struct DbfParseRule;

impl ParseRule for DbfParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-dbf.parse.dbf".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".dbf".into()],
                media_types: Vec::new(),
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let tabular = read_dbf_tabular(&bytes)?;
        serde_json::to_vec(&tabular)
            .map(ParseOutput::from)
            .map_err(|e| BinocError::Other(format!("serialize dbf tabular artifact: {e}")))
    }
}

/// Read a `.dbf` byte buffer into a typed [`TabularData`].
///
/// The field descriptors in the header define the table columns (names and
/// declared dBASE types); each record becomes a row. Records are read in field
/// declaration order via [`Reader::fields`], so cells stay aligned to the
/// headers regardless of the order the decoder yields them.
///
/// Public so a fusing parser (e.g. the shapefile layer in `binoc-shapefile`) can
/// surface a shapefile's sibling `.dbf` attribute table as a `tabular_v1` child
/// using the exact same producer a standalone `.dbf` uses (CFM-83).
pub fn read_dbf_tabular(bytes: &[u8]) -> BinocResult<TabularData> {
    let mut reader = Reader::new(std::io::Cursor::new(bytes))
        .map_err(|e| BinocError::Other(format!("dbf: {e}")))?;

    let fields = reader.fields().to_vec();
    // The deletion-flag pseudo-field is exposed by `fields()` but is not a real
    // column; skip it so headers/types/cells describe only user fields.
    let user_fields: Vec<_> = fields
        .iter()
        .filter(|f| f.name() != "DeletionFlag")
        .collect();

    let headers: Vec<String> = user_fields.iter().map(|f| f.name().to_string()).collect();
    let column_types: Vec<Option<String>> = user_fields
        .iter()
        .map(|f| Some(field_type_name(f.field_type())))
        .collect();

    let records = reader
        .read()
        .map_err(|e| BinocError::Other(format!("dbf: {e}")))?;

    let mut rows: Vec<Vec<Value>> = Vec::with_capacity(records.len());
    for record in records {
        let mut row = Vec::with_capacity(headers.len());
        for header in &headers {
            let cell = record
                .get(header)
                .map(field_value_to_cell)
                .unwrap_or(Value::Null);
            row.push(cell);
        }
        rows.push(row);
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

/// Map a dBASE [`FieldValue`] to a Binoc cell [`Value`].
///
/// - Character / Memo → [`Value::String`]. dBASE pads Character fields with
///   trailing spaces to the declared field width; we trim trailing whitespace
///   so the logical value round-trips. An all-pad Character field reads back as
///   `None` and maps to [`Value::Null`].
/// - Numeric / Float / Double / Integer / Currency → [`Value::Number`]
///   (an empty/absent Numeric or Float reads as `None` → [`Value::Null`]).
/// - Logical → [`Value::Bool`] (`None` → [`Value::Null`]).
/// - Date / DateTime → [`Value::String`] in ISO-8601 form.
fn field_value_to_cell(value: &FieldValue) -> Value {
    match value {
        FieldValue::Character(s) => match s {
            Some(text) => Value::String(text.trim_end().to_string()),
            None => Value::Null,
        },
        FieldValue::Memo(text) => Value::String(text.trim_end().to_string()),
        FieldValue::Numeric(n) => n.map_or(Value::Null, f64_to_cell),
        FieldValue::Float(f) => f.map_or(Value::Null, |v| f64_to_cell(v as f64)),
        FieldValue::Double(f) => f64_to_cell(*f),
        FieldValue::Currency(c) => f64_to_cell(*c),
        FieldValue::Integer(i) => Value::Number((*i).into()),
        FieldValue::Logical(b) => b.map_or(Value::Null, Value::Bool),
        FieldValue::Date(d) => match d {
            Some(date) => Value::String(format_date(date)),
            None => Value::Null,
        },
        FieldValue::DateTime(dt) => {
            let date = dt.date();
            let time = dt.time();
            Value::String(format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                date.year(),
                date.month(),
                date.day(),
                time.hours(),
                time.minutes(),
                time.seconds(),
            ))
        }
    }
}

/// Format a dBASE [`dbase::Date`] as an ISO-8601 calendar date (`YYYY-MM-DD`).
fn format_date(date: &dbase::Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

fn f64_to_cell(f: f64) -> Value {
    serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
}

/// The dBASE field-type name recorded in `column_types` for a column, matching
/// the variant names in [`dbase::FieldType`].
fn field_type_name(field_type: FieldType) -> String {
    match field_type {
        FieldType::Character => "Character",
        FieldType::Numeric => "Numeric",
        FieldType::Logical => "Logical",
        FieldType::Float => "Float",
        FieldType::Integer => "Integer",
        FieldType::Currency => "Currency",
        FieldType::DateTime => "DateTime",
        FieldType::Double => "Double",
        FieldType::Date => "Date",
        FieldType::Memo => "Memo",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbase::{FieldName, Record, TableWriterBuilder};
    use std::convert::TryFrom;

    /// Build a small `.dbf` in memory with a typed mix of fields, then read it
    /// back and assert the declared `column_types` and typed cell `Value`s.
    #[test]
    fn reads_typed_columns_and_cells() {
        let builder = TableWriterBuilder::new()
            .add_numeric_field(FieldName::try_from("id").unwrap(), 10, 0)
            .add_character_field(FieldName::try_from("label").unwrap(), 20)
            .add_numeric_field(FieldName::try_from("score").unwrap(), 10, 2)
            .add_logical_field(FieldName::try_from("active").unwrap());

        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = builder.build_with_dest(&mut buf);

        let mut record = Record::default();
        record.insert("id".to_string(), FieldValue::Numeric(Some(7.0)));
        record.insert(
            "label".to_string(),
            FieldValue::Character(Some("alpha".into())),
        );
        record.insert("score".to_string(), FieldValue::Numeric(Some(88.5)));
        record.insert("active".to_string(), FieldValue::Logical(Some(true)));
        writer.write_record(&record).unwrap();
        writer.finalize().unwrap();
        drop(writer);

        let bytes = buf.into_inner();
        let tabular = read_dbf_tabular(&bytes).unwrap();

        assert_eq!(tabular.headers, ["id", "label", "score", "active"]);
        assert!(tabular.has_header);
        assert_eq!(
            tabular.column_types,
            [
                Some("Numeric".to_string()),
                Some("Character".to_string()),
                Some("Numeric".to_string()),
                Some("Logical".to_string()),
            ]
        );
        assert_eq!(tabular.rows.len(), 1);
        let row = &tabular.rows[0];
        assert_eq!(
            row[0],
            Value::Number(serde_json::Number::from_f64(7.0).unwrap())
        );
        // Trailing pad spaces from the fixed-width Character field are trimmed.
        assert_eq!(row[1], Value::String("alpha".into()));
        assert_eq!(
            row[2],
            Value::Number(serde_json::Number::from_f64(88.5).unwrap())
        );
        assert_eq!(row[3], Value::Bool(true));
    }
}
