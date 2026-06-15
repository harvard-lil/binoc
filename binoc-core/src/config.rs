use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use binoc_sdk::{check_sdk_compatibility, BinocError, Renderer};

/// Dataset configuration loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetConfig {
    #[serde(default = "default_renderers")]
    pub renderers: Vec<String>,
    #[serde(default)]
    pub output: OutputConfig,
    /// Dataset-level semantic configuration. Core carries this JSON and passes
    /// it through to plugins, but does not interpret it.
    #[serde(default)]
    pub dataset: serde_json::Value,
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
            renderers: default_renderers(),
            output: OutputConfig::default(),
            dataset: serde_json::Value::Null,
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
    renderers: BTreeMap<String, Arc<dyn Renderer>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            renderers: BTreeMap::new(),
        }
    }

    pub fn register_renderer(&mut self, renderer: Arc<dyn Renderer>) -> Result<(), BinocError> {
        let desc = renderer.descriptor();
        check_sdk_compatibility(&desc.name, &desc.sdk_version)?;
        self.renderers.insert(desc.name.clone(), renderer);
        Ok(())
    }

    pub fn get_renderer(&self, name: &str) -> Option<Arc<dyn Renderer>> {
        self.renderers.get(name).cloned()
    }

    pub fn renderer_names(&self) -> Vec<String> {
        self.renderers.keys().cloned().collect()
    }

    pub fn resolve(&self, config: &DatasetConfig) -> Result<ResolvedPlugins, BinocError> {
        let renderers: Result<Vec<_>, _> = config
            .renderers
            .iter()
            .map(|name| {
                self.get_renderer(name)
                    .ok_or_else(|| BinocError::Config(format!("unknown renderer: {name}")))
            })
            .collect();

        Ok(ResolvedPlugins {
            renderers: renderers?,
        })
    }
}

/// The result of resolving a config against a registry.
pub struct ResolvedPlugins {
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

    #[test]
    fn registry_register_and_retrieve() {
        let mut registry = PluginRegistry::new();
        registry.register_renderer(Arc::new(MockRenderer)).unwrap();

        assert_eq!(
            registry.get_renderer("mock").unwrap().descriptor().name,
            "mock"
        );
        assert!(registry.get_renderer("unknown").is_none());
    }

    #[test]
    fn resolve_returns_error_for_unknown() {
        let registry = PluginRegistry::new();
        let config = DatasetConfig {
            renderers: vec!["unknown-renderer".into()],
            output: OutputConfig::default(),
            dataset: serde_json::Value::Null,
        };
        let result = registry.resolve(&config);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_preserves_config_order() {
        let mut registry = PluginRegistry::new();
        registry
            .register_renderer(Arc::new(NamedRenderer("first")))
            .unwrap();
        registry
            .register_renderer(Arc::new(NamedRenderer("second")))
            .unwrap();
        registry
            .register_renderer(Arc::new(NamedRenderer("third")))
            .unwrap();

        let config = DatasetConfig {
            renderers: vec!["third".into(), "first".into(), "second".into()],
            output: OutputConfig::default(),
            dataset: serde_json::Value::Null,
        };
        let resolved = registry.resolve(&config).unwrap();
        assert_eq!(resolved.renderers[0].descriptor().name, "third");
        assert_eq!(resolved.renderers[1].descriptor().name, "first");
        assert_eq!(resolved.renderers[2].descriptor().name, "second");
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

    struct MockRenderer;
    impl Renderer for MockRenderer {
        fn descriptor(&self) -> RendererDescriptor {
            RendererDescriptor::new("mock", "txt")
        }
        fn render(
            &self,
            _changesets: &[Changeset],
            _config: &serde_json::Value,
        ) -> BinocResult<String> {
            Ok(String::new())
        }
    }

    struct NamedRenderer(&'static str);
    impl Renderer for NamedRenderer {
        fn descriptor(&self) -> RendererDescriptor {
            RendererDescriptor::new(self.0, self.0)
        }
        fn render(
            &self,
            _changesets: &[Changeset],
            _config: &serde_json::Value,
        ) -> BinocResult<String> {
            Ok(String::new())
        }
    }
}
