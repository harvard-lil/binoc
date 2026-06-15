//! parquet correspondence rule pack for Binoc.
//!
//! Provides [`ParquetParseRule`], which reads a single-table `.parquet` file,
//! and [`ArrowIpcParseRule`], which reads an Arrow IPC *file* / Feather v2
//! (`.arrow`, `.feather`, `.ipc`), both into a typed `tabular_v1()` artifact
//! (populating `column_types` with each column's Arrow logical type). Row/
//! column/cell diffing is handled by the generic stdlib tabular writer, which
//! consumes the shared `tabular_v1` format.

mod arrow_ipc_rule;
mod parquet_rule;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use arrow_ipc_rule::ArrowIpcParseRule;
pub use parquet_rule::ParquetParseRule;

/// Register this pack's parse rules into an engine config.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(ParquetParseRule)));
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(ArrowIpcParseRule)));
}

#[cfg(feature = "test-support")]
pub mod test_support;
