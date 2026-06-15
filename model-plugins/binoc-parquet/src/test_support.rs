//! Test-vector helpers for `binoc-parquet`. Enabled by the `test-support`
//! feature. Provides [`ParquetMaterializer`], a [`VectorMaterializer`] that
//! builds a real `.parquet` file from a `.parquet.d` staging directory and a
//! real Arrow IPC file (Feather v2) from a `.arrow.d` staging directory. The
//! output format is chosen by the `out_path` extension.
//!
//! ## Staging format
//!
//! The staging directory contains a single `table.json` of the form:
//!
//! ```json
//! {
//!   "columns": [
//!     { "name": "id",    "type": "int64"   },
//!     { "name": "price", "type": "double"  },
//!     { "name": "name",  "type": "utf8"    },
//!     { "name": "active","type": "boolean" }
//!   ],
//!   "rows": [
//!     [1, 9.99, "widget", true],
//!     [2, 14.5, "gadget", false]
//!   ]
//! }
//! ```
//!
//! Supported column types: `int64`, `double`, `utf8`, `boolean`. A `null` JSON
//! value in any cell produces a null in that column.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::FileWriter;
use arrow::record_batch::RecordBatch;
use binoc_stdlib::test_vectors::VectorMaterializer;
use parquet::arrow::ArrowWriter;
use serde::Deserialize;

/// Builds `.parquet` files from `.parquet.d` staging directories and Arrow IPC
/// (Feather v2) `.arrow` files from `.arrow.d` staging directories, each
/// containing a single `table.json` (see module docs for the format). The
/// output format is selected by the `out_path` extension.
pub struct ParquetMaterializer;

#[derive(Deserialize)]
struct StagingTable {
    columns: Vec<StagingColumn>,
    rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct StagingColumn {
    name: String,
    #[serde(rename = "type")]
    col_type: String,
}

impl VectorMaterializer for ParquetMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".parquet.d", ".arrow.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let json_path = staging_dir.join("table.json");
        let text = std::fs::read_to_string(&json_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", json_path.display()));
        let table: StagingTable = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("parse {}: {e}", json_path.display()));

        let fields: Vec<Field> = table
            .columns
            .iter()
            .map(|c| Field::new(&c.name, arrow_data_type(&c.col_type), true))
            .collect();
        let schema = Arc::new(Schema::new(fields));

        let arrays: Vec<ArrayRef> = table
            .columns
            .iter()
            .enumerate()
            .map(|(col_idx, col)| build_array(&col.col_type, &table.rows, col_idx))
            .collect();

        let batch = RecordBatch::try_new(schema.clone(), arrays)
            .unwrap_or_else(|e| panic!("build record batch for {}: {e}", out_path.display()));

        if out_path.exists() {
            std::fs::remove_file(out_path)
                .unwrap_or_else(|e| panic!("remove_file {}: {e}", out_path.display()));
        }
        let file =
            File::create(out_path).unwrap_or_else(|e| panic!("create {}: {e}", out_path.display()));

        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        match ext {
            "parquet" => {
                let mut writer = ArrowWriter::try_new(file, schema, None)
                    .unwrap_or_else(|e| panic!("parquet writer {}: {e}", out_path.display()));
                writer
                    .write(&batch)
                    .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
                writer
                    .close()
                    .unwrap_or_else(|e| panic!("close {}: {e}", out_path.display()));
            }
            "arrow" | "feather" | "ipc" => {
                let mut writer = FileWriter::try_new(file, &schema)
                    .unwrap_or_else(|e| panic!("arrow-ipc writer {}: {e}", out_path.display()));
                writer
                    .write(&batch)
                    .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
                writer
                    .finish()
                    .unwrap_or_else(|e| panic!("finish {}: {e}", out_path.display()));
            }
            other => panic!("unsupported materializer output extension: .{other}"),
        }
    }
}

fn arrow_data_type(col_type: &str) -> DataType {
    match col_type {
        "int64" => DataType::Int64,
        "double" => DataType::Float64,
        "utf8" => DataType::Utf8,
        "boolean" => DataType::Boolean,
        other => panic!("unsupported staging column type: {other}"),
    }
}

fn build_array(col_type: &str, rows: &[Vec<serde_json::Value>], col_idx: usize) -> ArrayRef {
    let cells = rows.iter().map(|row| &row[col_idx]);
    match col_type {
        "int64" => Arc::new(Int64Array::from_iter(cells.map(|v| match v {
            serde_json::Value::Null => None,
            other => Some(other.as_i64().expect("int64 cell")),
        }))) as ArrayRef,
        "double" => Arc::new(Float64Array::from_iter(cells.map(|v| match v {
            serde_json::Value::Null => None,
            other => Some(other.as_f64().expect("double cell")),
        }))) as ArrayRef,
        "utf8" => Arc::new(StringArray::from_iter(cells.map(|v| match v {
            serde_json::Value::Null => None,
            other => Some(other.as_str().expect("utf8 cell").to_string()),
        }))) as ArrayRef,
        "boolean" => Arc::new(BooleanArray::from_iter(cells.map(|v| match v {
            serde_json::Value::Null => None,
            other => Some(other.as_bool().expect("boolean cell")),
        }))) as ArrayRef,
        other => panic!("unsupported staging column type: {other}"),
    }
}
