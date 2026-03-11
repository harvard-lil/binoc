pub mod comparators;
pub mod outputters;
pub mod transformers;

use std::sync::Arc;

use binoc_core::config::PluginRegistry;
use outputters::markdown::MarkdownOutputter;

/// Register all standard library plugins into a registry.
pub fn register_stdlib(registry: &mut PluginRegistry) {
    registry.register_comparator(
        "binoc.zip",
        Arc::new(comparators::zip_compare::ZipComparator),
    );
    registry.register_comparator(
        "binoc.tar",
        Arc::new(comparators::tar_compare::TarComparator),
    );
    registry.register_comparator(
        "binoc.directory",
        Arc::new(comparators::directory::DirectoryComparator),
    );
    registry.register_comparator(
        "binoc.csv",
        Arc::new(comparators::csv_compare::CsvComparator),
    );
    registry.register_comparator("binoc.text", Arc::new(comparators::text::TextComparator));
    registry.register_comparator(
        "binoc.binary",
        Arc::new(comparators::binary::BinaryComparator),
    );

    registry.register_transformer(
        "binoc.move_detector",
        Arc::new(transformers::move_detector::MoveDetector),
    );
    registry.register_transformer(
        "binoc.copy_detector",
        Arc::new(transformers::copy_detector::CopyDetector),
    );
    registry.register_transformer(
        "binoc.column_reorder_detector",
        Arc::new(transformers::column_reorder::ColumnReorderDetector),
    );

    registry.register_outputter("binoc.markdown", Arc::new(MarkdownOutputter));
}

/// Create a fully configured registry with all stdlib plugins.
pub fn default_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_stdlib(&mut registry);
    registry
}

/// Shared test-vector harness so plugins can run vectors without duplicating logic.
/// Enabled by default; use `default-features = false` to omit insta/toml/rusqlite.
#[cfg(feature = "test-vectors")]
pub mod test_vectors;
