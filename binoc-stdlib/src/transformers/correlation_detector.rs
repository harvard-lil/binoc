//! Tree-wide leaf correlation transformer: combines move and copy
//! detection into one pass at the root.
//!
//! Runs exactly once per diff, at the root of the tree (see
//! [`NodeShapeFilter::Root`]). Indexes every `add`/`remove`/`identical`
//! leaf in the tree by content hash, hydrating hashes on demand via
//! [`ItemRef::resolve_hash`]. Matched pairs are rewritten as single
//! `move` / `copy` nodes at the destination's parent container.
//!
//! Aggregation: when multiple leaves share a hash on either side, emits
//! a single aggregated node (with `details.sources` / `details.destinations`)
//! rather than repeated 1:1 pairings. Behavior is controlled by the
//! transformer's config:
//!
//! ```json
//! { "aggregate_many_to_many": true }
//! ```
//!
//! Default: `aggregate_many_to_many = true`.

use binoc_sdk::*;

use super::correlation::{
    apply_rewrite, collect_and_hydrate, english_list, file_name_of, group_by_hash, parent_path_of,
    source_label_for_move, HashGroup, LeafEntry, RewritePlan,
};

pub struct CorrelationDetector;

#[derive(Debug, Clone)]
struct Config {
    aggregate_many_to_many: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aggregate_many_to_many: true,
        }
    }
}

impl Config {
    fn from_value(v: &serde_json::Value) -> Self {
        let mut out = Self::default();
        if let Some(b) = v.get("aggregate_many_to_many").and_then(|x| x.as_bool()) {
            out.aggregate_many_to_many = b;
        }
        out
    }
}

impl Transformer for CorrelationDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.correlation_detector")
            .with_node_shape(NodeShapeFilter::Root)
    }

    fn transform(
        &self,
        mut node: DiffNode,
        data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let cfg = Config::from_value(config);

        // Pass 1: index + hydrate.
        let entries = collect_and_hydrate(&mut node, data);
        if entries.is_empty() {
            return TransformResult::Unchanged;
        }

        // Pass 2: group by hash, build rewrite plan.
        let groups = group_by_hash(entries);
        let mut plan = RewritePlan::default();

        for (_hash, group) in groups {
            if group.is_trivial() {
                continue;
            }
            plan_group(&group, &cfg, &mut plan);
        }

        if plan.is_empty() {
            return TransformResult::Unchanged;
        }

        // Pass 3: apply rewrite to the tree.
        let rewritten = apply_rewrite(node, &plan);
        TransformResult::Replace(Box::new(rewritten))
    }
}

/// Plan the rewrite for one hash group (all leaves that share a
/// content hash).
fn plan_group(group: &HashGroup, cfg: &Config, plan: &mut RewritePlan) {
    let n_adds = group.adds.len();
    let n_removes = group.removes.len();
    let has_identical = !group.identicals.is_empty();

    // Nothing to do: no adds on this side, or nothing to pair them with.
    if n_adds == 0 || (n_removes == 0 && !has_identical) {
        return;
    }

    let aggregate = cfg.aggregate_many_to_many && (n_adds > 1 || n_removes > 1);

    if n_removes > 0 {
        // Move (add ↔ remove). If there are also identicals in the tree
        // with the same content, they remain untouched — we don't
        // convert "kept files" into copies when an explicit remove+add
        // pair shows the content is being relocated.
        emit_move(group, aggregate, plan);
    } else {
        // Copy: adds pair with existing identical leaves of same hash.
        emit_copy(group, aggregate, plan);
    }
}

fn emit_move(group: &HashGroup, aggregate: bool, plan: &mut RewritePlan) {
    let adds = &group.adds;
    let removes = &group.removes;

    if aggregate {
        // Single aggregated move node listing all sources and destinations.
        let mut sorted_adds: Vec<&LeafEntry> = adds.iter().collect();
        sorted_adds.sort_by(|a, b| a.path.cmp(&b.path));
        let mut sorted_removes: Vec<&LeafEntry> = removes.iter().collect();
        sorted_removes.sort_by(|a, b| a.path.cmp(&b.path));

        let primary = sorted_adds[0];
        let primary_source = sorted_removes[0];

        let mut node = DiffNode::new("move", &primary.item_type, &primary.path)
            .with_source_path(&primary_source.path)
            .with_tag("binoc.move");

        let summary = aggregated_move_summary(&sorted_removes, &sorted_adds);
        node.summary = Some(summary);

        if sorted_removes.len() > 1 {
            let sources: Vec<String> = sorted_removes.iter().map(|e| e.path.clone()).collect();
            node.details
                .insert("sources".into(), serde_json::json!(sources));
        }
        if sorted_adds.len() > 1 {
            let destinations: Vec<String> = sorted_adds.iter().map(|e| e.path.clone()).collect();
            node.details
                .insert("destinations".into(), serde_json::json!(destinations));
        }

        // Remove all originals; insert one node at the primary destination's parent.
        for a in &sorted_adds {
            plan.schedule_remove(&a.path);
        }
        for r in &sorted_removes {
            plan.schedule_remove(&r.path);
        }
        plan.schedule_insert(parent_path_of(&primary.path), node);
        return;
    }

    // Non-aggregated: greedy 1:1 pairing (move) + any leftovers on either side stay.
    let pairs = adds.len().min(removes.len());
    let mut sorted_adds: Vec<&LeafEntry> = adds.iter().collect();
    sorted_adds.sort_by(|a, b| a.path.cmp(&b.path));
    let mut sorted_removes: Vec<&LeafEntry> = removes.iter().collect();
    sorted_removes.sort_by(|a, b| a.path.cmp(&b.path));

    for i in 0..pairs {
        let add = sorted_adds[i];
        let rm = sorted_removes[i];
        let node = DiffNode::new("move", &add.item_type, &add.path)
            .with_summary(format!(
                "Moved from {}",
                source_label_for_move(&rm.path, &add.path)
            ))
            .with_source_path(&rm.path)
            .with_tag("binoc.move");
        plan.schedule_remove(&add.path);
        plan.schedule_remove(&rm.path);
        plan.schedule_insert(parent_path_of(&add.path), node);
    }
}

fn emit_copy(group: &HashGroup, aggregate: bool, plan: &mut RewritePlan) {
    let adds = &group.adds;
    let identicals = &group.identicals;

    let mut sorted_adds: Vec<&LeafEntry> = adds.iter().collect();
    sorted_adds.sort_by(|a, b| a.path.cmp(&b.path));
    let mut sorted_identicals: Vec<&LeafEntry> = identicals.iter().collect();
    sorted_identicals.sort_by(|a, b| a.path.cmp(&b.path));

    let source = sorted_identicals[0];

    if aggregate && sorted_adds.len() > 1 {
        let primary = sorted_adds[0];
        let mut node = DiffNode::new("copy", &primary.item_type, &primary.path)
            .with_source_path(&source.path)
            .with_tag("binoc.copy");
        let destinations: Vec<String> = sorted_adds.iter().map(|e| e.path.clone()).collect();
        node.summary = Some(aggregated_copy_summary(&source.path, &sorted_adds));
        node.details
            .insert("destinations".into(), serde_json::json!(destinations));
        for a in &sorted_adds {
            plan.schedule_remove(&a.path);
        }
        plan.schedule_insert(parent_path_of(&primary.path), node);
        return;
    }

    for add in sorted_adds {
        let node = DiffNode::new("copy", &add.item_type, &add.path)
            .with_summary(format!("Copied from {}", file_name_of(&source.path)))
            .with_source_path(&source.path)
            .with_tag("binoc.copy");
        plan.schedule_remove(&add.path);
        plan.schedule_insert(parent_path_of(&add.path), node);
    }
}

fn aggregated_move_summary(removes: &[&LeafEntry], adds: &[&LeafEntry]) -> String {
    let src_paths: Vec<&str> = removes.iter().map(|e| e.path.as_str()).collect();
    let dst_paths: Vec<&str> = adds.iter().map(|e| e.path.as_str()).collect();
    let src_display = display_names(&src_paths);
    let dst_display = display_names(&dst_paths);
    let src_refs: Vec<&str> = src_display.iter().map(String::as_str).collect();
    let dst_refs: Vec<&str> = dst_display.iter().map(String::as_str).collect();
    match (removes.len(), adds.len()) {
        (1, 1) => format!("Moved from {}", src_refs[0]),
        (_, 1) => format!("Moved from {}", english_list(&src_refs)),
        (1, _) => format!("Moved from {} to {}", src_refs[0], english_list(&dst_refs)),
        _ => format!(
            "{} moved to {}",
            english_list(&src_refs),
            english_list(&dst_refs)
        ),
    }
}

fn aggregated_copy_summary(source_path: &str, adds: &[&LeafEntry]) -> String {
    let src = file_name_of(source_path);
    let dst_paths: Vec<&str> = adds.iter().map(|e| e.path.as_str()).collect();
    let dst_display = display_names(&dst_paths);
    let dst_refs: Vec<&str> = dst_display.iter().map(String::as_str).collect();
    match adds.len() {
        1 => format!("Copied from {src}"),
        _ => format!("Copied from {src} to {}", english_list(&dst_refs)),
    }
}

/// Prefer bare file names, but fall back to full paths when basenames
/// collide (e.g. `kept-copy.txt` at both root and `outer.zip/kept-copy.txt`).
fn display_names(paths: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = paths.iter().map(|p| file_name_of(p).to_string()).collect();
    let unique: std::collections::BTreeSet<&String> = names.iter().collect();
    if unique.len() != names.len() {
        names = paths.iter().map(|p| p.to_string()).collect();
    }
    names
}
