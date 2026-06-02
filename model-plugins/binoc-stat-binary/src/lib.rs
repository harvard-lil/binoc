mod stat_binary;

pub use stat_binary::{Sas7bdatComparator, StataComparator, XptComparator};

#[cfg(feature = "test-support")]
pub mod test_support;

binoc_sdk::export_plugin! {
    module: binoc_stat_binary,
    comparators: [StataComparator, Sas7bdatComparator, XptComparator],
}
