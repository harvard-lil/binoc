mod sqlite;

pub use sqlite::SqliteComparator;

#[cfg(feature = "test-support")]
pub mod test_support;

binoc_sdk::export_plugin! {
    module: binoc_sqlite,
    comparators: [SqliteComparator],
}
