use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use binoc_sdk::*;

use crate::data_access::LocalDataAccess;

const MAX_CHANGESET_DIAGNOSTICS: usize = 16;

/// The core engine: processes a work queue of item pairs, dispatching to
/// comparators, assembling the diff tree, then running transformers.
/// Type-ignorant — it does not know what a directory, zip, or CSV is.
pub struct Controller {
    comparators: Vec<(Arc<dyn Comparator>, ComparatorDescriptor)>,
    transformers: Vec<(Arc<dyn Transformer>, TransformerDescriptor)>,
    transformer_configs: BTreeMap<String, serde_json::Value>,
    dataset_config: serde_json::Value,
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
            transformer_configs: BTreeMap::new(),
            dataset_config: serde_json::Value::Null,
        }
    }

    /// Attach per-transformer configuration. Keyed by transformer name
    /// (e.g. `"binoc.folder_move_detector"`); unset entries pass
    /// [`serde_json::Value::Null`] to the transformer.
    pub fn with_transformer_configs(
        mut self,
        configs: BTreeMap<String, serde_json::Value>,
    ) -> Self {
        self.transformer_configs = configs;
        self
    }

    /// Attach dataset-level semantic configuration. The controller does not
    /// deserialize this value; it is exposed to plugins under `dataset`.
    pub fn with_dataset_config(mut self, config: serde_json::Value) -> Self {
        self.dataset_config = config;
        self
    }

    fn config_for(&self, name: &str) -> serde_json::Value {
        let plugin = self
            .transformer_configs
            .get(name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if self.dataset_config.is_null() {
            return plugin;
        }

        match plugin {
            serde_json::Value::Object(mut map) => {
                map.entry("dataset")
                    .or_insert_with(|| self.dataset_config.clone());
                serde_json::Value::Object(map)
            }
            serde_json::Value::Null => serde_json::json!({
                "dataset": self.dataset_config.clone(),
            }),
            other => serde_json::json!({
                "plugin": other,
                "dataset": self.dataset_config.clone(),
            }),
        }
    }

    /// Diff two snapshots and produce a changeset.
    pub fn diff(&self, from_path: &str, to_path: &str) -> BinocResult<Changeset> {
        let data = Arc::new(LocalDataAccess::new_for_diff(
            Path::new(from_path),
            Path::new(to_path),
        )?);

        let root_pair = Self::make_root_pair(from_path, to_path, &data)?;
        let root_node = self.process_pair(root_pair, &data)?;

        let root_node = self
            .run_transformers(root_node, &data)
            .and_then(Self::prune_identical);

        // Transient session fields (`source_items`, `artifacts`) are live on the
        // wire for plugin ABI use during diffing, but they are not meaningful
        // outside this session: handles reference temp paths and the artifact
        // cache under `data_root`, which will not survive beyond this call.
        // Strip them before handing the changeset back to any caller so that
        // JSON output, renderers, snapshots, and Python/CLI code never see
        // session-local state. Extract rebuilds them on demand by replaying
        // the comparator chain.
        let mut changeset = Changeset::new(from_path, to_path, root_node);
        changeset.hoist_node_diagnostics();
        changeset.dedupe_and_cap_diagnostics(MAX_CHANGESET_DIAGNOSTICS);
        changeset.strip_transient();
        Ok(changeset)
    }

    /// Diff an ordered snapshot sequence and produce one changeset per
    /// consecutive pair.
    pub fn diff_many<S>(&self, snapshots: &[S]) -> BinocResult<Vec<Changeset>>
    where
        S: AsRef<str>,
    {
        if snapshots.len() < 2 {
            return Err(BinocError::Config(
                "diff_many requires at least two snapshots".into(),
            ));
        }

        snapshots
            .windows(2)
            .map(|pair| self.diff(pair[0].as_ref(), pair[1].as_ref()))
            .collect()
    }

    /// Build the root `ItemPair` for a diff.
    ///
    /// Directories get `logical_path = ""` (their children build relative
    /// paths). Files get the filename from `to_path` (or `from_path` as
    /// fallback) so that extension-based comparator dispatch works.
    fn make_root_pair(
        from_path: &str,
        to_path: &str,
        data: &Arc<LocalDataAccess>,
    ) -> BinocResult<ItemPair> {
        let from = Path::new(from_path);
        let to = Path::new(to_path);

        let logical = if to.is_dir() && from.is_dir() {
            ""
        } else {
            Self::filename_or_empty(to)
                .or_else(|| Self::filename_or_empty(from))
                .unwrap_or("")
        };

        let left = data.register_local(from, logical)?;
        let right = data.register_local(to, logical)?;
        Ok(ItemPair::both(left, right))
    }

    fn filename_or_empty(path: &Path) -> Option<&str> {
        path.file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty())
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

        let data = Arc::new(LocalDataAccess::new_for_diff(
            Path::new(snapshot_a),
            Path::new(snapshot_b),
        )?);
        let current_pair = if let Some(pair) =
            Self::explicit_replay_pair(target, snapshot_a, snapshot_b, &data)?
        {
            pair
        } else {
            let mut pair = Self::make_root_pair(snapshot_a, snapshot_b, &data)?;
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
                pair = comparator.reopen(&pair, node_path, data.as_ref())?;
            }
            pair
        };

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
            let results = self.apply_transformer(current, transformer, desc, data, true);
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

    /// Bottom-up traversal: recurse into children first, then apply the
    /// transformer to the current node. A transformer sees each matching
    /// node in the tree with its children already transformed.
    ///
    /// `is_root` is true only at the outermost invocation; it's used to
    /// enable `NodeShapeFilter::Root` dispatch for tree-wide transformers
    /// (correlation, folder-move roll-up).
    ///
    /// See `docs/adr/transformer_scope_yagni.md` for the traversal and
    /// dispatch design.
    fn apply_transformer(
        &self,
        mut node: DiffNode,
        transformer: &Arc<dyn Transformer>,
        desc: &TransformerDescriptor,
        data: &Arc<LocalDataAccess>,
        is_root: bool,
    ) -> Vec<DiffNode> {
        let trans_name = desc.name.clone();

        // Tree-wide (Root-scope) transformers walk the tree themselves —
        // don't pre-descend into children.
        if !matches!(desc.node_shape, NodeShapeFilter::Root) {
            node.children = node
                .children
                .into_iter()
                .flat_map(|child| self.apply_transformer(child, transformer, desc, data, false))
                .collect();
        }

        if !Self::transformer_matches(desc, &node, is_root) {
            return vec![node];
        }

        let config = self.config_for(&trans_name);
        let mut results: Vec<DiffNode> =
            match transformer.transform(node.clone(), data.as_ref(), &config) {
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
            };

        // Inflate any `pending_recompare` requests the transformer set on
        // its result nodes. Each flagged pair is re-dispatched through the
        // comparator pipeline and merged into its host node before the next
        // transformer in the pipeline sees the tree.
        for result_node in &mut results {
            self.inflate_pending_recompares(result_node, data);
        }

        results
    }

    /// Walk `node` and inflate any descendant with `pending_recompare` set:
    /// re-dispatch the pair through the comparator pipeline, then merge the
    /// resulting `item_type`, `comparator`, `source_items`, `artifacts`,
    /// `details`, and `children` into the host node. The comparator's own
    /// summary (e.g. "2 lines added") is stashed in the `content_summary`
    /// annotation so renderers can surface it without
    /// overwriting the host's move headline.
    ///
    /// `pending_recompare` is `take()`n (cleared) on every visited node,
    /// regardless of whether `process_pair` succeeds, so the field never
    /// escapes a session.
    fn inflate_pending_recompares(&self, node: &mut DiffNode, data: &Arc<LocalDataAccess>) {
        if let Some(pair) = node.pending_recompare.take() {
            let replay_pair = pair.clone();
            match self.process_pair(pair, data) {
                Ok(result) => Self::merge_recompare_result(node, replay_pair, result),
                Err(err) => {
                    node.push_diagnostic(
                        Diagnostic::warning(
                            "binoc.recompare-failed",
                            format!("Could not recompare '{}': {err}", node.path),
                        )
                        .with_location(node.path.clone()),
                    );
                }
            }
        }
        for child in &mut node.children {
            self.inflate_pending_recompares(child, data);
        }
    }

    fn merge_recompare_result(node: &mut DiffNode, pair: ItemPair, result: DiffNode) {
        if result.action == "identical" {
            node.source_items = Some(pair);
            for (k, v) in result.details {
                node.details.entry(k).or_insert(v);
            }
            if node.action == "modify"
                && node.source_path.is_none()
                && !node.tags.contains("binoc.path-change")
            {
                node.action = "identical".into();
            } else {
                node.details
                    .entry("content_identical".into())
                    .or_insert_with(|| serde_json::json!(true));
            }
            return;
        }

        node.item_type.clone_from(&result.item_type);
        if result.comparator.is_some() {
            node.comparator.clone_from(&result.comparator);
        }
        if result.source_items.is_some() {
            node.source_items = result.source_items;
        } else {
            node.source_items = Some(pair);
        }
        node.artifacts.extend(result.artifacts);
        // Union content-derived tags (e.g. binoc.content-changed,
        // binoc.lines-added) into the host so the correspondence node
        // reflects both identity and content changes.
        node.tags.extend(result.tags);
        for (k, v) in result.details {
            node.details.entry(k).or_insert(v);
        }
        if let Some(summary) = &result.summary {
            if !summary.is_empty() {
                if node.summary.is_none() {
                    node.summary = Some(summary.clone());
                }
                if node.binoc_annotation("content_summary").is_none() {
                    node.annotate_from("binoc", "content_summary", serde_json::json!(summary));
                }
            }
        }
        // Splice point: future non-Root transformers that need same-pass
        // recursion through inflated children can re-apply themselves here.
        // Subsequent transformers in the outer pipeline already see these
        // children on later iterations.
        node.children = result.children;
    }

    fn explicit_replay_pair(
        target: &DiffNode,
        snapshot_a: &str,
        snapshot_b: &str,
        data: &Arc<LocalDataAccess>,
    ) -> BinocResult<Option<ItemPair>> {
        let left_path = target
            .source_path
            .as_deref()
            .or_else(|| detail_str(target, "source_path"));
        let right_path = detail_str(target, "destination_path");
        if left_path.is_none() && right_path.is_none() {
            return Ok(None);
        }

        let logical = target.path.as_str();
        let left_rel = left_path.unwrap_or(logical);
        let right_rel = right_path.unwrap_or(logical);
        let left = data.register_local(&Path::new(snapshot_a).join(left_rel), logical)?;
        let right = data.register_local(&Path::new(snapshot_b).join(right_rel), logical)?;
        Ok(Some(ItemPair::both(left, right)))
    }

    /// All fields are AND (every non-empty field must pass).
    /// Within each field, values are OR (any value satisfies that field).
    /// Empty/default fields are unconstrained (always pass).
    /// A descriptor with all fields empty/default matches nothing.
    fn transformer_matches(desc: &TransformerDescriptor, node: &DiffNode, is_root: bool) -> bool {
        let dominated = match desc.node_shape {
            NodeShapeFilter::Container => !node.children.is_empty(),
            NodeShapeFilter::Leaf => node.children.is_empty(),
            NodeShapeFilter::Root => is_root,
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

fn detail_str<'a>(node: &'a DiffNode, key: &str) -> Option<&'a str> {
    node.details.get(key)?.as_str()
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
    }
    impl Transformer for ReplaceTransformerMock {
        fn descriptor(&self) -> TransformerDescriptor {
            TransformerDescriptor::new("replace-test")
                .with_match_types(self.match_types.clone())
                .with_match_tags(self.match_tags.clone())
                .with_match_actions(self.match_actions.clone())
        }
        fn transform(
            &self,
            node: DiffNode,
            _data: &dyn DataAccess,
            _config: &serde_json::Value,
        ) -> TransformResult {
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
    fn controller_diff_many_emits_consecutive_pairwise_changesets() {
        let dir = tempfile::tempdir().unwrap();
        let snap_a = dir.path().join("snapshot-a");
        let snap_b = dir.path().join("snapshot-b");
        let snap_c = dir.path().join("snapshot-c");
        std::fs::create_dir(&snap_a).unwrap();
        std::fs::create_dir(&snap_b).unwrap();
        std::fs::create_dir(&snap_c).unwrap();

        let snapshots = vec![
            snap_a.to_string_lossy().to_string(),
            snap_b.to_string_lossy().to_string(),
            snap_c.to_string_lossy().to_string(),
        ];

        let controller = Controller::new(vec![leaf_comparator()], vec![]);
        let changesets = controller.diff_many(&snapshots).unwrap();

        assert_eq!(changesets.len(), 2);
        assert_eq!(changesets[0].from_snapshot, snapshots[0]);
        assert_eq!(changesets[0].to_snapshot, snapshots[1]);
        assert_eq!(changesets[1].from_snapshot, snapshots[1]);
        assert_eq!(changesets[1].to_snapshot, snapshots[2]);
    }

    #[test]
    fn controller_diff_many_rejects_short_sequences() {
        let controller = Controller::new(vec![leaf_comparator()], vec![]);
        let err = controller.diff_many::<String>(&[]).unwrap_err();
        assert!(err.to_string().contains("at least two snapshots"));
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
            size: None,
            media_type: None,
            handle: "/tmp/a.csv".into(),
        };
        let right = ItemRef {
            logical_path: "data.csv".into(),
            is_dir: false,
            content_hash: None,
            size: None,
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
            size: None,
            media_type: None,
            handle: "/tmp/a.txt".into(),
        };
        let right = ItemRef {
            logical_path: "data.txt".into(),
            is_dir: false,
            content_hash: None,
            size: None,
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
        fn transform(
            &self,
            _node: DiffNode,
            _data: &dyn DataAccess,
            _config: &serde_json::Value,
        ) -> TransformResult {
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

    // ── NodeShapeFilter::Root dispatch ─────────────────────────────────

    struct RootCountingTransformer {
        count: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Transformer for RootCountingTransformer {
        fn descriptor(&self) -> TransformerDescriptor {
            TransformerDescriptor::new("root-counter").with_node_shape(NodeShapeFilter::Root)
        }
        fn transform(
            &self,
            node: DiffNode,
            _data: &dyn DataAccess,
            _config: &serde_json::Value,
        ) -> TransformResult {
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            TransformResult::Replace(Box::new(node.with_tag("root-visited")))
        }
    }

    #[test]
    fn root_shape_filter_fires_once_even_on_nested_tree() {
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(from_dir.path().join("b.txt"), b"b").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(to_dir.path().join("b.txt"), b"b modified").unwrap();

        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let controller = Controller::new(
            vec![Arc::new(DirExpandComparator), leaf_comparator()],
            vec![Arc::new(RootCountingTransformer {
                count: count.clone(),
            })],
        );
        let changeset = controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "Root transformer must fire exactly once"
        );
        let root = changeset.root.as_ref().unwrap();
        assert!(root.tags.contains("root-visited"));
    }

    #[test]
    fn root_shape_filter_does_not_match_descendants() {
        // When the root matcher is tag-gated, children with the same tag
        // must NOT fire the transformer — only the root does.
        struct Mixer {
            seen: Arc<std::sync::atomic::AtomicUsize>,
        }
        impl Transformer for Mixer {
            fn descriptor(&self) -> TransformerDescriptor {
                TransformerDescriptor::new("root-only")
                    .with_node_shape(NodeShapeFilter::Root)
                    .with_match_actions(vec!["modify".into()])
            }
            fn transform(
                &self,
                node: DiffNode,
                _data: &dyn DataAccess,
                _config: &serde_json::Value,
            ) -> TransformResult {
                self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                TransformResult::Replace(Box::new(node))
            }
        }

        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(from_dir.path().join("b.txt"), b"b").unwrap();
        std::fs::write(to_dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(to_dir.path().join("b.txt"), b"b2").unwrap();

        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let controller = Controller::new(
            vec![Arc::new(DirExpandComparator), leaf_comparator()],
            vec![Arc::new(Mixer { seen: seen.clone() })],
        );
        controller
            .diff(
                from_dir.path().to_string_lossy().as_ref(),
                to_dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn transformer_config_is_passed_through() {
        struct ConfigReader {
            out: Arc<Mutex<serde_json::Value>>,
        }
        impl Transformer for ConfigReader {
            fn descriptor(&self) -> TransformerDescriptor {
                TransformerDescriptor::new("cfg-reader").with_match_actions(vec!["modify".into()])
            }
            fn transform(
                &self,
                node: DiffNode,
                _data: &dyn DataAccess,
                config: &serde_json::Value,
            ) -> TransformResult {
                *self.out.lock().unwrap() = config.clone();
                TransformResult::Replace(Box::new(node))
            }
        }

        use std::sync::Mutex;
        let out = Arc::new(Mutex::new(serde_json::Value::Null));
        let reader = Arc::new(ConfigReader { out: out.clone() });

        let mut configs = BTreeMap::new();
        configs.insert(
            "cfg-reader".to_string(),
            serde_json::json!({ "threshold": 0.42 }),
        );

        let controller = Controller::new(vec![leaf_comparator()], vec![reader])
            .with_transformer_configs(configs)
            .with_dataset_config(serde_json::json!({
                "tables": [{
                    "logical_name": "people",
                    "columns": ["id"]
                }]
            }));
        let dir = tempfile::tempdir().unwrap();
        controller
            .diff(
                dir.path().to_string_lossy().as_ref(),
                dir.path().to_string_lossy().as_ref(),
            )
            .unwrap();
        let got = out.lock().unwrap().clone();
        assert_eq!(
            got,
            serde_json::json!({
                "threshold": 0.42,
                "dataset": {
                    "tables": [{
                        "logical_name": "people",
                        "columns": ["id"]
                    }]
                }
            })
        );
    }
}
