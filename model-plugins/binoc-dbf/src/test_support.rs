//! Test-vector helpers for `binoc-dbf`. Enabled by the `test-support` feature.
//! Provides [`DbfMaterializer`], a [`VectorMaterializer`] that builds real
//! `.dbf` tables from `.dbf.d` staging directories.
//!
//! Staging layout (one `.dbf.d` directory per source file):
//!
//! ```text
//! data.dbf.d/
//!   table.json   # field definitions + rows, as JSON
//! ```
//!
//! `table.json` has the shape:
//!
//! ```json
//! {
//!   "fields": [
//!     {"name": "id",    "type": "Numeric"},
//!     {"name": "label", "type": "Character"},
//!     {"name": "active", "type": "Logical"}
//!   ],
//!   "rows": [
//!     [1, "alpha", true],
//!     [2, "beta",  false]
//!   ]
//! }
//! ```
//!
//! Supported field types: `Character`, `Numeric`, `Logical`. Each row is an
//! array of cell values aligned to `fields` by position. Cell encoding:
//! - Character: JSON string (`null` → empty/absent).
//! - Numeric: JSON number (`null` → absent).
//! - Logical: JSON boolean (`null` → absent).
//!
//! The fields are added to a `dbase::TableWriterBuilder` (Character width is
//! taken to be 50; Numeric is declared with width 20 and 4 decimals — wide
//! enough for the small values these vectors use), and each row is written as a
//! `dbase::Record`. The encoded table is written to the sibling artifact path
//! (`data.dbf`).

use std::convert::TryFrom;
use std::path::Path;

use binoc_stdlib::test_vectors::VectorMaterializer;
use dbase::{FieldName, FieldValue, Record, TableWriterBuilder};
use serde::Deserialize;

/// Builds `.dbf` tables from `.dbf.d` staging directories.
pub struct DbfMaterializer;

#[derive(Deserialize)]
struct TableSpec {
    fields: Vec<FieldSpec>,
    rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct FieldSpec {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
}

impl VectorMaterializer for DbfMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".dbf.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let table_path = staging_dir.join("table.json");
        let table_json = std::fs::read_to_string(&table_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", table_path.display()));
        let spec: TableSpec = serde_json::from_str(&table_json)
            .unwrap_or_else(|e| panic!("parse {}: {e}", table_path.display()));

        // Declare each column on the builder according to its dBASE type.
        let mut builder = TableWriterBuilder::new();
        for field in &spec.fields {
            let name = FieldName::try_from(field.name.as_str())
                .unwrap_or_else(|e| panic!("invalid field name `{}`: {e}", field.name));
            builder = match field.field_type.as_str() {
                "Character" => builder.add_character_field(name, 50),
                "Numeric" => builder.add_numeric_field(name, 20, 4),
                "Logical" => builder.add_logical_field(name),
                other => panic!(
                    "unsupported field type `{other}` for `{}` in {}",
                    field.name,
                    table_path.display()
                ),
            };
        }

        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = builder.build_with_dest(&mut buf);

        for (row_idx, row) in spec.rows.iter().enumerate() {
            assert_eq!(
                row.len(),
                spec.fields.len(),
                "{} row {} has {} cells, expected {}",
                table_path.display(),
                row_idx,
                row.len(),
                spec.fields.len()
            );
            let mut record = Record::default();
            for (field, cell) in spec.fields.iter().zip(row) {
                let value = cell_to_field_value(&field.field_type, cell).unwrap_or_else(|e| {
                    panic!(
                        "{} row {} field `{}`: {e}",
                        table_path.display(),
                        row_idx,
                        field.name
                    )
                });
                record.insert(field.name.clone(), value);
            }
            writer
                .write_record(&record)
                .unwrap_or_else(|e| panic!("write {} row {}: {e}", out_path.display(), row_idx));
        }
        writer
            .finalize()
            .unwrap_or_else(|e| panic!("finalize {}: {e}", out_path.display()));
        drop(writer);

        std::fs::write(out_path, buf.into_inner())
            .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
    }
}

/// Convert a JSON cell to a `dbase::FieldValue` for the declared column type.
fn cell_to_field_value(field_type: &str, cell: &serde_json::Value) -> Result<FieldValue, String> {
    match field_type {
        "Character" => match cell {
            serde_json::Value::Null => Ok(FieldValue::Character(None)),
            serde_json::Value::String(s) => Ok(FieldValue::Character(Some(s.clone()))),
            other => Err(format!("expected string, found {other}")),
        },
        "Numeric" => match cell {
            serde_json::Value::Null => Ok(FieldValue::Numeric(None)),
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(|f| FieldValue::Numeric(Some(f)))
                .ok_or_else(|| format!("number {n} is not representable as f64")),
            other => Err(format!("expected number, found {other}")),
        },
        "Logical" => match cell {
            serde_json::Value::Null => Ok(FieldValue::Logical(None)),
            serde_json::Value::Bool(b) => Ok(FieldValue::Logical(Some(*b))),
            other => Err(format!("expected boolean, found {other}")),
        },
        other => Err(format!("unsupported field type `{other}`")),
    }
}
