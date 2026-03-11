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
    #[error("{0}")]
    Other(String),
}

/// Context available to comparators and transformers. Manages temp
/// directories and caches parsed data for cross-phase access.
pub struct CompareContext {
    temp_dirs: std::sync::Mutex<Vec<tempfile::TempDir>>,
    data_cache: std::sync::Mutex<std::collections::HashMap<String, ReopenedData>>,
}

impl CompareContext {
    pub fn new() -> Self {
        Self {
            temp_dirs: std::sync::Mutex::new(Vec::new()),
            data_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Keep a temp directory alive for the duration of comparison.
    /// Returns the path to the temp directory.
    pub fn keep_temp_dir(&self, dir: tempfile::TempDir) -> std::path::PathBuf {
        let path = dir.path().to_path_buf();
        self.temp_dirs.lock().unwrap().push(dir);
        path
    }

    /// Cache reopened data for a node, keyed by logical path.
    pub fn cache_data(&self, path: &str, data: ReopenedData) {
        self.data_cache
            .lock()
            .unwrap()
            .insert(path.to_string(), data);
    }

    /// Retrieve cached data for a node by logical path.
    pub fn get_cached_data(&self, path: &str) -> Option<ReopenedData> {
        self.data_cache.lock().unwrap().get(path).cloned()
    }
}

impl Default for CompareContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A plugin that claims an item pair and either emits a leaf diff or
/// expands the pair into child items for further processing.
pub trait Comparator: Send + Sync {
    /// Unique name for this comparator (e.g. "binoc.csv").
    fn name(&self) -> &str;

    /// File extensions this comparator handles (e.g. [".csv", ".tsv"]).
    fn handles_extensions(&self) -> &[&str] {
        &[]
    }

    /// MIME media types this comparator handles (e.g. ["application/zip"]).
    /// Checked after extension matching but before `can_handle`.
    fn handles_media_types(&self) -> &[&str] {
        &[]
    }

    /// Imperative dispatch: return true if this comparator can handle the given pair.
    fn can_handle(&self, pair: &ItemPair) -> bool {
        let _ = pair;
        false
    }

    /// Whether this comparator wants to process items even when their
    /// content hashes match (byte-identical). Default false — the controller
    /// short-circuits to "identical" without dispatching. Override to true
    /// for comparators that need to expand identical containers (e.g. to
    /// make their internal structure visible to transformers).
    fn handles_identical(&self) -> bool {
        false
    }

    /// Compare an item pair. Both, one, or neither side may be present.
    fn compare(&self, pair: &ItemPair, ctx: &CompareContext) -> BinocResult<CompareResult>;

    /// Reopen a container to resolve a child's physical path.
    /// Container comparators (directory, zip) implement this to reconstruct
    /// access during the extract chain.
    fn reopen(
        &self,
        _pair: &ItemPair,
        _child_path: &str,
        _ctx: &CompareContext,
    ) -> BinocResult<ItemPair> {
        Err(BinocError::Extract(format!(
            "{} does not support reopen",
            self.name()
        )))
    }

    /// Reopen leaf data: parse source files into format-neutral form.
    /// Called during extract to provide data for transformer extraction.
    fn reopen_data(&self, _pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<ReopenedData> {
        Err(BinocError::Extract(format!(
            "{} does not support reopen_data",
            self.name()
        )))
    }

    /// Extract user-facing data from this comparator's node.
    /// Called when no transformer modified the node (comparator is last toucher).
    fn extract(
        &self,
        _data: &ReopenedData,
        _node: &DiffNode,
        _aspect: &str,
    ) -> Option<ExtractResult> {
        None
    }
}

/// A plugin that rewrites the completed diff tree.
pub trait Transformer: Send + Sync {
    /// Unique name for this transformer (e.g. "binoc.move_detector").
    fn name(&self) -> &str;

    /// Node item_types to match.
    fn match_types(&self) -> &[&str] {
        &[]
    }

    /// Nodes with any of these tags.
    fn match_tags(&self) -> &[&str] {
        &[]
    }

    /// Node diff kinds to match.
    fn match_kinds(&self) -> &[&str] {
        &[]
    }

    /// Whether this transformer operates on individual nodes or whole subtrees.
    fn scope(&self) -> TransformScope {
        TransformScope::Node
    }

    /// Imperative filter: return true if this transformer should process the node.
    fn can_handle(&self, _node: &DiffNode) -> bool {
        false
    }

    /// Rewrite a matched node. Receives the CompareContext for cached data access.
    fn transform(&self, node: DiffNode, ctx: &CompareContext) -> TransformResult;

    /// Suggested phase for default ordering.
    fn suggested_phase(&self) -> &str {
        "default"
    }

    /// Extract user-facing data from a node this transformer modified.
    /// Receives the reopened data from the comparator (via `reopen_data`).
    fn extract(
        &self,
        _data: &ReopenedData,
        _node: &DiffNode,
        _aspect: &str,
    ) -> Option<ExtractResult> {
        None
    }
}

/// A plugin that renders migrations into a human-readable format.
/// Outputters are the final stage of the pipeline: IR -> presentation.
///
/// Each outputter receives its own config section (a `serde_json::Value`)
/// from the dataset config's `output.<name>` block. The outputter is
/// responsible for deserializing this into its own config type, applying
/// its own defaults for missing fields.
pub trait Outputter: Send + Sync {
    /// Unique name for this outputter (e.g. "binoc.markdown").
    fn name(&self) -> &str;

    /// File extension for sidecar output (e.g. "md", "html").
    fn file_extension(&self) -> &str;

    /// Render one or more migrations into a string.
    ///
    /// `config` is the outputter-specific config section from the dataset
    /// config. May be an empty object if the user provided no config for
    /// this outputter; the implementation should apply sensible defaults.
    fn render(&self, migrations: &[Migration], config: &serde_json::Value) -> BinocResult<String>;
}
