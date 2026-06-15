use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;

use binoc_sdk::{
    disjoint_cover, file_name, tabular_v1, BinocError, BinocResult, Candidate, Coverage,
    DataAccess, Diagnostic, EngineView, FileCorrespondenceRule, FileSelector, GlobalClaim,
    IdentityToken, ItemRef, LinkProposal, NodeId, PairDescriptor, PairOutput, PairRule,
    ProjectionHint, Side, Summary, TabularData, TreeSide,
};
use regex::Regex;

const FUZZY_MAX_BYTES: u64 = 8 * 1024 * 1024;

pub struct RootPair;

impl PairRule for RootPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.root".into(),
            emits: vec!["binoc.pair.root".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, _data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let root = view.root(TreeSide::Right);
        let item_type = if view.item(root).is_dir {
            "directory"
        } else {
            "file"
        };
        Ok(vec![LinkProposal {
            left: view.root(TreeSide::Left).index,
            right: root.index,
            evidence: "binoc.pair.root".into(),
            settled: false,
            projection: ProjectionHint::default().item_type(item_type),
        }]
        .into())
    }
}

#[derive(Default)]
pub struct HashPair {
    pub settle_renames: bool,
}

impl PairRule for HashPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.hash".into(),
            emits: vec!["binoc.pair.hash".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut proposals = Vec::new();
        let mut right_by_hash: BTreeMap<String, HashCandidateBucket> = BTreeMap::new();
        let mut right_by_path: BTreeMap<&str, (NodeId, String)> = BTreeMap::new();
        for id in view.nodes(TreeSide::Right) {
            let item = view.item(id);
            if item.is_dir {
                continue;
            }
            let hash = item.resolve_hash(data)?;
            right_by_hash
                .entry(hash.clone())
                .or_default()
                .push(id, file_name(&item.logical_path).to_string());
            right_by_path.insert(&item.logical_path, (id, hash));
        }

        let mut used_left = BTreeSet::new();
        let mut used_right = BTreeSet::new();
        for id in view.nodes(TreeSide::Left) {
            let item = view.item(id);
            if item.is_dir {
                continue;
            }
            if let Some((right_id, right_hash)) = right_by_path.get(item.logical_path.as_str()) {
                let left_hash = item.resolve_hash(data)?;
                if &left_hash == right_hash {
                    proposals.push(LinkProposal {
                        left: id.index,
                        right: right_id.index,
                        evidence: "binoc.pair.hash".into(),
                        settled: true,
                        projection: ProjectionHint::default(),
                    });
                    used_left.insert(id.index);
                    used_right.insert(right_id.index);
                }
            }
        }

        for bucket in right_by_hash.values_mut() {
            bucket.sort_by_path(view);
        }

        for id in view.nodes(TreeSide::Left) {
            let item = view.item(id);
            if item.is_dir || used_left.contains(&id.index) || view.is_linked(id) {
                continue;
            }
            let hash = item.resolve_hash(data)?;
            let Some(candidates) = right_by_hash.get_mut(&hash) else {
                continue;
            };
            let left_name = file_name(&item.logical_path);
            if let Some(right_id) = candidates
                .next_for_name(left_name, &used_right, view)
                .or_else(|| candidates.next_any(&used_right, view))
            {
                let mut projection = move_projection_hint();
                if archive_like(&item.logical_path)
                    || archive_like(&view.item(right_id).logical_path)
                {
                    projection = projection.tag("binoc.folder-move");
                }
                proposals.push(LinkProposal {
                    left: id.index,
                    right: right_id.index,
                    evidence: "binoc.pair.hash".into(),
                    settled: self.settle_renames,
                    projection,
                });
                used_left.insert(id.index);
                used_right.insert(right_id.index);
            }
        }

        Ok(proposals.into())
    }
}

#[derive(Default)]
struct HashCandidateBucket {
    all: Vec<NodeId>,
    by_name: BTreeMap<String, Vec<NodeId>>,
    next_all: usize,
    next_by_name: BTreeMap<String, usize>,
}

impl HashCandidateBucket {
    fn push(&mut self, id: NodeId, name: String) {
        self.all.push(id);
        self.by_name.entry(name).or_default().push(id);
    }

    fn sort_by_path(&mut self, view: &dyn EngineView) {
        let by_path = |left: &NodeId, right: &NodeId| {
            view.item(*left)
                .logical_path
                .cmp(&view.item(*right).logical_path)
        };
        self.all.sort_by(by_path);
        for candidates in self.by_name.values_mut() {
            candidates.sort_by(by_path);
        }
    }

    fn next_for_name(
        &mut self,
        name: &str,
        used_right: &BTreeSet<u32>,
        view: &dyn EngineView,
    ) -> Option<NodeId> {
        let candidates = self.by_name.get(name)?;
        let cursor = self.next_by_name.entry(name.to_string()).or_insert(0);
        next_unused(candidates, cursor, used_right, view)
    }

    fn next_any(&mut self, used_right: &BTreeSet<u32>, view: &dyn EngineView) -> Option<NodeId> {
        next_unused(&self.all, &mut self.next_all, used_right, view)
    }
}

fn next_unused(
    candidates: &[NodeId],
    cursor: &mut usize,
    used_right: &BTreeSet<u32>,
    view: &dyn EngineView,
) -> Option<NodeId> {
    while let Some(id) = candidates.get(*cursor).copied() {
        *cursor += 1;
        if !used_right.contains(&id.index) && !view.is_linked(id) {
            return Some(id);
        }
    }
    None
}

pub struct CopyPair;

impl PairRule for CopyPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.copy".into(),
            emits: vec!["binoc.pair.copy".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut left_by_hash: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
        for id in view.nodes(TreeSide::Left) {
            let item = view.item(id);
            if !item.is_dir {
                let hash = item.resolve_hash(data)?;
                left_by_hash.entry(hash).or_default().push(id);
            }
        }

        let mut proposals = Vec::new();
        for right_id in view.nodes(TreeSide::Right) {
            let item = view.item(right_id);
            if item.is_dir || view.is_linked(right_id) {
                continue;
            }
            let hash = item.resolve_hash(data)?;
            let Some(candidates) = left_by_hash.get(&hash) else {
                continue;
            };
            let mut candidates = candidates.clone();
            candidates.sort_by(|a, b| view.item(*a).logical_path.cmp(&view.item(*b).logical_path));
            if let Some(left_id) = candidates.first() {
                proposals.push(LinkProposal {
                    left: left_id.index,
                    right: right_id.index,
                    evidence: "binoc.pair.copy".into(),
                    settled: true,
                    projection: ProjectionHint::default().action("copy").tag("binoc.copy"),
                });
            }
        }
        Ok(proposals.into())
    }
}

#[derive(Default)]
pub struct DeclaredPair {
    pub pairs: Vec<(String, String)>,
    pub rules: Vec<FileCorrespondenceRule>,
}

impl PairRule for DeclaredPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.declared".into(),
            emits: vec!["binoc.pair.declared".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, _data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut left_by_path = BTreeMap::new();
        let mut right_by_path = BTreeMap::new();
        for id in view.nodes(TreeSide::Left) {
            left_by_path.insert(view.item(id).logical_path.as_str(), id);
        }
        for id in view.nodes(TreeSide::Right) {
            right_by_path.insert(view.item(id).logical_path.as_str(), id);
        }
        let mut proposals = Vec::new();
        for (left, right) in &self.pairs {
            if let (Some(left_id), Some(right_id)) = (
                left_by_path.get(left.as_str()),
                right_by_path.get(right.as_str()),
            ) {
                proposals.push(LinkProposal {
                    left: left_id.index,
                    right: right_id.index,
                    evidence: "binoc.pair.declared".into(),
                    settled: false,
                    projection: move_hint_if_paths_differ(view, *left_id, *right_id)
                        .tag("binoc.declared-correspondence"),
                });
            }
        }
        for proposal in declared_rule_proposals(view, &self.rules) {
            proposals.push(proposal);
        }
        Ok(proposals.into())
    }

    fn final_diagnostics(
        &self,
        view: &dyn EngineView,
        _data: &dyn DataAccess,
    ) -> BinocResult<Vec<Diagnostic>> {
        Ok(declared_rule_diagnostics(view, &self.rules))
    }
}

#[derive(Debug, Clone)]
struct DeclaredMatch {
    id: NodeId,
}

fn declared_rule_proposals(
    view: &dyn EngineView,
    rules: &[FileCorrespondenceRule],
) -> Vec<LinkProposal> {
    let mut proposals = Vec::new();
    for rule in rules {
        let left = declared_keyed_matches(view, TreeSide::Left, &rule.left, &rule.key);
        let right = declared_keyed_matches(view, TreeSide::Right, &rule.right, &rule.key);
        for (key, left_matches) in &left {
            let Some(right_matches) = right.get(key) else {
                continue;
            };
            if left_matches.len() != 1 || right_matches.len() != 1 {
                continue;
            }
            let left_match = &left_matches[0];
            let right_match = &right_matches[0];
            proposals.push(LinkProposal {
                left: left_match.id.index,
                right: right_match.id.index,
                evidence: "binoc.pair.declared".into(),
                settled: false,
                projection: move_hint_if_paths_differ(view, left_match.id, right_match.id)
                    .tag("binoc.declared-correspondence"),
            });
        }
    }
    proposals
}

fn declared_rule_diagnostics(
    view: &dyn EngineView,
    rules: &[FileCorrespondenceRule],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for rule in rules {
        if let Some(err) = validate_selector_regex(&rule.left) {
            diagnostics.push(Diagnostic::warning(
                "binoc.declared_correspondence.invalid_left_regex",
                Summary::new()
                    .text(format!(
                        "File correspondence rule '{}' has an invalid left path_regex: ",
                        rule.name
                    ))
                    .text(err.to_string()),
            ));
            continue;
        }
        if let Some(err) = validate_selector_regex(&rule.right) {
            diagnostics.push(Diagnostic::warning(
                "binoc.declared_correspondence.invalid_right_regex",
                Summary::new()
                    .text(format!(
                        "File correspondence rule '{}' has an invalid right path_regex: ",
                        rule.name
                    ))
                    .text(err.to_string()),
            ));
            continue;
        }

        let left = declared_keyed_matches(view, TreeSide::Left, &rule.left, &rule.key);
        let right = declared_keyed_matches(view, TreeSide::Right, &rule.right, &rule.key);
        let paired = left.iter().any(|(key, left_matches)| {
            right
                .get(key)
                .is_some_and(|right_matches| left_matches.len() == 1 && right_matches.len() == 1)
        });
        if paired {
            continue;
        }

        if let Some((key, left_count, right_count)) = left.iter().find_map(|(key, left_matches)| {
            right.get(key).and_then(|right_matches| {
                (left_matches.len() != 1 || right_matches.len() != 1).then_some((
                    key.clone(),
                    left_matches.len(),
                    right_matches.len(),
                ))
            })
        }) {
            diagnostics.push(Diagnostic::warning(
                "binoc.declared_correspondence.duplicate_key",
                Summary::new()
                    .text(format!(
                        "File correspondence rule '{}' has ambiguous key '{key}' (left matches: ",
                        rule.name
                    ))
                    .uint(left_count as u64)
                    .text(", right matches: ")
                    .uint(right_count as u64)
                    .text(")"),
            ));
            continue;
        }

        diagnostics.push(Diagnostic::warning(
            "binoc.declared_correspondence.no_matching_files",
            Summary::new()
                .text(format!(
                    "File correspondence rule '{}' had no effect: the left selector matched ",
                    rule.name
                ))
                .uint(left.values().map(Vec::len).sum::<usize>() as u64)
                .text(" items and the right selector matched ")
                .uint(right.values().map(Vec::len).sum::<usize>() as u64)
                .text(" items"),
        ));
    }
    diagnostics
}

fn declared_keyed_matches(
    view: &dyn EngineView,
    side: TreeSide,
    selector: &FileSelector,
    key_template: &str,
) -> BTreeMap<String, Vec<DeclaredMatch>> {
    let mut matches: BTreeMap<String, Vec<DeclaredMatch>> = BTreeMap::new();
    for id in view.nodes(side) {
        let path = &view.item(id).logical_path;
        let Some(captures) = selector_captures(selector, path) else {
            continue;
        };
        let Some(key) = expand_template(key_template, &captures) else {
            continue;
        };
        matches.entry(key).or_default().push(DeclaredMatch { id });
    }
    matches
}

fn validate_selector_regex(selector: &FileSelector) -> Option<String> {
    selector
        .path_regex
        .as_deref()
        .and_then(|pattern| Regex::new(pattern).err())
        .map(|err| err.to_string())
}

fn selector_captures(selector: &FileSelector, path: &str) -> Option<BTreeMap<String, String>> {
    if selector
        .path
        .as_deref()
        .is_some_and(|expected| expected != path)
    {
        return None;
    }
    let mut captures_by_name = BTreeMap::new();
    if let Some(pattern) = selector.path_regex.as_deref() {
        let regex = Regex::new(pattern).ok()?;
        let captures = regex.captures(path)?;
        for name in regex.capture_names().flatten() {
            if let Some(value) = captures.name(name) {
                captures_by_name.insert(name.to_string(), value.as_str().to_string());
            }
        }
    }
    if selector.path.is_none() && selector.path_regex.is_none() {
        return None;
    }
    Some(captures_by_name)
}

fn expand_template(template: &str, captures: &BTreeMap<String, String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let end = after_start.find('}')?;
        let name = &after_start[..end];
        out.push_str(captures.get(name)?);
        rest = &after_start[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

pub struct NameUnderPairedParent;

impl PairRule for NameUnderPairedParent {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.name".into(),
            emits: vec!["binoc.pair.name".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, _data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut proposals = Vec::new();
        for link in view.links() {
            let left_children = view.children(link.left);
            let right_children = view.children(link.right);
            if left_children.is_empty() || right_children.is_empty() {
                continue;
            }
            let mut right_by_name = BTreeMap::new();
            for id in &right_children {
                right_by_name.insert(file_name(&view.item(*id).logical_path), *id);
            }
            for left_id in &left_children {
                let name = file_name(&view.item(*left_id).logical_path);
                if let Some(right_id) = right_by_name.get(name) {
                    let pair_exists = view
                        .links_of(*left_id)
                        .iter()
                        .any(|link| link.right.index == right_id.index);
                    let both_free = !view.is_linked(*left_id) && !view.is_linked(*right_id);
                    if pair_exists || both_free {
                        proposals.push(LinkProposal {
                            left: left_id.index,
                            right: right_id.index,
                            evidence: "binoc.pair.name".into(),
                            settled: false,
                            projection: ProjectionHint::default(),
                        });
                    }
                }
            }
        }
        Ok(proposals.into())
    }
}

pub struct FuzzyPair {
    pub threshold: f64,
    pub rename_limit: usize,
    pub size_ratio: f64,
}

impl Default for FuzzyPair {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            rename_limit: 400,
            size_ratio: 10.0,
        }
    }
}

impl PairRule for FuzzyPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.fuzzy".into(),
            emits: vec!["binoc.pair.fuzzy".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let unlinked_files = |side| {
            view.nodes(side)
                .into_iter()
                .filter(|id| {
                    let item = view.item(*id);
                    !item.is_dir && !view.is_linked(*id) && !view.has_children(*id)
                })
                .collect::<Vec<_>>()
        };
        let removes = unlinked_files(TreeSide::Left);
        let adds = unlinked_files(TreeSide::Right);
        if removes.is_empty() || adds.is_empty() {
            return Ok(Vec::new().into());
        }
        let candidate_count = removes.len().saturating_mul(adds.len());
        if candidate_count > self.rename_limit {
            return Ok(PairOutput {
                proposals: Vec::new(),
                diagnostics: vec![Diagnostic::suggestion(
                    "binoc.fuzzy_pair_limit",
                    Summary::new()
                        .text("Skipped fuzzy rename detection for ")
                        .uint(candidate_count as u64)
                        .text(" candidate pairs over limit ")
                        .uint(self.rename_limit as u64),
                )],
            });
        }

        struct Scored {
            left: NodeId,
            right: NodeId,
            score: f64,
        }
        let mut scored = Vec::new();
        let mut diagnostics = Vec::new();
        let mut left_bytes = BTreeMap::new();
        let mut right_bytes = BTreeMap::new();
        for &left in &removes {
            for &right in &adds {
                let left_item = view.item(left);
                let right_item = view.item(right);
                if !extensions_match(&left_item.logical_path, &right_item.logical_path) {
                    continue;
                }
                if let (Some(left_size), Some(right_size)) = (left_item.size, right_item.size) {
                    if !sizes_within_ratio(left_size, right_size, self.size_ratio) {
                        continue;
                    }
                }
                let left_data = left_bytes
                    .entry(left.index)
                    .or_insert_with(|| read_fuzzy_bytes(left_item, data));
                let Some(left_data) = left_data
                    .as_ref()
                    .map_err(|err| BinocError::Other(err.to_string()))?
                else {
                    diagnostics.push(fuzzy_size_diagnostic(left_item));
                    continue;
                };
                let right_data = right_bytes
                    .entry(right.index)
                    .or_insert_with(|| read_fuzzy_bytes(right_item, data));
                let Some(right_data) = right_data
                    .as_ref()
                    .map_err(|err| BinocError::Other(err.to_string()))?
                else {
                    diagnostics.push(fuzzy_size_diagnostic(right_item));
                    continue;
                };
                if !sizes_within_ratio(
                    left_data.len() as u64,
                    right_data.len() as u64,
                    self.size_ratio,
                ) || is_binary(left_data)
                    || is_binary(right_data)
                {
                    continue;
                }
                scored.push(Scored {
                    left,
                    right,
                    score: token_set_similarity(left_data, right_data),
                });
            }
        }
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    (
                        &view.item(a.left).logical_path,
                        &view.item(a.right).logical_path,
                    )
                        .cmp(&(
                            &view.item(b.left).logical_path,
                            &view.item(b.right).logical_path,
                        ))
                })
        });
        let mut used_left = BTreeSet::new();
        let mut used_right = BTreeSet::new();
        let mut proposals = Vec::new();
        for candidate in scored {
            if candidate.score < self.threshold {
                break;
            }
            if used_left.contains(&candidate.left.index)
                || used_right.contains(&candidate.right.index)
            {
                continue;
            }
            used_left.insert(candidate.left.index);
            used_right.insert(candidate.right.index);
            proposals.push(LinkProposal {
                left: candidate.left.index,
                right: candidate.right.index,
                evidence: "binoc.pair.fuzzy".into(),
                settled: false,
                projection: move_projection_hint(),
            });
        }
        Ok(PairOutput {
            proposals,
            diagnostics,
        })
    }
}

/// Pairs unlinked single-table leaves by their PARSED tabular content, so a
/// table reformatted across serializations (e.g. `data.csv` -> `data.tsv`)
/// reads as one reformatted-and-edited table instead of a remove + add.
///
/// The prefilter is "both leaves carry a `tabular_v1` artifact", which routes
/// tabular renames around `FuzzyPair`'s same-suffix gate. Scoring is
/// format-independent: it combines header-name set overlap with sampled-row
/// cell-token overlap, both Jaccard-style, so CSV and TSV encodings of the same
/// table score high. Scope is strictly the symmetric single-table leaf case
/// (`tabular_v1` <-> `tabular_v1`); stacked tables and collections are left to
/// other rules.
pub struct TabularPair {
    pub threshold: f64,
    /// Upper bound on `removes * adds` candidate pairs before the O(n*m)
    /// content scan is skipped (with a diagnostic) so a large migration can't
    /// explode.
    pub candidate_limit: usize,
    /// Rows sampled per table when scoring cell-token overlap.
    pub row_sample: usize,
}

impl Default for TabularPair {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            candidate_limit: 400,
            row_sample: 64,
        }
    }
}

impl PairRule for TabularPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.tabular".into(),
            emits: vec!["binoc.pair.tabular".into()],
            reads: vec![tabular_v1()],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let unlinked_tabular_leaves = |side| -> BinocResult<Vec<(NodeId, TabularData)>> {
            let mut out = Vec::new();
            for id in view.nodes(side) {
                let item = view.item(id);
                if item.is_dir || view.is_linked(id) || view.has_children(id) {
                    continue;
                }
                let Some(bytes) = view.artifact_bytes(id, &tabular_v1(), data)? else {
                    continue;
                };
                let table: TabularData = serde_json::from_slice(&bytes)
                    .map_err(|err| BinocError::Other(format!("decode tabular artifact: {err}")))?;
                out.push((id, table));
            }
            Ok(out)
        };
        let removes = unlinked_tabular_leaves(TreeSide::Left)?;
        let adds = unlinked_tabular_leaves(TreeSide::Right)?;
        if removes.is_empty() || adds.is_empty() {
            return Ok(Vec::new().into());
        }
        let candidate_count = removes.len().saturating_mul(adds.len());
        if candidate_count > self.candidate_limit {
            return Ok(PairOutput {
                proposals: Vec::new(),
                diagnostics: vec![Diagnostic::suggestion(
                    "binoc.tabular_pair_limit",
                    Summary::new()
                        .text("Skipped tabular reformat detection for ")
                        .uint(candidate_count as u64)
                        .text(" candidate pairs over limit ")
                        .uint(self.candidate_limit as u64),
                )],
            });
        }

        struct Scored {
            left: NodeId,
            right: NodeId,
            score: f64,
        }
        let mut scored = Vec::new();
        for (left, left_table) in &removes {
            let left_features = TableFeatures::new(left_table, self.row_sample);
            for (right, right_table) in &adds {
                let right_features = TableFeatures::new(right_table, self.row_sample);
                scored.push(Scored {
                    left: *left,
                    right: *right,
                    score: left_features.similarity(&right_features),
                });
            }
        }
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    (
                        &view.item(a.left).logical_path,
                        &view.item(a.right).logical_path,
                    )
                        .cmp(&(
                            &view.item(b.left).logical_path,
                            &view.item(b.right).logical_path,
                        ))
                })
        });
        let mut used_left = BTreeSet::new();
        let mut used_right = BTreeSet::new();
        let mut proposals = Vec::new();
        for candidate in scored {
            if candidate.score < self.threshold {
                break;
            }
            if used_left.contains(&candidate.left.index)
                || used_right.contains(&candidate.right.index)
            {
                continue;
            }
            used_left.insert(candidate.left.index);
            used_right.insert(candidate.right.index);
            let mut projection = move_projection_hint();
            if serialization_differs(
                &view.item(candidate.left).logical_path,
                &view.item(candidate.right).logical_path,
            ) {
                projection = projection.tag("binoc.serialization-change");
            }
            proposals.push(LinkProposal {
                left: candidate.left.index,
                right: candidate.right.index,
                evidence: "binoc.pair.tabular".into(),
                settled: false,
                projection,
            });
        }
        Ok(proposals.into())
    }
}

/// Format-independent fingerprint of a single table for similarity scoring.
struct TableFeatures {
    headers: BTreeSet<String>,
    cell_tokens: BTreeSet<String>,
}

impl TableFeatures {
    fn new(table: &TabularData, row_sample: usize) -> Self {
        let headers = table
            .headers
            .iter()
            .map(|header| header.trim().to_ascii_lowercase())
            .filter(|header| !header.is_empty())
            .collect();
        let mut cell_tokens = BTreeSet::new();
        for row in table.rows.iter().take(row_sample) {
            for cell in row {
                for token in cell.as_text().split(|ch: char| !ch.is_alphanumeric()) {
                    if !token.is_empty() {
                        cell_tokens.insert(token.to_ascii_lowercase());
                    }
                }
            }
        }
        Self {
            headers,
            cell_tokens,
        }
    }

    /// Mean of header-name and cell-token Jaccard overlap. Equal weighting keeps
    /// a table recognizable when only one of schema or data drifts.
    fn similarity(&self, other: &TableFeatures) -> f64 {
        0.5 * jaccard(&self.headers, &other.headers)
            + 0.5 * jaccard(&self.cell_tokens, &other.cell_tokens)
    }
}

fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.len() + right.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// True when two logical paths carry different file extensions, signalling a
/// serialization/format change (e.g. `.csv` vs `.tsv`).
fn serialization_differs(left: &str, right: &str) -> bool {
    fn ext(path: &str) -> Option<String> {
        file_name(path)
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
    }
    let (left_ext, right_ext) = (ext(left), ext(right));
    left_ext != right_ext
}

pub struct ContainerFromChildEvidence {
    pub threshold: f64,
}

impl Default for ContainerFromChildEvidence {
    fn default() -> Self {
        Self { threshold: 0.8 }
    }
}

impl PairRule for ContainerFromChildEvidence {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.container_from_children".into(),
            emits: vec!["binoc.pair.container_from_children".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, _data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut votes: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for link in view.links() {
            let (Some(left_parent), Some(right_parent)) =
                (view.parent(link.left), view.parent(link.right))
            else {
                continue;
            };
            if view.is_linked(left_parent) || view.is_linked(right_parent) {
                continue;
            }
            *votes
                .entry((left_parent.index, right_parent.index))
                .or_insert(0) += 1;
        }

        struct Candidate {
            left: u32,
            right: u32,
            count: usize,
            ratio: f64,
        }

        let mut candidates = Vec::new();
        for ((left, right), count) in votes {
            let left_id = NodeId {
                side: TreeSide::Left,
                index: left,
            };
            let right_id = NodeId {
                side: TreeSide::Right,
                index: right,
            };
            let left_children = view.children(left_id).len();
            let right_children = view.children(right_id).len();
            if left_children == 0 || right_children == 0 {
                continue;
            }
            let ratio =
                (count as f64 / left_children as f64).max(count as f64 / right_children as f64);
            if ratio >= self.threshold {
                candidates.push(Candidate {
                    left,
                    right,
                    count,
                    ratio,
                });
            }
        }

        candidates.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| {
                    b.ratio
                        .partial_cmp(&a.ratio)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| (a.left, a.right).cmp(&(b.left, b.right)))
        });
        let mut used_left = BTreeSet::new();
        let mut used_right = BTreeSet::new();
        let mut proposals = Vec::new();
        for candidate in candidates {
            if used_left.contains(&candidate.left) || used_right.contains(&candidate.right) {
                continue;
            }
            used_left.insert(candidate.left);
            used_right.insert(candidate.right);
            proposals.push(LinkProposal {
                left: candidate.left,
                right: candidate.right,
                evidence: "binoc.pair.container_from_children".into(),
                settled: false,
                projection: move_hint_if_paths_differ(
                    view,
                    NodeId {
                        side: TreeSide::Left,
                        index: candidate.left,
                    },
                    NodeId {
                        side: TreeSide::Right,
                        index: candidate.right,
                    },
                ),
            });
        }
        Ok(proposals.into())
    }
}

fn move_hint_if_paths_differ(view: &dyn EngineView, left: NodeId, right: NodeId) -> ProjectionHint {
    if view.item(left).logical_path != view.item(right).logical_path {
        let mut hint = move_projection_hint();
        if view.item(left).is_dir
            || view.item(right).is_dir
            || view.has_children(left)
            || view.has_children(right)
        {
            hint = hint.tag("binoc.folder-move");
        }
        hint
    } else {
        ProjectionHint::default()
    }
}

fn move_projection_hint() -> ProjectionHint {
    ProjectionHint::default().tag("binoc.move")
}

fn archive_like(path: &str) -> bool {
    path.ends_with(".zip")
        || path.ends_with(".tar")
        || path.ends_with(".tar.gz")
        || path.ends_with(".tgz")
        || path.ends_with(".gz")
}

fn read_fuzzy_bytes(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<Option<Vec<u8>>> {
    if item.size.is_some_and(|size| size > FUZZY_MAX_BYTES) {
        return Ok(None);
    }
    let mut reader = data.open_read(item)?;
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(FUZZY_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(BinocError::Io)?;
    if bytes.len() as u64 > FUZZY_MAX_BYTES {
        Ok(None)
    } else {
        Ok(Some(bytes))
    }
}

fn fuzzy_size_diagnostic(item: &ItemRef) -> Diagnostic {
    Diagnostic::suggestion(
        "binoc.fuzzy_pair_bytes_limit",
        Summary::new()
            .text("Skipped fuzzy content scoring for item over ")
            .uint(FUZZY_MAX_BYTES)
            .text(" bytes"),
    )
    .with_location(item.logical_path.clone())
}

fn extensions_match(left: &str, right: &str) -> bool {
    fn ext(path: &str) -> Option<String> {
        path.rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
    }
    ext(left) == ext(right)
}

fn sizes_within_ratio(left: u64, right: u64, max_ratio: f64) -> bool {
    match (left, right) {
        (0, 0) => true,
        (0, _) | (_, 0) => false,
        (left, right) => (left.max(right) as f64) / (left.min(right) as f64) <= max_ratio,
    }
}

fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(8192)].contains(&0)
}

fn token_set_similarity(left: &[u8], right: &[u8]) -> f64 {
    fn tokens(bytes: &[u8]) -> HashSet<&[u8]> {
        bytes
            .split(|byte| matches!(byte, b'\n' | b'\r' | b',' | b'\t' | b' '))
            .filter(|token| !token.is_empty())
            .collect()
    }
    let left = tokens(left);
    let right = tokens(right);
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.len() + right.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

// ── Partition split/merge (CFM-72) ──────────────────────────────────────────

/// Evidence/verb/tag vocabulary for partition claims.
const PARTITION_EVIDENCE: &str = "binoc.pair.partition";
const TAG_SPLIT: &str = "binoc.tabular_split";
const TAG_MERGE: &str = "binoc.tabular_merge";

/// Conservative split/merge detector over partition identities (CFM-72).
///
/// Runs on the *residue* the exact and (registration-order) earlier rules leave
/// unlinked. For each unmatched, partition-capable node it asks the SDK coverage
/// query whether that node's identity tokens are the clean disjoint union of a
/// set of unmatched nodes on the other side. When they are — complete (residual
/// 0), disjoint, unambiguous, and not a whole-artifact 1:1 — it claims a 1→N
/// split (or N→1 merge) as a settled link fan; otherwise it declines, emitting a
/// `binoc.possible_split` suggestion for the near miss and leaving the nodes to
/// honest add/remove. There are no similarity dials: it is the exact-tier analog
/// of [`HashPair`].
pub struct PartitionPair {
    /// Upper bound on residue nodes considered per side before the O(n·m)
    /// coverage scan is skipped (with a diagnostic).
    pub residue_cap: usize,
}

impl Default for PartitionPair {
    fn default() -> Self {
        Self { residue_cap: 256 }
    }
}

/// An unmatched, partition-capable node and its identity tokens.
struct Residue {
    id: NodeId,
    tokens: Vec<IdentityToken>,
}

impl PartitionPair {
    /// Unlinked, non-container leaves on `side` that carry partition identities,
    /// sorted by logical path for deterministic claims.
    fn residue(
        &self,
        view: &dyn EngineView,
        data: &dyn DataAccess,
        side: TreeSide,
    ) -> BinocResult<Vec<Residue>> {
        let mut out = Vec::new();
        for id in view.nodes(side) {
            let item = view.item(id);
            if item.is_dir || view.has_children(id) {
                continue;
            }
            // Two kinds of existing link disqualify a node from being a split/merge
            // candidate:
            //   * a *settled* link — a confirmed 1:1 (exact move/copy); and
            //   * a *same-path* link — an in-place modify (`data.csv` ↔ `data.csv`),
            //     a strong 1:1 that an unchanged shared row must not turn into a
            //     spurious partition near miss.
            // A node carrying only an unsettled *cross-path* link (a fuzzy rename)
            // is still fair game: partition outranks the fuzzy rules, so a clean
            // claim upgrades that link rather than being blocked by it — the fuzzy
            // rule often fires a round before the parsed artifacts partition needs
            // are materialized.
            let path = &item.logical_path;
            let disqualified = view.links_of(id).iter().any(|link| {
                link.settled
                    || view.item(link.left).logical_path == *path
                        && view.item(link.right).logical_path == *path
            });
            if disqualified {
                continue;
            }
            let Some(tokens) = view.identity_tokens(id, data)? else {
                continue;
            };
            if tokens.is_empty() {
                continue;
            }
            out.push(Residue { id, tokens });
        }
        out.sort_by(|a, b| {
            view.item(a.id)
                .logical_path
                .cmp(&view.item(b.id).logical_path)
        });
        Ok(out)
    }
}

impl PairRule for PartitionPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.partition".into(),
            emits: vec![PARTITION_EVIDENCE.into()],
            // Partition needs each candidate's parsed content materialized on the
            // unlinked node, exactly like the fuzzy tabular rule.
            reads: vec![tabular_v1()],
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let left = self.residue(view, data, TreeSide::Left)?;
        let right = self.residue(view, data, TreeSide::Right)?;
        if left.is_empty() || right.is_empty() {
            return Ok(PairOutput::default());
        }
        if left.len() > self.residue_cap || right.len() > self.residue_cap {
            return Ok(PairOutput {
                proposals: Vec::new(),
                diagnostics: vec![Diagnostic::suggestion(
                    "binoc.partition_limit",
                    Summary::new()
                        .text("Skipped split/merge detection: residue ")
                        .uint(left.len() as u64)
                        .text("×")
                        .uint(right.len() as u64)
                        .text(" exceeds limit ")
                        .uint(self.residue_cap as u64),
                )],
            });
        }

        let mut proposals = Vec::new();
        // Near misses are recorded by node, then emitted only for nodes neither
        // scan ultimately claims — a merge *part* looks like a split near miss
        // (it is a subset of the merged whole), so warning before both scans
        // finish would be misleading.
        let mut near_misses: BTreeMap<NodeId, (String, Side)> = BTreeMap::new();
        let mut used_left: BTreeSet<u32> = BTreeSet::new();
        let mut used_right: BTreeSet<u32> = BTreeSet::new();

        // Splits: one left whole reconstructed by several right parts.
        let right_pool: Vec<Candidate<NodeId>> = right
            .iter()
            .map(|r| Candidate {
                node: r.id,
                tokens: r.tokens.clone(),
            })
            .collect();
        for whole in &left {
            let candidate = Candidate {
                node: whole.id,
                tokens: whole.tokens.clone(),
            };
            match disjoint_cover(&candidate, &right_pool) {
                Coverage::Clean(m) => {
                    if used_left.contains(&m.whole.index)
                        || m.parts.iter().any(|p| used_right.contains(&p.index))
                    {
                        continue;
                    }
                    used_left.insert(m.whole.index);
                    let from_path = view.item(m.whole).logical_path.clone();
                    for part in &m.parts {
                        used_right.insert(part.index);
                        proposals.push(LinkProposal {
                            left: m.whole.index,
                            right: part.index,
                            evidence: PARTITION_EVIDENCE.into(),
                            // A clean partition is exact and complete: settle the
                            // link so no spurious content diff is written between
                            // the whole and its part.
                            settled: true,
                            projection: ProjectionHint::default()
                                .action("tabular_split")
                                .tag(TAG_SPLIT)
                                .summary(
                                    Summary::new()
                                        .text("Split from ")
                                        .path(from_path.clone(), Side::From),
                                ),
                        });
                    }
                }
                Coverage::NearMiss => {
                    near_misses.insert(
                        whole.id,
                        (view.item(whole.id).logical_path.clone(), Side::From),
                    );
                }
                Coverage::None => {}
            }
        }

        // Merges: one right whole reconstructed by several left parts. Skip nodes
        // already claimed by a split so the two directions never contest.
        let left_pool: Vec<Candidate<NodeId>> = left
            .iter()
            .filter(|l| !used_left.contains(&l.id.index))
            .map(|l| Candidate {
                node: l.id,
                tokens: l.tokens.clone(),
            })
            .collect();
        for whole in &right {
            if used_right.contains(&whole.id.index) {
                continue;
            }
            let candidate = Candidate {
                node: whole.id,
                tokens: whole.tokens.clone(),
            };
            match disjoint_cover(&candidate, &left_pool) {
                Coverage::Clean(m) => {
                    if used_right.contains(&m.whole.index)
                        || m.parts.iter().any(|p| used_left.contains(&p.index))
                    {
                        continue;
                    }
                    used_right.insert(m.whole.index);
                    for part in &m.parts {
                        used_left.insert(part.index);
                        proposals.push(LinkProposal {
                            left: part.index,
                            right: m.whole.index,
                            evidence: PARTITION_EVIDENCE.into(),
                            settled: true,
                            projection: ProjectionHint::default().tag(TAG_MERGE),
                        });
                    }
                }
                Coverage::NearMiss => {
                    near_misses.insert(
                        whole.id,
                        (view.item(whole.id).logical_path.clone(), Side::To),
                    );
                }
                Coverage::None => {}
            }
        }

        // Emit a `possible_split` suggestion only for nodes neither scan claimed.
        let diagnostics = near_misses
            .into_iter()
            .filter(|(id, _)| match id.side {
                TreeSide::Left => !used_left.contains(&id.index),
                TreeSide::Right => !used_right.contains(&id.index),
            })
            .map(|(_, (path, side))| possible_split_diagnostic(&path, side))
            .collect();

        Ok(PairOutput {
            proposals,
            diagnostics,
        })
    }

    fn final_claims(
        &self,
        view: &dyn EngineView,
        data: &dyn DataAccess,
    ) -> BinocResult<Vec<GlobalClaim>> {
        let mut by_left: BTreeMap<u32, Vec<(NodeId, NodeId)>> = BTreeMap::new();
        let mut by_right: BTreeMap<u32, Vec<(NodeId, NodeId)>> = BTreeMap::new();
        for link in view.links() {
            if link.evidence != PARTITION_EVIDENCE {
                continue;
            }
            if link.projection.tags.iter().any(|t| t == TAG_SPLIT) {
                by_left
                    .entry(link.left.index)
                    .or_default()
                    .push((link.left, link.right));
            } else if link.projection.tags.iter().any(|t| t == TAG_MERGE) {
                by_right
                    .entry(link.right.index)
                    .or_default()
                    .push((link.left, link.right));
            }
        }

        let mut claims = Vec::new();
        for group in by_left.values() {
            if group.len() < 2 {
                continue;
            }
            let from = view.item(group[0].0).logical_path.clone();
            let mut to: Vec<String> = group
                .iter()
                .map(|(_, right)| view.item(*right).logical_path.clone())
                .collect();
            to.sort();
            let covered = view
                .identity_tokens(group[0].0, data)?
                .map(|t| t.len())
                .unwrap_or(0);
            claims.push(partition_claim(TAG_SPLIT, &from, &to, covered, true));
        }
        for group in by_right.values() {
            if group.len() < 2 {
                continue;
            }
            let to = view.item(group[0].1).logical_path.clone();
            let mut from: Vec<String> = group
                .iter()
                .map(|(left, _)| view.item(*left).logical_path.clone())
                .collect();
            from.sort();
            let covered = view
                .identity_tokens(group[0].1, data)?
                .map(|t| t.len())
                .unwrap_or(0);
            claims.push(partition_claim(TAG_MERGE, &to, &from, covered, false));
        }
        Ok(claims)
    }
}

fn possible_split_diagnostic(path: &str, side: Side) -> Diagnostic {
    Diagnostic::suggestion(
        "binoc.possible_split",
        Summary::new().text("'").path(path.to_string(), side).text(
            "' shares rows with other unmatched tables but the relationship \
                 is not a clean partition (residual, shared, or extra rows); left as add/remove",
        ),
    )
}

/// Build a split or merge [`GlobalClaim`]. For a split, `whole` is the one input
/// and `members` the outputs; for a merge the roles invert (one output,
/// several inputs).
fn partition_claim(
    tag: &str,
    whole: &str,
    members: &[String],
    covered: usize,
    is_split: bool,
) -> GlobalClaim {
    let summary = if is_split {
        let mut s = Summary::new()
            .path(whole.to_string(), Side::From)
            .text(" split into ");
        for (index, member) in members.iter().enumerate() {
            if index > 0 {
                s = s.text(", ");
            }
            s = s.path(member.clone(), Side::To);
        }
        s
    } else {
        let mut s = Summary::new();
        for (index, member) in members.iter().enumerate() {
            if index > 0 {
                s = s.text(", ");
            }
            s = s.path(member.clone(), Side::From);
        }
        s.text(" merged into ").path(whole.to_string(), Side::To)
    };
    let (from, to): (serde_json::Value, serde_json::Value) = if is_split {
        (whole.into(), members.to_vec().into())
    } else {
        (members.to_vec().into(), whole.into())
    };
    GlobalClaim::new(tag)
        .with_param("from", from)
        .with_param("to", to)
        .with_param("parts", members.len().into())
        .with_param("covered", covered.into())
        .with_param("residual", 0.into())
        .with_summary(summary)
}
