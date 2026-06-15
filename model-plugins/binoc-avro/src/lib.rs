//! Avro correspondence rule pack for Binoc.
//!
//! Parses Avro Object Container Files (`.avro`) into the standard `tabular_v1`
//! artifact so the generic tabular writers, compaction, and extractors handle
//! them without knowing the source format.

mod avro;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use avro::AvroParseRule;

/// Register this pack's parse rules and writers into an engine config.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(AvroParseRule)));
}

#[cfg(feature = "test-support")]
pub mod test_support;
