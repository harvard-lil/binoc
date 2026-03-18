use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ir::{DiffNode, Migration};
use crate::types::*;

pub type BinocResult<T> = Result<T, BinocError>;

#[derive(Debug, thiserror::Error)]
pub enum BinocError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config error: {0}")]
    Config(String),
    #[error("comparator error in {comparator}: {message}")]
    Comparator { comparator: String, message: String },
    #[error("no comparator found for item: {0}")]
    NoComparator(String),
    #[error("csv error: {0}")]
    Csv(String),
    #[error("zip error: {0}")]
    Zip(String),
    #[error("tar error: {0}")]
    Tar(String),
    #[error("extract error: {0}")]
    Extract(String),
    #[error(
        "SDK version mismatch: {plugin} (plugin '{name}') is not compatible with host SDK {host}"
    )]
    SdkVersion {
        name: String,
        plugin: String,
        host: String,
    },
    #[error("{0}")]
    Other(String),
}

// ── Descriptors ─────────────────────────────────────────────────────

pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Oldest SDK minor version that this host can still accept.
/// Bump this when a protocol change makes older plugins incompatible.
/// Leave it alone when only adding new `#[serde(default)]` fields.
const MIN_COMPATIBLE_MINOR: u64 = 1;

/// Check whether a plugin's SDK version is compatible with this host's SDK.
///
/// During 0.x: plugin minor version must be in `[MIN_COMPATIBLE_MINOR, host_minor]`
/// (same major, patch may differ).
/// After 1.0: plugin major must equal host major, plugin minor <= host minor
/// (standard semver — host is backward-compatible within a major).
pub fn check_sdk_compatibility(plugin_name: &str, plugin_version: &str) -> BinocResult<()> {
    let host = parse_semver(SDK_VERSION);
    let plugin = parse_semver(plugin_version);

    let compatible = match (host, plugin) {
        (Some((hm, hi, _)), Some((pm, pi, _))) if hm == 0 => {
            hm == pm && pi >= MIN_COMPATIBLE_MINOR && pi <= hi
        }
        (Some((hm, hi, _)), Some((pm, pi, _))) => hm == pm && pi <= hi,
        _ => false,
    };

    if compatible {
        Ok(())
    } else {
        Err(BinocError::SdkVersion {
            name: plugin_name.to_string(),
            plugin: plugin_version.to_string(),
            host: SDK_VERSION.to_string(),
        })
    }
}

fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Static metadata for a comparator plugin. Serializable — can be sent as
/// a message, embedded in WASM custom sections, or written to a manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ComparatorDescriptor {
    pub sdk_version: String,
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub media_types: Vec<String>,
    #[serde(default)]
    pub scope: ItemScope,
    #[serde(default)]
    pub handles_identical: bool,
}

impl ComparatorDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            sdk_version: SDK_VERSION.into(),
            name: name.into(),
            extensions: Vec::new(),
            media_types: Vec::new(),
            scope: ItemScope::Files,
            handles_identical: false,
        }
    }

    pub fn with_extensions(mut self, exts: Vec<String>) -> Self {
        self.extensions = exts;
        self
    }

    pub fn with_media_types(mut self, types: Vec<String>) -> Self {
        self.media_types = types;
        self
    }

    pub fn with_scope(mut self, scope: ItemScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_handles_identical(mut self, handles: bool) -> Self {
        self.handles_identical = handles;
        self
    }
}

/// Static metadata for a transformer plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TransformerDescriptor {
    pub sdk_version: String,
    pub name: String,
    #[serde(default)]
    pub match_types: Vec<String>,
    #[serde(default)]
    pub match_tags: Vec<String>,
    #[serde(default)]
    pub match_kinds: Vec<String>,
    #[serde(default)]
    pub scope: TransformScope,
    #[serde(default = "default_phase")]
    pub suggested_phase: String,
}

fn default_phase() -> String {
    "default".into()
}

impl TransformerDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            sdk_version: SDK_VERSION.into(),
            name: name.into(),
            match_types: Vec::new(),
            match_tags: Vec::new(),
            match_kinds: Vec::new(),
            scope: TransformScope::Node,
            suggested_phase: "default".into(),
        }
    }

    pub fn with_match_types(mut self, types: Vec<String>) -> Self {
        self.match_types = types;
        self
    }

    pub fn with_match_tags(mut self, tags: Vec<String>) -> Self {
        self.match_tags = tags;
        self
    }

    pub fn with_match_kinds(mut self, kinds: Vec<String>) -> Self {
        self.match_kinds = kinds;
        self
    }

    pub fn with_scope(mut self, scope: TransformScope) -> Self {
        self.scope = scope;
        self
    }
}

/// Static metadata for a renderer plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RendererDescriptor {
    pub sdk_version: String,
    pub name: String,
    pub file_extension: String,
}

impl RendererDescriptor {
    pub fn new(name: impl Into<String>, file_extension: impl Into<String>) -> Self {
        Self {
            sdk_version: SDK_VERSION.into(),
            name: name.into(),
            file_extension: file_extension.into(),
        }
    }
}

// ── DataAccess ──────────────────────────────────────────────────────

/// Mediates all data I/O for plugins. Replaces direct filesystem access
/// (`Item.physical_path`) and shared mutable context (`CompareContext`).
///
/// In-process: backed by the local filesystem + temp dirs.
/// Cross-ABI: backed by a shared `data_root` directory so host and plugin
/// can exchange cached data via `store()`/`load()`.
pub trait DataAccess: Send + Sync {
    /// Read the full contents of an item as bytes.
    fn read_bytes(&self, item: &ItemRef) -> BinocResult<Vec<u8>>;

    /// Open a streaming reader for an item.
    fn open_read(&self, item: &ItemRef) -> BinocResult<Box<dyn std::io::Read + Send>>;

    /// Get a local filesystem path for tools that require one (e.g. SQLite).
    /// Not available on all backends — prefer read_bytes/open_read.
    fn local_path(&self, item: &ItemRef) -> BinocResult<PathBuf>;

    /// Make new data available as an item (for container expansion).
    /// Returns an ItemRef usable in child ItemPairs.
    fn provide(&self, logical_path: &str, content: &[u8]) -> BinocResult<ItemRef>;

    /// Get a fresh writable workspace directory.
    /// Managed by the DataAccess — cleaned up when the diff operation completes.
    fn workspace(&self) -> BinocResult<PathBuf>;

    /// Register a local filesystem path as a known item.
    /// Returns an ItemRef that can be used in child ItemPairs.
    fn register_local(&self, physical: &Path, logical: &str) -> BinocResult<ItemRef>;

    /// Cache opaque data for cross-phase access, keyed by a string.
    /// Filesystem-backed under `data_root()` so data is visible across the
    /// C ABI boundary (host and plugin share the same data_root).
    fn store(&self, key: &str, data: &[u8]) -> BinocResult<()>;

    /// Retrieve cached data by key.
    fn load(&self, key: &str) -> BinocResult<Option<Vec<u8>>>;

    /// Session-level root directory shared between host and plugins.
    /// Cache files live at `<data_root>/.cache/`. ABI requests carry this
    /// path so native plugins can construct a `LocalDataAccess` that reads
    /// from the same cache.
    fn data_root(&self) -> BinocResult<PathBuf>;
}

// ── Plugin traits ───────────────────────────────────────────────────

/// A plugin that claims an item pair and either emits a leaf diff or
/// expands the pair into child items for further processing.
///
/// Routing is fully declarative via [`ComparatorDescriptor`]. If the
/// descriptor matches but the comparator discovers at compare-time that
/// it cannot handle the item, it returns [`CompareResult::Skip`].
pub trait Comparator: Send + Sync {
    fn descriptor(&self) -> ComparatorDescriptor;

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult>;

    /// Reconstruct physical access to a child item without re-diffing.
    /// Container comparators (zip, directory, tar) override this to
    /// extract or resolve a child path within the container, returning
    /// an `ItemPair` that downstream comparators can work with.
    ///
    /// Used by the extract chain: the controller walks ancestor nodes
    /// calling `reopen()` to progressively reconstruct the scratchpad.
    fn reopen(
        &self,
        _pair: &ItemPair,
        _child_path: &str,
        _data: &dyn DataAccess,
    ) -> BinocResult<ItemPair> {
        Err(BinocError::Extract(format!(
            "{} does not support reopen",
            self.descriptor().name
        )))
    }

    /// Extract user-facing data from a node this comparator produced.
    fn extract(
        &self,
        _node: &DiffNode,
        _aspect: &str,
        _data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        None
    }
}

/// A plugin that rewrites the completed diff tree.
///
/// Matching is declarative via [`TransformerDescriptor`]. If a matched
/// node should not be transformed, return [`TransformResult::Unchanged`].
pub trait Transformer: Send + Sync {
    fn descriptor(&self) -> TransformerDescriptor;

    fn transform(&self, node: DiffNode, data: &dyn DataAccess) -> TransformResult;

    /// Extract user-facing data from a node this transformer modified.
    fn extract(
        &self,
        _node: &DiffNode,
        _aspect: &str,
        _data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        None
    }
}

/// A plugin that renders migrations into a human-readable format.
pub trait Renderer: Send + Sync {
    fn descriptor(&self) -> RendererDescriptor;

    fn render(&self, migrations: &[Migration], config: &serde_json::Value) -> BinocResult<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_version_is_compatible() {
        assert!(check_sdk_compatibility("test", SDK_VERSION).is_ok());
    }

    #[test]
    fn patch_difference_is_compatible() {
        let host = parse_semver(SDK_VERSION).unwrap();
        let tweaked = format!("{}.{}.99", host.0, host.1);
        assert!(check_sdk_compatibility("test", &tweaked).is_ok());
    }

    #[test]
    fn older_minor_within_floor_is_compatible() {
        let host = parse_semver(SDK_VERSION).unwrap();
        if host.0 != 0 || host.1 < MIN_COMPATIBLE_MINOR {
            return;
        }
        let oldest_ok = format!("0.{}.0", MIN_COMPATIBLE_MINOR);
        assert!(check_sdk_compatibility("test", &oldest_ok).is_ok());
    }

    #[test]
    fn older_minor_below_floor_rejected() {
        if MIN_COMPATIBLE_MINOR == 0 {
            return; // no floor to test
        }
        let too_old = format!("0.{}.0", MIN_COMPATIBLE_MINOR - 1);
        assert!(check_sdk_compatibility("test", &too_old).is_err());
    }

    #[test]
    fn newer_minor_rejected_during_0x() {
        let host = parse_semver(SDK_VERSION).unwrap();
        if host.0 != 0 {
            return;
        }
        let tweaked = format!("0.{}.0", host.1 + 1);
        assert!(check_sdk_compatibility("test", &tweaked).is_err());
    }

    #[test]
    fn garbage_version_rejected() {
        assert!(check_sdk_compatibility("test", "not-a-version").is_err());
    }
}
