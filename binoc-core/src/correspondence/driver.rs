use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use binoc_sdk::{
    ArtifactFormat, ArtifactSubject, BinocError, BinocResult, CoreRule, CorrespondenceEngineConfig,
    DataAccess, Diagnostic, Edit, EngineView, ExpandOutput, ExpandRule, ExtractResult, ItemRef,
    LinkCtx, LinkRef, NodeId, ParseOutput, ParseRule, ProjectionAnnotator, ShapeFilter, TreeSide,
    WriterDescriptor,
};
use rayon::prelude::*;

use super::cost::cost;
use super::project::{project, ActionLine, Projection};
use super::store::{ApplyOutcome, Store};
use super::trace::{RunTrace, TraceRecorder, TraceStep};

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
    pub writer_used: BTreeMap<usize, BTreeSet<String>>,
    pub unwritten_links: Vec<usize>,
    pub compaction_accepted: BTreeMap<String, u64>,
    pub compaction_rejected: BTreeMap<String, u64>,
    pub rule_elapsed_nanos: BTreeMap<String, u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DescriptionCost {
    pub description_cost: u64,
    pub edit_cost: u64,
    pub link_count: u64,
    pub links: Vec<LinkDescriptionCost>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compaction_accepted: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compaction_rejected: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unwritten_links: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct LinkDescriptionCost {
    pub index: usize,
    pub left_path: String,
    pub right_path: String,
    pub evidence: String,
    pub proposer: String,
    /// The set of writers (artifact + structural) that contributed edits to this
    /// link, sorted. Empty when the link is settled, invisible, or unwritten.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub writers: BTreeSet<String>,
    pub settled: bool,
    pub edit_count: u64,
    pub edit_cost: u64,
}

impl RunStats {
    fn bump(map: &mut BTreeMap<String, u64>, key: &str) {
        *map.entry(key.to_string()).or_insert(0) += 1;
    }

    fn record_elapsed(&mut self, rule: &str, elapsed: Duration) {
        *self.rule_elapsed_nanos.entry(rule.to_string()).or_insert(0) += elapsed.as_nanos();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Serial,
    ParallelParse,
}

const PARALLEL_PARSE_MIN_JOBS: usize = 32;
const PARALLEL_PARSE_MAX_JOBS: usize = 1024;

impl CorrespondenceRunResult {
    pub fn description_cost(&self) -> DescriptionCost {
        let mut links = Vec::new();
        let mut edit_cost = 0;
        for (index, link) in self.store.links.iter() {
            let edits = self
                .edit_lists
                .get(&index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let link_edit_cost = cost(edits);
            edit_cost += link_edit_cost;
            links.push(LinkDescriptionCost {
                index,
                left_path: self.store.left.node(link.left).item.logical_path.clone(),
                right_path: self.store.right.node(link.right).item.logical_path.clone(),
                evidence: link.evidence.clone(),
                proposer: link.proposer.clone(),
                writers: self
                    .stats
                    .writer_used
                    .get(&index)
                    .cloned()
                    .unwrap_or_default(),
                settled: link.settled,
                edit_count: edits.len() as u64,
                edit_cost: link_edit_cost,
            });
        }
        let link_count = self.store.links.len() as u64;
        DescriptionCost {
            description_cost: edit_cost + link_count,
            edit_cost,
            link_count,
            links,
            compaction_accepted: self.stats.compaction_accepted.clone(),
            compaction_rejected: self.stats.compaction_rejected.clone(),
            unwritten_links: self.stats.unwritten_links.clone(),
        }
    }

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
        let writer_names = self.stats.writer_used.get(&link_index).ok_or_else(|| {
            BinocError::Extract(format!(
                "node '{}' has no edit-list writer recorded",
                line.path
            ))
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
        // Route the aspect request to whichever contributing writer can satisfy
        // it. Each writer sees only the provenance-scoped segment it produced, so
        // an aspect on a multi-artifact link reaches the right content type. The
        // first writer that yields a result wins.
        for writer in &config.writers {
            let descriptor = writer.descriptor();
            if !writer_names.contains(&descriptor.name) {
                continue;
            }
            let scoped = scoped_edits(edits, &writer_provenance(&descriptor));
            if let Some(result) = writer.extract(&ctx, &scoped, aspect, data)? {
                return Ok(result);
            }
        }
        Err(BinocError::Extract(format!(
            "no edit-list writer for node '{}' can extract aspect '{aspect}'",
            line.path
        )))
    }
}

pub fn run(
    config: &CorrespondenceEngineConfig,
    left_root: ItemRef,
    right_root: ItemRef,
    data: &dyn DataAccess,
) -> BinocResult<CorrespondenceRunResult> {
    run_with_execution(
        config,
        left_root,
        right_root,
        data,
        ExecutionMode::ParallelParse,
    )
}

pub fn run_with_execution(
    config: &CorrespondenceEngineConfig,
    left_root: ItemRef,
    right_root: ItemRef,
    data: &dyn DataAccess,
    execution: ExecutionMode,
) -> BinocResult<CorrespondenceRunResult> {
    run_inner(config, left_root, right_root, data, execution, None)
}

/// Run the engine in serial mode while capturing a full [`RunTrace`] for
/// replay/visualization. Serial execution keeps step ordering deterministic,
/// which matters for a step-by-step replay; the intended target is smaller
/// comparisons rather than large parallel runs.
pub fn run_traced(
    config: &CorrespondenceEngineConfig,
    left_root: ItemRef,
    right_root: ItemRef,
    data: &dyn DataAccess,
) -> BinocResult<(CorrespondenceRunResult, RunTrace)> {
    let mut recorder = TraceRecorder::default();
    let result = run_inner(
        config,
        left_root,
        right_root,
        data,
        ExecutionMode::Serial,
        Some(&mut recorder),
    )?;
    let trace = recorder.finish(&result.store, result.stats.rounds);
    Ok((result, trace))
}

fn run_inner(
    config: &CorrespondenceEngineConfig,
    left_root: ItemRef,
    right_root: ItemRef,
    data: &dyn DataAccess,
    execution: ExecutionMode,
    mut trace: Option<&mut TraceRecorder>,
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

    // Derive each parse rule's link gate from the ruleset rather than reading a
    // hand-set `requires_link` flag. A parse rule's output must exist pre-link
    // iff some pair rule declares that format in its `reads` (pair rules are the
    // only pre-link artifact consumers today — writers/annotators run post-link,
    // and expand reads raw data, not artifacts). Two fields that can never
    // legally disagree collapse into one derived predicate.
    //
    // Limitation: `descriptor.output` is a parse rule's PRIMARY format only.
    // Child artifacts published via `ParsedChild` (e.g. the stacked-table parser
    // emitting child `tabular_v1` artifacts) are not declared in the descriptor,
    // so this derivation can't see them. Harmless for the current ruleset (no
    // pair rule needs a format produced only as an undeclared child pre-link);
    // see docs/adr/2026-06-13-derived_requires_link.md.
    let preconsumed_formats: BTreeSet<ArtifactFormat> = config
        .rules
        .iter()
        .filter_map(|rule| match rule {
            CoreRule::Pair(pair_rule) => Some(pair_rule.descriptor().reads),
            _ => None,
        })
        .flatten()
        .collect();

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
                    let mut jobs = Vec::new();
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
                            jobs.push(ExpandJob {
                                side,
                                index,
                                beneath,
                                item: item.clone(),
                            });
                        }
                    }
                    let started = Instant::now();
                    let results = run_expand_jobs(execution, &jobs, rule.as_ref(), data);
                    stats.record_elapsed(&descriptor.name, started.elapsed());
                    for result in results {
                        let path = result.item.logical_path.clone();
                        let output = match result.output {
                            Ok(output) => output,
                            Err(err) => {
                                let diagnostic = rule_failure_diagnostic(
                                    "expand",
                                    &descriptor.name,
                                    result.side,
                                    &path,
                                    err,
                                )?;
                                append_rule_diagnostics(
                                    &mut diagnostics,
                                    &descriptor.name,
                                    vec![diagnostic],
                                );
                                expanded.insert(key(result.side, result.index));
                                continue;
                            }
                        };
                        append_rule_diagnostics(
                            &mut diagnostics,
                            &descriptor.name,
                            output.diagnostics,
                        );
                        let mut child_indices = Vec::new();
                        for child in output.children {
                            let projection = child.projection_hint.clone();
                            let child_index = store.tree_mut(result.side).add_child(
                                result.index,
                                child,
                                projection,
                            );
                            child_indices.push(child_index);
                        }
                        expanded.insert(key(result.side, result.index));
                        RunStats::bump(&mut stats.fires, &descriptor.name);
                        if result.beneath {
                            RunStats::bump(&mut stats.fires_beneath_settled, &descriptor.name);
                        }
                        stats.events.push(FireEvent {
                            round,
                            rule: descriptor.name.clone(),
                            kind: "expand",
                            subject: format!("{}:{}", result.side.label(), path),
                        });
                        if let Some(trace) = trace.as_deref_mut() {
                            let step = trace.push(TraceStep::Expand {
                                round,
                                rule: descriptor.name.clone(),
                                side: result.side,
                                node: result.index,
                                children: child_indices.clone(),
                            });
                            for &child_index in &child_indices {
                                trace.record_node_birth(result.side, child_index, step);
                            }
                        }
                        changed = true;
                    }
                }
                CoreRule::Parse(rule) => {
                    let descriptor = rule.descriptor();
                    let mut jobs = Vec::new();
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
                            // Link-gated unless some pair rule consumes this
                            // output format pre-link (see `preconsumed_formats`).
                            if !preconsumed_formats.contains(&descriptor.output)
                                && store.links.of_node(id).is_empty()
                            {
                                continue;
                            }
                            RunStats::bump(&mut stats.invocations, &descriptor.name);
                            let beneath = store.beneath_settled(side, index, true);
                            if beneath && !descriptor.fires_beneath_settled {
                                RunStats::bump(&mut stats.suppressed, &descriptor.name);
                                continue;
                            }
                            jobs.push(ParseJob {
                                id,
                                parsed_key,
                                beneath,
                                item: item.clone(),
                            });
                        }
                    }
                    let started = Instant::now();
                    let results = run_parse_jobs(execution, &jobs, rule.as_ref(), data);
                    stats.record_elapsed(&descriptor.name, started.elapsed());
                    for result in results {
                        let path = result.item.logical_path.clone();
                        let output = match result.output {
                            Ok(output) => output,
                            Err(err) => {
                                let diagnostic = rule_failure_diagnostic(
                                    "parse",
                                    &descriptor.name,
                                    result.id.side,
                                    &path,
                                    err,
                                )?;
                                append_rule_diagnostics(
                                    &mut diagnostics,
                                    &descriptor.name,
                                    vec![diagnostic],
                                );
                                parsed.insert(result.parsed_key);
                                continue;
                            }
                        };
                        append_rule_diagnostics(
                            &mut diagnostics,
                            &descriptor.name,
                            output.diagnostics,
                        );
                        // A parse rule may decline a node by returning empty bytes
                        // and no children — a content self-filter. This publishes
                        // no artifact (freeing the output format) but memoizes the
                        // decision so the rule is not retried. Used, e.g., by the
                        // JSON parsers to split record collections (-> tabular)
                        // from everything else (-> structured_document).
                        if output.bytes.is_empty() && output.children.is_empty() {
                            parsed.insert(result.parsed_key);
                            continue;
                        }
                        // Let the parse rule name the node it just decomposed
                        // (e.g. a container parse labeling its node "SQLite
                        // database"); fields it sets win over the prior guess.
                        if !binoc_sdk::projection_hint_is_default(&output.projection) {
                            store
                                .tree_mut(result.id.side)
                                .node_mut(result.id.index)
                                .projection
                                .overlay_from(&output.projection);
                        }
                        let subject = match result.id.side {
                            TreeSide::Left => ArtifactSubject::Left,
                            TreeSide::Right => ArtifactSubject::Right,
                        };
                        // A "container parse" emits only children (empty parent
                        // bytes); the parent becomes a plain container node with no
                        // artifact, exactly like an archive expansion. Publish the
                        // parent artifact only when there are parent bytes.
                        if !output.bytes.is_empty() {
                            let artifact = data.publish_artifact(
                                &descriptor.output,
                                subject,
                                &descriptor.name,
                                &output.bytes,
                            )?;
                            store.add_artifact(result.id, artifact);
                        }
                        // Secondary artifacts on the parsed node itself (e.g. a
                        // `parser_metadata_v1` bag alongside the primary
                        // `tabular_v1`, or on a container with empty `bytes`).
                        // Like child artifacts, these are not declared in the
                        // descriptor's primary `output`.
                        for extra in output.artifacts {
                            let artifact = data.publish_artifact(
                                &extra.format,
                                subject,
                                &descriptor.name,
                                &extra.bytes,
                            )?;
                            store.add_artifact(result.id, artifact);
                        }
                        let mut child_indices = Vec::new();
                        for child in output.children {
                            let projection = child.item.projection_hint.clone();
                            let child_index = store.tree_mut(result.id.side).add_child(
                                result.id.index,
                                child.item,
                                projection,
                            );
                            child_indices.push(child_index);
                            let child_id = NodeId {
                                side: result.id.side,
                                index: child_index,
                            };
                            for child_artifact in child.artifacts {
                                let artifact = data.publish_artifact(
                                    &child_artifact.format,
                                    subject,
                                    &descriptor.name,
                                    &child_artifact.bytes,
                                )?;
                                store.add_artifact(child_id, artifact);
                            }
                        }
                        parsed.insert(result.parsed_key);
                        RunStats::bump(&mut stats.fires, &descriptor.name);
                        if result.beneath {
                            RunStats::bump(&mut stats.fires_beneath_settled, &descriptor.name);
                        }
                        stats.events.push(FireEvent {
                            round,
                            rule: descriptor.name.clone(),
                            kind: "parse",
                            subject: format!("{}:{}", result.id.side.label(), path),
                        });
                        if let Some(trace) = trace.as_deref_mut() {
                            let step = trace.push(TraceStep::Parse {
                                round,
                                rule: descriptor.name.clone(),
                                side: result.id.side,
                                node: result.id.index,
                                format: descriptor.output.to_string(),
                                children: child_indices.clone(),
                            });
                            for &child_index in &child_indices {
                                trace.record_node_birth(result.id.side, child_index, step);
                            }
                        }
                        changed = true;
                    }
                }
                CoreRule::Pair(rule) => {
                    let descriptor = rule.descriptor();
                    RunStats::bump(&mut stats.invocations, &descriptor.name);
                    let started = Instant::now();
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
                        let proposal_left = proposal.left;
                        let proposal_right = proposal.right;
                        let proposal_evidence = proposal.evidence.clone();
                        let proposal_settled = proposal.settled;
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
                                if let Some(trace) = trace.as_deref_mut() {
                                    let link = link_index_of(&store, proposal_left, proposal_right);
                                    let step = trace.push(TraceStep::LinkAdd {
                                        round,
                                        rule: descriptor.name.clone(),
                                        link,
                                        left: proposal_left,
                                        right: proposal_right,
                                        evidence: proposal_evidence,
                                        settled: proposal_settled,
                                    });
                                    trace.record_link_birth(link, step);
                                }
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
                                if let Some(trace) = trace.as_deref_mut() {
                                    let link = link_index_of(&store, proposal_left, proposal_right);
                                    let revision = store.links.revisions.last();
                                    trace.push(TraceStep::LinkUpgrade {
                                        round,
                                        rule: descriptor.name.clone(),
                                        link,
                                        old_evidence: revision
                                            .map(|r| r.old_evidence.clone())
                                            .unwrap_or_default(),
                                        new_evidence: proposal_evidence,
                                        old_proposer: revision
                                            .map(|r| r.old_proposer.clone())
                                            .unwrap_or_default(),
                                        new_proposer: descriptor.name.clone(),
                                        settled: proposal_settled,
                                    });
                                }
                                changed = true;
                            }
                            ApplyOutcome::Unchanged => {}
                        }
                    }
                    stats.record_elapsed(&descriptor.name, started.elapsed());
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

            let Some(link_ref) = view
                .links_of(left_id)
                .into_iter()
                .find(|l| l.index == index)
            else {
                edit_lists.insert(index, Vec::new());
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

            // Dispatch composes, then selects (CFM-81). The link's edit list is
            // the concatenation of:
            //   * one artifact writer per *present* artifact format — within a
            //     format, registration order picks the first writer that returns
            //     `Some` (the substitutability / dialect axis);
            //   * each applicable structural writer (empty `formats`), excluding
            //     the fallback;
            //   * the fallback, but only when nothing above claimed the link.
            // Each contribution's edits are stamped with the producer's
            // provenance so downstream compaction/extract/summary stay
            // per-content-type. Ordering is deterministic: artifact formats in
            // sorted order, then structural writers in registration order.
            let mut writers_used: BTreeSet<String> = BTreeSet::new();
            let mut artifact_segments: BTreeMap<ArtifactFormat, Vec<Edit>> = BTreeMap::new();
            let mut structural_edits: Vec<Edit> = Vec::new();
            let mut fallback: Option<&Arc<dyn binoc_sdk::EditListWriter>> = None;
            // A link is "claimed" once any non-fallback writer returns `Some`
            // (even an empty edit list), exactly as the old first-match loop's
            // `break` claimed it. The fallback fires only on unclaimed links.
            let mut claimed = false;

            for writer in &config.writers {
                let descriptor = writer.descriptor();
                if !writer_matches(&descriptor, &store, left_id, right_id) {
                    continue;
                }
                let is_fallback = descriptor.formats.is_empty() && is_fallback_writer(&descriptor);
                if is_fallback {
                    // Defer the fallback until we know whether the link was
                    // claimed by anything else.
                    fallback.get_or_insert(writer);
                    continue;
                }
                let provenance = writer_provenance(&descriptor);
                if descriptor.formats.is_empty() {
                    // Structural writer (container/text).
                    if let Some(output) = writer.write(&ctx, data)? {
                        append_rule_diagnostics(
                            &mut diagnostics,
                            &descriptor.name,
                            output.diagnostics,
                        );
                        claimed = true;
                        writers_used.insert(descriptor.name.clone());
                        structural_edits.extend(stamp(output.edits, &provenance));
                    }
                } else {
                    // Artifact writer. Within a format, the first writer (by
                    // registration order) that returns `Some` wins; later
                    // writers for an already-produced format are skipped.
                    let format = descriptor.formats[0].clone();
                    if artifact_segments.contains_key(&format) {
                        continue;
                    }
                    if let Some(output) = writer.write(&ctx, data)? {
                        append_rule_diagnostics(
                            &mut diagnostics,
                            &descriptor.name,
                            output.diagnostics,
                        );
                        claimed = true;
                        writers_used.insert(descriptor.name.clone());
                        artifact_segments.insert(format, stamp(output.edits, &provenance));
                    }
                }
            }

            // Concatenate in deterministic order: artifact formats sorted, then
            // structural contributions.
            let mut composed: Vec<Edit> = Vec::new();
            for (_format, segment) in artifact_segments {
                composed.extend(segment);
            }
            composed.extend(structural_edits);

            if !claimed {
                if let Some(writer) = fallback {
                    let descriptor = writer.descriptor();
                    if let Some(output) = writer.write(&ctx, data)? {
                        append_rule_diagnostics(
                            &mut diagnostics,
                            &descriptor.name,
                            output.diagnostics,
                        );
                        claimed = true;
                        writers_used.insert(descriptor.name.clone());
                        composed.extend(stamp(output.edits, &writer_provenance(&descriptor)));
                    }
                }
            }

            if let Some(trace) = trace.as_deref_mut() {
                if !writers_used.is_empty() {
                    trace.push(TraceStep::Write {
                        writers: writers_used.iter().cloned().collect(),
                        link: index,
                        edits: composed.clone(),
                    });
                }
            }

            if claimed {
                stats.writer_used.insert(index, writers_used);
            } else {
                stats.unwritten_links.push(index);
            }
            edit_lists.insert(index, composed);
        }
    }

    {
        let view = CoreEngineView::new(&store, false);
        for rule in &config.compaction {
            for (link_index, edits) in edit_lists.iter_mut() {
                if edits.is_empty() {
                    continue;
                }
                let link = store.links.link(*link_index);
                let ctx = LinkCtx {
                    view: &view,
                    link: view.link_ref(*link_index),
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
                // Format-scoped compaction (CFM-81): a rule that declares a
                // `format()` sees and rewrites only the provenance-scoped segment
                // of that format, never the whole mixed list. A rule with no
                // declared format keeps operating on the full list.
                let scope = rule.format().map(|format| format.to_string());
                let scoped: Vec<Edit> = match &scope {
                    Some(provenance) => scoped_edits(edits, provenance),
                    None => edits.clone(),
                };
                if scoped.is_empty() {
                    continue;
                }
                if let Some(rewritten_segment) = rule.rewrite(&ctx, &scoped, data)? {
                    // Re-stamp the rewritten segment with its format provenance
                    // (rewrite rules synthesize fresh edits without it), then
                    // splice it back into the full list at the position of the
                    // first edit it replaced — preserving other content types'
                    // edits in place.
                    let rewritten = match &scope {
                        Some(provenance) => {
                            splice_scoped(edits, provenance, stamp(rewritten_segment, provenance))
                        }
                        None => rewritten_segment,
                    };
                    let accepted = cost(&rewritten) < cost(edits);
                    if let Some(trace) = trace.as_deref_mut() {
                        trace.push(TraceStep::Compact {
                            rule: rule.name().to_string(),
                            link: *link_index,
                            accepted,
                            before: edits.clone(),
                            after: rewritten.clone(),
                        });
                    }
                    if accepted {
                        *edits = rewritten;
                        RunStats::bump(&mut stats.compaction_accepted, rule.name());
                    } else {
                        RunStats::bump(&mut stats.compaction_rejected, rule.name());
                    }
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

#[derive(Debug, Clone)]
struct ExpandJob {
    side: TreeSide,
    index: u32,
    beneath: bool,
    item: ItemRef,
}

struct ExpandJobResult {
    side: TreeSide,
    index: u32,
    beneath: bool,
    item: ItemRef,
    output: BinocResult<ExpandOutput>,
}

fn run_expand_jobs(
    execution: ExecutionMode,
    jobs: &[ExpandJob],
    rule: &dyn ExpandRule,
    data: &dyn DataAccess,
) -> Vec<ExpandJobResult> {
    let run_one = |job: &ExpandJob| ExpandJobResult {
        side: job.side,
        index: job.index,
        beneath: job.beneath,
        item: job.item.clone(),
        output: rule.expand(&job.item, data),
    };
    match execution {
        ExecutionMode::Serial => jobs.iter().map(run_one).collect(),
        ExecutionMode::ParallelParse => jobs.iter().map(run_one).collect(),
    }
}

#[derive(Debug, Clone)]
struct ParseJob {
    id: NodeId,
    parsed_key: (u8, u32, ArtifactFormat),
    beneath: bool,
    item: ItemRef,
}

struct ParseJobResult {
    id: NodeId,
    parsed_key: (u8, u32, ArtifactFormat),
    beneath: bool,
    item: ItemRef,
    output: BinocResult<ParseOutput>,
}

fn run_parse_jobs(
    execution: ExecutionMode,
    jobs: &[ParseJob],
    rule: &dyn ParseRule,
    data: &dyn DataAccess,
) -> Vec<ParseJobResult> {
    let run_one = |job: &ParseJob| ParseJobResult {
        id: job.id,
        parsed_key: job.parsed_key.clone(),
        beneath: job.beneath,
        item: job.item.clone(),
        output: rule.parse(&job.item, data),
    };
    match execution {
        ExecutionMode::Serial => jobs.iter().map(run_one).collect(),
        ExecutionMode::ParallelParse if should_parallelize_parse(jobs.len()) => {
            jobs.par_iter().map(run_one).collect()
        }
        ExecutionMode::ParallelParse => jobs.iter().map(run_one).collect(),
    }
}

fn should_parallelize_parse(job_count: usize) -> bool {
    (PARALLEL_PARSE_MIN_JOBS..=PARALLEL_PARSE_MAX_JOBS).contains(&job_count)
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

fn rule_failure_diagnostic(
    kind: &str,
    rule_name: &str,
    side: TreeSide,
    path: &str,
    err: BinocError,
) -> BinocResult<Diagnostic> {
    if matches!(err, BinocError::PathPolicy(_)) {
        return Err(err);
    }
    Ok(Diagnostic::error(
        format!("binoc.rule.{kind}_failed"),
        format!("{kind} rule '{rule_name}' failed for {side:?} node '{path}': {err}"),
    )
    .with_location(format!("{}:{path}", side.label())))
}

/// Look up the link index for a just-applied `(left, right)` node pair. Used by
/// the trace recorder; the link is guaranteed to exist immediately after
/// `LinkStore::apply` returns `Added` or `Upgraded`.
fn link_index_of(store: &Store, left: u32, right: u32) -> usize {
    store
        .links
        .of_left(left)
        .iter()
        .copied()
        .find(|&index| store.links.link(index).right == right)
        .expect("applied link must exist in the link store")
}

/// The provenance key for a writer's edits. Artifact writers tag with their
/// (single) artifact format's display string; structural writers tag with their
/// own name. This is the key compaction (`CompactionRule::format`) and extract
/// route on to stay per-content-type within a link's merged edit list.
fn writer_provenance(descriptor: &WriterDescriptor) -> String {
    match descriptor.formats.first() {
        Some(format) => format.to_string(),
        None => descriptor.name.clone(),
    }
}

/// Whether a writer is the deferred last-resort fallback (CFM-81).
fn is_fallback_writer(descriptor: &WriterDescriptor) -> bool {
    descriptor.fallback
}

/// Stamp every edit with `provenance`, overwriting any prior tag. Applied to a
/// writer's output and to compaction rewrites so a content type's segment stays
/// identifiable after rewriting.
fn stamp(edits: Vec<Edit>, provenance: &str) -> Vec<Edit> {
    edits
        .into_iter()
        .map(|edit| edit.with_provenance(provenance))
        .collect()
}

/// The edits in `edits` tagged with `provenance`, in order.
fn scoped_edits(edits: &[Edit], provenance: &str) -> Vec<Edit> {
    edits
        .iter()
        .filter(|edit| edit.provenance.as_deref() == Some(provenance))
        .cloned()
        .collect()
}

/// Replace the `provenance`-scoped run within `original` with `replacement`,
/// keeping all other-provenance edits in their original positions. The
/// replacement is inserted at the position of the first matching edit; if no
/// edit currently carries `provenance`, the replacement is appended.
fn splice_scoped(original: &[Edit], provenance: &str, replacement: Vec<Edit>) -> Vec<Edit> {
    let mut out = Vec::with_capacity(original.len() + replacement.len());
    let mut inserted = false;
    for edit in original {
        if edit.provenance.as_deref() == Some(provenance) {
            if !inserted {
                out.extend(replacement.iter().cloned());
                inserted = true;
            }
        } else {
            out.push(edit.clone());
        }
    }
    if !inserted {
        out.extend(replacement);
    }
    out
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
                fires_beneath_settled: false,
            }
        }

        fn parse(&self, _item: &ItemRef, _data: &dyn DataAccess) -> BinocResult<ParseOutput> {
            self.fires.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1, 2, 3].into())
        }
    }

    /// Declares a pre-link read of `format` without proposing any links, so the
    /// derived gate treats parse rules producing `format` as un-gated even on
    /// unlinked nodes.
    struct ReadsFormatPair {
        format: ArtifactFormat,
    }

    impl PairRule for ReadsFormatPair {
        fn descriptor(&self) -> binoc_sdk::PairDescriptor {
            binoc_sdk::PairDescriptor {
                name: "reads_format".into(),
                emits: vec![],
                reads: vec![self.format.clone()],
                sees_beneath_settled: false,
            }
        }

        fn propose(
            &self,
            _view: &dyn EngineView,
            _data: &dyn DataAccess,
        ) -> BinocResult<binoc_sdk::PairOutput> {
            Ok(Vec::new().into())
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
                    format: format.clone(),
                    fires: Arc::clone(&second_fires),
                })),
                CoreRule::Pair(Arc::new(ReadsFormatPair { format })),
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

        fn rewrite(
            &self,
            _ctx: &LinkCtx<'_>,
            _edits: &[Edit],
            _data: &dyn DataAccess,
        ) -> BinocResult<Option<Vec<Edit>>> {
            Ok(Some(vec![
                Edit::new("test.same_cost", serde_json::json!({})),
                Edit::new("test.extra_cost", serde_json::json!({"extra": true})),
            ]))
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
                fallback: false,
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
                reads: vec![],
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

    // ── CFM-81 composition fixtures ──────────────────────────────────────
    //
    // A parse rule that publishes TWO orthogonal artifacts on the parsed node:
    // its primary `bytes` artifact (format = descriptor.output) plus a second
    // artifact in a different format. This is the multi-artifact-per-node shape
    // that composing dispatch exists to render.
    struct TwoArtifactParse {
        primary: ArtifactFormat,
        secondary: ArtifactFormat,
    }

    impl ParseRule for TwoArtifactParse {
        fn descriptor(&self) -> ParseDescriptor {
            ParseDescriptor {
                name: "two_artifact".into(),
                input: Default::default(),
                output: self.primary.clone(),
                fires_beneath_settled: false,
            }
        }

        fn parse(&self, _item: &ItemRef, _data: &dyn DataAccess) -> BinocResult<ParseOutput> {
            Ok(ParseOutput {
                bytes: vec![1],
                artifacts: vec![binoc_sdk::ParsedArtifact {
                    format: self.secondary.clone(),
                    bytes: vec![2],
                }],
                ..Default::default()
            })
        }
    }

    /// An artifact writer for one format that emits `verb` `count` times
    /// (provenance is stamped by the dispatcher, not the writer).
    struct OneFormatWriter {
        name: &'static str,
        format: ArtifactFormat,
        verb: &'static str,
        count: usize,
    }

    impl binoc_sdk::EditListWriter for OneFormatWriter {
        fn descriptor(&self) -> WriterDescriptor {
            WriterDescriptor {
                name: self.name.into(),
                formats: vec![self.format.clone()],
                input: Default::default(),
                shape: ShapeFilter::Any,
                fallback: false,
            }
        }

        fn write(
            &self,
            _ctx: &LinkCtx<'_>,
            _data: &dyn DataAccess,
        ) -> BinocResult<Option<binoc_sdk::WriteOutput>> {
            Ok(Some(
                (0..self.count)
                    .map(|_| Edit::new(self.verb, serde_json::json!({})))
                    .collect::<Vec<_>>()
                    .into(),
            ))
        }
    }

    /// A structural writer (empty formats) emitting one edit.
    struct StructuralEditWriter;

    impl binoc_sdk::EditListWriter for StructuralEditWriter {
        fn descriptor(&self) -> WriterDescriptor {
            WriterDescriptor {
                name: "structural".into(),
                formats: vec![],
                input: Default::default(),
                shape: ShapeFilter::Any,
                fallback: false,
            }
        }

        fn write(
            &self,
            _ctx: &LinkCtx<'_>,
            _data: &dyn DataAccess,
        ) -> BinocResult<Option<binoc_sdk::WriteOutput>> {
            Ok(Some(
                vec![Edit::new("structural.note", serde_json::json!({}))].into(),
            ))
        }
    }

    /// A compaction rule scoped to one format that collapses its segment to a
    /// single cheaper edit. It asserts it only ever sees its own format's edits.
    struct FormatScopedCompaction {
        format: ArtifactFormat,
    }

    impl CompactionRule for FormatScopedCompaction {
        fn name(&self) -> &str {
            "format_scoped"
        }

        fn format(&self) -> Option<ArtifactFormat> {
            Some(self.format.clone())
        }

        fn rewrite(
            &self,
            _ctx: &LinkCtx<'_>,
            edits: &[Edit],
            _data: &dyn DataAccess,
        ) -> BinocResult<Option<Vec<Edit>>> {
            // Format-scoping guarantee: the rule only receives its own segment.
            assert!(
                edits.iter().all(|edit| edit.verb == "artifact_a.edit"),
                "format-scoped compaction saw a foreign edit: {:?}",
                edits.iter().map(|e| &e.verb).collect::<Vec<_>>()
            );
            Ok(Some(vec![Edit::new(
                "artifact_a.compacted",
                serde_json::json!({}),
            )]))
        }
    }

    #[test]
    fn dispatch_composes_artifacts_then_structural_with_scoped_compaction() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("data.bin");
        std::fs::write(&file, b"data").unwrap();
        let data = binoc_sdk::LocalDataAccess::new();
        let left = data.register_local(&file, "data.bin").unwrap();
        let right = data.register_local(&file, "data.bin").unwrap();
        // Two formats whose Display strings sort A < B, so concatenation order is
        // deterministic and checkable.
        let format_a = ArtifactFormat::new("test", "artifact_a", 1);
        let format_b = ArtifactFormat::new("test", "artifact_b", 1);
        let config = CorrespondenceEngineConfig {
            rules: vec![
                CoreRule::Parse(Arc::new(TwoArtifactParse {
                    primary: format_a.clone(),
                    secondary: format_b.clone(),
                })),
                CoreRule::Pair(Arc::new(SingleRootPair)),
            ],
            writers: vec![
                // Registration order intentionally B-before-A and structural in
                // the middle, to prove ordering is by format (sorted), not by
                // registration, and that structural always lands last.
                Arc::new(OneFormatWriter {
                    name: "writer_b",
                    format: format_b.clone(),
                    verb: "artifact_b.edit",
                    count: 1,
                }),
                Arc::new(StructuralEditWriter),
                // A emits two edits so the scoped compaction (2 -> 1) genuinely
                // reduces cost and is accepted.
                Arc::new(OneFormatWriter {
                    name: "writer_a",
                    format: format_a.clone(),
                    verb: "artifact_a.edit",
                    count: 2,
                }),
            ],
            compaction: vec![Arc::new(FormatScopedCompaction {
                format: format_a.clone(),
            })],
            annotators: vec![],
            row_keys: BTreeMap::new(),
            row_identity_policies: BTreeMap::new(),
            root_projection: ProjectionHint::default(),
            dataset_configurator: None,
        };

        let result = run(&config, left, right, &data).expect("run");

        let (&index, edits) = result
            .edit_lists
            .iter()
            .find(|(_, edits)| !edits.is_empty())
            .expect("a written link");

        // Composition + deterministic ordering: artifact formats sorted (A then
        // B), structural last. Compaction rewrote only A's segment.
        let verbs: Vec<&str> = edits.iter().map(|edit| edit.verb.as_str()).collect();
        assert_eq!(
            verbs,
            vec!["artifact_a.compacted", "artifact_b.edit", "structural.note"]
        );

        // Provenance: each edit carries its producer's tag. The compacted A edit
        // keeps A's format provenance; B carries B's format; structural carries
        // the writer name.
        assert_eq!(
            edits[0].provenance.as_deref(),
            Some(format_a.to_string().as_str())
        );
        assert_eq!(
            edits[1].provenance.as_deref(),
            Some(format_b.to_string().as_str())
        );
        assert_eq!(edits[2].provenance.as_deref(), Some("structural"));

        // Writer-set bookkeeping: all three contributors recorded, no fallback.
        let writers = result.stats.writer_used.get(&index).expect("writer set");
        assert_eq!(
            writers.iter().cloned().collect::<Vec<_>>(),
            vec![
                "structural".to_string(),
                "writer_a".to_string(),
                "writer_b".to_string()
            ]
        );

        assert_eq!(
            result.stats.compaction_accepted.get("format_scoped"),
            Some(&1)
        );
    }

    #[test]
    fn fallback_fires_only_when_no_other_writer_claims() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("data.bin");
        std::fs::write(&file, b"data").unwrap();
        let data = binoc_sdk::LocalDataAccess::new();
        let left = data.register_local(&file, "data.bin").unwrap();
        let right = data.register_local(&file, "data.bin").unwrap();

        struct ClaimingFallback;
        impl binoc_sdk::EditListWriter for ClaimingFallback {
            fn descriptor(&self) -> WriterDescriptor {
                WriterDescriptor {
                    name: "fallback".into(),
                    formats: vec![],
                    input: Default::default(),
                    shape: ShapeFilter::Any,
                    fallback: true,
                }
            }
            fn write(
                &self,
                _ctx: &LinkCtx<'_>,
                _data: &dyn DataAccess,
            ) -> BinocResult<Option<binoc_sdk::WriteOutput>> {
                Ok(Some(
                    vec![Edit::new("fallback.edit", serde_json::json!({}))].into(),
                ))
            }
        }

        // With a structural writer that claims, the fallback must not fire.
        let with_claim = CorrespondenceEngineConfig {
            rules: vec![CoreRule::Pair(Arc::new(SingleRootPair))],
            writers: vec![Arc::new(StructuralEditWriter), Arc::new(ClaimingFallback)],
            compaction: vec![],
            annotators: vec![],
            row_keys: BTreeMap::new(),
            row_identity_policies: BTreeMap::new(),
            root_projection: ProjectionHint::default(),
            dataset_configurator: None,
        };
        let result = run(&with_claim, left.clone(), right.clone(), &data).expect("run");
        let writers = result.stats.writer_used.values().next().expect("writers");
        assert!(writers.contains("structural"));
        assert!(
            !writers.contains("fallback"),
            "fallback fired despite a claim"
        );

        // With no claiming writer, the fallback is the sole contributor.
        let only_fallback = CorrespondenceEngineConfig {
            rules: vec![CoreRule::Pair(Arc::new(SingleRootPair))],
            writers: vec![Arc::new(ClaimingFallback)],
            compaction: vec![],
            annotators: vec![],
            row_keys: BTreeMap::new(),
            row_identity_policies: BTreeMap::new(),
            root_projection: ProjectionHint::default(),
            dataset_configurator: None,
        };
        let result = run(&only_fallback, left, right, &data).expect("run");
        let writers = result.stats.writer_used.values().next().expect("writers");
        assert_eq!(
            writers.iter().cloned().collect::<Vec<_>>(),
            vec!["fallback"]
        );
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
