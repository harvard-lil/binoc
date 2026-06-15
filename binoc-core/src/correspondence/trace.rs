//! Structured replay trace for a correspondence run.
//!
//! A [`RunTrace`] is an opt-in, fully serializable record of how the engine
//! built its result: the final left/right side trees, the links between them,
//! and the ordered list of every rewrite [`TraceStep`] that fired (expand,
//! parse, link add/upgrade, edit-list write, compaction). It is richer than the
//! lightweight `RunStats.events` log — each step carries node/link indices and
//! edit payloads — so a viewer can replay the whole comparison step by step.
//!
//! Capture is off by default and adds no work to an untraced run; it is enabled
//! only through [`super::driver::run_traced`]. Every node and link records the
//! `born_step` at which it first appeared, letting a viewer lay out the final
//! trees once and then reveal them progressively as the timeline advances.

use std::collections::BTreeMap;

use binoc_sdk::{Edit, TreeSide};
use serde::{Deserialize, Serialize};

use super::store::{SideTree, Store};

/// A complete, replayable record of one correspondence run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTrace {
    pub from_snapshot: String,
    pub to_snapshot: String,
    /// Number of saturation rounds the engine ran.
    pub rounds: u32,
    /// Final left side tree, indexed by node index.
    pub left: Vec<TraceNode>,
    /// Final right side tree, indexed by node index.
    pub right: Vec<TraceNode>,
    /// Final link table, indexed by link index.
    pub links: Vec<TraceLink>,
    /// Ordered rewrite steps, in the order they fired.
    pub steps: Vec<TraceStep>,
    /// Final rendered changelog text (e.g. Markdown), if the caller chose to
    /// attach it. The correspondence engine does not render; the CLI fills this
    /// in so a replay can show the end product building up. `None` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// One node in a side tree, with the step at which it was created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceNode {
    pub index: u32,
    pub path: String,
    pub parent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    pub is_dir: bool,
    /// Index into [`RunTrace::steps`] of the step that created this node, or
    /// `0` for root nodes that exist before any step fires.
    pub born_step: usize,
}

/// One correspondence link, with the step at which it was first added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceLink {
    pub index: usize,
    pub left: u32,
    pub right: u32,
    pub evidence: String,
    pub proposer: String,
    pub settled: bool,
    pub born_step: usize,
}

/// A single rewrite that fired during the run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceStep {
    /// An expand rule unpacked `node` into `children`.
    Expand {
        round: u32,
        rule: String,
        side: TreeSide,
        node: u32,
        children: Vec<u32>,
    },
    /// A parse rule produced an artifact (and possibly child nodes) for `node`.
    Parse {
        round: u32,
        rule: String,
        side: TreeSide,
        node: u32,
        format: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<u32>,
    },
    /// A pair rule proposed a new link.
    LinkAdd {
        round: u32,
        rule: String,
        link: usize,
        left: u32,
        right: u32,
        evidence: String,
        settled: bool,
    },
    /// A higher-priority pair rule revised an existing link.
    LinkUpgrade {
        round: u32,
        rule: String,
        link: usize,
        old_evidence: String,
        new_evidence: String,
        old_proposer: String,
        new_proposer: String,
        settled: bool,
    },
    /// An edit-list writer explained a link's differences.
    Write {
        writer: String,
        link: usize,
        edits: Vec<Edit>,
    },
    /// A compaction rule rewrote a link's edit list (accepted iff cheaper).
    Compact {
        rule: String,
        link: usize,
        accepted: bool,
        before: Vec<Edit>,
        after: Vec<Edit>,
    },
}

fn side_key(side: TreeSide) -> u8 {
    match side {
        TreeSide::Left => 0,
        TreeSide::Right => 1,
    }
}

/// Accumulates a [`RunTrace`] as the driver runs. The driver pushes a step for
/// each event and records the birth step of any node or link it creates;
/// [`TraceRecorder::finish`] then snapshots the final trees and links.
#[derive(Default)]
pub struct TraceRecorder {
    steps: Vec<TraceStep>,
    node_born: BTreeMap<(u8, u32), usize>,
    link_born: BTreeMap<usize, usize>,
}

impl TraceRecorder {
    /// Append a step, returning its index in [`RunTrace::steps`].
    pub fn push(&mut self, step: TraceStep) -> usize {
        let index = self.steps.len();
        self.steps.push(step);
        index
    }

    /// Record that a node first appeared at `step` (first writer wins).
    pub fn record_node_birth(&mut self, side: TreeSide, index: u32, step: usize) {
        self.node_born
            .entry((side_key(side), index))
            .or_insert(step);
    }

    /// Record that a link first appeared at `step` (first writer wins).
    pub fn record_link_birth(&mut self, index: usize, step: usize) {
        self.link_born.entry(index).or_insert(step);
    }

    /// Snapshot the final trees/links into a complete [`RunTrace`]. Snapshot
    /// names are left empty for the caller (the controller) to fill in.
    pub fn finish(self, store: &Store, rounds: u32) -> RunTrace {
        let left = self.build_nodes(&store.left, TreeSide::Left);
        let right = self.build_nodes(&store.right, TreeSide::Right);
        let links = store
            .links
            .iter()
            .map(|(index, link)| TraceLink {
                index,
                left: link.left,
                right: link.right,
                evidence: link.evidence.clone(),
                proposer: link.proposer.clone(),
                settled: link.settled,
                born_step: self.link_born.get(&index).copied().unwrap_or(0),
            })
            .collect();
        RunTrace {
            from_snapshot: String::new(),
            to_snapshot: String::new(),
            rounds,
            left,
            right,
            links,
            steps: self.steps,
            output: None,
        }
    }

    fn build_nodes(&self, tree: &SideTree, side: TreeSide) -> Vec<TraceNode> {
        (0..tree.len() as u32)
            .map(|index| {
                let node = tree.node(index);
                let item_type = node
                    .projection
                    .item_type
                    .clone()
                    .or_else(|| node.item.projection_hint.item_type.clone());
                TraceNode {
                    index,
                    path: node.item.logical_path.clone(),
                    parent: node.parent,
                    item_type,
                    is_dir: node.item.is_dir,
                    born_step: self
                        .node_born
                        .get(&(side_key(side), index))
                        .copied()
                        .unwrap_or(0),
                }
            })
            .collect()
    }
}
