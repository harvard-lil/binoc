use std::collections::BTreeMap;

use binoc_sdk::*;

/// Detects when an added file has the same content hash as an identical
/// (unchanged) file elsewhere in the same container.
pub struct CopyDetector;

impl Transformer for CopyDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.copy_detector")
            .with_match_types(vec!["directory".into(), "zip_archive".into()])
            .with_scope(TransformScope::Subtree)
    }

    fn transform(&self, mut node: DiffNode, _data: &dyn DataAccess) -> TransformResult {
        let has_adds = node.children.iter().any(|c| c.kind == "add");
        let has_identicals = node.children.iter().any(|c| c.kind == "identical");

        if !has_adds || !has_identicals {
            return TransformResult::Unchanged;
        }

        let mut identical_by_hash: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut add_by_hash: BTreeMap<String, Vec<usize>> = BTreeMap::new();

        for (i, child) in node.children.iter().enumerate() {
            if child.kind == "identical" {
                if let Some(hash) = extract_hash(child) {
                    identical_by_hash.entry(hash).or_default().push(i);
                }
            } else if child.kind == "add" {
                if let Some(hash) = extract_hash(child) {
                    add_by_hash.entry(hash).or_default().push(i);
                }
            }
        }

        let mut copy_pairs: Vec<(usize, usize)> = Vec::new();
        for (hash, add_idxs) in &add_by_hash {
            if let Some(identical_idxs) = identical_by_hash.get(hash) {
                for add_idx in add_idxs {
                    copy_pairs.push((identical_idxs[0], *add_idx));
                }
            }
        }

        if copy_pairs.is_empty() {
            return TransformResult::Unchanged;
        }

        let mut converted: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let mut new_children = Vec::new();

        for (identical_idx, add_idx) in &copy_pairs {
            converted.insert(*add_idx);

            let source = &node.children[*identical_idx];
            let added = &node.children[*add_idx];

            let source_name = std::path::Path::new(&source.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| source.path.clone());
            let copy_node = DiffNode::new("copy", &added.item_type, &added.path)
                .with_summary(format!("Copied from {source_name}"))
                .with_source_path(&source.path)
                .with_tag("binoc.copy");

            new_children.push(copy_node);
        }

        for (i, child) in node.children.into_iter().enumerate() {
            if !converted.contains(&i) {
                new_children.push(child);
            }
        }

        new_children.sort_by(|a, b| a.path.cmp(&b.path));
        node.children = new_children;
        TransformResult::Replace(Box::new(node))
    }
}

fn extract_hash(node: &DiffNode) -> Option<String> {
    node.details
        .get("hash")
        .or_else(|| node.details.get("hash_right"))
        .or_else(|| node.details.get("hash_left"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
