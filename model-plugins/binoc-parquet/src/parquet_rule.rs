//! `ParquetParse`: reads a `.parquet` file into a typed [`TabularData`] published
//! under the `tabular_v1()` artifact format. The generic stdlib tabular writer
//! consumes that artifact to emit cell/row/column edits, so this rule only has
//! to faithfully transcode Arrow values into `Value` and record each column's
//! declared (Arrow logical) type in `column_types`.

use std::fs::File;
use std::path::Path;

use std::sync::Arc;

use arrow::array::{Array, ArrayRef, AsArray};
use arrow::datatypes::{DataType, Field, Float16Type, Float32Type, Float64Type, Schema};
use arrow::datatypes::{
    Int16Type, Int32Type, Int64Type, Int8Type, UInt16Type, UInt32Type, UInt64Type, UInt8Type,
};
use arrow::json::ArrayWriter;
use arrow::record_batch::RecordBatch;
use binoc_sdk::*;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

#[derive(Default)]
pub struct ParquetParseRule;

impl ParseRule for ParquetParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-parquet.parse.parquet".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".parquet".into()],
                media_types: Vec::new(),
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let phys = data.local_path(item)?;
        let table = read_parquet(&phys)?;
        serde_json::to_vec(&table)
            .map(ParseOutput::from)
            .map_err(|e| BinocError::Other(format!("serialize parquet tabular artifact: {e}")))
    }
}

/// Read a parquet file at `path` into a typed [`TabularData`].
fn read_parquet(path: &Path) -> BinocResult<TabularData> {
    let file = File::open(path).map_err(|e| BinocError::Other(format!("open parquet: {e}")))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| BinocError::Other(format!("parquet reader: {e}")))?;
    let schema = builder.schema().clone();

    let reader = builder
        .build()
        .map_err(|e| BinocError::Other(format!("parquet reader build: {e}")))?;

    let mut batches: Vec<RecordBatch> = Vec::new();
    for batch in reader {
        batches.push(batch.map_err(|e| BinocError::Other(format!("read parquet batch: {e}")))?);
    }

    record_batches_to_tabular(&schema, &batches)
}

/// Build a typed [`TabularData`] from an Arrow [`Schema`] and its
/// [`RecordBatch`]es. Shared by the parquet and Arrow IPC parse rules: headers
/// and `column_types` come from the schema, and every cell is transcoded to a
/// [`Value`] via [`cell_value`].
pub(crate) fn record_batches_to_tabular(
    schema: &Schema,
    batches: &[RecordBatch],
) -> BinocResult<TabularData> {
    let headers: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let column_types: Vec<Option<String>> = schema
        .fields()
        .iter()
        .map(|f| Some(arrow_type_name(f.data_type())))
        .collect();

    let mut rows: Vec<Vec<Value>> = Vec::new();
    for batch in batches {
        append_batch_rows(batch, &mut rows)?;
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

/// Append every row of a record batch to `rows`, mapping each Arrow cell to a
/// [`Value`].
fn append_batch_rows(batch: &RecordBatch, rows: &mut Vec<Vec<Value>>) -> BinocResult<()> {
    let num_rows = batch.num_rows();
    let columns = batch.columns();
    rows.reserve(num_rows);
    for row_idx in 0..num_rows {
        let mut row = Vec::with_capacity(columns.len());
        for column in columns {
            row.push(cell_value(column.as_ref(), row_idx)?);
        }
        rows.push(row);
    }
    Ok(())
}

/// Map a single Arrow array element at `idx` to a [`Value`].
///
/// Scalars (integers, floats, bool, utf8) become the corresponding typed
/// `Value`; nulls become `Value::Null`; everything else (lists, structs, maps,
/// binary, dates, ...) is rendered to JSON via Arrow's own JSON encoder and fed
/// through [`Value::from_json`], landing as `Value::Nested` for containers.
fn cell_value(array: &dyn Array, idx: usize) -> BinocResult<Value> {
    if array.is_null(idx) {
        return Ok(Value::Null);
    }
    let value = match array.data_type() {
        DataType::Boolean => Value::Bool(array.as_boolean().value(idx)),
        DataType::Int8 => num_i64(array.as_primitive::<Int8Type>().value(idx) as i64),
        DataType::Int16 => num_i64(array.as_primitive::<Int16Type>().value(idx) as i64),
        DataType::Int32 => num_i64(array.as_primitive::<Int32Type>().value(idx) as i64),
        DataType::Int64 => num_i64(array.as_primitive::<Int64Type>().value(idx)),
        DataType::UInt8 => num_u64(array.as_primitive::<UInt8Type>().value(idx) as u64),
        DataType::UInt16 => num_u64(array.as_primitive::<UInt16Type>().value(idx) as u64),
        DataType::UInt32 => num_u64(array.as_primitive::<UInt32Type>().value(idx) as u64),
        DataType::UInt64 => num_u64(array.as_primitive::<UInt64Type>().value(idx)),
        DataType::Float16 => num_f64(array.as_primitive::<Float16Type>().value(idx).to_f64()),
        DataType::Float32 => num_f64(array.as_primitive::<Float32Type>().value(idx) as f64),
        DataType::Float64 => num_f64(array.as_primitive::<Float64Type>().value(idx)),
        DataType::Utf8 => Value::String(array.as_string::<i32>().value(idx).to_string()),
        DataType::LargeUtf8 => Value::String(array.as_string::<i64>().value(idx).to_string()),
        // Everything else (lists, structs, maps, binary, temporal, decimal, ...)
        // round-trips through Arrow's JSON encoder so containers land as
        // `Value::Nested` and exotic scalars as their natural JSON.
        _ => Value::from_json(arrow_cell_to_json(array, idx)?),
    };
    Ok(value)
}

/// Serialize a single Arrow cell to JSON via Arrow's own JSON writer.
///
/// The cell is wrapped in a one-row, one-column [`RecordBatch`] named `"v"`;
/// the writer yields a JSON array of row objects (`[{"v": ...}]`) and we pluck
/// the `"v"` field. Absent (null) fields are encoded as JSON null.
fn arrow_cell_to_json(array: &dyn Array, idx: usize) -> BinocResult<serde_json::Value> {
    let slice: ArrayRef = array.slice(idx, 1);
    let field = Field::new("v", array.data_type().clone(), true);
    let schema = Arc::new(Schema::new(vec![field]));
    let batch = RecordBatch::try_new(schema, vec![slice])
        .map_err(|e| BinocError::Other(format!("arrow value batch: {e}")))?;

    let mut buf = Vec::new();
    let mut writer = ArrayWriter::new(&mut buf);
    writer
        .write(&batch)
        .map_err(|e| BinocError::Other(format!("arrow value to json: {e}")))?;
    writer
        .finish()
        .map_err(|e| BinocError::Other(format!("arrow value to json: {e}")))?;

    let rows: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_slice(&buf)
        .map_err(|e| BinocError::Other(format!("decode arrow json: {e}")))?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|mut row| row.remove("v"))
        .unwrap_or(serde_json::Value::Null))
}

fn num_i64(v: i64) -> Value {
    Value::Number(serde_json::Number::from(v))
}

fn num_u64(v: u64) -> Value {
    Value::Number(serde_json::Number::from(v))
}

fn num_f64(v: f64) -> Value {
    serde_json::Number::from_f64(v)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Short logical type name for an Arrow data type, used for `column_types`.
/// Mirrors the lowercase Arrow type vocabulary ("int64", "double", "utf8", ...).
fn arrow_type_name(dt: &DataType) -> String {
    match dt {
        DataType::Boolean => "boolean".into(),
        DataType::Int8 => "int8".into(),
        DataType::Int16 => "int16".into(),
        DataType::Int32 => "int32".into(),
        DataType::Int64 => "int64".into(),
        DataType::UInt8 => "uint8".into(),
        DataType::UInt16 => "uint16".into(),
        DataType::UInt32 => "uint32".into(),
        DataType::UInt64 => "uint64".into(),
        DataType::Float16 => "float16".into(),
        DataType::Float32 => "float".into(),
        DataType::Float64 => "double".into(),
        DataType::Utf8 => "utf8".into(),
        DataType::LargeUtf8 => "large_utf8".into(),
        DataType::Binary => "binary".into(),
        DataType::LargeBinary => "large_binary".into(),
        DataType::Date32 | DataType::Date64 => "date".into(),
        DataType::Timestamp(_, _) => "timestamp".into(),
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => "list".into(),
        DataType::Struct(_) => "struct".into(),
        DataType::Map(_, _) => "map".into(),
        // Fall back to Arrow's own Debug rendering, lowercased, for the long
        // tail (decimal, interval, duration, union, ...).
        other => format!("{other:?}").to_lowercase(),
    }
}
