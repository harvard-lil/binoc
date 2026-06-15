pub mod correspondence;
pub mod renderers;

use std::sync::Arc;

use binoc_core::config::PluginRegistry;
use renderers::markdown::MarkdownRenderer;

/// Register all standard library plugins into a registry.
pub fn register_stdlib(registry: &mut PluginRegistry) {
    let r = |res: Result<(), _>| res.expect("same-build plugin must be SDK-compatible");
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
