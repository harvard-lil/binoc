pub mod comparators;
pub mod renderers;
pub mod transformers;

use std::sync::Arc;

use binoc_core::config::PluginRegistry;
use renderers::markdown::MarkdownRenderer;

/// Register all standard library plugins into a registry.
pub fn register_stdlib(registry: &mut PluginRegistry) {
    let r = |res: Result<(), _>| res.expect("same-build plugin must be SDK-compatible");
    r(registry.register_comparator(Arc::new(comparators::zip_compare::ZipComparator)));
    r(registry.register_comparator(Arc::new(comparators::tar_compare::TarComparator)));
    r(registry.register_comparator(Arc::new(comparators::gzip_compare::GzipComparator)));
    r(registry.register_comparator(Arc::new(comparators::directory::DirectoryComparator)));
    r(registry.register_comparator(Arc::new(comparators::csv_compare::CsvComparator)));
    r(registry.register_comparator(Arc::new(comparators::text::TextComparator)));
    r(registry.register_comparator(Arc::new(comparators::binary::BinaryComparator)));

    r(registry.register_transformer(Arc::new(
        transformers::declared_correspondence::DeclaredCorrespondence,
    )));
    r(registry.register_transformer(Arc::new(
        transformers::correlation_detector::CorrelationDetector,
    )));
    r(registry.register_transformer(Arc::new(
        transformers::fuzzy_correlation_detector::FuzzyCorrelationDetector,
    )));
    r(registry.register_transformer(Arc::new(
        transformers::folder_move_detector::FolderMoveDetector,
    )));
    r(registry.register_transformer(Arc::new(transformers::table_splitter::TableSplitter)));
    r(registry.register_transformer(Arc::new(transformers::tabular_analyzer::TabularAnalyzer)));
    r(registry.register_transformer(Arc::new(
        transformers::tabular_stats_annotator::TabularStatsAnnotator,
    )));
    r(registry.register_transformer(Arc::new(
        transformers::table_collection_analyzer::TableCollectionAnalyzer,
    )));

    r(registry.register_renderer(Arc::new(MarkdownRenderer)));
}

/// Create a fully configured registry with all stdlib plugins.
pub fn default_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_stdlib(&mut registry);
    registry
}

#[cfg(feature = "test-vectors")]
pub mod test_vectors;
