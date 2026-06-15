//! dBASE correspondence rule pack for Binoc.
//!
//! Parses dBASE `.dbf` tables into the standard `tabular_v1` artifact so the
//! generic tabular writers, compaction, and extractors handle them without
//! knowing the source format.

mod dbf;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use dbf::DbfParseRule;

/// Register this pack's parse rules and writers into an engine config.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(DbfParseRule)));
}

#[cfg(feature = "test-support")]
pub mod test_support;
