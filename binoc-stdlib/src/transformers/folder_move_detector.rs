//! Folder-move rollup: when every leaf under a destination directory
//! is a `move` (or `copy`) from a consistent source directory, collapse
//! the pair into a single folder-level `move` (or `copy`) node.
//!
//! Runs at the root, after [`super::correlation_detector::CorrelationDetector`].
//!
//! ```json
//! { "threshold": 1.0 }
//! ```
//!
//! Default threshold is `1.0` (strict: every leaf descendant of both
//! sides must be accounted for as a matching move or copy). Lower
//! thresholds (0.0–1.0) are accepted by the config parser but treated
//! as strict for v1 — partial rollup semantics are deliberately left
//! for future work, see `docs/adr/transformer_scope_yagni.md`.

use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;

pub struct FolderMoveDetector;

#[derive(Debug, Clone)]
struct Config {
    threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self { threshold: 1.0 }
    }
}

impl Config {
    fn from_value(v: &serde_json::Value) -> Self {
        let mut out = Self::default();
        if let Some(t) = v.get("threshold").and_then(|x| x.as_f64()) {
            out.threshold = t.clamp(0.0, 1.0);
        }
        out
    }
}

impl Transformer for FolderMoveDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.folder_move_detector")
            .with_node_shape(NodeShapeFilter::Root)
    }

    fn transform(
        &self,
        node: DiffNode,
        _data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let cfg = Config::from_value(config);

        // Collect rollups top-down; don't descend into a container once
        // it's been selected for rollup (outermost wins).
        let mut rollups: BTreeMap<String, Rollup> = BTreeMap::new();
        collect_rollups(&node, cfg.threshold, &mut rollups);

        if rollups.is_empty() {
            return TransformResult::Unchanged;
        }

        // Source containers (the origin side) get dropped entirely.
        let source_paths: BTreeSet<String> =
            rollups.values().map(|r| r.source_path.clone()).collect();

        let rewritten = apply_rollups(node, &rollups, &source_paths);
        TransformResult::Replace(Box::new(rewritten))
    }
}

#[derive(Debug, Clone)]
struct Rollup {
    dst_path: String,
    source_path: String,
    kind: RollupKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RollupKind {
    Move,
    Copy,
}

/// Top-down walk: if this container rolls up, record it and stop.
/// Otherwise recurse.
fn collect_rollups(node: &DiffNode, threshold: f64, out: &mut BTreeMap<String, Rollup>) {
    if let Some(r) = try_rollup(node, threshold) {
        out.insert(r.dst_path.clone(), r);
        return;
    }
    for child in &node.children {
        collect_rollups(child, threshold, out);
    }
}

/// Does this container roll up into a single folder move / copy?
/// Returns the rollup info if yes.
fn try_rollup(container: &DiffNode, threshold: f64) -> Option<Rollup> {
    // Only consider directory-shaped destination containers with
    // children. We don't roll up archives or leaves.
    if container.item_type != "directory" || container.children.is_empty() {
        return None;
    }
    if !matches!(container.action.as_str(), "add" | "modify") {
        return None;
    }

    // Collect every leaf descendant. A "leaf" here means a node with no
    // children (the tree beneath `container`).
    let mut leaves: Vec<&DiffNode> = Vec::new();
    gather_leaves(container, &mut leaves);
    if leaves.is_empty() {
        return None;
    }

    // All leaves must be moves (or all copies) with a source_path.
    let kind = infer_kind(&leaves)?;
    let tag = match kind {
        RollupKind::Move => "binoc.move",
        RollupKind::Copy => "binoc.copy",
    };

    let mut matched = 0usize;
    let mut source_prefix: Option<String> = None;

    for leaf in &leaves {
        if !leaf.tags.contains(tag) {
            continue;
        }
        let Some(src) = leaf.source_path.as_deref() else {
            continue;
        };
        let Some(rel) = strip_prefix_as_child(&leaf.path, &container.path) else {
            continue;
        };
        // Expect: src ends with "/" + rel (or equals rel if the source
        // container is the root).
        let Some(prefix) = strip_suffix_as_parent(src, rel) else {
            continue;
        };
        match &source_prefix {
            None => source_prefix = Some(prefix.to_string()),
            Some(existing) if existing == prefix => {}
            _ => return None, // inconsistent source prefixes
        }
        matched += 1;
    }

    let fraction = matched as f64 / leaves.len() as f64;
    if fraction < threshold {
        return None;
    }
    // V1: only fully strict rollups actually fire. Partial is gated out.
    if matched != leaves.len() {
        return None;
    }

    let source_path = source_prefix?;
    // Destination and source must not be the same path (would indicate
    // a comparator bug or a move-in-place, neither of which should fold).
    if source_path == container.path {
        return None;
    }
    Some(Rollup {
        dst_path: container.path.clone(),
        source_path,
        kind,
    })
}

fn gather_leaves<'a>(node: &'a DiffNode, out: &mut Vec<&'a DiffNode>) {
    if node.children.is_empty() {
        out.push(node);
        return;
    }
    for c in &node.children {
        gather_leaves(c, out);
    }
}

fn infer_kind(leaves: &[&DiffNode]) -> Option<RollupKind> {
    let mut has_move = false;
    let mut has_copy = false;
    for leaf in leaves {
        if leaf.tags.contains("binoc.move") {
            has_move = true;
        } else if leaf.tags.contains("binoc.copy") {
            has_copy = true;
        } else {
            return None; // any non-move/copy leaf disqualifies
        }
    }
    match (has_move, has_copy) {
        (true, false) => Some(RollupKind::Move),
        (false, true) => Some(RollupKind::Copy),
        _ => None, // mixed or empty
    }
}

/// Return the suffix of `path` relative to `parent` (with no leading slash).
/// `parent=""` means everything under root. Returns `None` if `path` is not
/// actually under `parent`.
fn strip_prefix_as_child<'a>(path: &'a str, parent: &str) -> Option<&'a str> {
    if parent.is_empty() {
        return Some(path);
    }
    if let Some(rest) = path.strip_prefix(parent) {
        rest.strip_prefix('/')
    } else {
        None
    }
}

/// Return the prefix of `path` with `suffix` (and its separating `/`)
/// removed. `suffix` is expected to equal the tail of `path`; returns
/// `None` if it doesn't.
fn strip_suffix_as_parent<'a>(path: &'a str, suffix: &str) -> Option<&'a str> {
    if path == suffix {
        return Some("");
    }
    let rest = path.strip_suffix(suffix)?;
    rest.strip_suffix('/')
}

/// Rewrite the tree per collected rollups:
/// - At each destination container path: replace with a bare folder-level
///   move/copy node (no children).
/// - Delete the source containers entirely from wherever they sit.
fn apply_rollups(
    node: DiffNode,
    rollups: &BTreeMap<String, Rollup>,
    source_paths: &BTreeSet<String>,
) -> DiffNode {
    // Rewrite children first (bottom-up for removal of source containers),
    // but replace at this node if it's a destination.
    let mut new_children: Vec<DiffNode> = node
        .children
        .into_iter()
        .filter(|c| !source_paths.contains(&c.path))
        .map(|c| apply_rollups(c, rollups, source_paths))
        .collect();

    if let Some(rollup) = rollups.get(&node.path) {
        let (action, tag, summary) = match rollup.kind {
            RollupKind::Move => (
                "move",
                "binoc.move",
                format!("Folder moved from {}", display_name(&rollup.source_path)),
            ),
            RollupKind::Copy => (
                "copy",
                "binoc.copy",
                format!("Folder copied from {}", display_name(&rollup.source_path)),
            ),
        };
        let mut folded = DiffNode::new(action, &node.item_type, &node.path)
            .with_source_path(&rollup.source_path)
            .with_summary(summary)
            .with_tag(tag)
            .with_tag("binoc.folder-move");
        // Preserve comparator provenance so extract chains still work.
        folded.comparator = node.comparator.clone();
        folded.transformed_by = node.transformed_by.clone();
        return folded;
    }

    new_children.sort_by(|a, b| a.path.cmp(&b.path));
    DiffNode {
        children: new_children,
        ..node
    }
}

fn display_name(path: &str) -> String {
    if path.is_empty() {
        "<root>".to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_strict() {
        let cfg = Config::from_value(&serde_json::Value::Null);
        assert_eq!(cfg.threshold, 1.0);
    }

    #[test]
    fn config_reads_threshold() {
        let cfg = Config::from_value(&serde_json::json!({ "threshold": 0.5 }));
        assert_eq!(cfg.threshold, 0.5);
    }

    #[test]
    fn config_clamps_threshold() {
        let cfg = Config::from_value(&serde_json::json!({ "threshold": 2.0 }));
        assert_eq!(cfg.threshold, 1.0);
        let cfg = Config::from_value(&serde_json::json!({ "threshold": -1.0 }));
        assert_eq!(cfg.threshold, 0.0);
    }

    #[test]
    fn strip_prefix_as_child_root() {
        assert_eq!(strip_prefix_as_child("a/b", ""), Some("a/b"));
    }

    #[test]
    fn strip_prefix_as_child_nested() {
        assert_eq!(strip_prefix_as_child("docs/a.txt", "docs"), Some("a.txt"));
        assert_eq!(strip_prefix_as_child("docs/a.txt", "other"), None);
    }

    #[test]
    fn strip_suffix_as_parent_yields_source() {
        assert_eq!(strip_suffix_as_parent("docs/a.txt", "a.txt"), Some("docs"));
    }

    #[test]
    fn strip_suffix_as_parent_root_when_equal() {
        assert_eq!(strip_suffix_as_parent("a.txt", "a.txt"), Some(""));
    }
}
