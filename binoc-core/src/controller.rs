use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;

use binoc_sdk::*;

use crate::data_access::LocalDataAccess;

/// The core engine: processes a work queue of item pairs, dispatching to
/// comparators, assembling the diff tree, then running transformers.
/// Type-ignorant — it does not know what a directory, zip, or CSV is.
pub struct Controller {
    comparators: Vec<(Arc<dyn Comparator>, ComparatorDescriptor)>,
    transformers: Vec<(Arc<dyn Transformer>, TransformerDescriptor)>,
}

impl Controller {
    pub fn new(
        comparators: Vec<Arc<dyn Comparator>>,
        transformers: Vec<Arc<dyn Transformer>>,
    ) -> Self {
        let comparators = comparators
            .into_iter()
            .map(|c| {
                let desc = c.descriptor();
                (c, desc)
            })
            .collect();
        let transformers = transformers
            .into_iter()
            .map(|t| {
                let desc = t.descriptor();
                (t, desc)
            })
            .collect();

        Self {
            comparators,
            transformers,
        }
    }

    /// Diff two snapshots and produce a changeset.
    pub fn diff(&self, from_path: &str, to_path: &str) -> BinocResult<Changeset> {
        let data = Arc::new(LocalDataAccess::new());

        let left = data.register_local(Path::new(from_path), "")?;
        let right = data.register_local(Path::new(to_path), "")?;
        let root_pair = ItemPair::both(left, right);

        let root_node = self.process_pair(root_pair, &data)?;

        let root_node = self
            .run_transformers(root_node, &data)
            .and_then(Self::prune_identical);

        Ok(Changeset::new(from_path, to_path, root_node))
    }

    /// Extract data from a specific node in a changeset.
    ///
    /// Implements the reopen walk: traverses the ancestor chain calling
    /// `reopen()` on each container comparator to reconstruct the
    /// scratchpad, then `compare()` at the target leaf to regenerate
    /// artifacts, and finally `extract()` on the last toucher.
    pub fn extract(
        &self,
        changeset: &Changeset,
        node_path: &str,
        aspect: &str,
        snapshot_a: &str,
        snapshot_b: &str,
    ) -> BinocResult<ExtractResult> {
        let root = changeset
            .root
            .as_ref()
            .ok_or_else(|| BinocError::Extract("changeset has no root".into()))?;

        let target = Self::find_node(root, node_path)
            .ok_or_else(|| BinocError::Extract(format!("node not found: {node_path}")))?;

        let ancestor_chain = Self::build_ancestor_chain(root, node_path)
            .ok_or_else(|| BinocError::Extract(format!("cannot build path to {node_path}")))?;

        let data = Arc::new(LocalDataAccess::new());
        let left = data.register_local(Path::new(snapshot_a), "")?;
        let right = data.register_local(Path::new(snapshot_b), "")?;
        let mut current_pair = ItemPair::both(left, right);

        for ancestor in &ancestor_chain {
            if ancestor.path == node_path {
                break;
            }
            let comp_name = ancestor.comparator.as_deref().ok_or_else(|| {
                BinocError::Extract(format!(
                    "ancestor '{}' has no comparator recorded",
                    ancestor.path
                ))
            })?;
            let comparator = self.find_comparator_by_name(comp_name).ok_or_else(|| {
                BinocError::Extract(format!("comparator '{comp_name}' not found in registry"))
            })?;
            current_pair = comparator.reopen(&current_pair, node_path, data.as_ref())?;
        }

        let comp_name = target.comparator.as_deref().ok_or_else(|| {
            BinocError::Extract(format!("node '{}' has no comparator recorded", target.path))
        })?;
        let comparator = self.find_comparator_by_name(comp_name).ok_or_else(|| {
            BinocError::Extract(format!("comparator '{comp_name}' not found in registry"))
        })?;
        let compare_result = comparator.compare(&current_pair, data.as_ref())?;

        let mut target_node = target.clone();
        target_node.source_items = Some(current_pair);

        match compare_result {
            CompareResult::Leaf(n) | CompareResult::Expand(n, _) => {
                target_node.artifacts = n.artifacts;
            }
            _ => {}
        }

        if let Some(last_transformer_name) = target_node.transformed_by.last().cloned() {
            let transformer = self
                .find_transformer_by_name(&last_transformer_name)
                .ok_or_else(|| {
                    BinocError::Extract(format!(
                        "transformer '{last_transformer_name}' not found in registry"
                    ))
                })?;
            transformer
                .extract(&target_node, aspect, data.as_ref())
                .ok_or_else(|| {
                    BinocError::Extract(format!(
                        "transformer '{last_transformer_name}' cannot extract aspect '{aspect}' from node '{}'",
                        target_node.path
                    ))
                })
        } else {
            comparator
                .extract(&target_node, aspect, data.as_ref())
                .ok_or_else(|| {
                    BinocError::Extract(format!(
                        "comparator '{comp_name}' cannot extract aspect '{aspect}' from node '{}'",
                        target_node.path
                    ))
                })
        }
    }

    /// Build a chain of ancestor nodes from root to the target path.
    fn build_ancestor_chain<'a>(
        node: &'a DiffNode,
        target_path: &str,
    ) -> Option<Vec<&'a DiffNode>> {
        if node.path == target_path {
            return Some(vec![node]);
        }
        for child in &node.children {
            if let Some(mut chain) = Self::build_ancestor_chain(child, target_path) {
                chain.insert(0, node);
                return Some(chain);
            }
        }
        None
    }

    fn find_node<'a>(node: &'a DiffNode, target_path: &str) -> Option<&'a DiffNode> {
        if node.path == target_path {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = Self::find_node(child, target_path) {
                return Some(found);
            }
        }
        None
    }

    fn find_comparator_by_name(&self, name: &str) -> Option<Arc<dyn Comparator>> {
        self.comparators
            .iter()
            .find(|(_, d)| d.name == name)
            .map(|(c, _)| Arc::clone(c))
    }

    fn find_transformer_by_name(&self, name: &str) -> Option<Arc<dyn Transformer>> {
        self.transformers
            .iter()
            .find(|(_, d)| d.name == name)
            .map(|(t, _)| Arc::clone(t))
    }

    /// Recursively process an item pair through the comparator pipeline.
    fn process_pair(&self, pair: ItemPair, data: &Arc<LocalDataAccess>) -> BinocResult<DiffNode> {
        if let Some(hash) = pair.matching_content_hash() {
            let dominated = self
                .find_comparator_desc(&pair)
                .is_some_and(|(_, d)| d.handles_identical);
            if !dominated {
                return Ok(DiffNode::new("identical", "", pair.logical_path())
                    .with_detail("hash", serde_json::json!(hash)));
            }
        }

        for (comparator, desc) in self.matching_comparators(&pair) {
            let result = comparator.compare(&pair, data.as_ref())?;

            match result {
                CompareResult::Skip => continue,

                CompareResult::Identical => {
                    let mut node = DiffNode::new("identical", "", pair.logical_path());
                    node.comparator = Some(desc.name.clone());
                    node.source_items = Some(pair.clone());
                    Self::attach_content_hashes(&mut node, &pair);
                    return Ok(node);
                }

                CompareResult::Leaf(mut node) => {
                    node.comparator = Some(desc.name.clone());
                    node.source_items = Some(pair.clone());
                    Self::attach_content_hashes(&mut node, &pair);
                    return Ok(node);
                }

                CompareResult::Expand(mut container, children) => {
                    let child_nodes = self.process_children(children, data)?;
                    container.children = child_nodes;
                    container.comparator = Some(desc.name.clone());
                    container.source_items = Some(pair.clone());
                    Self::attach_content_hashes(&mut container, &pair);
                    return Ok(container);
                }

                _ => {
                    return Err(BinocError::Other(
                        "unknown CompareResult variant".to_string(),
                    ));
                }
            }
        }

        Err(BinocError::NoComparator(pair.logical_path().to_string()))
    }

    /// Attach content hashes from ItemRef metadata to a DiffNode.
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

    fn process_children(
        &self,
        children: Vec<ItemPair>,
        data: &Arc<LocalDataAccess>,
    ) -> BinocResult<Vec<DiffNode>> {
        let results: Vec<BinocResult<DiffNode>> = children
            .into_par_iter()
            .map(|pair| self.process_pair(pair, data))
            .collect();

        let mut nodes = Vec::new();
        for result in results {
            nodes.push(result?);
        }
        nodes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(nodes)
    }

    /// Yield comparators whose descriptors match the given pair, in pipeline order.
    fn matching_comparators(
        &self,
        pair: &ItemPair,
    ) -> impl Iterator<Item = (&Arc<dyn Comparator>, &ComparatorDescriptor)> {
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

        self.comparators
            .iter()
            .filter(move |(_, desc)| {
                match desc.scope {
                    ItemScope::Files if is_dir => return false,
                    ItemScope::Containers if !is_dir => return false,
                    _ => {}
                }

                if let Some(ref p) = path_lower {
                    if !desc.extensions.is_empty()
                        && desc
                            .extensions
                            .iter()
                            .any(|ext| p.ends_with(&ext.to_lowercase()))
                    {
                        return true;
                    }
                }

                if let Some(ref m) = media {
                    if !desc.media_types.is_empty()
                        && desc.media_types.iter().any(|mt| mt.eq_ignore_ascii_case(m))
                    {
                        return true;
                    }
                }

                desc.extensions.is_empty() && desc.media_types.is_empty()
            })
            .map(|(c, d)| (c, d))
    }

    /// Find the first comparator whose descriptor matches.
    fn find_comparator_desc(
        &self,
        pair: &ItemPair,
    ) -> Option<(&Arc<dyn Comparator>, &ComparatorDescriptor)> {
        self.matching_comparators(pair).next()
    }

    fn run_transformers(&self, root: DiffNode, data: &Arc<LocalDataAccess>) -> Option<DiffNode> {
        let mut current = root;
        for (transformer, desc) in &self.transformers {
            let results = self.apply_transformer(current, transformer, desc, data);
            match results.len() {
                0 => return None,
                1 => current = results.into_iter().next().unwrap(),
                _ => {
                    panic!(
                        "transformer '{}' returned ReplaceMany at the root level, \
                         which is not supported (a changeset must have a single root node)",
                        desc.name
                    );
                }
            }
        }
        Some(current)
    }

    fn prune_identical(node: DiffNode) -> Option<DiffNode> {
        if node.action == "identical" {
            return None;
        }

        let had_children = !node.children.is_empty();
        let children: Vec<DiffNode> = node
            .children
            .into_iter()
            .filter_map(Self::prune_identical)
            .collect();

        if had_children && children.is_empty() && node.details.is_empty() && node.tags.is_empty() {
            return None;
        }

        Some(DiffNode { children, ..node })
    }

    fn apply_transformer(
        &self,
        mut node: DiffNode,
        transformer: &Arc<dyn Transformer>,
        desc: &TransformerDescriptor,
        data: &Arc<LocalDataAccess>,
    ) -> Vec<DiffNode> {
        let trans_name = desc.name.clone();

        match desc.scope {
            TransformScope::Node => {
                node.children = node
                    .children
                    .into_iter()
                    .flat_map(|child| self.apply_transformer(child, transformer, desc, data))
                    .collect();

                if Self::transformer_matches(desc, &node) {
                    match transformer.transform(node.clone(), data.as_ref()) {
                        TransformResult::Unchanged => vec![node],
                        TransformResult::Replace(mut new_node) => {
                            new_node.transformed_by.push(trans_name);
                            vec![*new_node]
                        }
                        TransformResult::ReplaceMany(nodes) => nodes
                            .into_iter()
                            .map(|mut n| {
                                n.transformed_by.push(trans_name.clone());
                                n
                            })
                            .collect(),
                        TransformResult::Remove => vec![],
                        _ => vec![node],
                    }
                } else {
                    vec![node]
                }
            }
            TransformScope::Subtree => {
                if Self::transformer_matches(desc, &node) {
                    match transformer.transform(node.clone(), data.as_ref()) {
                        TransformResult::Unchanged => vec![node],
                        TransformResult::Replace(mut new_node) => {
                            new_node.transformed_by.push(trans_name);
                            vec![*new_node]
                        }
                        TransformResult::ReplaceMany(nodes) => nodes
                            .into_iter()
                            .map(|mut n| {
                                n.transformed_by.push(trans_name.clone());
                                n
                            })
                            .collect(),
                        TransformResult::Remove => vec![],
                        _ => vec![node],
                    }
                } else {
                    node.children = node
                        .children
                        .into_iter()
                        .flat_map(|child| self.apply_transformer(child, transformer, desc, data))
                        .collect();
                    vec![node]
                }
            }
        }
    }

    /// All fields are AND (every non-empty field must pass).
    /// Within each field, values are OR (any value satisfies that field).
    /// Empty/default fields are unconstrained (always pass).
    /// A descriptor with all fields empty/default matches nothing.
    fn transformer_matches(desc: &TransformerDescriptor, node: &DiffNode) -> bool {
        let dominated = match desc.node_shape {
            NodeShapeFilter::Container => !node.children.is_empty(),
            NodeShapeFilter::Leaf => node.children.is_empty(),
            NodeShapeFilter::Any => true,
        };
        if !dominated {
            return false;
        }

        let types_ok =
            desc.match_types.is_empty() || desc.match_types.iter().any(|t| t == &node.item_type);
        let tags_ok =
            desc.match_tags.is_empty() || desc.match_tags.iter().any(|t| node.tags.contains(t));
        let actions_ok =
            desc.match_actions.is_empty() || desc.match_actions.iter().any(|k| k == &node.action);
        let artifacts_ok = desc.match_artifacts.is_empty()
            || desc
                .match_artifacts
                .iter()
                .any(|req| node.artifacts.iter().any(|a| a.format == *req));

        if !types_ok || !tags_ok || !actions_ok || !artifacts_ok {
            return false;
        }

        // At least one non-default field must be set, otherwise the
        // descriptor is unconstrained and matches nothing.
        !matches!(desc.node_shape, NodeShapeFilter::Any)
            || !desc.match_types.is_empty()
            || !desc.match_tags.is_empty()
            || !desc.match_actions.is_empty()
            || !desc.match_artifacts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct CatchAllComparator {
        result: fn(&ItemPair) -> CompareResult,
    }
    impl Comparator for CatchAllComparator {
        fn descriptor(&self) -> ComparatorDescriptor {
            ComparatorDescriptor::new("catch-all").with_scope(ItemScope::Any)
        }
        fn compare(&self, pair: &ItemPair, _data: &dyn DataAccess) -> BinocResult<CompareResult> {
            Ok((self.result)(pair))
        }
    }

    struct ExtComparator {
        name: &'static str,
        exts: Vec<String>,
    }
    impl Comparator for ExtComparator {
        fn descriptor(&self) -> ComparatorDescriptor {
            ComparatorDescriptor::new(self.name).with_extensions(self.exts.clone())
        }
        fn compare(&self, pair: &ItemPair, _data: &dyn DataAccess) -> BinocResult<CompareResult> {
            Ok(CompareResult::Leaf(
                DiffNode::new("modify", "file", pair.logical_path())
                    .with_detail("claimed_by", serde_json::json!(self.name)),
            ))
        }
    }

    struct DirExpandComparator;
    impl Comparator for DirExpandComparator {
        fn descriptor(&self) -> ComparatorDescriptor {
            ComparatorDescriptor::new("expand").with_scope(ItemScope::Containers)
        }
        fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
            let path = pair.logical_path();
            let left_local = pair.left.as_ref().map(|i| data.local_path(i)).transpose()?;
            let right_local = pair
                .right
                .as_ref()
                .map(|i| data.local_path(i))
                .transpose()?;

            let mut children = Vec::new();
            if let (Some(l), Some(r)) = (&left_local, &right_local) {
                for name in ["a.txt", "b.txt"] {
                    let lp = l.join(name);
                    let rp = r.join(name);
                    if lp.exists() && rp.exists() {
                        let logical = if path.is_empty() {
                            name.to_string()
                        } else {
                            format!("{path}/{name}")
                        };
                        let li = data.register_local(&lp, &logical)?;
                        let ri = data.register_local(&rp, &logical)?;
                        children.push(ItemPair::both(li, ri));
                    }
                }
            }

            Ok(CompareResult::Expand(
                DiffNode::new("modify", "directory", path),
                children,
            ))
        }
    }

    struct ReplaceTransformerMock {
        match_types: Vec<String>,
        match_tags: Vec<String>,
        match_actions: Vec<String>,
        scope: TransformScope,
    }
    impl Transformer for ReplaceTransformerMock {
        fn descriptor(&self) -> TransformerDescriptor {
            TransformerDescriptor::new("replace-test")
                .with_match_types(self.match_types.clone())
                .with_match_tags(self.match_tags.clone())
                .with_match_actions(self.match_actions.clone())
                .with_scope(self.scope)
        }
        fn transform(&self, node: DiffNode, _data: &dyn DataAccess) -> TransformResult {
            TransformResult::Replace(Box::new(
                node.with_tag("transformed")
                    .with_detail("by", serde_json::json!("replace-transformer")),
            ))
        }
    }

    fn leaf_comparator() -> Arc<dyn Comparator> {
        Arc::new(CatchAllComparator {
            result: |pair| {
                CompareResult::Leaf(DiffNode::new("modify", "file", pair.logical_path()))
            },
        })
    }

    fn identical_comparator() -> Arc<dyn Comparator> {
        Arc::new(CatchAllComparator {
            result: |_| CompareResult::Identical,
        })
    }

    #[test]
    fn controller_identical_comparator_produces_no_root_diff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let controller = Controller::new(vec![identical_comparator()], vec![]);
        let changeset = controller.diff(&path, &path).unwrap();
        assert!(changeset.root.is_none());
    }

    #[test]
    fn controller_leaf_comparator_produces_leaf_node() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let controller = Controller::new(vec![leaf_comparator()], vec![]);
        let changeset = controller.diff(&path, &path).unwrap();
        let root = changeset.root.as_ref().unwrap();
        assert_eq!(root.action, "modify");
        assert_eq!(root.item_type, "file");
    }

    #[test]
    fn controller_expand_processes_children() {
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(from_dir.path().join("b.txt"), b"b").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(to_dir.path().join("b.txt"), b"b modified").unwrap();

        let controller = Controller::new(
            vec![Arc::new(DirExpandComparator), leaf_comparator()],
            vec![],
        );
        let changeset = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let root = changeset.root.as_ref().unwrap();
        assert_eq!(root.action, "modify");
        assert_eq!(root.item_type, "directory");
        assert!(!root.children.is_empty());
    }

    #[test]
    fn dispatch_extension_match() {
        let controller = Controller::new(
            vec![Arc::new(ExtComparator {
                name: "csv-comp",
                exts: vec![".csv".into()],
            })],
            vec![],
        );
        let data = Arc::new(LocalDataAccess::new());
        let left = ItemRef {
            logical_path: "data.csv".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: "/tmp/a.csv".into(),
        };
        let right = ItemRef {
            logical_path: "data.csv".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: "/tmp/b.csv".into(),
        };
        let pair = ItemPair::both(left, right);
        let result = controller.process_pair(pair, &data).unwrap();
        assert_eq!(
            result.details.get("claimed_by"),
            Some(&serde_json::json!("csv-comp"))
        );
    }

    #[test]
    fn dispatch_no_match_returns_error() {
        let controller = Controller::new(
            vec![Arc::new(ExtComparator {
                name: "csv-comp",
                exts: vec![".csv".into()],
            })],
            vec![],
        );
        let data = Arc::new(LocalDataAccess::new());
        let left = ItemRef {
            logical_path: "data.txt".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: "/tmp/a.txt".into(),
        };
        let right = ItemRef {
            logical_path: "data.txt".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: "/tmp/b.txt".into(),
        };
        let pair = ItemPair::both(left, right);
        let result = controller.process_pair(pair, &data);
        assert!(result.is_err());
    }

    #[test]
    fn dispatch_scope_filters_dirs() {
        let controller = Controller::new(
            vec![
                Arc::new(ExtComparator {
                    name: "csv-comp",
                    exts: vec![".csv".into()],
                }),
                Arc::new(DirExpandComparator),
            ],
            vec![],
        );
        let dir = tempfile::tempdir().unwrap();
        let data = Arc::new(LocalDataAccess::new());
        let left = data.register_local(dir.path(), "archive.csv").unwrap();
        let right = data.register_local(dir.path(), "archive.csv").unwrap();
        let pair = ItemPair::both(left, right);
        let result = controller.process_pair(pair, &data).unwrap();
        assert_eq!(result.item_type, "directory");
    }

    #[test]
    fn transformer_matches_by_type() {
        let controller = Controller::new(
            vec![Arc::new(CatchAllComparator {
                result: |pair| {
                    CompareResult::Leaf(DiffNode::new("modify", "csv", pair.logical_path()))
                },
            })],
            vec![Arc::new(ReplaceTransformerMock {
                match_types: vec!["csv".into()],
                match_tags: vec![],
                match_actions: vec![],
                scope: TransformScope::Node,
            })],
        );
        let dir = tempfile::tempdir().unwrap();
        let changeset = controller
            .diff(
                dir.path().to_string_lossy().as_ref(),
                dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let root = changeset.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
    }

    #[test]
    fn transformer_matches_by_action() {
        let controller = Controller::new(
            vec![leaf_comparator()],
            vec![Arc::new(ReplaceTransformerMock {
                match_types: vec![],
                match_tags: vec![],
                match_actions: vec!["modify".into()],
                scope: TransformScope::Node,
            })],
        );
        let dir = tempfile::tempdir().unwrap();
        let changeset = controller
            .diff(
                dir.path().to_string_lossy().as_ref(),
                dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let root = changeset.root.as_ref().unwrap();
        assert!(root.tags.contains("transformed"));
    }

    struct RemoveTransformerMock;
    impl Transformer for RemoveTransformerMock {
        fn descriptor(&self) -> TransformerDescriptor {
            TransformerDescriptor::new("remove-test").with_match_actions(vec!["modify".into()])
        }
        fn transform(&self, _node: DiffNode, _data: &dyn DataAccess) -> TransformResult {
            TransformResult::Remove
        }
    }

    #[test]
    fn transformer_remove_eliminates_node() {
        let controller = Controller::new(
            vec![Arc::new(DirExpandComparator), leaf_comparator()],
            vec![Arc::new(RemoveTransformerMock)],
        );
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"x").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"y").unwrap();

        let changeset = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        assert!(changeset.root.is_none());
    }

    #[test]
    fn skip_result_falls_through() {
        struct SkipComparator;
        impl Comparator for SkipComparator {
            fn descriptor(&self) -> ComparatorDescriptor {
                ComparatorDescriptor::new("skipper").with_scope(ItemScope::Any)
            }
            fn compare(
                &self,
                _pair: &ItemPair,
                _data: &dyn DataAccess,
            ) -> BinocResult<CompareResult> {
                Ok(CompareResult::Skip)
            }
        }

        let controller = Controller::new(vec![Arc::new(SkipComparator), leaf_comparator()], vec![]);
        let dir = tempfile::tempdir().unwrap();
        let changeset = controller
            .diff(
                dir.path().to_string_lossy().as_ref(),
                dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let root = changeset.root.as_ref().unwrap();
        assert_eq!(root.comparator.as_deref(), Some("catch-all"));
    }
}
