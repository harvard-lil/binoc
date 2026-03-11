use std::collections::BTreeMap;

use binoc_core::ir::DiffNode;
use binoc_core::traits::{CompareContext, Transformer};
use binoc_core::types::*;

/// Correlates adds/removes by content hash; collapses matching pairs into `move` nodes.
pub struct MoveDetector;

impl Transformer for MoveDetector {
    fn name(&self) -> &str {
        "binoc.move_detector"
    }

    fn match_types(&self) -> &[&str] {
        &["directory", "zip_archive"]
    }

    fn scope(&self) -> TransformScope {
        TransformScope::Subtree
    }

    fn transform(&self, mut node: DiffNode, _ctx: &CompareContext) -> TransformResult {
        let has_adds = node.children.iter().any(|c| c.kind == "add");
        let has_removes = node.children.iter().any(|c| c.kind == "remove");

        if !has_adds || !has_removes {
            return TransformResult::Unchanged;
        }

        // Group adds and removes by their content hash
        let mut add_by_hash: BTreeMap<Option<String>, Vec<usize>> = BTreeMap::new();
        let mut remove_by_hash: BTreeMap<Option<String>, Vec<usize>> = BTreeMap::new();

        for (i, child) in node.children.iter().enumerate() {
            let hash = child
                .details
                .get("hash_right")
                .or_else(|| child.details.get("hash_left"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if child.kind == "add" {
                if let Some(h) = hash {
                    add_by_hash.entry(Some(h)).or_default().push(i);
                }
            } else if child.kind == "remove" {
                if let Some(h) = hash {
                    remove_by_hash.entry(Some(h)).or_default().push(i);
                }
            }
        }

        // Find matching hash pairs
        let mut move_indices: Vec<(usize, usize)> = Vec::new(); // (remove_idx, add_idx)
        for (hash, remove_idxs) in &remove_by_hash {
            if let Some(add_idxs) = add_by_hash.get(hash) {
                let pairs = remove_idxs.len().min(add_idxs.len());
                for i in 0..pairs {
                    move_indices.push((remove_idxs[i], add_idxs[i]));
                }
            }
        }

        if move_indices.is_empty() {
            return TransformResult::Unchanged;
        }

        let mut consumed: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let mut new_children = Vec::new();

        // Create move nodes for matched pairs
        for (remove_idx, add_idx) in &move_indices {
            consumed.insert(*remove_idx);
            consumed.insert(*add_idx);

            let removed = &node.children[*remove_idx];
            let added = &node.children[*add_idx];

            let source_name = std::path::Path::new(&removed.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| removed.path.clone());
            let move_node = DiffNode::new("move", &added.item_type, &added.path)
                .with_summary(format!("Moved from {source_name}"))
                .with_source_path(&removed.path)
                .with_tag("binoc.move");

            new_children.push(move_node);
        }

        // Keep non-consumed children
        for (i, child) in node.children.into_iter().enumerate() {
            if !consumed.contains(&i) {
                new_children.push(child);
            }
        }

        new_children.sort_by(|a, b| a.path.cmp(&b.path));
        node.children = new_children;
        TransformResult::Replace(node)
    }
}
