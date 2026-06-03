use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::types::{ArtifactDescriptor, ItemPair};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Suggestion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl Diagnostic {
    pub fn new(
        severity: DiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            location: None,
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Warning, code, message)
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Error, code, message)
    }

    pub fn suggestion(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Suggestion, code, message)
    }

    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    fn normalized(mut self) -> Self {
        if self.location.as_deref().is_some_and(|s| s.is_empty()) {
            self.location = None;
        }
        self
    }
}

/// Renderer-visible metadata attached to a diff node by a comparator or
/// transformer.
///
/// Annotations are intentionally progressively typed: producers can start with
/// a string or simple JSON value, and renderers can either display the generic
/// value shape or add package/key-specific handling later. The package namespace
/// keeps independently-authored plugins from colliding on common keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Annotation {
    pub package: String,
    pub key: String,
    pub value: serde_json::Value,
}

impl Annotation {
    pub fn new(
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        Self {
            package: package.into(),
            key: key.into(),
            value,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        self.value.as_str()
    }
}

/// A node in the diff tree — the central data structure of the system.
/// Every comparator emits it, every transformer rewrites it, and serializers
/// or bindings read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DiffNode {
    /// Open enum: "add", "remove", "modify", "move", "reorder",
    /// "schema_change", etc. Plugins may define new actions.
    pub action: String,

    /// Open string: "directory", "file", "tabular", "zip_archive", etc.
    /// No built-in types — conventions, not enforcement.
    pub item_type: String,

    /// Location within snapshot (logical path, including interior paths
    /// like "archive.zip/data/file.csv").
    pub path: String,

    /// For moves/renames: the original path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,

    /// Optional human-readable one-liner describing the change.
    /// Set by comparator or transformer; used by renderers for narrative rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Open bag of semantic tags, namespaced by convention.
    /// e.g. "binoc.column-reorder", "biobinoc.gap-change"
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<String>,

    /// Child diff nodes forming the tree structure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DiffNode>,

    /// Comparator-specific payload, schema determined by item_type convention.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,

    /// Renderer-visible, structured evidence blocks. Comparators and
    /// transformers populate these with bounded examples while they still have
    /// domain knowledge; renderers decide how much to display.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_blocks: Vec<DetailBlock>,

    /// Renderer-visible annotations supplied by comparators or transformers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,

    /// Which comparator produced this node (provenance for extract chain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparator: Option<String>,

    /// Transformers that modified this node, in order (provenance for extract chain).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transformed_by: Vec<String>,

    /// The original item pair that produced this node. Session-scoped working
    /// data: available during a live diff/transform session for transformers
    /// and extractors that need to re-read source data, and carried across the
    /// plugin ABI wire so separately-compiled plugins can access it. Callers
    /// writing changeset output must strip this via
    /// [`DiffNode::strip_transient`] before serializing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_items: Option<ItemPair>,

    /// Node-scoped diagnostics emitted during comparison or transform.
    /// Transient: the controller hoists them into [`Changeset::diagnostics`]
    /// at the end of the diff, then clears this field so the output shape
    /// stays as one durable top-level diagnostics list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,

    /// Published artifacts for this node. Session-scoped working data: carried
    /// across the plugin ABI wire as descriptors (the bytes live in the shared
    /// `data_root` cache), but not meaningful outside a session. Callers
    /// writing changeset output must strip this via
    /// [`DiffNode::strip_transient`] before serializing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,

    /// Request that the controller re-dispatch the given `ItemPair` through
    /// the comparator pipeline and merge the result into this semantic wrapper
    /// node before the next transformer runs. Set by transformers that have
    /// discovered a correspondence (for example, a rename-with-edits or a
    /// config-declared logical file pair) but still need normal comparators to
    /// parse the paired content. If the recomparison is identical, a plain
    /// `modify` wrapper is converted to `identical` so it can be pruned, while
    /// semantic wrappers such as moves remain reportable. Session-scoped
    /// working data: wire-visible so plugins can set it across the ABI
    /// boundary, but cleared by [`DiffNode::strip_transient`] before changeset
    /// output. The controller takes (clears) this field as it processes it;
    /// nodes in a finalized changeset never carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_recompare: Option<ItemPair>,
}

impl DiffNode {
    pub fn new(
        action: impl Into<String>,
        item_type: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            item_type: item_type.into(),
            path: path.into(),
            source_path: None,
            summary: None,
            tags: BTreeSet::new(),
            children: Vec::new(),
            details: BTreeMap::new(),
            detail_blocks: Vec::new(),
            annotations: Vec::new(),
            comparator: None,
            transformed_by: Vec::new(),
            source_items: None,
            diagnostics: Vec::new(),
            artifacts: Vec::new(),
            pending_recompare: None,
        }
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }

    pub fn with_children(mut self, children: Vec<DiffNode>) -> Self {
        self.children = children;
        self
    }

    pub fn with_detail_block(mut self, block: DetailBlock) -> Self {
        self.detail_blocks.push(block);
        self
    }

    pub fn with_annotation_from(
        mut self,
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.annotate_from(package, key, value);
        self
    }

    pub fn with_source_path(mut self, source: impl Into<String>) -> Self {
        self.source_path = Some(source.into());
        self
    }

    pub fn with_source_items(mut self, items: ItemPair) -> Self {
        self.source_items = Some(items);
        self
    }

    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.push_diagnostic(diagnostic);
        self
    }

    pub fn with_artifact(mut self, artifact: ArtifactDescriptor) -> Self {
        self.artifacts.push(artifact);
        self
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        let diagnostic = if diagnostic.location.is_none() && !self.path.is_empty() {
            diagnostic.with_location(self.path.clone())
        } else {
            diagnostic
        };
        self.diagnostics.push(diagnostic.normalized());
    }

    pub fn annotate_from(
        &mut self,
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) {
        let package = package.into();
        let key = key.into();
        if let Some(existing) = self
            .annotations
            .iter_mut()
            .find(|annotation| annotation.package == package && annotation.key == key)
        {
            existing.value = value;
        } else {
            self.annotations.push(Annotation::new(package, key, value));
        }
    }

    pub fn annotation(&self, package: &str, key: &str) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|annotation| annotation.package == package && annotation.key == key)
    }

    pub fn binoc_annotation(&self, key: &str) -> Option<&Annotation> {
        self.annotation("binoc", key)
    }

    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }

    pub fn all_tags(&self) -> BTreeSet<String> {
        let mut tags = self.tags.clone();
        for child in &self.children {
            tags.extend(child.all_tags());
        }
        tags
    }

    fn drain_diagnostics_into(&mut self, target: &mut Vec<Diagnostic>) {
        target.append(&mut self.diagnostics);
        for child in &mut self.children {
            child.drain_diagnostics_into(target);
        }
    }

    /// Recursively clear session-scoped transient fields (`source_items`,
    /// `diagnostics`, `artifacts`, `pending_recompare`) on this node and all
    /// descendants.
    ///
    /// These fields are wire-visible so the plugin ABI can move them across
    /// process-ready boundaries, but they are not meaningful outside a live
    /// session and must be stripped before writing changeset output intended
    /// for users (JSON files, renderer output, Python return values).
    pub fn strip_transient(&mut self) {
        self.source_items = None;
        self.diagnostics.clear();
        self.artifacts.clear();
        self.pending_recompare = None;
        for child in &mut self.children {
            child.strip_transient();
        }
    }
}

/// Renderer-visible, bounded evidence attached to a diff node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DetailBlock {
    /// Stable within this node, for anchors and extract selection.
    pub id: String,
    /// Open, namespaced kind such as `binoc.tabular.cell_changes.v1`.
    pub kind: String,
    /// Short renderer-facing label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Total matching items if known, including omitted examples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_count: Option<u64>,
    /// Captured examples for inline rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<DetailExample>,
    /// Named extract aspects for exhaustive retrieval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extract: Vec<ExtractHint>,
    /// Whether the producer truncated capture before exhausting candidates.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl DetailBlock {
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            label: None,
            total_count: None,
            examples: Vec::new(),
            extract: Vec::new(),
            truncated: false,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_total_count(mut self, total_count: u64) -> Self {
        self.total_count = Some(total_count);
        self
    }

    pub fn with_example(mut self, example: DetailExample) -> Self {
        self.examples.push(example);
        self
    }

    pub fn with_extract_hint(mut self, hint: ExtractHint) -> Self {
        self.extract.push(hint);
        self
    }
}

/// One bounded example inside a detail block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DetailExample {
    /// Structured locator such as row/column, line range, or key path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locator: BTreeMap<String, serde_json::Value>,
    /// Value before the change, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<ValuePreview>,
    /// Value after the change, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<ValuePreview>,
    /// Domain-specific structured context.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl DetailExample {
    pub fn new() -> Self {
        Self {
            locator: BTreeMap::new(),
            before: None,
            after: None,
            fields: BTreeMap::new(),
        }
    }
}

impl Default for DetailExample {
    fn default() -> Self {
        Self::new()
    }
}

/// A bounded preview of one value in a detail example.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ValuePreview {
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Pointer to an extract aspect that can return exhaustive content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExtractHint {
    /// Aspect name accepted by `binoc extract`.
    pub aspect: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ExtractHint {
    pub fn new(aspect: impl Into<String>) -> Self {
        Self {
            aspect: aspect.into(),
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A structured description of how to get from one snapshot to the next.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Changeset {
    pub from_snapshot: String,
    pub to_snapshot: String,
    pub root: Option<DiffNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl Changeset {
    pub fn new(from: impl Into<String>, to: impl Into<String>, root: Option<DiffNode>) -> Self {
        Self {
            from_snapshot: from.into(),
            to_snapshot: to.into(),
            root,
            metadata: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.root.as_ref().map_or(0, |r| r.node_count())
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic.normalized());
    }

    pub fn hoist_node_diagnostics(&mut self) {
        if let Some(root) = self.root.as_mut() {
            root.drain_diagnostics_into(&mut self.diagnostics);
        }
    }

    pub fn dedupe_and_cap_diagnostics(&mut self, max_diagnostics: usize) {
        let mut seen: BTreeSet<(String, Option<String>)> = BTreeSet::new();
        let mut deduped = Vec::with_capacity(self.diagnostics.len().min(max_diagnostics));

        for diagnostic in self.diagnostics.drain(..).map(Diagnostic::normalized) {
            let key = (diagnostic.code.clone(), diagnostic.location.clone());
            if seen.insert(key) {
                deduped.push(diagnostic);
                if deduped.len() >= max_diagnostics {
                    break;
                }
            }
        }

        self.diagnostics = deduped;
    }

    /// Recursively clear session-scoped transient fields on the root and all
    /// descendants. See [`DiffNode::strip_transient`].
    pub fn strip_transient(&mut self) {
        if let Some(root) = self.root.as_mut() {
            root.strip_transient();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_node_new_creates_node_with_correct_fields() {
        let node = DiffNode::new("modify", "file", "path/to/file.csv");
        assert_eq!(node.action, "modify");
        assert_eq!(node.item_type, "file");
        assert_eq!(node.path, "path/to/file.csv");
        assert!(node.source_path.is_none());
        assert!(node.tags.is_empty());
        assert!(node.children.is_empty());
        assert!(node.details.is_empty());
        assert!(node.detail_blocks.is_empty());
        assert!(node.annotations.is_empty());
    }

    #[test]
    fn diff_node_builder_methods_chain_correctly() {
        let child = DiffNode::new("add", "file", "child.txt");
        let node = DiffNode::new("modify", "directory", "dir")
            .with_tag("binoc.column-reorder")
            .with_tag("binoc.whitespace")
            .with_detail("lines_changed", serde_json::json!(42))
            .with_annotation_from("binoc", "note", serde_json::json!("check distribution"))
            .with_children(vec![child])
            .with_source_path("old/dir");

        assert_eq!(node.tags.len(), 2);
        assert!(node.tags.contains("binoc.column-reorder"));
        assert!(node.tags.contains("binoc.whitespace"));
        assert_eq!(
            node.details.get("lines_changed"),
            Some(&serde_json::json!(42))
        );
        assert_eq!(
            node.binoc_annotation("note")
                .map(|annotation| &annotation.value),
            Some(&serde_json::json!("check distribution"))
        );
        assert!(node.detail_blocks.is_empty());
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].path, "child.txt");
        assert_eq!(node.source_path.as_deref(), Some("old/dir"));
    }

    #[test]
    fn annotations_are_namespaced_and_replace_by_package_key() {
        let mut node = DiffNode::new("modify", "file", "data.csv");
        node.annotate_from("binoc", "note", serde_json::json!("first"));
        node.annotate_from("binoc", "note", serde_json::json!("second"));
        node.annotate_from("example.plugin", "note", serde_json::json!("external"));

        assert_eq!(node.annotations.len(), 2);
        assert_eq!(
            node.binoc_annotation("note")
                .map(|annotation| &annotation.value),
            Some(&serde_json::json!("second"))
        );
        assert_eq!(
            node.annotation("example.plugin", "note")
                .map(|annotation| &annotation.value),
            Some(&serde_json::json!("external"))
        );
    }

    #[test]
    fn node_count_leaf_returns_one() {
        let node = DiffNode::new("add", "file", "file.txt");
        assert_eq!(node.node_count(), 1);
    }

    #[test]
    fn node_count_tree_returns_correct_total() {
        let node = DiffNode::new("modify", "dir", "dir").with_children(vec![
            DiffNode::new("add", "file", "a.txt"),
            DiffNode::new("modify", "dir", "sub").with_children(vec![DiffNode::new(
                "remove",
                "file",
                "sub/b.txt",
            )]),
        ]);
        assert_eq!(node.node_count(), 4);
    }

    #[test]
    fn all_tags_collects_from_entire_subtree() {
        let node = DiffNode::new("modify", "dir", "dir")
            .with_tag("root-tag")
            .with_children(vec![
                DiffNode::new("add", "file", "a").with_tag("child-tag"),
                DiffNode::new("remove", "file", "b")
                    .with_children(vec![
                        DiffNode::new("modify", "file", "c").with_tag("grandchild-tag")
                    ]),
            ]);
        let tags = node.all_tags();
        assert_eq!(tags.len(), 3);
        assert!(tags.contains("root-tag"));
        assert!(tags.contains("child-tag"));
        assert!(tags.contains("grandchild-tag"));
    }

    #[test]
    fn serde_round_trip_preserves_equality() {
        let node = DiffNode::new("move", "file", "new/path.csv")
            .with_tag("binoc.move")
            .with_detail("distance", serde_json::json!(10))
            .with_detail_block(
                DetailBlock::new("changed_cells", "binoc.tabular.cell_changes.v1")
                    .with_label("Changed cells")
                    .with_total_count(1)
                    .with_example(DetailExample {
                        locator: BTreeMap::from([
                            ("row".into(), serde_json::json!(1)),
                            ("column".into(), serde_json::json!("status")),
                        ]),
                        before: Some(ValuePreview {
                            value: serde_json::json!("draft"),
                            media_type: Some("text/plain".into()),
                            truncated: false,
                        }),
                        after: Some(ValuePreview {
                            value: serde_json::json!("published"),
                            media_type: Some("text/plain".into()),
                            truncated: false,
                        }),
                        fields: BTreeMap::new(),
                    })
                    .with_extract_hint(
                        ExtractHint::new("cells_changed").with_label("All changed cells"),
                    ),
            )
            .with_source_path("old/path.csv");
        let json = serde_json::to_string(&node).unwrap();
        let restored: DiffNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node.action, restored.action);
        assert_eq!(node.item_type, restored.item_type);
        assert_eq!(node.path, restored.path);
        assert_eq!(node.source_path, restored.source_path);
        assert_eq!(node.tags, restored.tags);
        assert_eq!(node.details, restored.details);
        assert_eq!(restored.detail_blocks.len(), 1);
        assert_eq!(restored.detail_blocks[0].examples.len(), 1);
    }

    #[test]
    fn changeset_construction_and_node_count() {
        let root = DiffNode::new("modify", "dir", "root").with_children(vec![
            DiffNode::new("add", "file", "root/a.txt"),
            DiffNode::new("remove", "file", "root/b.txt"),
        ]);
        let changeset = Changeset::new("v1", "v2", Some(root));
        assert_eq!(changeset.from_snapshot, "v1");
        assert_eq!(changeset.to_snapshot, "v2");
        assert_eq!(changeset.node_count(), 3);
    }

    #[test]
    fn transient_fields_round_trip_through_serde() {
        // Session-scoped transient fields (`source_items`, `artifacts`,
        // `diagnostics`) are wire-visible so the plugin ABI can carry them
        // across a (potentially process-isolated) boundary.
        use crate::types::{
            ArtifactDescriptor, ArtifactFormat, ArtifactSubject, ItemPair, ItemRef,
        };

        let artifact = ArtifactDescriptor {
            format: ArtifactFormat::new("binoc", "tabular", 1),
            subject: ArtifactSubject::Pair,
            producer: "binoc.csv".into(),
            handle: "cache/tabular-abc123".into(),
        };
        let source_items = ItemPair::both(
            ItemRef {
                logical_path: "data.csv".into(),
                is_dir: false,
                content_hash: None,
                size: None,
                media_type: None,
                handle: "/tmp/a/data.csv".into(),
            },
            ItemRef {
                logical_path: "data.csv".into(),
                is_dir: false,
                content_hash: None,
                size: None,
                media_type: None,
                handle: "/tmp/b/data.csv".into(),
            },
        );
        let child = DiffNode::new("modify", "tabular", "dir/data.csv")
            .with_artifact(artifact.clone())
            .with_source_items(source_items.clone())
            .with_diagnostic(Diagnostic::suggestion("binoc.demo", "Try a richer plugin"));
        let root = DiffNode::new("modify", "directory", "dir").with_children(vec![child]);

        let json = serde_json::to_string(&root).unwrap();
        let restored: DiffNode = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.children.len(), 1);
        let restored_child = &restored.children[0];
        assert_eq!(restored_child.artifacts.len(), 1, "child artifact missing");
        assert_eq!(restored_child.artifacts[0].handle, artifact.handle);
        assert!(
            restored_child.source_items.is_some(),
            "child source_items missing"
        );
        assert_eq!(restored_child.diagnostics.len(), 1);
    }

    #[test]
    fn hoisted_diagnostics_are_deduped_and_capped() {
        let mut root = DiffNode::new("modify", "directory", "");
        root.push_diagnostic(Diagnostic::suggestion(
            "binoc.binary-fallback",
            "Try a plugin",
        ));
        root.push_diagnostic(Diagnostic::suggestion(
            "binoc.binary-fallback",
            "Try a plugin",
        ));
        root.children = vec![
            DiffNode::new("modify", "file", "a.bin").with_diagnostic(Diagnostic::suggestion(
                "binoc.binary-fallback",
                "Try a plugin",
            )),
            DiffNode::new("modify", "file", "b.bin")
                .with_diagnostic(Diagnostic::warning("binoc.other", "Other issue")),
        ];

        let mut changeset = Changeset::new("a", "b", Some(root));
        changeset.hoist_node_diagnostics();
        changeset.dedupe_and_cap_diagnostics(2);

        assert_eq!(changeset.diagnostics.len(), 2);
        assert_eq!(changeset.diagnostics[0].code, "binoc.binary-fallback");
        assert_eq!(changeset.diagnostics[0].location, None);
        assert_eq!(changeset.diagnostics[1].location.as_deref(), Some("a.bin"));
    }

    #[test]
    fn strip_transient_clears_every_descendant() {
        use crate::types::{
            ArtifactDescriptor, ArtifactFormat, ArtifactSubject, ItemPair, ItemRef,
        };
        let artifact = ArtifactDescriptor {
            format: ArtifactFormat::new("binoc", "tabular", 1),
            subject: ArtifactSubject::Pair,
            producer: "binoc.csv".into(),
            handle: "h".into(),
        };
        let pair = ItemPair::both(
            ItemRef {
                logical_path: "x".into(),
                is_dir: false,
                content_hash: None,
                size: None,
                media_type: None,
                handle: "/tmp/a".into(),
            },
            ItemRef {
                logical_path: "x".into(),
                is_dir: false,
                content_hash: None,
                size: None,
                media_type: None,
                handle: "/tmp/b".into(),
            },
        );
        let mut grandchild = DiffNode::new("modify", "tabular", "a/b/c.csv")
            .with_artifact(artifact)
            .with_diagnostic(Diagnostic::warning("binoc.test", "test"));
        grandchild.pending_recompare = Some(pair);
        let child = DiffNode::new("modify", "directory", "a/b").with_children(vec![grandchild]);
        let mut root = DiffNode::new("modify", "directory", "a").with_children(vec![child]);
        root.strip_transient();
        fn all_empty(n: &DiffNode) -> bool {
            n.artifacts.is_empty()
                && n.diagnostics.is_empty()
                && n.source_items.is_none()
                && n.pending_recompare.is_none()
                && n.children.iter().all(all_empty)
        }
        assert!(all_empty(&root));
    }

    #[test]
    fn changeset_node_count_none_root() {
        let changeset = Changeset::new("v1", "v2", None);
        assert_eq!(changeset.node_count(), 0);
    }
}
