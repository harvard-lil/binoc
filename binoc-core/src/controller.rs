use rayon::prelude::*;
use std::sync::Arc;

use crate::ir::{DiffNode, Migration};
use crate::traits::*;
use crate::types::*;

/// The core engine: processes a work queue of item pairs, dispatching to
/// comparators, assembling the diff tree, then running transformers.
/// Type-ignorant — it does not know what a directory, zip, or CSV is.
pub struct Controller {
    comparators: Vec<Arc<dyn Comparator>>,
    transformers: Vec<Arc<dyn Transformer>>,
}

impl Controller {
    pub fn new(
        comparators: Vec<Arc<dyn Comparator>>,
        transformers: Vec<Arc<dyn Transformer>>,
    ) -> Self {
        Self {
            comparators,
            transformers,
        }
    }

    /// Diff two snapshots and produce a migration.
    pub fn diff(&self, from_path: &str, to_path: &str) -> BinocResult<Migration> {
        let ctx = Arc::new(CompareContext::new());

        let left = crate::types::Item::new(from_path, "");
        let right = crate::types::Item::new(to_path, "");
        let root_pair = ItemPair::both(left, right);

        let root_node = self.process_pair(root_pair, &ctx)?;

        let root_node = self
            .run_transformers(root_node, &ctx)
            .and_then(Self::prune_identical);

        Ok(Migration::new(from_path, to_path, root_node))
    }

    /// Extract data from a specific node in a migration.
    /// Walks the ancestor chain calling `reopen` to reconstruct physical
    /// access, then calls the last toucher's `extract` method.
    pub fn extract(
        &self,
        migration: &Migration,
        node_path: &str,
        aspect: &str,
        snapshot_a: &str,
        snapshot_b: &str,
    ) -> BinocResult<ExtractResult> {
        let root = migration
            .root
            .as_ref()
            .ok_or_else(|| BinocError::Extract("migration has no root".into()))?;

        let ancestor_chain = Self::find_ancestor_chain(root, node_path)
            .ok_or_else(|| BinocError::Extract(format!("node not found: {node_path}")))?;

        let target = ancestor_chain
            .last()
            .ok_or_else(|| BinocError::Extract("empty ancestor chain".into()))?;

        let ctx = Arc::new(CompareContext::new());

        let mut current_pair = ItemPair::both(Item::new(snapshot_a, ""), Item::new(snapshot_b, ""));

        // Walk ancestor chain (excluding the target itself), reopening at each level
        for ancestor in &ancestor_chain[..ancestor_chain.len() - 1] {
            let comp_name = ancestor.comparator.as_deref().ok_or_else(|| {
                BinocError::Extract(format!(
                    "node '{}' has no comparator recorded",
                    ancestor.path
                ))
            })?;

            let comparator = self.find_comparator_by_name(comp_name).ok_or_else(|| {
                BinocError::Extract(format!("comparator '{comp_name}' not found in registry"))
            })?;

            let ancestor_idx = ancestor_chain
                .iter()
                .position(|n| std::ptr::eq(*n, *ancestor))
                .unwrap();
            let next_path = &ancestor_chain[ancestor_idx + 1].path;

            current_pair = comparator.reopen(&current_pair, next_path, &ctx)?;
        }

        // Now current_pair points at the target's source files.
        // Determine who extracts: last transformer or the comparator.
        if let Some(last_transformer_name) = target.transformed_by.last() {
            let transformer = self
                .find_transformer_by_name(last_transformer_name)
                .ok_or_else(|| {
                    BinocError::Extract(format!(
                        "transformer '{last_transformer_name}' not found in registry"
                    ))
                })?;

            let comp_name = target.comparator.as_deref().ok_or_else(|| {
                BinocError::Extract(format!(
                    "node '{}' has no comparator for data access",
                    target.path
                ))
            })?;

            let comparator = self.find_comparator_by_name(comp_name).ok_or_else(|| {
                BinocError::Extract(format!("comparator '{comp_name}' not found in registry"))
            })?;

            let data = comparator.reopen_data(&current_pair, &ctx)?;
            transformer.extract(&data, target, aspect)
                .ok_or_else(|| BinocError::Extract(format!(
                    "transformer '{last_transformer_name}' cannot extract aspect '{aspect}' from node '{}'",
                    target.path
                )))
        } else {
            let comp_name = target.comparator.as_deref().ok_or_else(|| {
                BinocError::Extract(format!("node '{}' has no comparator recorded", target.path))
            })?;

            let comparator = self.find_comparator_by_name(comp_name).ok_or_else(|| {
                BinocError::Extract(format!("comparator '{comp_name}' not found in registry"))
            })?;

            let data = comparator.reopen_data(&current_pair, &ctx)?;
            comparator.extract(&data, target, aspect).ok_or_else(|| {
                BinocError::Extract(format!(
                    "comparator '{comp_name}' cannot extract aspect '{aspect}' from node '{}'",
                    target.path
                ))
            })
        }
    }

    /// Find the chain of ancestor nodes from root to the target node (inclusive).
    fn find_ancestor_chain<'a>(node: &'a DiffNode, target_path: &str) -> Option<Vec<&'a DiffNode>> {
        if node.path == target_path {
            return Some(vec![node]);
        }
        for child in &node.children {
            if let Some(mut chain) = Self::find_ancestor_chain(child, target_path) {
                chain.insert(0, node);
                return Some(chain);
            }
        }
        None
    }

    fn find_comparator_by_name(&self, name: &str) -> Option<Arc<dyn Comparator>> {
        self.comparators.iter().find(|c| c.name() == name).cloned()
    }

    fn find_transformer_by_name(&self, name: &str) -> Option<Arc<dyn Transformer>> {
        self.transformers.iter().find(|t| t.name() == name).cloned()
    }

    /// Recursively process an item pair through the comparator pipeline.
    /// Identical items produce nodes with kind "identical" so transformers
    /// can see the full comparison result. Pruning happens after transformers.
    fn process_pair(&self, pair: ItemPair, ctx: &Arc<CompareContext>) -> BinocResult<DiffNode> {
        // Short-circuit: if both items have matching content hashes and no
        // comparator opts in to processing identical items, skip dispatch.
        if let Some(hash) = pair.matching_content_hash() {
            let dominated = self
                .find_comparator(&pair)
                .is_some_and(|c| c.handles_identical());
            if !dominated {
                return Ok(DiffNode::new("identical", "", pair.logical_path())
                    .with_detail("hash", serde_json::json!(hash)));
            }
        }

        let comparator = self
            .find_comparator(&pair)
            .ok_or_else(|| BinocError::NoComparator(pair.logical_path().to_string()))?;

        let comp_name = comparator.name().to_string();
        let result = comparator.compare(&pair, ctx)?;

        let mut node = match result {
            CompareResult::Identical => DiffNode::new("identical", "", pair.logical_path()),

            CompareResult::Leaf(node) => node,

            CompareResult::Expand(mut container, children) => {
                let child_nodes = self.process_children(children, ctx)?;
                container.children = child_nodes;
                container
            }
        };

        node.comparator = Some(comp_name);
        Self::attach_content_hashes(&mut node, &pair);
        Ok(node)
    }

    /// Attach content hashes from Item metadata to a DiffNode, filling in
    /// hash details that the comparator didn't already set. Ensures all
    /// nodes carry hashes for move/copy detection regardless of which
    /// comparator produced them.
    fn attach_content_hashes(node: &mut DiffNode, pair: &ItemPair) {
        let left_hash = pair.left.as_ref().and_then(|i| i.content_hash.as_deref());
        let right_hash = pair.right.as_ref().and_then(|i| i.content_hash.as_deref());

        match (left_hash, right_hash) {
            (Some(l), Some(r)) if l == r => {
                node.details
                    .entry("hash".into())
                    .or_insert_with(|| serde_json::json!(l));
            }
            _ => {
                if let Some(h) = left_hash {
                    node.details
                        .entry("hash_left".into())
                        .or_insert_with(|| serde_json::json!(h));
                }
                if let Some(h) = right_hash {
                    node.details
                        .entry("hash_right".into())
                        .or_insert_with(|| serde_json::json!(h));
                }
            }
        }
    }

    /// Process a list of child item pairs in parallel.
    fn process_children(
        &self,
        children: Vec<ItemPair>,
        ctx: &Arc<CompareContext>,
    ) -> BinocResult<Vec<DiffNode>> {
        let results: Vec<BinocResult<DiffNode>> = children
            .into_par_iter()
            .map(|pair| self.process_pair(pair, ctx))
            .collect();

        let mut nodes = Vec::new();
        for result in results {
            nodes.push(result?);
        }
        nodes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(nodes)
    }

    /// Find the first comparator in pipeline order that claims an item pair.
    /// For each comparator, checks declarative extension match, then media type
    /// match, then `can_handle`; first comparator to claim wins.
    fn find_comparator(&self, pair: &ItemPair) -> Option<Arc<dyn Comparator>> {
        // Directories are matched via can_handle, not by extension or media type.
        // This prevents e.g. extracted zip contents (directory with logical_path
        // "archive.zip") from being re-claimed by the zip comparator.
        let is_dir = pair.is_dir();
        let path_lower = if is_dir {
            None
        } else {
            Some(pair.logical_path().to_lowercase())
        };
        let media = if is_dir {
            None
        } else {
            pair.media_type().map(|s| s.to_owned())
        };

        for comparator in &self.comparators {
            if let Some(ref p) = path_lower {
                let exts = comparator.handles_extensions();
                if !exts.is_empty()
                    && exts
                        .iter()
                        .any(|handled| p.ends_with(&handled.to_lowercase()))
                {
                    return Some(Arc::clone(comparator));
                }
            }

            if let Some(ref m) = media {
                let types = comparator.handles_media_types();
                if !types.is_empty() && types.iter().any(|handled| handled.eq_ignore_ascii_case(m))
                {
                    return Some(Arc::clone(comparator));
                }
            }

            if comparator.can_handle(pair) {
                return Some(Arc::clone(comparator));
            }
        }

        None
    }

    /// Run all transformers in order over the diff tree.
    /// Returns None if the root itself was removed by a transformer.
    fn run_transformers(&self, root: DiffNode, ctx: &Arc<CompareContext>) -> Option<DiffNode> {
        let mut current = root;
        for transformer in &self.transformers {
            let results = self.apply_transformer(current, transformer, ctx);
            match results.len() {
                0 => return None,
                1 => current = results.into_iter().next().unwrap(),
                _ => {
                    // ReplaceMany at root level: no parent to splice into,
                    // so keep the first result. This mirrors the structural
                    // constraint that a Migration has exactly one root.
                    current = results.into_iter().next().unwrap();
                }
            }
        }
        Some(current)
    }

    /// Recursively prune nodes that are still "identical" after transformers.
    /// Returns None if the node itself should be removed from the tree.
    fn prune_identical(node: DiffNode) -> Option<DiffNode> {
        if node.kind == "identical" {
            return None;
        }

        let had_children = !node.children.is_empty();
        let children: Vec<DiffNode> = node
            .children
            .into_iter()
            .filter_map(Self::prune_identical)
            .collect();

        // Prune containers that lost all their change-bearing children
        // and carry no own payload (details/tags). Leaf nodes (never had
        // children) are always preserved — they represent actual changes.
        if had_children && children.is_empty() && node.details.is_empty() && node.tags.is_empty() {
            return None;
        }

        Some(DiffNode { children, ..node })
    }

    /// Apply a single transformer to a node (and recursively to its children).
    /// Returns a Vec because the node may be removed (empty), kept/replaced
    /// (single element), or expanded into multiple siblings (ReplaceMany).
    fn apply_transformer(
        &self,
        mut node: DiffNode,
        transformer: &Arc<dyn Transformer>,
        ctx: &Arc<CompareContext>,
    ) -> Vec<DiffNode> {
        let trans_name = transformer.name().to_string();

        match transformer.scope() {
            TransformScope::Node => {
                node.children = node
                    .children
                    .into_iter()
                    .flat_map(|child| self.apply_transformer(child, transformer, ctx))
                    .collect();

                if self.transformer_matches(transformer, &node) {
                    match transformer.transform(node.clone(), ctx) {
                        TransformResult::Unchanged => vec![node],
                        TransformResult::Replace(mut new_node) => {
                            new_node.transformed_by.push(trans_name);
                            vec![new_node]
                        }
                        TransformResult::ReplaceMany(nodes) => nodes
                            .into_iter()
                            .map(|mut n| {
                                n.transformed_by.push(trans_name.clone());
                                n
                            })
                            .collect(),
                        TransformResult::Remove => vec![],
                    }
                } else {
                    vec![node]
                }
            }
            TransformScope::Subtree => {
                if self.transformer_matches(transformer, &node) {
                    match transformer.transform(node.clone(), ctx) {
                        TransformResult::Unchanged => vec![node],
                        TransformResult::Replace(mut new_node) => {
                            new_node.transformed_by.push(trans_name);
                            vec![new_node]
                        }
                        TransformResult::ReplaceMany(nodes) => nodes
                            .into_iter()
                            .map(|mut n| {
                                n.transformed_by.push(trans_name.clone());
                                n
                            })
                            .collect(),
                        TransformResult::Remove => vec![],
                    }
                } else {
                    node.children = node
                        .children
                        .into_iter()
                        .flat_map(|child| self.apply_transformer(child, transformer, ctx))
                        .collect();
                    vec![node]
                }
            }
        }
    }

    /// Check if a transformer's declarative filters match a node.
    fn transformer_matches(&self, transformer: &Arc<dyn Transformer>, node: &DiffNode) -> bool {
        let types = transformer.match_types();
        if !types.is_empty() && types.contains(&node.item_type.as_str()) {
            return true;
        }

        let tags = transformer.match_tags();
        if !tags.is_empty() && tags.iter().any(|t| node.tags.contains(*t)) {
            return true;
        }

        let kinds = transformer.match_kinds();
        if !kinds.is_empty() && kinds.contains(&node.kind.as_str()) {
            return true;
        }

        transformer.can_handle(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::DiffNode;
    use crate::traits::CompareContext;
    use crate::traits::{Comparator, Transformer};
    use crate::types::{CompareResult, ItemPair, TransformResult, TransformScope};

    struct CatchAllIdenticalComparator;
    impl Comparator for CatchAllIdenticalComparator {
        fn name(&self) -> &str {
            "catch-all-identical"
        }
        fn can_handle(&self, _pair: &ItemPair) -> bool {
            true
        }
        fn compare(&self, _pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Identical)
        }
    }

    struct LeafComparator;
    impl Comparator for LeafComparator {
        fn name(&self) -> &str {
            "leaf"
        }
        fn can_handle(&self, _pair: &ItemPair) -> bool {
            true
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(DiffNode::new(
                "modify",
                "file",
                pair.logical_path(),
            )))
        }
    }

    struct TypedLeafComparator(&'static str);
    impl Comparator for TypedLeafComparator {
        fn name(&self) -> &str {
            "typed-leaf"
        }
        fn can_handle(&self, _pair: &ItemPair) -> bool {
            true
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(DiffNode::new(
                "modify",
                self.0,
                pair.logical_path(),
            )))
        }
    }

    struct TaggedLeafComparator(&'static str);
    impl Comparator for TaggedLeafComparator {
        fn name(&self) -> &str {
            "tagged-leaf"
        }
        fn can_handle(&self, _pair: &ItemPair) -> bool {
            true
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(
                DiffNode::new("modify", "file", pair.logical_path()).with_tag(self.0),
            ))
        }
    }

    struct ExpandComparator;
    impl Comparator for ExpandComparator {
        fn name(&self) -> &str {
            "expand"
        }
        fn can_handle(&self, pair: &ItemPair) -> bool {
            pair.is_dir()
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            let path = pair.logical_path();
            let left_path = pair.left.as_ref().map(|i| i.physical_path.clone());
            let right_path = pair.right.as_ref().map(|i| i.physical_path.clone());

            let children = match (left_path, right_path) {
                (Some(l), Some(r)) => {
                    let mut pairs = Vec::new();
                    for (lp, rp) in [
                        (l.join("a.txt"), r.join("a.txt")),
                        (l.join("b.txt"), r.join("b.txt")),
                    ] {
                        if lp.exists() && rp.exists() {
                            let left = Item::new(
                                &lp,
                                format!("{path}/{}", lp.file_name().unwrap().to_string_lossy()),
                            );
                            let right = Item::new(
                                &rp,
                                format!("{path}/{}", rp.file_name().unwrap().to_string_lossy()),
                            );
                            pairs.push(ItemPair::both(left, right));
                        }
                    }
                    pairs
                }
                _ => vec![],
            };

            Ok(CompareResult::Expand(
                DiffNode::new("modify", "directory", path),
                children,
            ))
        }
    }

    #[test]
    fn controller_identical_comparator_produces_no_root_diff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let controller = Controller::new(vec![Arc::new(CatchAllIdenticalComparator)], vec![]);
        let migration = controller.diff(&path, &path).unwrap();
        assert!(migration.root.is_none());
    }

    #[test]
    fn controller_leaf_comparator_produces_leaf_node() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let controller = Controller::new(vec![Arc::new(LeafComparator)], vec![]);
        let migration = controller.diff(&path, &path).unwrap();
        let root = migration.root.as_ref().unwrap();
        assert_eq!(root.kind, "modify");
        assert_eq!(root.item_type, "file");
    }

    #[test]
    fn controller_expand_processes_children_recursively() {
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(from_dir.path().join("b.txt"), b"b").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(to_dir.path().join("b.txt"), b"b modified").unwrap();

        let controller = Controller::new(
            vec![Arc::new(ExpandComparator), Arc::new(LeafComparator)],
            vec![],
        );
        let migration = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();

        let root = migration.root.as_ref().unwrap();
        assert_eq!(root.kind, "modify");
        assert_eq!(root.item_type, "directory");
        assert!(!root.children.is_empty());
    }

    struct ReplaceTransformer {
        match_types: &'static [&'static str],
        match_tags: &'static [&'static str],
        match_kinds: &'static [&'static str],
        can_handle: bool,
        scope: TransformScope,
    }
    impl Transformer for ReplaceTransformer {
        fn name(&self) -> &str {
            "replace-test"
        }
        fn match_types(&self) -> &[&str] {
            self.match_types
        }
        fn match_tags(&self) -> &[&str] {
            self.match_tags
        }
        fn match_kinds(&self) -> &[&str] {
            self.match_kinds
        }
        fn scope(&self) -> TransformScope {
            self.scope
        }
        fn can_handle(&self, _node: &DiffNode) -> bool {
            self.can_handle
        }
        fn transform(&self, node: DiffNode, _ctx: &CompareContext) -> TransformResult {
            TransformResult::Replace(
                node.with_tag("transformed")
                    .with_detail("by", serde_json::json!("replace-transformer")),
            )
        }
    }

    #[test]
    fn transformer_matches_by_type() {
        let controller = Controller::new(
            vec![Arc::new(TypedLeafComparator("csv"))],
            vec![Arc::new(ReplaceTransformer {
                match_types: &["csv"],
                match_tags: &[],
                match_kinds: &[],
                can_handle: false,
                scope: TransformScope::Node,
            })],
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let migration = controller.diff(&path, &path).unwrap();
        let root = migration.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
        assert_eq!(
            root.details.get("by"),
            Some(&serde_json::json!("replace-transformer"))
        );
    }

    #[test]
    fn transformer_matches_by_tag() {
        let controller = Controller::new(
            vec![Arc::new(TaggedLeafComparator("binoc.modify"))],
            vec![Arc::new(ReplaceTransformer {
                match_types: &[],
                match_tags: &["binoc.modify"],
                match_kinds: &[],
                can_handle: false,
                scope: TransformScope::Node,
            })],
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let migration = controller.diff(&path, &path).unwrap();
        let root = migration.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
    }

    #[test]
    fn transformer_matches_by_kind() {
        let controller = Controller::new(
            vec![Arc::new(LeafComparator)],
            vec![Arc::new(ReplaceTransformer {
                match_types: &[],
                match_tags: &[],
                match_kinds: &["modify"],
                can_handle: false,
                scope: TransformScope::Node,
            })],
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let migration = controller.diff(&path, &path).unwrap();
        let root = migration.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
    }

    #[test]
    fn transformer_matches_via_can_handle() {
        let controller = Controller::new(
            vec![Arc::new(LeafComparator)],
            vec![Arc::new(ReplaceTransformer {
                match_types: &[],
                match_tags: &[],
                match_kinds: &[],
                can_handle: true,
                scope: TransformScope::Node,
            })],
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let migration = controller.diff(&path, &path).unwrap();
        let root = migration.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
    }

    // --- Comparators for dispatch tests ---

    /// Comparator that claims items by file extension, tagging output with its name.
    struct ExtensionComparator {
        name: &'static str,
        extensions: &'static [&'static str],
    }
    impl Comparator for ExtensionComparator {
        fn name(&self) -> &str {
            self.name
        }
        fn handles_extensions(&self) -> &[&str] {
            self.extensions
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(
                DiffNode::new("modify", "file", pair.logical_path())
                    .with_detail("claimed_by", serde_json::json!(self.name)),
            ))
        }
    }

    /// Comparator that claims items via `can_handle` based on a path substring check.
    struct SelectiveCanHandleComparator {
        name: &'static str,
        path_contains: &'static str,
    }
    impl Comparator for SelectiveCanHandleComparator {
        fn name(&self) -> &str {
            self.name
        }
        fn can_handle(&self, pair: &ItemPair) -> bool {
            pair.logical_path().contains(self.path_contains)
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(
                DiffNode::new("modify", "file", pair.logical_path())
                    .with_detail("claimed_by", serde_json::json!(self.name)),
            ))
        }
    }

    /// Comparator that claims items by media type.
    struct MediaTypeComparator {
        name: &'static str,
        media_types: &'static [&'static str],
    }
    impl Comparator for MediaTypeComparator {
        fn name(&self) -> &str {
            self.name
        }
        fn handles_media_types(&self) -> &[&str] {
            self.media_types
        }
        fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(
                DiffNode::new("modify", "file", pair.logical_path())
                    .with_detail("claimed_by", serde_json::json!(self.name)),
            ))
        }
    }

    #[test]
    fn dispatch_extension_match_claims_item() {
        let controller = Controller::new(
            vec![Arc::new(ExtensionComparator {
                name: "csv-comp",
                extensions: &[".csv"],
            })],
            vec![],
        );
        let pair = ItemPair::both(
            Item::new(std::path::Path::new("/tmp/a.csv"), "data.csv"),
            Item::new(std::path::Path::new("/tmp/b.csv"), "data.csv"),
        );
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("csv-comp"))
        );
    }

    #[test]
    fn dispatch_extension_mismatch_skips_comparator() {
        let controller = Controller::new(
            vec![
                Arc::new(ExtensionComparator {
                    name: "csv-comp",
                    extensions: &[".csv"],
                }),
                Arc::new(LeafComparator),
            ],
            vec![],
        );
        let pair = ItemPair::both(
            Item::new(std::path::Path::new("/tmp/a.txt"), "data.txt"),
            Item::new(std::path::Path::new("/tmp/b.txt"), "data.txt"),
        );
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        // csv-comp can't claim .txt, so LeafComparator (via can_handle) wins
        assert!(!result.details.contains_key("claimed_by"));
    }

    #[test]
    fn dispatch_can_handle_comparator_beats_later_extension_comparator() {
        let controller = Controller::new(
            vec![
                Arc::new(SelectiveCanHandleComparator {
                    name: "custom",
                    path_contains: "special",
                }),
                Arc::new(ExtensionComparator {
                    name: "csv-comp",
                    extensions: &[".csv"],
                }),
            ],
            vec![],
        );
        let pair = ItemPair::both(
            Item::new(std::path::Path::new("/tmp/a.csv"), "special.csv"),
            Item::new(std::path::Path::new("/tmp/b.csv"), "special.csv"),
        );
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("custom"))
        );
    }

    #[test]
    fn dispatch_extension_comparator_beats_later_can_handle_comparator() {
        let controller = Controller::new(
            vec![
                Arc::new(ExtensionComparator {
                    name: "csv-comp",
                    extensions: &[".csv"],
                }),
                Arc::new(SelectiveCanHandleComparator {
                    name: "custom",
                    path_contains: "special",
                }),
            ],
            vec![],
        );
        let pair = ItemPair::both(
            Item::new(std::path::Path::new("/tmp/a.csv"), "special.csv"),
            Item::new(std::path::Path::new("/tmp/b.csv"), "special.csv"),
        );
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("csv-comp"))
        );
    }

    #[test]
    fn dispatch_can_handle_falls_through_to_next_when_no_match() {
        let controller = Controller::new(
            vec![
                Arc::new(SelectiveCanHandleComparator {
                    name: "custom",
                    path_contains: "special",
                }),
                Arc::new(ExtensionComparator {
                    name: "csv-comp",
                    extensions: &[".csv"],
                }),
            ],
            vec![],
        );
        let pair = ItemPair::both(
            Item::new(std::path::Path::new("/tmp/a.csv"), "ordinary.csv"),
            Item::new(std::path::Path::new("/tmp/b.csv"), "ordinary.csv"),
        );
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("csv-comp"))
        );
    }

    #[test]
    fn dispatch_no_match_returns_error() {
        let controller = Controller::new(
            vec![Arc::new(ExtensionComparator {
                name: "csv-comp",
                extensions: &[".csv"],
            })],
            vec![],
        );
        let pair = ItemPair::both(
            Item::new(std::path::Path::new("/tmp/a.txt"), "data.txt"),
            Item::new(std::path::Path::new("/tmp/b.txt"), "data.txt"),
        );
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no comparator found"));
    }

    #[test]
    fn dispatch_media_type_match_claims_item() {
        let controller = Controller::new(
            vec![Arc::new(MediaTypeComparator {
                name: "zip-comp",
                media_types: &["application/zip"],
            })],
            vec![],
        );
        let mut right = Item::new(std::path::Path::new("/tmp/b.dat"), "data.dat");
        right.media_type = Some("application/zip".into());
        let mut left = Item::new(std::path::Path::new("/tmp/a.dat"), "data.dat");
        left.media_type = Some("application/zip".into());
        let pair = ItemPair::both(left, right);
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("zip-comp"))
        );
    }

    #[test]
    fn dispatch_extension_beats_media_type_on_same_comparator() {
        // Extension matching is checked before media type, so an extension
        // comparator earlier in the pipeline claims before a media type comparator.
        let controller = Controller::new(
            vec![
                Arc::new(ExtensionComparator {
                    name: "csv-comp",
                    extensions: &[".csv"],
                }),
                Arc::new(MediaTypeComparator {
                    name: "zip-comp",
                    media_types: &["application/zip"],
                }),
            ],
            vec![],
        );
        let mut right = Item::new(std::path::Path::new("/tmp/b.csv"), "data.csv");
        right.media_type = Some("application/zip".into());
        let mut left = Item::new(std::path::Path::new("/tmp/a.csv"), "data.csv");
        left.media_type = Some("application/zip".into());
        let pair = ItemPair::both(left, right);
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("csv-comp"))
        );
    }

    #[test]
    fn dispatch_media_type_beats_later_can_handle() {
        let controller = Controller::new(
            vec![
                Arc::new(MediaTypeComparator {
                    name: "zip-comp",
                    media_types: &["application/zip"],
                }),
                Arc::new(LeafComparator),
            ],
            vec![],
        );
        let mut right = Item::new(std::path::Path::new("/tmp/b.dat"), "data.dat");
        right.media_type = Some("application/zip".into());
        let mut left = Item::new(std::path::Path::new("/tmp/a.dat"), "data.dat");
        left.media_type = Some("application/zip".into());
        let pair = ItemPair::both(left, right);
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("zip-comp"))
        );
    }

    #[test]
    fn dispatch_media_type_skipped_for_directories() {
        // Directories should not match on media type, only via can_handle.
        let controller = Controller::new(
            vec![
                Arc::new(MediaTypeComparator {
                    name: "zip-comp",
                    media_types: &["application/zip"],
                }),
                Arc::new(ExpandComparator),
            ],
            vec![],
        );
        let dir = tempfile::tempdir().unwrap();
        let mut left = Item::new(dir.path(), "archive.zip");
        left.media_type = Some("application/zip".into());
        let mut right = Item::new(dir.path(), "archive.zip");
        right.media_type = Some("application/zip".into());
        let pair = ItemPair::both(left, right);
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        // ExpandComparator (directory handler) should win, not the media type comparator
        assert_eq!(result.item_type, "directory");
    }

    #[test]
    fn dispatch_media_type_mismatch_falls_through() {
        let controller = Controller::new(
            vec![
                Arc::new(MediaTypeComparator {
                    name: "zip-comp",
                    media_types: &["application/zip"],
                }),
                Arc::new(LeafComparator),
            ],
            vec![],
        );
        let mut right = Item::new(std::path::Path::new("/tmp/b.dat"), "data.dat");
        right.media_type = Some("text/plain".into());
        let mut left = Item::new(std::path::Path::new("/tmp/a.dat"), "data.dat");
        left.media_type = Some("text/plain".into());
        let pair = ItemPair::both(left, right);
        let ctx = Arc::new(CompareContext::new());
        let result = controller.process_pair(pair, &ctx).unwrap();
        // zip-comp doesn't match text/plain, so LeafComparator (can_handle) wins
        assert!(!result.details.contains_key("claimed_by"));
    }

    struct RemoveTransformer;
    impl Transformer for RemoveTransformer {
        fn name(&self) -> &str {
            "remove-test"
        }
        fn match_kinds(&self) -> &[&str] {
            &["modify"]
        }
        fn transform(&self, _node: DiffNode, _ctx: &CompareContext) -> TransformResult {
            TransformResult::Remove
        }
    }

    #[test]
    fn transformer_remove_eliminates_node() {
        let controller = Controller::new(
            vec![Arc::new(ExpandComparator), Arc::new(LeafComparator)],
            vec![Arc::new(RemoveTransformer)],
        );
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"x").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"y").unwrap();

        let migration = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        // RemoveTransformer matches "modify" kind. Both the root (directory)
        // and the child (file) have kind "modify", so all nodes are removed
        // and the migration should have no root.
        assert!(migration.root.is_none());
    }

    #[test]
    fn transformer_remove_splices_out_child() {
        struct RemoveFileTransformer;
        impl Transformer for RemoveFileTransformer {
            fn name(&self) -> &str {
                "remove-file"
            }
            fn match_types(&self) -> &[&str] {
                &["file"]
            }
            fn transform(&self, _node: DiffNode, _ctx: &CompareContext) -> TransformResult {
                TransformResult::Remove
            }
        }

        let controller = Controller::new(
            vec![Arc::new(ExpandComparator), Arc::new(LeafComparator)],
            vec![Arc::new(RemoveFileTransformer)],
        );
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"x").unwrap();
        std::fs::write(from_dir.path().join("b.txt"), b"y").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"x2").unwrap();
        std::fs::write(to_dir.path().join("b.txt"), b"y2").unwrap();

        let migration = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        // The directory container itself has kind "modify" but type "directory",
        // so it's not matched. Its file children are spliced out entirely.
        let root = migration.root.as_ref().unwrap();
        assert_eq!(root.item_type, "directory");
        assert!(root.children.is_empty(), "file children should be removed");
    }

    struct ReplaceManyTransformer;
    impl Transformer for ReplaceManyTransformer {
        fn name(&self) -> &str {
            "replace-many"
        }
        fn match_types(&self) -> &[&str] {
            &["file"]
        }
        fn transform(&self, node: DiffNode, _ctx: &CompareContext) -> TransformResult {
            TransformResult::ReplaceMany(vec![
                DiffNode::new("add", "file", format!("{}.part1", node.path)),
                DiffNode::new("add", "file", format!("{}.part2", node.path)),
            ])
        }
    }

    #[test]
    fn transformer_replace_many_splices_siblings() {
        let controller = Controller::new(
            vec![Arc::new(ExpandComparator), Arc::new(LeafComparator)],
            vec![Arc::new(ReplaceManyTransformer)],
        );
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"x").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"y").unwrap();

        let migration = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let root = migration.root.as_ref().unwrap();
        // The single "file" child should be replaced with two siblings.
        assert_eq!(root.children.len(), 2);
        assert!(root.children[0].path.ends_with(".part1"));
        assert!(root.children[1].path.ends_with(".part2"));
    }

    #[test]
    fn transformer_unchanged_preserves_node() {
        struct NoOpTransformer;
        impl Transformer for NoOpTransformer {
            fn name(&self) -> &str {
                "noop"
            }
            fn match_kinds(&self) -> &[&str] {
                &["modify"]
            }
            fn transform(&self, _node: DiffNode, _ctx: &CompareContext) -> TransformResult {
                TransformResult::Unchanged
            }
        }

        let controller = Controller::new(
            vec![Arc::new(LeafComparator)],
            vec![Arc::new(NoOpTransformer)],
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let migration = controller.diff(&path, &path).unwrap();
        let root = migration.root.as_ref().unwrap();
        assert_eq!(root.kind, "modify");
        assert!(!root.tags.contains("transformed"));
    }

    #[test]
    fn transformer_subtree_receives_entire_subtree() {
        let controller = Controller::new(
            vec![Arc::new(ExpandComparator), Arc::new(LeafComparator)],
            vec![Arc::new(ReplaceTransformer {
                match_types: &["directory"],
                match_tags: &[],
                match_kinds: &[],
                can_handle: false,
                scope: TransformScope::Subtree,
            })],
        );
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"").unwrap();

        let migration = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let root = migration.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
    }
}
