//! Folder-move rollup: when most leaves under a destination directory
//! are `move` (or `copy`) descendants from a consistent source
//! directory, roll the pair up into one folder-level `move` (or `copy`)
//! while keeping remainder changes beneath it.
//!
//! Runs at the root, after [`super::correlation_detector::CorrelationDetector`].
//!
//! ```json
//! { "threshold": 0.8 }
//! ```
//!
//! Default threshold is `0.8`: at least 80% of destination leaves must
//! be consistently sourced moves/copies of the same dominant kind.
//! Modified moves (`binoc.move.modified`) and renamed children count as
//! evidence; the rolled-up node notes how many files were modified and
//! keeps renamed or modified children beneath it as detail. The
//! remainder stays attached under the rolled-up destination node as
//! ordinary adds/removes/modifies.

use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;

pub struct FolderMoveDetector;

#[derive(Debug, Clone)]
struct Config {
    threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self { threshold: 0.8 }
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
            .with_emits_tags(vec![
                "binoc.move".into(),
                "binoc.copy".into(),
                "binoc.folder-move".into(),
            ])
            .with_emits_actions(vec!["move".into(), "copy".into(), "modify".into()])
            .with_emits_item_types(vec![])
            .with_publishes_artifacts(vec![])
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

        let mut source_index = BTreeMap::new();
        index_nodes_by_path(&node, &mut source_index);

        let rewritten = apply_rollups(node, &rollups, &source_paths, &source_index);
        TransformResult::Replace(Box::new(rewritten))
    }
}

#[derive(Debug, Clone)]
struct Rollup {
    dst_path: String,
    source_path: String,
    kind: RollupKind,
    matched_rel_paths: BTreeSet<String>,
    modified_count: usize,
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

    let mut candidates: Vec<LeafMatch> = Vec::new();
    let mut source_prefix: Option<String> = None;

    for leaf in &leaves {
        let Some(candidate) = classify_move_leaf(leaf, &container.path) else {
            continue;
        };
        match &source_prefix {
            None => source_prefix = Some(candidate.source_prefix.clone()),
            Some(existing) if *existing == candidate.source_prefix => {}
            _ => return None, // inconsistent source prefixes
        }
        candidates.push(candidate);
    }

    let kind = infer_kind(&candidates)?;
    let matched_rel_paths: BTreeSet<String> = candidates
        .iter()
        .filter(|c| c.kind == kind)
        .map(|c| c.rel_path.clone())
        .collect();
    let matched = matched_rel_paths.len();
    let fraction = matched as f64 / leaves.len() as f64;
    if fraction < threshold {
        return None;
    }
    let modified_count = candidates
        .iter()
        .filter(|c| c.kind == kind && c.modified)
        .count();

    let source_path = source_prefix?;
    // Destination and source must not be the same path (would indicate
    // a comparator bug or a move-in-place, neither of which should fold).
    if source_path == container.path {
        return None;
    }
    // A non-root source that strictly contains the destination cannot be
    // a container move: those leaves migrated into a child container, and
    // folding would delete the destination along with its source ancestor.
    // Root ("") stays allowed — it is never deleted, so "everything moved
    // into a subfolder" still folds.
    if !source_path.is_empty() && strip_prefix_as_child(&container.path, &source_path).is_some() {
        return None;
    }
    Some(Rollup {
        dst_path: container.path.clone(),
        source_path,
        kind,
        matched_rel_paths,
        modified_count,
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

fn infer_kind(candidates: &[LeafMatch]) -> Option<RollupKind> {
    let move_count = candidates
        .iter()
        .filter(|c| c.kind == RollupKind::Move)
        .count();
    let copy_count = candidates
        .iter()
        .filter(|c| c.kind == RollupKind::Copy)
        .count();
    match move_count.cmp(&copy_count) {
        std::cmp::Ordering::Greater if move_count > 0 => Some(RollupKind::Move),
        std::cmp::Ordering::Less if copy_count > 0 => Some(RollupKind::Copy),
        _ => None,
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

#[derive(Debug, Clone)]
struct LeafMatch {
    rel_path: String,
    source_path: String,
    source_prefix: String,
    kind: RollupKind,
    modified: bool,
}

impl LeafMatch {
    /// A leaf is fully implied by its rollup when its source is exactly
    /// the rollup source plus its destination-relative path — i.e. it
    /// was neither renamed nor modified, so dropping it loses nothing.
    fn implied_source(&self) -> String {
        if self.source_prefix.is_empty() {
            self.rel_path.clone()
        } else {
            format!("{}/{}", self.source_prefix, self.rel_path)
        }
    }
}

fn classify_move_leaf(leaf: &DiffNode, dst_prefix: &str) -> Option<LeafMatch> {
    if !leaf.children.is_empty() {
        return None;
    }

    let kind = if leaf.action == "move" && leaf.tags.contains("binoc.move") {
        RollupKind::Move
    } else if leaf.action == "copy" && leaf.tags.contains("binoc.copy") {
        RollupKind::Copy
    } else {
        return None;
    };

    let src = leaf.source_path.as_deref()?;
    let rel = strip_prefix_as_child(&leaf.path, dst_prefix)?;
    let prefix = derive_source_prefix(src, rel)?;
    Some(LeafMatch {
        rel_path: rel.to_string(),
        source_path: src.to_string(),
        source_prefix: prefix.to_string(),
        kind,
        modified: has_modification_detail(leaf),
    })
}

/// Infer the source container a moved leaf came from. The leaf's
/// destination-relative directory structure must match the tail of the
/// source's directory structure, but the basename is free to differ —
/// a child that was renamed as part of the move still counts.
fn derive_source_prefix<'a>(src: &'a str, rel: &str) -> Option<&'a str> {
    let src_parent = parent_dir_of(src);
    let rel_parent = parent_dir_of(rel);
    if rel_parent.is_empty() {
        Some(src_parent)
    } else {
        strip_suffix_as_parent(src_parent, rel_parent)
    }
}

/// Parent directory of `path` (`""` for a bare filename).
fn parent_dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn has_modification_detail(node: &DiffNode) -> bool {
    node.tags.contains("binoc.move.modified")
        || node.tags.contains("binoc.copy.modified")
        || node.tags.contains("binoc.content-changed")
        || node.binoc_annotation("content_summary").is_some()
        || node.binoc_annotation("tabular_summary").is_some()
        || !node.children.is_empty()
}

fn index_nodes_by_path(node: &DiffNode, out: &mut BTreeMap<String, DiffNode>) {
    out.insert(node.path.clone(), node.clone());
    for child in &node.children {
        index_nodes_by_path(child, out);
    }
}

/// Rewrite the tree per collected rollups:
/// - At each destination container path: replace with a bare folder-level
///   move/copy node (no children).
/// - Delete the source containers entirely from wherever they sit.
fn apply_rollups(
    node: DiffNode,
    rollups: &BTreeMap<String, Rollup>,
    source_paths: &BTreeSet<String>,
    source_index: &BTreeMap<String, DiffNode>,
) -> DiffNode {
    if let Some(rollup) = rollups.get(&node.path) {
        let (action, tag, verb) = match rollup.kind {
            RollupKind::Move => ("move", "binoc.move", "moved"),
            RollupKind::Copy => ("copy", "binoc.copy", "copied"),
        };
        let mut summary = Summary::new()
            .text(format!("Folder {verb} from "))
            .path(display_name(&rollup.source_path), Side::From);
        if rollup.modified_count > 0 {
            summary = summary
                .text(" (")
                .count(rollup.modified_count as u64, "file")
                .text(" modified within)");
        }
        let mut children = rewrite_destination_children(node.children, rollup, source_index);
        if let Some(source_node) = source_index.get(&rollup.source_path) {
            children.extend(relocate_source_remainders(source_node, rollup));
        }
        children = merge_same_path_nodes(children);
        let mut folded = DiffNode::new(action, &node.item_type, &node.path)
            .with_source_path(&rollup.source_path)
            .with_summary(summary)
            .with_tag(tag)
            .with_tag("binoc.folder-move");
        folded.children = children;
        // Preserve comparator provenance so extract chains still work.
        folded.comparator = node.comparator.clone();
        folded.transformed_by = node.transformed_by.clone();
        return folded;
    }

    // Rewrite children first (bottom-up for removal of source containers).
    let mut new_children: Vec<DiffNode> = node
        .children
        .into_iter()
        .filter(|c| !source_paths.contains(&c.path))
        .map(|c| apply_rollups(c, rollups, source_paths, source_index))
        .collect();

    new_children = merge_same_path_nodes(new_children);
    DiffNode {
        children: new_children,
        ..node
    }
}

fn rewrite_destination_children(
    children: Vec<DiffNode>,
    rollup: &Rollup,
    source_index: &BTreeMap<String, DiffNode>,
) -> Vec<DiffNode> {
    let mut out: Vec<DiffNode> = Vec::new();
    for child in children {
        if child.path == rollup.dst_path {
            // A same-path shell around the rolled-up container (e.g. the
            // directory node a zip comparator expands into shares the
            // archive's path). Hoist its children so remainder and
            // retained detail survive.
            out.extend(rewrite_destination_children(
                child.children,
                rollup,
                source_index,
            ));
        } else if let Some(rewritten) = rewrite_destination_node(child, rollup, source_index) {
            out.push(rewritten);
        }
    }
    merge_same_path_nodes(out)
}

fn rewrite_destination_node(
    node: DiffNode,
    rollup: &Rollup,
    source_index: &BTreeMap<String, DiffNode>,
) -> Option<DiffNode> {
    let rel = strip_prefix_as_child(&node.path, &rollup.dst_path)?.to_string();
    if node.children.is_empty() {
        return rewrite_destination_leaf(node, rollup);
    }

    let mut new_children = rewrite_destination_children(node.children, rollup, source_index);
    new_children = merge_same_path_nodes(new_children);

    if new_children.is_empty() && node.summary.is_none() && node.tags.is_empty() {
        return None;
    }

    Some(normalize_rollup_remainder_container(
        DiffNode {
            children: new_children,
            ..node
        },
        &rel,
        rollup,
        source_index,
    ))
}

/// Rewrite a leaf beneath a rolled-up destination. Matched leaves whose
/// move is fully implied by the folder move (same relative path, not
/// modified) carry no extra information and are dropped; matched leaves
/// that were renamed or modified stay as detail beneath the folded node.
/// Everything else is a remainder.
fn rewrite_destination_leaf(node: DiffNode, rollup: &Rollup) -> Option<DiffNode> {
    if let Some(candidate) = classify_move_leaf(&node, &rollup.dst_path) {
        if candidate.kind == rollup.kind && rollup.matched_rel_paths.contains(&candidate.rel_path) {
            if !candidate.modified && candidate.source_path == candidate.implied_source() {
                return None;
            }
            return Some(node);
        }
    }
    Some(maybe_demote_move_like_remainder(node))
}

fn normalize_rollup_remainder_container(
    mut node: DiffNode,
    rel: &str,
    rollup: &Rollup,
    source_index: &BTreeMap<String, DiffNode>,
) -> DiffNode {
    if node.action != "add" {
        return node;
    }

    let source_path = if rollup.source_path.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        rollup.source_path.clone()
    } else {
        format!("{}/{}", rollup.source_path, rel)
    };

    let has_source_container = source_index
        .get(&source_path)
        .is_some_and(|source| source.item_type == node.item_type);
    let has_matched_descendant = rollup
        .matched_rel_paths
        .iter()
        .any(|matched| matched == rel || matched.starts_with(&format!("{rel}/")));

    if has_source_container || has_matched_descendant {
        node.action = "modify".to_string();
    }

    node
}

fn maybe_demote_move_like_remainder(mut node: DiffNode) -> DiffNode {
    if matches!(node.action.as_str(), "move" | "copy") {
        let detail: Option<Summary> = node
            .binoc_annotation("tabular_summary")
            .and_then(Annotation::as_str)
            .or_else(|| {
                node.binoc_annotation("content_summary")
                    .and_then(Annotation::as_str)
            })
            .map(Summary::from);
        node.action = "modify".to_string();
        node.source_path = None;
        node.tags.remove("binoc.move");
        node.tags.remove("binoc.copy");
        node.tags.remove("binoc.move.modified");
        node.tags.remove("binoc.copy.modified");
        node.summary = detail.or_else(|| {
            // Drop a bare "Moved from .../Copied from ..." headline (now
            // demoted to a plain modify); keep any other prior summary.
            let is_path_statement = node.summary.as_ref().is_some_and(|s| {
                let text = s.plain_text();
                text.starts_with("Moved from ") || text.starts_with("Copied from ")
            });
            if is_path_statement {
                None
            } else {
                node.summary.clone()
            }
        });
    }
    node
}

fn relocate_source_remainders(source_node: &DiffNode, rollup: &Rollup) -> Vec<DiffNode> {
    source_node
        .children
        .iter()
        .filter_map(|child| relocate_source_node(child, rollup))
        .collect()
}

fn relocate_source_node(node: &DiffNode, rollup: &Rollup) -> Option<DiffNode> {
    let rel = strip_prefix_as_child(&node.path, &rollup.source_path)?;
    if node.children.is_empty() && rollup.matched_rel_paths.contains(rel) {
        return None;
    }

    let new_path = if rollup.dst_path.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        rollup.dst_path.clone()
    } else {
        format!("{}/{}", rollup.dst_path, rel)
    };

    let mut cloned = node.clone();
    cloned.path = new_path;
    cloned.children = node
        .children
        .iter()
        .filter_map(|child| relocate_source_node(child, rollup))
        .collect();

    if cloned.children.is_empty() && cloned.summary.is_none() && cloned.tags.is_empty() {
        return None;
    }

    Some(cloned)
}

fn merge_same_path_nodes(mut nodes: Vec<DiffNode>) -> Vec<DiffNode> {
    nodes.sort_by(|a, b| a.path.cmp(&b.path));

    let mut merged: Vec<DiffNode> = Vec::new();
    for mut node in nodes {
        node.children = merge_same_path_nodes(node.children);

        if let Some(prev) = merged.pop() {
            if prev.path == node.path {
                merged.push(merge_node_pair(prev, node));
            } else {
                merged.push(prev);
                merged.push(node);
            }
        } else {
            merged.push(node);
        }
    }

    merged
}

fn merge_node_pair(left: DiffNode, right: DiffNode) -> DiffNode {
    match (left.action.as_str(), right.action.as_str()) {
        ("add", "remove") | ("remove", "add") => merge_add_remove_pair(left, right),
        _ => {
            let mut left = left;
            left.children.extend(right.children);
            left.children = merge_same_path_nodes(left.children);
            left
        }
    }
}

fn merge_add_remove_pair(left: DiffNode, right: DiffNode) -> DiffNode {
    let (mut add, remove) = if left.action == "add" {
        (left, right)
    } else {
        (right, left)
    };

    add.action = "modify".to_string();
    add.source_path = None;
    add.summary = None;
    add.children.extend(remove.children);
    add.children = merge_same_path_nodes(add.children);
    add.tags.extend(remove.tags);
    add.tags.remove("binoc.move");
    add.tags.remove("binoc.copy");
    add.tags.remove("binoc.move.modified");
    add.tags.remove("binoc.copy.modified");
    add
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
    fn config_defaults_to_partial_rollup_threshold() {
        let cfg = Config::from_value(&serde_json::Value::Null);
        assert_eq!(cfg.threshold, 0.8);
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

    #[test]
    fn derive_source_prefix_allows_basename_rename() {
        assert_eq!(
            derive_source_prefix("data.zip/old.csv", "new.csv"),
            Some("data.zip")
        );
    }

    #[test]
    fn derive_source_prefix_nested_requires_matching_dirs() {
        assert_eq!(
            derive_source_prefix("old/data/x.csv", "data/y.csv"),
            Some("old")
        );
        assert_eq!(derive_source_prefix("old/other/x.csv", "data/x.csv"), None);
    }

    fn move_leaf(path: &str, source: &str) -> DiffNode {
        DiffNode::new("move", "tabular", path)
            .with_source_path(source)
            .with_tag("binoc.move")
    }

    fn modified_move_leaf(path: &str, source: &str) -> DiffNode {
        move_leaf(path, source).with_tag("binoc.move.modified")
    }

    fn container(path: &str, action: &str, children: Vec<DiffNode>) -> DiffNode {
        let mut node = DiffNode::new(action, "directory", path);
        node.children = children;
        node
    }

    #[test]
    fn renamed_and_modified_single_child_rolls_up() {
        let c = container(
            "archive.zip",
            "add",
            vec![modified_move_leaf(
                "archive.zip/new.csv",
                "data.zip/old.csv",
            )],
        );
        let rollup = try_rollup(&c, 0.8).expect("renamed+modified child should roll up");
        assert_eq!(rollup.source_path, "data.zip");
        assert_eq!(rollup.kind, RollupKind::Move);
        assert_eq!(rollup.modified_count, 1);
        assert!(rollup.matched_rel_paths.contains("new.csv"));
    }

    #[test]
    fn move_from_ancestor_container_into_child_does_not_roll_up() {
        // outer/beta.txt -> outer/inner/renamed.txt is a file move into a
        // child container, not a move of `inner` from `outer`.
        let c = container(
            "outer.zip/inner.zip",
            "modify",
            vec![move_leaf(
                "outer.zip/inner.zip/renamed.txt",
                "outer.zip/beta.txt",
            )],
        );
        assert!(try_rollup(&c, 0.8).is_none());
    }

    #[test]
    fn children_from_different_source_containers_do_not_roll_up() {
        let c = container(
            "dst",
            "add",
            vec![
                move_leaf("dst/a.csv", "src_one/a.csv"),
                move_leaf("dst/b.csv", "src_two/b.csv"),
            ],
        );
        assert!(try_rollup(&c, 0.8).is_none());
    }

    #[test]
    fn copy_children_roll_up_as_copy() {
        let c = container(
            "dst",
            "add",
            vec![
                DiffNode::new("copy", "tabular", "dst/a.csv")
                    .with_source_path("src/a.csv")
                    .with_tag("binoc.copy"),
                DiffNode::new("copy", "tabular", "dst/b.csv")
                    .with_source_path("src/b.csv")
                    .with_tag("binoc.copy"),
            ],
        );
        let rollup = try_rollup(&c, 0.8).expect("clean copies should roll up");
        assert_eq!(rollup.kind, RollupKind::Copy);
        assert_eq!(rollup.modified_count, 0);
    }

    #[test]
    fn mixed_clean_and_modified_children_roll_up_with_modifications_surfaced() {
        let dst = container(
            "dst",
            "add",
            vec![
                move_leaf("dst/a.csv", "src/a.csv"),
                modified_move_leaf("dst/b.csv", "src/b.csv"),
            ],
        );
        let src = container("src", "remove", vec![]);
        let mut root = DiffNode::new("modify", "directory", "");
        root.children = vec![dst, src];

        let mut rollups = BTreeMap::new();
        collect_rollups(&root, 0.8, &mut rollups);
        let rollup = rollups.get("dst").expect("dst should roll up");
        assert_eq!(rollup.modified_count, 1);

        let source_paths: BTreeSet<String> =
            rollups.values().map(|r| r.source_path.clone()).collect();
        let mut source_index = BTreeMap::new();
        index_nodes_by_path(&root, &mut source_index);
        let rewritten = apply_rollups(root, &rollups, &source_paths, &source_index);

        // The dangling source container is gone; only the folded move remains.
        assert_eq!(rewritten.children.len(), 1);
        let folded = &rewritten.children[0];
        assert_eq!(folded.action, "move");
        assert_eq!(folded.source_path.as_deref(), Some("src"));
        let summary = folded
            .summary
            .as_ref()
            .expect("folded summary")
            .plain_text();
        assert!(
            summary.contains("1 file modified within"),
            "summary should surface the modification: {summary}"
        );
        // The clean, fully-implied child is dropped; the modified child
        // stays beneath the folded node as detail.
        assert_eq!(folded.children.len(), 1);
        assert_eq!(folded.children[0].path, "dst/b.csv");
        assert!(folded.children[0].tags.contains("binoc.move.modified"));
    }

    #[test]
    fn clean_renamed_child_rolls_up_and_keeps_rename_detail() {
        let dst = container(
            "dst",
            "add",
            vec![
                move_leaf("dst/a.csv", "src/a.csv"),
                move_leaf("dst/renamed.csv", "src/original.csv"),
            ],
        );
        let mut root = DiffNode::new("modify", "directory", "");
        root.children = vec![dst];

        let mut rollups = BTreeMap::new();
        collect_rollups(&root, 0.8, &mut rollups);
        let rollup = rollups
            .get("dst")
            .expect("renamed child should still roll up");
        assert_eq!(rollup.source_path, "src");
        assert_eq!(rollup.modified_count, 0);

        let source_paths: BTreeSet<String> =
            rollups.values().map(|r| r.source_path.clone()).collect();
        let mut source_index = BTreeMap::new();
        index_nodes_by_path(&root, &mut source_index);
        let rewritten = apply_rollups(root, &rollups, &source_paths, &source_index);

        let folded = &rewritten.children[0];
        assert_eq!(folded.action, "move");
        // The same-name child is implied and dropped; the renamed child
        // keeps its move node so the rename stays visible.
        assert_eq!(folded.children.len(), 1);
        assert_eq!(folded.children[0].path, "dst/renamed.csv");
        assert_eq!(
            folded.children[0].source_path.as_deref(),
            Some("src/original.csv")
        );
    }
}
