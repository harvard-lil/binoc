mod stat_binary;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use stat_binary::{Sas7bdatParseRule, StataParseRule, XptParseRule};

pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(XptParseRule)));
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(Sas7bdatParseRule)));
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(StataParseRule)));
}

#[cfg(feature = "python")]
#[pyo3::pymodule]
fn binoc_stat_binary(_m: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    Ok(())
}

#[cfg(feature = "test-support")]
pub mod test_support;
