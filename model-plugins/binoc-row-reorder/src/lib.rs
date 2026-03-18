mod row_reorder;

pub use row_reorder::RowReorderDetector;

binoc_sdk::export_plugin! {
    module: binoc_row_reorder,
    transformers: [RowReorderDetector],
}
