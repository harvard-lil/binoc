use std::collections::BTreeMap;

use binoc_sdk::{
    ArtifactDescriptor, ArtifactFormat, ItemRef, LinkProposal, NodeId, ProjectionHint, TreeSide,
};

#[derive(Debug)]
pub struct SideNode {
    pub item: ItemRef,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub projection: ProjectionHint,
}

#[derive(Debug)]
pub struct SideTree {
    pub side: TreeSide,
    nodes: Vec<SideNode>,
    /// Subsumption (CFM-83): a member node folded into a fusing result node. A
    /// *flag*, never a deletion — `NodeId`s are index-stable and subsumed members
    /// survive as the result node's provenance. The value is the result node that
    /// claimed the member. A subsumed node is excluded from dispatch and from
    /// projection as a loose sibling, but is still reachable via the tree.
    subsumed_by: BTreeMap<u32, u32>,
}

impl SideTree {
    pub fn new(side: TreeSide, root: ItemRef, projection: ProjectionHint) -> Self {
        Self {
            side,
            nodes: vec![SideNode {
                item: root,
                parent: None,
                children: Vec::new(),
                projection,
            }],
            subsumed_by: BTreeMap::new(),
        }
    }

    /// Mark `member` as subsumed by `result` (the fused node that claimed it).
    /// Idempotent; the first claimant wins (arity-descending precedence means the
    /// largest successful claim is offered the member first).
    pub fn subsume(&mut self, member: u32, result: u32) {
        self.subsumed_by.entry(member).or_insert(result);
    }

    /// Whether `index` has been folded into a fusing result node.
    pub fn is_subsumed(&self, index: u32) -> bool {
        self.subsumed_by.contains_key(&index)
    }

    /// The result node that subsumed `index`, if any (member-level provenance).
    pub fn subsumer(&self, index: u32) -> Option<u32> {
        self.subsumed_by.get(&index).copied()
    }

    /// The members subsumed by `result`, in index order (result-node provenance).
    pub fn subsumed_members(&self, result: u32) -> Vec<u32> {
        self.subsumed_by
            .iter()
            .filter(|&(_, &r)| r == result)
            .map(|(&m, _)| m)
            .collect()
    }

    pub fn root(&self) -> u32 {
        0
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, index: u32) -> &SideNode {
        &self.nodes[index as usize]
    }

    pub fn node_mut(&mut self, index: u32) -> &mut SideNode {
        &mut self.nodes[index as usize]
    }

    pub fn add_child(&mut self, parent: u32, item: ItemRef, projection: ProjectionHint) -> u32 {
        let index = self.nodes.len() as u32;
        self.nodes.push(SideNode {
            item,
            parent: Some(parent),
            children: Vec::new(),
            projection,
        });
        self.nodes[parent as usize].children.push(index);
        index
    }

    pub fn ancestors(&self, index: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut current = self.nodes[index as usize].parent;
        while let Some(parent) = current {
            out.push(parent);
            current = self.nodes[parent as usize].parent;
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub left: u32,
    pub right: u32,
    pub evidence: String,
    pub proposer: String,
    pub priority: u32,
    pub settled: bool,
    pub projection: ProjectionHint,
}

#[derive(Debug, Clone)]
pub struct Revision {
    pub link: usize,
    pub old_priority: u32,
    pub new_priority: u32,
    pub old_evidence: String,
    pub new_evidence: String,
    pub old_proposer: String,
    pub new_proposer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Added,
    Upgraded,
    Unchanged,
}

#[derive(Debug, Default)]
pub struct LinkStore {
    links: Vec<Link>,
    by_pair: BTreeMap<(u32, u32), usize>,
    by_left: BTreeMap<u32, Vec<usize>>,
    by_right: BTreeMap<u32, Vec<usize>>,
    pub revisions: Vec<Revision>,
}

impl LinkStore {
    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &Link)> {
        self.links.iter().enumerate()
    }

    pub fn link(&self, index: usize) -> &Link {
        &self.links[index]
    }

    pub fn of_left(&self, index: u32) -> &[usize] {
        self.by_left.get(&index).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn of_right(&self, index: u32) -> &[usize] {
        self.by_right.get(&index).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn of_node(&self, id: NodeId) -> &[usize] {
        match id.side {
            TreeSide::Left => self.of_left(id.index),
            TreeSide::Right => self.of_right(id.index),
        }
    }

    pub fn linked(&self, left: u32, right: u32) -> bool {
        self.by_pair.contains_key(&(left, right))
    }

    pub fn apply(&mut self, proposal: LinkProposal, proposer: &str, priority: u32) -> ApplyOutcome {
        if let Some(&index) = self.by_pair.get(&(proposal.left, proposal.right)) {
            let existing = &mut self.links[index];
            if priority > existing.priority {
                self.revisions.push(Revision {
                    link: index,
                    old_priority: existing.priority,
                    new_priority: priority,
                    old_evidence: existing.evidence.clone(),
                    new_evidence: proposal.evidence.clone(),
                    old_proposer: existing.proposer.clone(),
                    new_proposer: proposer.to_string(),
                });
                existing.evidence = proposal.evidence;
                existing.proposer = proposer.to_string();
                existing.priority = priority;
                existing.settled = proposal.settled;
                existing.projection = proposal.projection;
                ApplyOutcome::Upgraded
            } else {
                ApplyOutcome::Unchanged
            }
        } else {
            let index = self.links.len();
            self.links.push(Link {
                left: proposal.left,
                right: proposal.right,
                evidence: proposal.evidence,
                proposer: proposer.to_string(),
                priority,
                settled: proposal.settled,
                projection: proposal.projection,
            });
            self.by_pair.insert((proposal.left, proposal.right), index);
            self.by_left.entry(proposal.left).or_default().push(index);
            self.by_right.entry(proposal.right).or_default().push(index);
            ApplyOutcome::Added
        }
    }
}

pub struct Store {
    pub left: SideTree,
    pub right: SideTree,
    pub links: LinkStore,
    artifacts: BTreeMap<(u8, u32), Vec<ArtifactDescriptor>>,
}

fn side_key(side: TreeSide) -> u8 {
    match side {
        TreeSide::Left => 0,
        TreeSide::Right => 1,
    }
}

impl Store {
    pub fn new(left_root: ItemRef, right_root: ItemRef, root_projection: ProjectionHint) -> Self {
        Self {
            left: SideTree::new(TreeSide::Left, left_root, root_projection.clone()),
            right: SideTree::new(TreeSide::Right, right_root, root_projection),
            links: LinkStore::default(),
            artifacts: BTreeMap::new(),
        }
    }

    pub fn tree(&self, side: TreeSide) -> &SideTree {
        match side {
            TreeSide::Left => &self.left,
            TreeSide::Right => &self.right,
        }
    }

    pub fn tree_mut(&mut self, side: TreeSide) -> &mut SideTree {
        match side {
            TreeSide::Left => &mut self.left,
            TreeSide::Right => &mut self.right,
        }
    }

    pub fn item(&self, id: NodeId) -> &ItemRef {
        &self.tree(id.side).node(id.index).item
    }

    pub fn projection(&self, id: NodeId) -> &ProjectionHint {
        &self.tree(id.side).node(id.index).projection
    }

    pub fn add_artifact(&mut self, id: NodeId, descriptor: ArtifactDescriptor) {
        self.artifacts
            .entry((side_key(id.side), id.index))
            .or_default()
            .push(descriptor);
    }

    pub fn artifacts_of(&self, id: NodeId) -> &[ArtifactDescriptor] {
        self.artifacts
            .get(&(side_key(id.side), id.index))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn artifact(&self, id: NodeId, format: &ArtifactFormat) -> Option<&ArtifactDescriptor> {
        self.artifacts_of(id)
            .iter()
            .find(|artifact| &artifact.format == format)
    }

    /// Whether `id` is subsumed (folded into a fusing result node), and thus
    /// excluded from dispatch and from sibling projection.
    pub fn is_subsumed(&self, id: NodeId) -> bool {
        self.tree(id.side).is_subsumed(id.index)
    }

    pub fn is_settled_endpoint(&self, side: TreeSide, index: u32) -> bool {
        let links = match side {
            TreeSide::Left => self.links.of_left(index),
            TreeSide::Right => self.links.of_right(index),
        };
        links.iter().any(|&link| self.links.link(link).settled)
    }

    pub fn beneath_settled(&self, side: TreeSide, index: u32, include_self: bool) -> bool {
        if include_self && self.is_settled_endpoint(side, index) {
            return true;
        }
        self.tree(side)
            .ancestors(index)
            .into_iter()
            .any(|ancestor| self.is_settled_endpoint(side, ancestor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, is_dir: bool) -> ItemRef {
        ItemRef {
            logical_path: path.into(),
            is_dir,
            content_hash: None,
            size: None,
            media_type: None,
            projection_hint: ProjectionHint::default().item_type(if is_dir {
                "tree"
            } else {
                "leaf"
            }),
            handle: path.into(),
        }
    }

    #[test]
    fn side_tree_is_append_only_and_preserves_parent_links() {
        let mut tree = SideTree::new(TreeSide::Left, item("", true), ProjectionHint::default());
        let first = tree.add_child(0, item("a", false), ProjectionHint::default());
        let second = tree.add_child(0, item("b", false), ProjectionHint::default());

        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert_eq!(tree.node(0).children, vec![1, 2]);
        assert_eq!(tree.node(first).parent, Some(0));
        assert_eq!(tree.node(second).item.logical_path, "b");
    }

    #[test]
    fn link_store_revises_only_at_strictly_higher_priority() {
        let mut links = LinkStore::default();
        let proposal = LinkProposal {
            left: 1,
            right: 2,
            evidence: "weak".into(),
            settled: false,
            projection: ProjectionHint::default(),
        };
        assert_eq!(links.apply(proposal, "weak-rule", 1), ApplyOutcome::Added);

        let lower = LinkProposal {
            left: 1,
            right: 2,
            evidence: "lower".into(),
            settled: true,
            projection: ProjectionHint::default(),
        };
        assert_eq!(links.apply(lower, "lower-rule", 1), ApplyOutcome::Unchanged);
        assert_eq!(links.link(0).evidence, "weak");
        assert!(!links.link(0).settled);

        let higher = LinkProposal {
            left: 1,
            right: 2,
            evidence: "strong".into(),
            settled: true,
            projection: ProjectionHint::default().tag("strong-tag"),
        };
        assert_eq!(
            links.apply(higher, "strong-rule", 2),
            ApplyOutcome::Upgraded
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links.link(0).evidence, "strong");
        assert!(links.link(0).settled);
        assert_eq!(links.of_left(1), &[0]);
        assert_eq!(links.of_right(2), &[0]);
        assert_eq!(links.revisions.len(), 1);
        assert!(links.revisions[0].old_priority < links.revisions[0].new_priority);
    }

    #[test]
    fn settled_scope_hides_descendants_but_not_endpoint_by_default() {
        let mut store = Store::new(item("", true), item("", true), ProjectionHint::default());
        let left_child = store
            .left
            .add_child(0, item("child", false), ProjectionHint::default());
        let right_child = store
            .right
            .add_child(0, item("child", false), ProjectionHint::default());
        let proposal = LinkProposal {
            left: 0,
            right: 0,
            evidence: "root".into(),
            settled: true,
            projection: ProjectionHint::default(),
        };
        store.links.apply(proposal, "root-rule", 1);

        assert!(!store.beneath_settled(TreeSide::Left, 0, false));
        assert!(store.beneath_settled(TreeSide::Left, 0, true));
        assert!(store.beneath_settled(TreeSide::Left, left_child, false));
        assert!(store.beneath_settled(TreeSide::Right, right_child, false));
    }
}
