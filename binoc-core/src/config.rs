use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use binoc_sdk::{check_sdk_compatibility, BinocError, Comparator, Renderer, Transformer};

/// Dataset configuration loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetConfig {
    #[serde(default)]
    pub comparators: Vec<String>,
    #[serde(default)]
    pub transformers: Vec<String>,
    #[serde(default = "default_renderers")]
    pub renderers: Vec<String>,
    #[serde(default)]
    pub output: OutputConfig,
    /// Dataset-level semantic configuration. Core carries this JSON and passes
    /// it through to plugins, but does not interpret it.
    #[serde(default)]
    pub dataset: serde_json::Value,
    /// Per-transformer configuration, keyed by transformer name
    /// (e.g. `"binoc.folder_move_detector"`). Unset entries pass
    /// [`serde_json::Value::Null`] to the transformer.
    #[serde(default)]
    pub transformer_config: TransformerConfig,
}

/// Per-transformer configuration. Flattened into a map by transformer name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformerConfig {
    #[serde(flatten)]
    pub sections: BTreeMap<String, serde_json::Value>,
}

impl TransformerConfig {
    pub fn get_for_transformer(&self, name: &str) -> serde_json::Value {
        if let Some(v) = self.sections.get(name) {
            return v.clone();
        }
        if let Some(short) = name.strip_prefix("binoc.") {
            if let Some(v) = self.sections.get(short) {
                return v.clone();
            }
        }
        let qualified = format!("binoc.{name}");
        if let Some(v) = self.sections.get(&qualified) {
            return v.clone();
        }
        serde_json::Value::Null
    }

    /// Returns the full map keyed by transformer name, with any short-name
    /// aliases normalized to their `binoc.*` form.
    pub fn as_map(&self) -> BTreeMap<String, serde_json::Value> {
        self.sections.clone()
    }
}

fn default_renderers() -> Vec<String> {
    vec!["binoc.markdown".into()]
}

/// Per-renderer configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(flatten)]
    pub sections: BTreeMap<String, serde_json::Value>,
}

impl OutputConfig {
    pub fn get_for_renderer(&self, name: &str) -> serde_json::Value {
        if let Some(v) = self.sections.get(name) {
            return v.clone();
        }
        if let Some(short) = name.strip_prefix("binoc.") {
            if let Some(v) = self.sections.get(short) {
                return v.clone();
            }
        }
        let qualified = format!("binoc.{name}");
        if let Some(v) = self.sections.get(&qualified) {
            return v.clone();
        }
        serde_json::Value::Object(Default::default())
    }
}

impl DatasetConfig {
    pub fn from_file(path: &Path) -> Result<Self, BinocError> {
        let contents = std::fs::read_to_string(path).map_err(BinocError::Io)?;
        serde_yaml::from_str(&contents).map_err(|e| BinocError::Config(e.to_string()))
    }

    pub fn default_config() -> Self {
        Self {
            comparators: vec![
                "binoc.zip".into(),
                "binoc.tar".into(),
                "binoc.gzip".into(),
                "binoc.directory".into(),
                "binoc.csv".into(),
                "binoc.text".into(),
                "binoc.binary".into(),
            ],
            transformers: vec![
                "binoc.declared_correspondence".into(),
                "binoc.correlation_detector".into(),
                "binoc.fuzzy_correlation_detector".into(),
                "binoc.folder_move_detector".into(),
                "binoc.table_splitter".into(),
                "binoc.tabular_analyzer".into(),
                "binoc.tabular_stats_annotator".into(),
                "binoc.table_collection_analyzer".into(),
            ],
            renderers: default_renderers(),
            output: OutputConfig::default(),
            dataset: serde_json::Value::Null,
            transformer_config: TransformerConfig::default(),
        }
    }
}

impl Default for DatasetConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Registry mapping plugin names to instances.
pub struct PluginRegistry {
    comparators: BTreeMap<String, Arc<dyn Comparator>>,
    transformers: BTreeMap<String, Arc<dyn Transformer>>,
    renderers: BTreeMap<String, Arc<dyn Renderer>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            comparators: BTreeMap::new(),
            transformers: BTreeMap::new(),
            renderers: BTreeMap::new(),
        }
    }

    pub fn register_comparator(
        &mut self,
        comparator: Arc<dyn Comparator>,
    ) -> Result<(), BinocError> {
        let desc = comparator.descriptor();
        check_sdk_compatibility(&desc.name, &desc.sdk_version)?;
        self.comparators.insert(desc.name.clone(), comparator);
        Ok(())
    }

    pub fn register_transformer(
        &mut self,
        transformer: Arc<dyn Transformer>,
    ) -> Result<(), BinocError> {
        let desc = transformer.descriptor();
        check_sdk_compatibility(&desc.name, &desc.sdk_version)?;
        self.transformers.insert(desc.name.clone(), transformer);
        Ok(())
    }

    pub fn register_renderer(&mut self, renderer: Arc<dyn Renderer>) -> Result<(), BinocError> {
        let desc = renderer.descriptor();
        check_sdk_compatibility(&desc.name, &desc.sdk_version)?;
        self.renderers.insert(desc.name.clone(), renderer);
        Ok(())
    }

    pub fn get_comparator(&self, name: &str) -> Option<Arc<dyn Comparator>> {
        self.comparators.get(name).cloned()
    }

    pub fn get_transformer(&self, name: &str) -> Option<Arc<dyn Transformer>> {
        self.transformers.get(name).cloned()
    }

    pub fn get_renderer(&self, name: &str) -> Option<Arc<dyn Renderer>> {
        self.renderers.get(name).cloned()
    }

    pub fn comparator_names(&self) -> Vec<String> {
        self.comparators.keys().cloned().collect()
    }

    pub fn transformer_names(&self) -> Vec<String> {
        self.transformers.keys().cloned().collect()
    }

    pub fn renderer_names(&self) -> Vec<String> {
        self.renderers.keys().cloned().collect()
    }

    pub fn default_config(&self) -> DatasetConfig {
        let mut config = DatasetConfig::default_config();

        let extra_comparators: Vec<String> = self
            .comparators
            .keys()
            .filter(|name| !config.comparators.contains(name))
            .cloned()
            .collect();

        if !extra_comparators.is_empty() {
            let insert_pos = config
                .comparators
                .iter()
                .position(|n| n == "binoc.binary")
                .unwrap_or(config.comparators.len());
            for (i, name) in extra_comparators.into_iter().enumerate() {
                config.comparators.insert(insert_pos + i, name);
            }
        }

        let extra_transformers: Vec<String> = self
            .transformers
            .keys()
            .filter(|name| !config.transformers.contains(name))
            .cloned()
            .collect();
        config.transformers.extend(extra_transformers);

        config
    }

    pub fn resolve(&self, config: &DatasetConfig) -> Result<ResolvedPlugins, BinocError> {
        let comparators: Result<Vec<_>, _> = config
            .comparators
            .iter()
            .map(|name| {
                self.get_comparator(name)
                    .ok_or_else(|| BinocError::Config(format!("unknown comparator: {name}")))
            })
            .collect();

        let transformers: Result<Vec<_>, _> = config
            .transformers
            .iter()
            .map(|name| {
                self.get_transformer(name)
                    .ok_or_else(|| BinocError::Config(format!("unknown transformer: {name}")))
            })
            .collect();

        let renderers: Result<Vec<_>, _> = config
            .renderers
            .iter()
            .map(|name| {
                self.get_renderer(name)
                    .ok_or_else(|| BinocError::Config(format!("unknown renderer: {name}")))
            })
            .collect();

        Ok(ResolvedPlugins {
            comparators: comparators?,
            transformers: transformers?,
            renderers: renderers?,
        })
    }
}

/// The result of resolving a config against a registry.
pub struct ResolvedPlugins {
    pub comparators: Vec<Arc<dyn Comparator>>,
    pub transformers: Vec<Arc<dyn Transformer>>,
    pub renderers: Vec<Arc<dyn Renderer>>,
}

impl ResolvedPlugins {
    pub fn renderer_for_extension(
        &self,
        ext: &str,
    ) -> Result<Option<Arc<dyn Renderer>>, BinocError> {
        let matches: Vec<_> = self
            .renderers
            .iter()
            .filter(|o| o.descriptor().file_extension == ext)
            .collect();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0].clone())),
            _ => {
                let names: Vec<_> = matches
                    .iter()
                    .map(|o| o.descriptor().name.clone())
                    .collect();
                Err(BinocError::Config(format!(
                    "ambiguous extension .{ext}: claimed by {}; use format:path syntax",
                    names.join(", "),
                )))
            }
        }
    }

    pub fn renderer_by_name(&self, name: &str) -> Option<Arc<dyn Renderer>> {
        self.renderers
            .iter()
            .find(|o| o.descriptor().name == name)
            .or_else(|| {
                let qualified = format!("binoc.{name}");
                self.renderers
                    .iter()
                    .find(|o| o.descriptor().name == qualified)
            })
            .cloned()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::*;

    struct MockComparator(&'static str);
    impl Comparator for MockComparator {
        fn descriptor(&self) -> ComparatorDescriptor {
            ComparatorDescriptor::new(self.0)
        }
        fn compare(&self, _pair: &ItemPair, _data: &dyn DataAccess) -> BinocResult<CompareResult> {
            Ok(CompareResult::Identical)
        }
    }

    struct MockTransformer(&'static str);
    impl Transformer for MockTransformer {
        fn descriptor(&self) -> TransformerDescriptor {
            TransformerDescriptor::new(self.0)
        }
        fn transform(
            &self,
            _node: DiffNode,
            _data: &dyn DataAccess,
            _config: &serde_json::Value,
        ) -> TransformResult {
            TransformResult::Unchanged
        }
    }

    #[test]
    fn registry_register_and_retrieve() {
        let mut registry = PluginRegistry::new();
        registry
            .register_comparator(Arc::new(MockComparator("mock-comp")))
            .unwrap();
        registry
            .register_transformer(Arc::new(MockTransformer("mock-trans")))
            .unwrap();

        assert_eq!(
            registry
                .get_comparator("mock-comp")
                .unwrap()
                .descriptor()
                .name,
            "mock-comp"
        );
        assert_eq!(
            registry
                .get_transformer("mock-trans")
                .unwrap()
                .descriptor()
                .name,
            "mock-trans"
        );
        assert!(registry.get_comparator("unknown").is_none());
    }

    #[test]
    fn resolve_returns_error_for_unknown() {
        let registry = PluginRegistry::new();
        let config = DatasetConfig {
            comparators: vec!["unknown-comparator".into()],
            transformers: vec![],
            renderers: vec![],
            output: OutputConfig::default(),
            dataset: serde_json::Value::Null,
            transformer_config: TransformerConfig::default(),
        };
        let result = registry.resolve(&config);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_preserves_config_order() {
        let mut registry = PluginRegistry::new();
        registry
            .register_comparator(Arc::new(MockComparator("first")))
            .unwrap();
        registry
            .register_comparator(Arc::new(MockComparator("second")))
            .unwrap();
        registry
            .register_comparator(Arc::new(MockComparator("third")))
            .unwrap();

        let config = DatasetConfig {
            comparators: vec!["third".into(), "first".into(), "second".into()],
            transformers: vec![],
            renderers: vec![],
            output: OutputConfig::default(),
            dataset: serde_json::Value::Null,
            transformer_config: TransformerConfig::default(),
        };
        let resolved = registry.resolve(&config).unwrap();
        assert_eq!(resolved.comparators[0].descriptor().name, "third");
        assert_eq!(resolved.comparators[1].descriptor().name, "first");
        assert_eq!(resolved.comparators[2].descriptor().name, "second");
    }

    #[test]
    fn rejects_incompatible_comparator_version() {
        struct BadVersionComparator;
        impl Comparator for BadVersionComparator {
            fn descriptor(&self) -> ComparatorDescriptor {
                let mut desc = ComparatorDescriptor::new("bad-version");
                desc.sdk_version = "99.0.0".into();
                desc
            }
            fn compare(
                &self,
                _pair: &ItemPair,
                _data: &dyn DataAccess,
            ) -> BinocResult<CompareResult> {
                Ok(CompareResult::Identical)
            }
        }

        let mut registry = PluginRegistry::new();
        let result = registry.register_comparator(Arc::new(BadVersionComparator));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("bad-version"),
            "error should name the plugin: {msg}"
        );
        assert!(
            msg.contains("99.0.0"),
            "error should include plugin version: {msg}"
        );
    }

    #[test]
    fn rejects_incompatible_transformer_version() {
        struct BadVersionTransformer;
        impl Transformer for BadVersionTransformer {
            fn descriptor(&self) -> TransformerDescriptor {
                let mut desc = TransformerDescriptor::new("bad-trans");
                desc.sdk_version = "99.0.0".into();
                desc
            }
            fn transform(
                &self,
                _node: DiffNode,
                _data: &dyn DataAccess,
                _config: &serde_json::Value,
            ) -> TransformResult {
                TransformResult::Unchanged
            }
        }

        let mut registry = PluginRegistry::new();
        let result = registry.register_transformer(Arc::new(BadVersionTransformer));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_incompatible_renderer_version() {
        struct BadVersionRenderer;
        impl Renderer for BadVersionRenderer {
            fn descriptor(&self) -> RendererDescriptor {
                let mut desc = RendererDescriptor::new("bad-out", "txt");
                desc.sdk_version = "99.0.0".into();
                desc
            }
            fn render(
                &self,
                _changesets: &[Changeset],
                _config: &serde_json::Value,
            ) -> BinocResult<String> {
                Ok(String::new())
            }
        }

        let mut registry = PluginRegistry::new();
        let result = registry.register_renderer(Arc::new(BadVersionRenderer));
        assert!(result.is_err());
    }
}
