use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A node in the diff tree — the central data structure of the system.
/// Every comparator emits it, every transformer rewrites it, and serializers
/// or bindings read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffNode {
    /// Open enum: "add", "remove", "modify", "move", "reorder",
    /// "schema_change", etc. Plugins may define new kinds.
    pub kind: String,

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
    /// Set by comparator or transformer; used by outputters for narrative rendering.
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

    /// Transformer-added metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, serde_json::Value>,

    /// Which comparator produced this node (provenance for extract chain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparator: Option<String>,

    /// Transformers that modified this node, in order (provenance for extract chain).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transformed_by: Vec<String>,
}

impl DiffNode {
    pub fn new(
        kind: impl Into<String>,
        item_type: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            item_type: item_type.into(),
            path: path.into(),
            source_path: None,
            summary: None,
            tags: BTreeSet::new(),
            children: Vec::new(),
            details: BTreeMap::new(),
            annotations: BTreeMap::new(),
            comparator: None,
            transformed_by: Vec::new(),
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

    pub fn with_source_path(mut self, source: impl Into<String>) -> Self {
        self.source_path = Some(source.into());
        self
    }

    /// Recursively count all nodes in this subtree (including self).
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }

    /// Collect all tags from this node and all descendants.
    pub fn all_tags(&self) -> BTreeSet<String> {
        let mut tags = self.tags.clone();
        for child in &self.children {
            tags.extend(child.all_tags());
        }
        tags
    }
}

/// A structured description of how to get from one snapshot to the next.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub from_snapshot: String,
    pub to_snapshot: String,
    pub root: Option<DiffNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl Migration {
    pub fn new(from: impl Into<String>, to: impl Into<String>, root: Option<DiffNode>) -> Self {
        Self {
            from_snapshot: from.into(),
            to_snapshot: to.into(),
            root,
            metadata: BTreeMap::new(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.root.as_ref().map_or(0, |r| r.node_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_node_new_creates_node_with_correct_fields() {
        let node = DiffNode::new("modify", "file", "path/to/file.csv");
        assert_eq!(node.kind, "modify");
        assert_eq!(node.item_type, "file");
        assert_eq!(node.path, "path/to/file.csv");
        assert!(node.source_path.is_none());
        assert!(node.tags.is_empty());
        assert!(node.children.is_empty());
        assert!(node.details.is_empty());
        assert!(node.annotations.is_empty());
    }

    #[test]
    fn diff_node_builder_methods_chain_correctly() {
        let child = DiffNode::new("add", "file", "child.txt");
        let node = DiffNode::new("modify", "directory", "dir")
            .with_tag("binoc.column-reorder")
            .with_tag("binoc.whitespace")
            .with_detail("lines_changed", serde_json::json!(42))
            .with_children(vec![child])
            .with_source_path("old/dir");

        assert_eq!(node.tags.len(), 2);
        assert!(node.tags.contains("binoc.column-reorder"));
        assert!(node.tags.contains("binoc.whitespace"));
        assert_eq!(
            node.details.get("lines_changed"),
            Some(&serde_json::json!(42))
        );
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].path, "child.txt");
        assert_eq!(node.source_path.as_deref(), Some("old/dir"));
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
            .with_source_path("old/path.csv");
        let json = serde_json::to_string(&node).unwrap();
        let restored: DiffNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node.kind, restored.kind);
        assert_eq!(node.item_type, restored.item_type);
        assert_eq!(node.path, restored.path);
        assert_eq!(node.source_path, restored.source_path);
        assert_eq!(node.tags, restored.tags);
        assert_eq!(node.details, restored.details);
    }

    #[test]
    fn migration_construction_and_node_count() {
        let root = DiffNode::new("modify", "dir", "root").with_children(vec![
            DiffNode::new("add", "file", "root/a.txt"),
            DiffNode::new("remove", "file", "root/b.txt"),
        ]);
        let migration = Migration::new("v1", "v2", Some(root));
        assert_eq!(migration.from_snapshot, "v1");
        assert_eq!(migration.to_snapshot, "v2");
        assert_eq!(migration.node_count(), 3);
    }

    #[test]
    fn migration_node_count_none_root() {
        let migration = Migration::new("v1", "v2", None);
        assert_eq!(migration.node_count(), 0);
    }
}
