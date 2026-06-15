use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use binoc_sdk::{
    ArtifactSubject, BinocError, BinocResult, CoreRule, CorrespondenceEngineConfig, DataAccess,
    Diagnostic, Edit, EngineView, ExtractResult, ItemRef, LinkCtx, LinkRef, NodeId,
    ProjectionAnnotator, ShapeFilter, TreeSide, WriterDescriptor,
};

use super::cost::cost;
use super::project::{project, ActionLine, Projection};
use super::store::{ApplyOutcome, Store};

#[derive(Debug, Clone)]
pub struct FireEvent {
    pub round: u32,
    pub rule: String,
    pub kind: &'static str,
    pub subject: String,
}

#[derive(Debug, Default)]
pub struct RunStats {
    pub rounds: u32,
    pub invocations: BTreeMap<String, u64>,
    pub fires: BTreeMap<String, u64>,
    pub suppressed: BTreeMap<String, u64>,
    pub fires_beneath_settled: BTreeMap<String, u64>,
    pub links_added: u64,
    pub links_upgraded: u64,
    pub priorities: BTreeMap<String, u32>,
    pub events: Vec<FireEvent>,
    pub writer_used: BTreeMap<usize, String>,
    pub unwritten_links: Vec<usize>,
    pub compaction_accepted: BTreeMap<String, u64>,
    pub compaction_rejected: BTreeMap<String, u64>,
}

impl RunStats {
    fn bump(map: &mut BTreeMap<String, u64>, key: &str) {
        *map.entry(key.to_string()).or_insert(0) += 1;
    }

    pub fn fires_of(&self, rule: &str) -> u64 {
        self.fires.get(rule).copied().unwrap_or(0)
    }

    pub fn invocations_of(&self, rule: &str) -> u64 {
        self.invocations.get(rule).copied().unwrap_or(0)
    }

    pub fn suppressed_of(&self, rule: &str) -> u64 {
        self.suppressed.get(rule).copied().unwrap_or(0)
    }
}

pub struct CorrespondenceRunResult {
    pub store: Store,
    pub edit_lists: BTreeMap<usize, Vec<Edit>>,
    pub annotators: Vec<Arc<dyn ProjectionAnnotator>>,
    pub diagnostics: Vec<Diagnostic>,
    pub stats: RunStats,
}

impl CorrespondenceRunResult {
    pub fn project(&self) -> Projection {
        project(&self.store, &self.edit_lists, &self.annotators)
    }

    pub fn extract_line(
        &self,
        config: &CorrespondenceEngineConfig,
        line: &ActionLine,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<ExtractResult> {
        let link_index = line.link_index.ok_or_else(|| {
            BinocError::Extract(format!(
                "node '{}' is not backed by a correspondence link",
                line.path
            ))
        })?;
        let writer_name = self.stats.writer_used.get(&link_index).ok_or_else(|| {
            BinocError::Extract(format!(
                "node '{}' has no edit-list writer recorded",
                line.path
            ))
        })?;
        let writer = config
            .writers
            .iter()
            .find(|writer| writer.descriptor().name == *writer_name)
            .ok_or_else(|| {
                BinocError::Extract(format!("edit-list writer '{writer_name}' not registered"))
            })?;
        let link = self.store.links.link(link_index);
        let view = CoreEngineView::new(&self.store, false);
        let link_ref = view.link_ref(link_index);
        let path = &self.store.right.node(link.right).item.logical_path;
        let ctx = LinkCtx {
            view: &view,
            link: link_ref,
            row_keys: config.row_keys.get(path).map(Vec::as_slice).unwrap_or(&[]),
            row_identity_policies: config
                .row_identity_policies
                .get(path)
                .copied()
                .unwrap_or_default(),
        };
        let edits = self
            .edit_lists
            .get(&link_index)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        writer.extract(&ctx, edits, aspect, data)?.ok_or_else(|| {
            BinocError::Extract(format!(
                "edit-list writer '{writer_name}' cannot extract aspect '{aspect}' from node '{}'",
                line.path
            ))
        })
    }
}

pub fn run(
    config: &CorrespondenceEngineConfig,
    left_root: ItemRef,
    right_root: ItemRef,
    data: &dyn DataAccess,
) -> BinocResult<CorrespondenceRunResult> {
    let mut store = Store::new(left_root, right_root, config.root_projection.clone());
    let mut stats = RunStats::default();
    let mut diagnostics = Vec::new();

    let pair_count = config
        .rules
        .iter()
        .filter(|rule| matches!(rule, CoreRule::Pair(_)))
        .count() as u32;
    let mut next_pair = 0u32;
    let mut priorities = Vec::with_capacity(config.rules.len());
    for rule in &config.rules {
        if let CoreRule::Pair(pair_rule) = rule {
            let priority = pair_count - next_pair;
            next_pair += 1;
            stats
                .priorities
                .insert(pair_rule.descriptor().name, priority);
            priorities.push(priority);
        } else {
            priorities.push(0);
        }
    }

    let mut expanded: BTreeSet<(u8, u32)> = BTreeSet::new();
    let mut parsed: BTreeSet<(u8, u32, binoc_sdk::ArtifactFormat)> = BTreeSet::new();

    fn key(side: TreeSide, index: u32) -> (u8, u32) {
        (
            match side {
                TreeSide::Left => 0,
                TreeSide::Right => 1,
            },
            index,
        )
    }

    loop {
        stats.rounds += 1;
        let round = stats.rounds;
        let mut changed = false;
        let frontier = [store.left.len() as u32, store.right.len() as u32];
        let frontier_of = |side: TreeSide| match side {
            TreeSide::Left => frontier[0],
            TreeSide::Right => frontier[1],
        };

        for (rule_index, rule) in config.rules.iter().enumerate() {
            match rule {
                CoreRule::Expand(rule) => {
                    let descriptor = rule.descriptor();
                    for side in [TreeSide::Left, TreeSide::Right] {
                        for index in 0..frontier_of(side) {
                            if expanded.contains(&key(side, index)) {
                                continue;
                            }
                            let item = &store.tree(side).node(index).item;
                            if !descriptor.input.matches(item) {
                                continue;
                            }
                            RunStats::bump(&mut stats.invocations, &descriptor.name);
                            let beneath = store.beneath_settled(side, index, true);
                            if beneath && !descriptor.fires_beneath_settled {
                                RunStats::bump(&mut stats.suppressed, &descriptor.name);
                                continue;
                            }
                            let output = rule.expand(item, data)?;
                            append_rule_diagnostics(
                                &mut diagnostics,
                                &descriptor.name,
                                output.diagnostics,
                            );
                            let path = item.logical_path.clone();
                            for child in output.children {
                                let projection = child.projection_hint.clone();
                                store.tree_mut(side).add_child(index, child, projection);
                            }
                            expanded.insert(key(side, index));
                            RunStats::bump(&mut stats.fires, &descriptor.name);
                            if beneath {
                                RunStats::bump(&mut stats.fires_beneath_settled, &descriptor.name);
                            }
                            stats.events.push(FireEvent {
                                round,
                                rule: descriptor.name.clone(),
                                kind: "expand",
                                subject: format!("{}:{}", side.label(), path),
                            });
                            changed = true;
                        }
                    }
                }
                CoreRule::Parse(rule) => {
                    let descriptor = rule.descriptor();
                    for side in [TreeSide::Left, TreeSide::Right] {
                        for index in 0..frontier_of(side) {
                            let parsed_key = (key(side, index).0, index, descriptor.output.clone());
                            if parsed.contains(&parsed_key) {
                                continue;
                            }
                            let id = NodeId { side, index };
                            let item = &store.tree(side).node(index).item;
                            if !descriptor.input.matches(item) {
                                continue;
                            }
                            if descriptor.requires_link && store.links.of_node(id).is_empty() {
                                continue;
                            }
                            RunStats::bump(&mut stats.invocations, &descriptor.name);
                            let beneath = store.beneath_settled(side, index, true);
                            if beneath && !descriptor.fires_beneath_settled {
                                RunStats::bump(&mut stats.suppressed, &descriptor.name);
                                continue;
                            }
                            let output = rule.parse(item, data)?;
                            append_rule_diagnostics(
                                &mut diagnostics,
                                &descriptor.name,
                                output.diagnostics,
                            );
                            let subject = match side {
                                TreeSide::Left => ArtifactSubject::Left,
                                TreeSide::Right => ArtifactSubject::Right,
                            };
                            let artifact = data.publish_artifact(
                                &descriptor.output,
                                subject,
                                &descriptor.name,
                                &output.bytes,
                            )?;
                            let path = item.logical_path.clone();
                            store.add_artifact(id, artifact);
                            parsed.insert(parsed_key);
                            RunStats::bump(&mut stats.fires, &descriptor.name);
                            if beneath {
                                RunStats::bump(&mut stats.fires_beneath_settled, &descriptor.name);
                            }
                            stats.events.push(FireEvent {
                                round,
                                rule: descriptor.name.clone(),
                                kind: "parse",
                                subject: format!("{}:{}", side.label(), path),
                            });
                            changed = true;
                        }
                    }
                }
                CoreRule::Pair(rule) => {
                    let descriptor = rule.descriptor();
                    RunStats::bump(&mut stats.invocations, &descriptor.name);
                    let output = {
                        let view = CoreEngineView::new(&store, descriptor.sees_beneath_settled);
                        rule.propose(&view, data)?
                    };
                    append_rule_diagnostics(&mut diagnostics, &descriptor.name, output.diagnostics);
                    for proposal in output.proposals {
                        if !descriptor
                            .emits
                            .iter()
                            .any(|emit| emit == &proposal.evidence)
                        {
                            return Err(BinocError::Other(format!(
                                "pair rule '{}' emitted undeclared evidence '{}'",
                                descriptor.name, proposal.evidence
                            )));
                        }
                        if !descriptor.sees_beneath_settled
                            && (store.beneath_settled(TreeSide::Left, proposal.left, false)
                                || store.beneath_settled(TreeSide::Right, proposal.right, false))
                        {
                            RunStats::bump(&mut stats.suppressed, &descriptor.name);
                            continue;
                        }
                        let subject = format!(
                            "{} <-> {} [{}]",
                            store.left.node(proposal.left).item.logical_path,
                            store.right.node(proposal.right).item.logical_path,
                            proposal.evidence
                        );
                        let outcome =
                            store
                                .links
                                .apply(proposal, &descriptor.name, priorities[rule_index]);
                        match outcome {
                            ApplyOutcome::Added => {
                                stats.links_added += 1;
                                RunStats::bump(&mut stats.fires, &descriptor.name);
                                stats.events.push(FireEvent {
                                    round,
                                    rule: descriptor.name.clone(),
                                    kind: "link-add",
                                    subject,
                                });
                                changed = true;
                            }
                            ApplyOutcome::Upgraded => {
                                stats.links_upgraded += 1;
                                RunStats::bump(&mut stats.fires, &descriptor.name);
                                stats.events.push(FireEvent {
                                    round,
                                    rule: descriptor.name.clone(),
                                    kind: "link-upgrade",
                                    subject,
                                });
                                changed = true;
                            }
                            ApplyOutcome::Unchanged => {}
                        }
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    for revision in &store.links.revisions {
        if revision.old_priority >= revision.new_priority {
            return Err(BinocError::Other(
                "link revision priority was not monotone".into(),
            ));
        }
    }

    {
        let view = CoreEngineView::new(&store, false);
        for rule in &config.rules {
            if let CoreRule::Pair(pair_rule) = rule {
                let descriptor = pair_rule.descriptor();
                append_rule_diagnostics(
                    &mut diagnostics,
                    &descriptor.name,
                    pair_rule.final_diagnostics(&view, data)?,
                );
            }
        }
    }

    let mut edit_lists = BTreeMap::new();
    {
        let view = CoreEngineView::new(&store, false);
        let link_indexes: Vec<usize> = store.links.iter().map(|(index, _)| index).collect();
        for index in link_indexes {
            let link = store.links.link(index);
            if link.settled {
                edit_lists.insert(index, Vec::new());
                continue;
            }
            let left_id = NodeId {
                side: TreeSide::Left,
                index: link.left,
            };
            let right_id = NodeId {
                side: TreeSide::Right,
                index: link.right,
            };
            if !view.visible(left_id) || !view.visible(right_id) {
                edit_lists.insert(index, Vec::new());
                continue;
            }

            let mut written = false;
            for writer in &config.writers {
                let descriptor = writer.descriptor();
                if !writer_matches(&descriptor, &store, left_id, right_id) {
                    continue;
                }
                let Some(link_ref) = view
                    .links_of(left_id)
                    .into_iter()
                    .find(|l| l.index == index)
                else {
                    continue;
                };
                let ctx = LinkCtx {
                    view: &view,
                    link: link_ref,
                    row_keys: config
                        .row_keys
                        .get(&store.right.node(link.right).item.logical_path)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                    row_identity_policies: config
                        .row_identity_policies
                        .get(&store.right.node(link.right).item.logical_path)
                        .copied()
                        .unwrap_or_default(),
                };
                if let Some(output) = writer.write(&ctx, data)? {
                    append_rule_diagnostics(&mut diagnostics, &descriptor.name, output.diagnostics);
                    stats.writer_used.insert(index, descriptor.name.clone());
                    edit_lists.insert(index, output.edits);
                    written = true;
                    break;
                }
            }
            if !written {
                stats.unwritten_links.push(index);
                edit_lists.insert(index, Vec::new());
            }
        }
    }

    for rule in &config.compaction {
        for edits in edit_lists.values_mut() {
            if edits.is_empty() {
                continue;
            }
            if let Some(rewritten) = rule.rewrite(edits) {
                if cost(&rewritten) < cost(edits) {
                    *edits = rewritten;
                    RunStats::bump(&mut stats.compaction_accepted, rule.name());
                } else {
                    RunStats::bump(&mut stats.compaction_rejected, rule.name());
                }
            }
        }
    }

    Ok(CorrespondenceRunResult {
        store,
        edit_lists,
        annotators: config.annotators.clone(),
        diagnostics,
        stats,
    })
}

fn append_rule_diagnostics(
    target: &mut Vec<Diagnostic>,
    rule_name: &str,
    diagnostics: Vec<Diagnostic>,
) {
    target.extend(diagnostics.into_iter().map(|mut diagnostic| {
        if diagnostic.location.is_none() {
            diagnostic.location = Some(rule_name.to_string());
        } else {
            diagnostic.location = Some(format!(
                "{}:{}",
                rule_name,
                diagnostic.location.take().unwrap_or_default()
            ));
        }
        diagnostic
    }));
}

fn writer_matches(
    descriptor: &WriterDescriptor,
    store: &Store,
    left: NodeId,
    right: NodeId,
) -> bool {
    if !descriptor.input.matches(store.item(left)) || !descriptor.input.matches(store.item(right)) {
        return false;
    }
    let left_children = !store.tree(left.side).node(left.index).children.is_empty();
    let right_children = !store.tree(right.side).node(right.index).children.is_empty();
    match descriptor.shape {
        ShapeFilter::Any => {}
        ShapeFilter::Container => {
            if !left_children && !right_children {
                return false;
            }
        }
        ShapeFilter::Leaf => {
            if left_children || right_children {
                return false;
            }
        }
    }
    descriptor.formats.iter().all(|format| {
        store.artifact(left, format).is_some() && store.artifact(right, format).is_some()
    })
}

struct CoreEngineView<'a> {
    store: &'a Store,
    sees_beneath_settled: bool,
}

impl<'a> CoreEngineView<'a> {
    fn new(store: &'a Store, sees_beneath_settled: bool) -> Self {
        Self {
            store,
            sees_beneath_settled,
        }
    }

    fn link_ref(&self, index: usize) -> LinkRef {
        let link = self.store.links.link(index);
        LinkRef {
            index,
            left: NodeId {
                side: TreeSide::Left,
                index: link.left,
            },
            right: NodeId {
                side: TreeSide::Right,
                index: link.right,
            },
            evidence: link.evidence.clone(),
            proposer: link.proposer.clone(),
            priority: link.priority,
            settled: link.settled,
            projection: link.projection.clone(),
        }
    }
}

impl EngineView for CoreEngineView<'_> {
    fn root(&self, side: TreeSide) -> NodeId {
        NodeId {
            side,
            index: self.store.tree(side).root(),
        }
    }

    fn visible(&self, id: NodeId) -> bool {
        self.sees_beneath_settled || !self.store.beneath_settled(id.side, id.index, false)
    }

    fn nodes(&self, side: TreeSide) -> Vec<NodeId> {
        (0..self.store.tree(side).len() as u32)
            .map(|index| NodeId { side, index })
            .filter(|id| self.visible(*id))
            .collect()
    }

    fn item(&self, id: NodeId) -> &ItemRef {
        self.store.item(id)
    }

    fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.store
            .tree(id.side)
            .node(id.index)
            .parent
            .map(|index| NodeId {
                side: id.side,
                index,
            })
    }

    fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.store
            .tree(id.side)
            .node(id.index)
            .children
            .iter()
            .map(|&index| NodeId {
                side: id.side,
                index,
            })
            .filter(|child| self.visible(*child))
            .collect()
    }

    fn has_children(&self, id: NodeId) -> bool {
        !self.store.tree(id.side).node(id.index).children.is_empty()
    }

    fn is_linked(&self, id: NodeId) -> bool {
        !self.store.links.of_node(id).is_empty()
    }

    fn links(&self) -> Vec<LinkRef> {
        self.store
            .links
            .iter()
            .map(|(index, _)| self.link_ref(index))
            .filter(|link| self.visible(link.left) && self.visible(link.right))
            .collect()
    }

    fn links_of(&self, id: NodeId) -> Vec<LinkRef> {
        self.store
            .links
            .of_node(id)
            .iter()
            .map(|&index| self.link_ref(index))
            .filter(|link| self.visible(link.left) && self.visible(link.right))
            .collect()
    }

    fn artifact_bytes(
        &self,
        id: NodeId,
        format: &binoc_sdk::ArtifactFormat,
        data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<u8>>> {
        match self.store.artifact(id, format) {
            Some(descriptor) => data.get_artifact(descriptor),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::{
        ArtifactFormat, CompactionRule, Edit, PairRule, ParseDescriptor, ParseOutput, ParseRule,
        ProjectionHint,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingParseRule {
        name: &'static str,
        format: ArtifactFormat,
        fires: Arc<AtomicUsize>,
    }

    impl ParseRule for CountingParseRule {
        fn descriptor(&self) -> ParseDescriptor {
            ParseDescriptor {
                name: self.name.into(),
                input: Default::default(),
                output: self.format.clone(),
                requires_link: false,
                fires_beneath_settled: false,
            }
        }

        fn parse(&self, _item: &ItemRef, _data: &dyn DataAccess) -> BinocResult<ParseOutput> {
            self.fires.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1, 2, 3].into())
        }
    }

    #[test]
    fn parse_rules_are_first_claim_wins_per_node_format() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("data.bin");
        std::fs::write(&file, b"data").unwrap();
        let data = binoc_sdk::LocalDataAccess::new();
        let left = data.register_local(&file, "data.bin").unwrap();
        let right = data.register_local(&file, "data.bin").unwrap();
        let format = ArtifactFormat::new("test", "parsed", 1);
        let first_fires = Arc::new(AtomicUsize::new(0));
        let second_fires = Arc::new(AtomicUsize::new(0));
        let config = CorrespondenceEngineConfig {
            rules: vec![
                CoreRule::Parse(Arc::new(CountingParseRule {
                    name: "first",
                    format: format.clone(),
                    fires: Arc::clone(&first_fires),
                })),
                CoreRule::Parse(Arc::new(CountingParseRule {
                    name: "second",
                    format,
                    fires: Arc::clone(&second_fires),
                })),
            ],
            writers: vec![],
            compaction: vec![],
            annotators: vec![],
            row_keys: BTreeMap::new(),
            row_identity_policies: BTreeMap::new(),
            root_projection: ProjectionHint::default(),
            dataset_configurator: None,
        };

        let result = run(&config, left, right, &data).expect("run");

        assert_eq!(first_fires.load(Ordering::SeqCst), 2);
        assert_eq!(second_fires.load(Ordering::SeqCst), 0);
        assert_eq!(
            result
                .store
                .artifacts_of(NodeId {
                    side: TreeSide::Left,
                    index: 0
                })
                .len(),
            1
        );
    }

    struct NonDecreasingCompaction;

    impl CompactionRule for NonDecreasingCompaction {
        fn name(&self) -> &str {
            "non_decreasing"
        }

        fn rewrite(&self, _edits: &[Edit]) -> Option<Vec<Edit>> {
            Some(vec![
                Edit::new("test.same_cost", serde_json::json!({})),
                Edit::new("test.extra_cost", serde_json::json!({"extra": true})),
            ])
        }
    }

    struct OneEditWriter;

    impl binoc_sdk::EditListWriter for OneEditWriter {
        fn descriptor(&self) -> WriterDescriptor {
            WriterDescriptor {
                name: "one_edit".into(),
                formats: vec![],
                input: Default::default(),
                shape: ShapeFilter::Any,
            }
        }

        fn write(
            &self,
            _ctx: &LinkCtx<'_>,
            _data: &dyn DataAccess,
        ) -> BinocResult<Option<binoc_sdk::WriteOutput>> {
            Ok(Some(
                vec![Edit::new("test.same_cost", serde_json::json!({}))].into(),
            ))
        }
    }

    struct SingleRootPair;

    impl PairRule for SingleRootPair {
        fn descriptor(&self) -> binoc_sdk::PairDescriptor {
            binoc_sdk::PairDescriptor {
                name: "root_pair".into(),
                emits: vec!["root".into()],
                sees_beneath_settled: false,
            }
        }

        fn propose(
            &self,
            view: &dyn EngineView,
            _data: &dyn DataAccess,
        ) -> BinocResult<binoc_sdk::PairOutput> {
            Ok(vec![binoc_sdk::LinkProposal {
                left: view.root(TreeSide::Left).index,
                right: view.root(TreeSide::Right).index,
                evidence: "root".into(),
                settled: false,
                projection: ProjectionHint::default(),
            }]
            .into())
        }
    }

    #[test]
    fn compaction_rejects_non_decreasing_rewrites() {
        let temp = tempfile::tempdir().expect("tempdir");
        let left_file = temp.path().join("left.txt");
        let right_file = temp.path().join("right.txt");
        std::fs::write(&left_file, b"left").unwrap();
        std::fs::write(&right_file, b"right").unwrap();
        let data = binoc_sdk::LocalDataAccess::new();
        let left = data.register_local(&left_file, "file.txt").unwrap();
        let right = data.register_local(&right_file, "file.txt").unwrap();
        let config = CorrespondenceEngineConfig {
            rules: vec![CoreRule::Pair(Arc::new(SingleRootPair))],
            writers: vec![Arc::new(OneEditWriter)],
            compaction: vec![Arc::new(NonDecreasingCompaction)],
            annotators: vec![],
            row_keys: BTreeMap::new(),
            row_identity_policies: BTreeMap::new(),
            root_projection: ProjectionHint::default(),
            dataset_configurator: None,
        };

        let result = run(&config, left, right, &data).expect("run");

        assert_eq!(
            result.stats.compaction_rejected.get("non_decreasing"),
            Some(&1)
        );
        let edits = result.edit_lists.values().next().expect("edit list");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].verb, "test.same_cost");
    }
}
