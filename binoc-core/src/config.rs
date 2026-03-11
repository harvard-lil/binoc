use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::traits::BinocError;

/// Dataset configuration loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetConfig {
    /// Ordered list of comparator names. First to claim wins.
    #[serde(default)]
    pub comparators: Vec<String>,

    /// Ordered list of transformer names. Run in order on the completed diff tree.
    #[serde(default)]
    pub transformers: Vec<String>,

    /// Ordered list of outputter names. Each runs to produce a sidecar or changelog.
    #[serde(default = "default_outputters")]
    pub outputters: Vec<String>,

    /// Output configuration.
    #[serde(default)]
    pub output: OutputConfig,
}

fn default_outputters() -> Vec<String> {
    vec!["binoc.markdown".into()]
}

/// Per-outputter configuration. Keys are outputter names (e.g. "markdown"
/// or "binoc.markdown"); values are outputter-specific config objects.
/// Each outputter receives its own section and handles its own defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(flatten)]
    pub sections: BTreeMap<String, serde_json::Value>,
}

impl OutputConfig {
    /// Look up config for a named outputter. Tries exact match, then
    /// strips/adds the "binoc." prefix to find a match.
    pub fn get_for_outputter(&self, name: &str) -> serde_json::Value {
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
    /// Load config from a YAML file.
    pub fn from_file(path: &Path) -> Result<Self, BinocError> {
        let contents = std::fs::read_to_string(path).map_err(BinocError::Io)?;
        serde_yaml::from_str(&contents).map_err(|e| BinocError::Config(e.to_string()))
    }

    /// Return the default configuration with the standard plugin pipeline.
    pub fn default_config() -> Self {
        Self {
            comparators: vec![
                "binoc.zip".into(),
                "binoc.tar".into(),
                "binoc.directory".into(),
                "binoc.csv".into(),
                "binoc.text".into(),
                "binoc.binary".into(),
            ],
            transformers: vec![
                "binoc.move_detector".into(),
                "binoc.copy_detector".into(),
                "binoc.column_reorder_detector".into(),
            ],
            outputters: default_outputters(),
            output: OutputConfig::default(),
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
    comparators: BTreeMap<String, std::sync::Arc<dyn crate::traits::Comparator>>,
    transformers: BTreeMap<String, std::sync::Arc<dyn crate::traits::Transformer>>,
    outputters: BTreeMap<String, std::sync::Arc<dyn crate::traits::Outputter>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            comparators: BTreeMap::new(),
            transformers: BTreeMap::new(),
            outputters: BTreeMap::new(),
        }
    }

    pub fn register_comparator(
        &mut self,
        name: impl Into<String>,
        comparator: std::sync::Arc<dyn crate::traits::Comparator>,
    ) {
        self.comparators.insert(name.into(), comparator);
    }

    pub fn register_transformer(
        &mut self,
        name: impl Into<String>,
        transformer: std::sync::Arc<dyn crate::traits::Transformer>,
    ) {
        self.transformers.insert(name.into(), transformer);
    }

    pub fn register_outputter(
        &mut self,
        name: impl Into<String>,
        outputter: std::sync::Arc<dyn crate::traits::Outputter>,
    ) {
        self.outputters.insert(name.into(), outputter);
    }

    pub fn get_comparator(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<dyn crate::traits::Comparator>> {
        self.comparators.get(name).cloned()
    }

    pub fn get_transformer(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<dyn crate::traits::Transformer>> {
        self.transformers.get(name).cloned()
    }

    pub fn get_outputter(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<dyn crate::traits::Outputter>> {
        self.outputters.get(name).cloned()
    }

    pub fn comparator_names(&self) -> Vec<String> {
        self.comparators.keys().cloned().collect()
    }

    pub fn transformer_names(&self) -> Vec<String> {
        self.transformers.keys().cloned().collect()
    }

    pub fn outputter_names(&self) -> Vec<String> {
        self.outputters.keys().cloned().collect()
    }

    /// Build a default config that includes all registered plugins.
    ///
    /// Comparators registered beyond the standard set are inserted just
    /// before `binoc.binary` (the catch-all fallback). Transformers
    /// registered beyond the standard set are appended at the end.
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

    /// Resolve a config into ordered lists of plugin instances.
    pub fn resolve(
        &self,
        config: &DatasetConfig,
    ) -> Result<ResolvedPlugins, crate::traits::BinocError> {
        let comparators: Result<Vec<_>, _> = config
            .comparators
            .iter()
            .map(|name| {
                self.get_comparator(name).ok_or_else(|| {
                    crate::traits::BinocError::Config(format!("unknown comparator: {name}"))
                })
            })
            .collect();

        let transformers: Result<Vec<_>, _> = config
            .transformers
            .iter()
            .map(|name| {
                self.get_transformer(name).ok_or_else(|| {
                    crate::traits::BinocError::Config(format!("unknown transformer: {name}"))
                })
            })
            .collect();

        let outputters: Result<Vec<_>, _> = config
            .outputters
            .iter()
            .map(|name| {
                self.get_outputter(name).ok_or_else(|| {
                    crate::traits::BinocError::Config(format!("unknown outputter: {name}"))
                })
            })
            .collect();

        Ok(ResolvedPlugins {
            comparators: comparators?,
            transformers: transformers?,
            outputters: outputters?,
        })
    }
}

/// The result of resolving a config against a registry.
pub struct ResolvedPlugins {
    pub comparators: Vec<std::sync::Arc<dyn crate::traits::Comparator>>,
    pub transformers: Vec<std::sync::Arc<dyn crate::traits::Transformer>>,
    pub outputters: Vec<std::sync::Arc<dyn crate::traits::Outputter>>,
}

impl ResolvedPlugins {
    /// Find an outputter by file extension. Returns `None` if no outputter
    /// claims the extension, or an error if multiple outputters claim it.
    pub fn outputter_for_extension(
        &self,
        ext: &str,
    ) -> Result<Option<std::sync::Arc<dyn crate::traits::Outputter>>, crate::traits::BinocError>
    {
        let matches: Vec<_> = self
            .outputters
            .iter()
            .filter(|o| o.file_extension() == ext)
            .collect();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0].clone())),
            _ => {
                let names: Vec<_> = matches.iter().map(|o| o.name()).collect();
                Err(crate::traits::BinocError::Config(format!(
                    "ambiguous extension .{ext}: claimed by {}; use format:path syntax",
                    names.join(", "),
                )))
            }
        }
    }

    /// Find an outputter by name (e.g. "markdown" matches "binoc.markdown").
    /// Tries exact match first, then checks for a `binoc.{name}` match.
    pub fn outputter_by_name(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<dyn crate::traits::Outputter>> {
        self.outputters
            .iter()
            .find(|o| o.name() == name)
            .or_else(|| {
                let qualified = format!("binoc.{name}");
                self.outputters.iter().find(|o| o.name() == qualified)
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
    use crate::ir::DiffNode;
    use crate::traits::CompareContext;
    use crate::traits::{Comparator, Transformer};
    use crate::types::{CompareResult, ItemPair, TransformResult};
    use std::sync::Arc;

    struct MockComparator(&'static str);
    impl Comparator for MockComparator {
        fn name(&self) -> &str {
            self.0
        }
        fn compare(
            &self,
            _pair: &ItemPair,
            _ctx: &CompareContext,
        ) -> crate::traits::BinocResult<CompareResult> {
            Ok(CompareResult::Identical)
        }
    }

    struct MockTransformer(&'static str);
    impl Transformer for MockTransformer {
        fn name(&self) -> &str {
            self.0
        }
        fn transform(&self, _node: DiffNode, _ctx: &CompareContext) -> TransformResult {
            TransformResult::Unchanged
        }
    }

    #[test]
    fn dataset_config_default_config_returns_expected_pipeline_names() {
        let config = DatasetConfig::default_config();
        assert_eq!(
            config.comparators,
            vec![
                "binoc.zip",
                "binoc.tar",
                "binoc.directory",
                "binoc.csv",
                "binoc.text",
                "binoc.binary",
            ]
        );
        assert_eq!(
            config.transformers,
            vec![
                "binoc.move_detector",
                "binoc.copy_detector",
                "binoc.column_reorder_detector",
            ]
        );
    }

    #[test]
    fn dataset_config_deserialization_from_yaml() {
        let yaml = r#"
comparators:
  - binoc.csv
  - binoc.text
transformers:
  - binoc.move_detector
output:
  markdown:
    significance:
      critical:
        - binoc.schema-change
"#;
        let config: DatasetConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.comparators, vec!["binoc.csv", "binoc.text"]);
        assert_eq!(config.transformers, vec!["binoc.move_detector"]);
        let md_val = config.output.get_for_outputter("binoc.markdown");
        let sig = md_val
            .get("significance")
            .expect("should have significance section");
        assert!(
            sig.get("critical").is_some(),
            "should have 'critical' category"
        );
    }

    #[test]
    fn output_config_get_for_outputter_resolves_short_and_qualified_names() {
        let mut sections = BTreeMap::new();
        sections.insert(
            "markdown".into(),
            serde_json::json!({"significance": {"test": ["binoc.test"]}}),
        );
        let config = OutputConfig { sections };

        let by_short = config.get_for_outputter("markdown");
        assert!(by_short.is_object());

        let by_qualified = config.get_for_outputter("binoc.markdown");
        assert!(by_qualified.is_object());
        assert_eq!(by_short, by_qualified);

        let missing = config.get_for_outputter("binoc.html");
        assert!(missing.is_object());
        assert!(missing.as_object().unwrap().is_empty());
    }

    #[test]
    fn plugin_registry_register_and_retrieve() {
        let mut registry = PluginRegistry::new();
        let comp = Arc::new(MockComparator("mock-comp")) as Arc<dyn Comparator>;
        let trans = Arc::new(MockTransformer("mock-trans")) as Arc<dyn Transformer>;

        registry.register_comparator("mock-comp", Arc::clone(&comp));
        registry.register_transformer("mock-trans", Arc::clone(&trans));

        assert_eq!(
            registry.get_comparator("mock-comp").unwrap().name(),
            "mock-comp"
        );
        assert_eq!(
            registry.get_transformer("mock-trans").unwrap().name(),
            "mock-trans"
        );
        assert!(registry.get_comparator("unknown").is_none());
        assert!(registry.get_transformer("unknown").is_none());
    }

    #[test]
    fn plugin_registry_resolve_returns_error_for_unknown_plugin() {
        let registry = PluginRegistry::new();
        let config = DatasetConfig {
            comparators: vec!["unknown-comparator".into()],
            transformers: vec![],
            outputters: vec![],
            output: OutputConfig::default(),
        };
        let result = registry.resolve(&config);
        match &result {
            Err(e) => assert!(format!("{e}").contains("unknown comparator")),
            Ok(_) => panic!("expected resolve to fail"),
        }
    }

    #[test]
    fn plugin_registry_resolve_returns_plugins_in_config_order() {
        let mut registry = PluginRegistry::new();
        registry.register_comparator("first", Arc::new(MockComparator("first")));
        registry.register_comparator("second", Arc::new(MockComparator("second")));
        registry.register_comparator("third", Arc::new(MockComparator("third")));

        let config = DatasetConfig {
            comparators: vec!["third".into(), "first".into(), "second".into()],
            transformers: vec![],
            outputters: vec![],
            output: OutputConfig::default(),
        };
        let resolved = registry.resolve(&config).unwrap();
        assert_eq!(resolved.comparators.len(), 3);
        assert_eq!(resolved.comparators[0].name(), "third");
        assert_eq!(resolved.comparators[1].name(), "first");
        assert_eq!(resolved.comparators[2].name(), "second");
    }
}
