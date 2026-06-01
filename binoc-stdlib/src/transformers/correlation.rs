//! Shared machinery for tree-wide correlation transformers
//! ([`super::correlation_detector`], [`super::folder_move_detector`]).
//!
//! These transformers index every correlatable leaf in the diff tree
//! (by content hash) and rewrite matched pairs across container
//! boundaries. Building that index — and hydrating any missing hashes —
//! is the same code both need, so it lives here.

use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;

/// One leaf that can participate in content-hash correlation: it is an
/// `add`/`remove`/`identical` node, has a known hash (either pre-
/// computed or resolved on demand), and is not a directory.
#[derive(Debug, Clone)]
pub(crate) struct LeafEntry {
    pub action: String,
    pub hash: String,
    pub path: String,
    pub item_type: String,
}

/// Mutably walk the tree and return an entry for every correlatable
/// leaf. Hashes missing from `details` are hydrated via
/// [`ItemRef::resolve_hash`] (reading bytes through `DataAccess`) and
/// written back into `details` so downstream transformers and renderers
/// can read them cheaply.
///
/// Leaves whose hash cannot be resolved (directory leaves, sourceless
/// nodes, I/O failures) are simply skipped.
pub(crate) fn collect_and_hydrate(node: &mut DiffNode, data: &dyn DataAccess) -> Vec<LeafEntry> {
    let mut out = Vec::new();
    visit(node, data, &mut out);
    out
}

fn visit(node: &mut DiffNode, data: &dyn DataAccess, out: &mut Vec<LeafEntry>) {
    if node.children.is_empty() {
        if let Some(entry) = as_leaf_entry(node, data) {
            out.push(entry);
        }
        return;
    }
    for child in &mut node.children {
        visit(child, data, out);
    }
}

fn as_leaf_entry(node: &mut DiffNode, data: &dyn DataAccess) -> Option<LeafEntry> {
    let action = node.action.clone();
    if action != "add" && action != "remove" && action != "identical" {
        return None;
    }
    let hash = hash_for(node, &action, data)?;
    Some(LeafEntry {
        action,
        hash,
        path: node.path.clone(),
        item_type: node.item_type.clone(),
    })
}

/// Return the side-appropriate hash for a leaf, hydrating from
/// `source_items` if `details` doesn't already have it. A hydrated hash
/// is written back into `details` so downstream readers reuse it.
fn hash_for(node: &mut DiffNode, action: &str, data: &dyn DataAccess) -> Option<String> {
    // Cheap path: hash already recorded on the node.
    for key in hash_detail_keys(action) {
        if let Some(existing) = node.details.get(*key).and_then(|v| v.as_str()) {
            return Some(existing.to_string());
        }
    }

    // Fallback: resolve via the source ItemRef if we have one.
    let source = match action {
        "add" => node.source_items.as_ref().and_then(|p| p.right.as_ref()),
        "remove" => node.source_items.as_ref().and_then(|p| p.left.as_ref()),
        "identical" => node
            .source_items
            .as_ref()
            .and_then(|p| p.right.as_ref().or(p.left.as_ref())),
        _ => None,
    }?;

    if source.is_dir {
        return None;
    }

    let hash = source.resolve_hash(data).ok()?;
    // Write back to the primary key for this side.
    let primary_key = match action {
        "add" => "hash_right",
        "remove" => "hash_left",
        "identical" => "hash",
        _ => return Some(hash),
    };
    node.details
        .insert(primary_key.into(), serde_json::json!(&hash));
    Some(hash)
}

fn hash_detail_keys(action: &str) -> &'static [&'static str] {
    match action {
        "add" => &["hash_right", "hash"],
        "remove" => &["hash_left", "hash"],
        "identical" => &["hash", "hash_right", "hash_left"],
        _ => &[],
    }
}

/// Return the parent-container path of a leaf path. For a leaf at
/// `"outer.zip/inner.zip/foo.txt"` returns `"outer.zip/inner.zip"`.
/// For a leaf at the root (no `/`) returns `""`.
pub(crate) fn parent_path_of(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent,
        None => "",
    }
}

/// File name component of a path (everything after the last `/`).
pub(crate) fn file_name_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(_, n)| n).unwrap_or(path)
}

/// Return a source-path label concise for leaf renames, but qualified
/// enough to make parent-container moves obvious.
pub(crate) fn source_label_for_move(source_path: &str, dest_path: &str) -> String {
    if parent_path_of(source_path) == parent_path_of(dest_path) {
        return file_name_of(source_path).to_string();
    }

    let source_segments: Vec<&str> = source_path.split('/').collect();
    let dest_segments: Vec<&str> = dest_path.split('/').collect();

    let mut common_prefix = 0;
    while common_prefix < source_segments.len()
        && common_prefix < dest_segments.len()
        && source_segments[common_prefix] == dest_segments[common_prefix]
    {
        common_prefix += 1;
    }

    if common_prefix >= source_segments.len() {
        return file_name_of(source_path).to_string();
    }

    let mut start = common_prefix;
    if source_segments.len() - start == 1 && source_segments.len() > 1 {
        start = start.saturating_sub(1);
    }

    source_segments[start..].join("/")
}

/// Plan for rewriting a tree in a single pass: set of leaf paths to
/// delete, plus new nodes to insert under each parent container.
#[derive(Default, Debug)]
pub(crate) struct RewritePlan {
    /// Leaves (by full path) to remove from the tree.
    pub remove_paths: BTreeSet<String>,
    /// New nodes to insert, keyed by parent container path.
    pub inserts: BTreeMap<String, Vec<DiffNode>>,
}

impl RewritePlan {
    pub fn is_empty(&self) -> bool {
        self.remove_paths.is_empty() && self.inserts.is_empty()
    }

    pub fn schedule_remove(&mut self, path: &str) {
        self.remove_paths.insert(path.to_string());
    }

    pub fn schedule_insert(&mut self, parent_path: &str, node: DiffNode) {
        self.inserts
            .entry(parent_path.to_string())
            .or_default()
            .push(node);
    }
}

/// Apply a [`RewritePlan`] to an owned tree, returning the rewritten
/// tree.
///
/// Semantics:
/// - Leaves whose path appears in `remove_paths` are dropped.
/// - Containers append any `inserts` entries whose key matches their
///   path. `inserts` is drained as it's consumed, so when multiple
///   containers share a path (e.g. the zip comparator's `zip_archive`
///   wrapper around a synthetic `directory` with the same path), the
///   innermost (first-visited in bottom-up order) takes the inserts
///   and subsequent containers at that path see nothing.
/// - Children are sorted by path after modification.
pub(crate) fn apply_rewrite(node: DiffNode, plan: &RewritePlan) -> DiffNode {
    let mut inserts = plan.inserts.clone();
    apply_rewrite_inner(node, &plan.remove_paths, &mut inserts)
}

fn apply_rewrite_inner(
    mut node: DiffNode,
    remove_paths: &BTreeSet<String>,
    inserts: &mut BTreeMap<String, Vec<DiffNode>>,
) -> DiffNode {
    if node.children.is_empty() {
        return node;
    }

    let mut new_children: Vec<DiffNode> = node
        .children
        .into_iter()
        .filter(|child| !remove_paths.contains(&child.path))
        .map(|child| apply_rewrite_inner(child, remove_paths, inserts))
        .collect();

    if let Some(added) = inserts.remove(&node.path) {
        new_children.extend(added);
    }

    new_children.sort_by(|a, b| a.path.cmp(&b.path));
    node.children = new_children;
    node
}

/// Group a flat list of leaf entries by content hash.
pub(crate) fn group_by_hash(entries: Vec<LeafEntry>) -> BTreeMap<String, HashGroup> {
    let mut out: BTreeMap<String, HashGroup> = BTreeMap::new();
    for e in entries {
        let group = out.entry(e.hash.clone()).or_default();
        match e.action.as_str() {
            "add" => group.adds.push(e),
            "remove" => group.removes.push(e),
            "identical" => group.identicals.push(e),
            _ => {}
        }
    }
    out
}

#[derive(Default, Debug, Clone)]
pub(crate) struct HashGroup {
    pub adds: Vec<LeafEntry>,
    pub removes: Vec<LeafEntry>,
    pub identicals: Vec<LeafEntry>,
}

impl HashGroup {
    pub fn is_trivial(&self) -> bool {
        self.adds.is_empty() && self.removes.is_empty()
    }
}

/// Format a list of file names (stripped of leading directory segments)
/// into readable English: `"A"`, `"A and B"`, `"A, B and C"`.
pub(crate) fn english_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [a] => (*a).to_string(),
        [a, b] => format!("{a} and {b}"),
        rest => {
            let (last, head) = rest.split_last().expect("non-empty");
            format!("{} and {}", head.join(", "), last)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_path_root() {
        assert_eq!(parent_path_of("foo.txt"), "");
    }

    #[test]
    fn parent_path_nested() {
        assert_eq!(
            parent_path_of("outer.zip/inner.zip/x.txt"),
            "outer.zip/inner.zip"
        );
    }

    #[test]
    fn file_name() {
        assert_eq!(file_name_of("a/b/c.txt"), "c.txt");
        assert_eq!(file_name_of("c.txt"), "c.txt");
    }

    #[test]
    fn move_label_keeps_leaf_rename_concise() {
        assert_eq!(
            source_label_for_move("docs/old-name.txt", "docs/new-name.txt"),
            "old-name.txt"
        );
    }

    #[test]
    fn move_label_includes_changed_parent_segment() {
        assert_eq!(
            source_label_for_move(
                "FoodData_Central_csv_2026-04-29/branded_food.csv",
                "FoodData_Central_csv_2026-04-30/branded_food.csv"
            ),
            "FoodData_Central_csv_2026-04-29/branded_food.csv"
        );
    }

    #[test]
    fn move_label_includes_parent_when_destination_gains_container() {
        assert_eq!(
            source_label_for_move("outer.zip/beta.txt", "outer.zip/inner.zip/beta-renamed.txt"),
            "outer.zip/beta.txt"
        );
    }

    #[test]
    fn move_label_falls_back_to_full_path_when_roots_diverge() {
        assert_eq!(
            source_label_for_move("outer.zip/inner.zip/gamma.txt", "gamma-renamed.txt"),
            "outer.zip/inner.zip/gamma.txt"
        );
    }

    #[test]
    fn english_list_formats() {
        assert_eq!(english_list(&["A"]), "A");
        assert_eq!(english_list(&["A", "B"]), "A and B");
        assert_eq!(english_list(&["A", "B", "C"]), "A, B and C");
    }
}
