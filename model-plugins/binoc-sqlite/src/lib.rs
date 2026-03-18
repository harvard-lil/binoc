mod sqlite;

pub use sqlite::SqliteComparator;

binoc_sdk::export_plugin! {
    module: binoc_sqlite,
    comparators: [SqliteComparator],
}
