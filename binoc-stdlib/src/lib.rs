pub mod comparators;
pub mod outputters;
pub mod transformers;

use std::sync::Arc;

use binoc_core::config::PluginRegistry;
use outputters::markdown::MarkdownOutputter;

/// Register all standard library plugins into a registry.
pub fn register_stdlib(registry: &mut PluginRegistry) {
    let r = |res: Result<(), _>| res.expect("same-build plugin must be SDK-compatible");
    r(registry.register_comparator(Arc::new(comparators::zip_compare::ZipComparator)));
    r(registry.register_comparator(Arc::new(comparators::tar_compare::TarComparator)));
    r(registry.register_comparator(Arc::new(comparators::directory::DirectoryComparator)));
    r(registry.register_comparator(Arc::new(comparators::csv_compare::CsvComparator)));
    r(registry.register_comparator(Arc::new(comparators::text::TextComparator)));
    r(registry.register_comparator(Arc::new(comparators::binary::BinaryComparator)));

    r(registry.register_transformer(Arc::new(transformers::move_detector::MoveDetector)));
    r(registry.register_transformer(Arc::new(transformers::copy_detector::CopyDetector)));
    r(registry.register_transformer(Arc::new(
        transformers::column_reorder::ColumnReorderDetector,
    )));

    r(registry.register_outputter(Arc::new(MarkdownOutputter)));
}

/// Create a fully configured registry with all stdlib plugins.
pub fn default_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_stdlib(&mut registry);
    registry
}

#[cfg(feature = "test-vectors")]
pub mod test_vectors;
