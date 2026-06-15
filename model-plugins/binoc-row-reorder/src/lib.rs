mod row_reorder;

use std::sync::Arc;

use binoc_sdk::CorrespondenceEngineConfig;

pub use row_reorder::RowReorderWriter;

pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config.writers.insert(0, Arc::new(RowReorderWriter));
}
