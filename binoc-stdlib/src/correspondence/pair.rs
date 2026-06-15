use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;

use binoc_sdk::{
    file_name, BinocError, BinocResult, DataAccess, Diagnostic, EngineView, FileCorrespondenceRule,
    FileSelector, ItemRef, LinkProposal, NodeId, PairDescriptor, PairOutput, PairRule,
    ProjectionHint, TreeSide,
};
use regex::Regex;

const FUZZY_MAX_BYTES: u64 = 8 * 1024 * 1024;

pub struct RootPair;

impl PairRule for RootPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.root".into(),
            emits: vec!["binoc.pair.root".into()],
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
            sees_beneath_settled: false,
        }
    }

    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput> {
        let mut proposals = Vec::new();
        let mut right_by_hash: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
        let mut right_by_path: BTreeMap<&str, (NodeId, String)> = BTreeMap::new();
        for id in view.nodes(TreeSide::Right) {
            let item = view.item(id);
            if item.is_dir {
                continue;
            }
            let hash = item.resolve_hash(data)?;
            right_by_hash.entry(hash.clone()).or_default().push(id);
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

        for id in view.nodes(TreeSide::Left) {
            let item = view.item(id);
            if item.is_dir || used_left.contains(&id.index) || view.is_linked(id) {
                continue;
            }
            let hash = item.resolve_hash(data)?;
            let Some(candidates) = right_by_hash.get(&hash) else {
                continue;
            };
            let left_name = file_name(&item.logical_path);
            let mut available: Vec<NodeId> = candidates
                .iter()
                .copied()
                .filter(|id| !used_right.contains(&id.index) && !view.is_linked(*id))
                .collect();
            available.sort_by(|a, b| {
                let a_name_match = file_name(&view.item(*a).logical_path) == left_name;
                let b_name_match = file_name(&view.item(*b).logical_path) == left_name;
                b_name_match
                    .cmp(&a_name_match)
                    .then_with(|| view.item(*a).logical_path.cmp(&view.item(*b).logical_path))
            });
            if let Some(right_id) = available.first() {
                let mut projection = move_projection_hint();
                if archive_like(&item.logical_path)
                    || archive_like(&view.item(*right_id).logical_path)
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

pub struct CopyPair;

impl PairRule for CopyPair {
    fn descriptor(&self) -> PairDescriptor {
        PairDescriptor {
            name: "binoc.pair.copy".into(),
            emits: vec!["binoc.pair.copy".into()],
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
                format!(
                    "File correspondence rule '{}' has an invalid left path_regex: {err}",
                    rule.name
                ),
            ));
            continue;
        }
        if let Some(err) = validate_selector_regex(&rule.right) {
            diagnostics.push(Diagnostic::warning(
                "binoc.declared_correspondence.invalid_right_regex",
                format!(
                    "File correspondence rule '{}' has an invalid right path_regex: {err}",
                    rule.name
                ),
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
                format!(
                    "File correspondence rule '{}' has ambiguous key '{}' (left matches: {}, right matches: {})",
                    rule.name, key, left_count, right_count
                ),
            ));
            continue;
        }

        diagnostics.push(Diagnostic::warning(
            "binoc.declared_correspondence.no_matching_files",
            format!(
                "File correspondence rule '{}' had no effect: the left selector matched {} items and the right selector matched {} items",
                rule.name,
                left.values().map(Vec::len).sum::<usize>(),
                right.values().map(Vec::len).sum::<usize>()
            ),
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
                    format!(
                        "Skipped fuzzy rename detection for {candidate_count} candidate pairs over limit {}",
                        self.rename_limit
                    ),
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
        format!(
            "Skipped fuzzy content scoring for item over {} bytes",
            FUZZY_MAX_BYTES
        ),
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
