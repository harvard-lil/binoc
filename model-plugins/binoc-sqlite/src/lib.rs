mod sqlite;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use sqlite::{SqliteCollectionWriter, SqliteParseRule};

pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(SqliteParseRule)));
    config.writers.insert(0, Arc::new(SqliteCollectionWriter));
}

#[cfg(feature = "python")]
#[pyo3::pymodule]
fn binoc_sqlite(_m: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    Ok(())
}

#[cfg(feature = "test-support")]
pub mod test_support;
