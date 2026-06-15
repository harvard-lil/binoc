//! `ArrowIpcParse`: reads an Arrow IPC *file* (a.k.a. Feather v2) into a typed
//! [`TabularData`] published under the `tabular_v1()` artifact format. Shares the
//! Arrow `RecordBatch` -> `TabularData` transcoding with the parquet rule via
//! [`crate::parquet_rule::record_batches_to_tabular`].
//!
//! Note: this reads the Arrow IPC *file* format (random-access, with a footer),
//! which is what `.feather` (v2) and `arrow::ipc::writer::FileWriter` produce —
//! not the streaming IPC format.

use std::fs::File;
use std::path::Path;

use arrow::ipc::reader::FileReader;
use arrow::record_batch::RecordBatch;
use binoc_sdk::*;

use crate::parquet_rule::record_batches_to_tabular;

#[derive(Default)]
pub struct ArrowIpcParseRule;

impl ParseRule for ArrowIpcParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-parquet.parse.arrow-ipc".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".arrow".into(), ".feather".into(), ".ipc".into()],
                media_types: Vec::new(),
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let phys = data.local_path(item)?;
        let table = read_arrow_ipc(&phys)?;
        serde_json::to_vec(&table)
            .map(ParseOutput::from)
            .map_err(|e| BinocError::Other(format!("serialize arrow-ipc tabular artifact: {e}")))
    }
}

/// Read an Arrow IPC file (Feather v2) at `path` into a typed [`TabularData`].
fn read_arrow_ipc(path: &Path) -> BinocResult<TabularData> {
    let file = File::open(path).map_err(|e| BinocError::Other(format!("open arrow-ipc: {e}")))?;
    let reader = FileReader::try_new(file, None)
        .map_err(|e| BinocError::Other(format!("arrow-ipc reader: {e}")))?;
    let schema = reader.schema();

    let mut batches: Vec<RecordBatch> = Vec::new();
    for batch in reader {
        batches.push(batch.map_err(|e| BinocError::Other(format!("read arrow-ipc batch: {e}")))?);
    }

    record_batches_to_tabular(&schema, &batches)
}
