use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use binoc_sdk::{
    edit_count_summary, file_name, Changeset, DiffNode, Edit, NodeId, ProjectionAnnotationContext,
    ProjectionAnnotator, ProjectionHint, Side, Source, Summary, TreeSide,
};
use serde_json::json;

use super::store::Store;

#[derive(Debug, Clone)]
pub struct ActionLine {
    pub link_index: Option<usize>,
    pub action: String,
    pub path: String,
    pub sources: Vec<Source>,
    pub evidence: Option<String>,
    pub verbs: Vec<String>,
    pub edits: Vec<Edit>,
    pub container: bool,
    pub depth: usize,
    pub summary: Summary,
    pub projection: ProjectionHint,
}

#[derive(Debug, Clone, Default)]
pub struct Projection {
    pub lines: Vec<ActionLine>,
    /// `item_type` for each container path (a node with children), so projection
    /// can label intermediate ancestor nodes that have no change-line of their
    /// own. Falls back to `"container"` when a node set no explicit `item_type`.
    pub container_item_types: BTreeMap<String, String>,
}

impl Projection {
    pub fn changed(&self) -> impl Iterator<Item = &ActionLine> {
        self.lines.iter().filter(|line| line.action != "identical")
    }

    pub fn find(&self, path: &str) -> Vec<&ActionLine> {
        self.lines.iter().filter(|line| line.path == path).collect()
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.action);
            out.push(' ');
            out.push_str(if line.path.is_empty() {
                "<root>"
            } else {
                &line.path
            });
            if let Some(source) = line.sources.iter().find(|source| source.side == Side::From) {
                out.push_str(" <- ");
                out.push_str(if source.path.is_empty() {
                    "<root>"
                } else {
                    &source.path
                });
            }
            if let Some(evidence) = &line.evidence {
                out.push_str(&format!(" [{evidence}]"));
            }
            if !line.verbs.is_empty() {
                out.push_str(&format!(" {{{}}}", line.verbs.join(", ")));
            }
            out.push('\n');
        }
        out
    }

    pub fn to_changeset(&self, from: impl Into<String>, to: impl Into<String>) -> Changeset {
        let mut root = DiffNode::new(
            if self.changed().next().is_some() {
                "modify"
            } else {
                "identical"
            },
            self.lines
                .iter()
                .find_map(|line| line.projection.item_type.clone())
                .unwrap_or_else(|| "item".into()),
            "",
        );
        let mut merged_paths = BTreeSet::new();
        for line in self.changed() {
            insert_line(
                &mut root,
                line,
                &mut merged_paths,
                &self.container_item_types,
            );
        }
        Changeset::new(from, to, Some(root))
    }
}

pub fn project(
    store: &Store,
    edit_lists: &BTreeMap<usize, Vec<Edit>>,
    annotators: &[Arc<dyn ProjectionAnnotator>],
) -> Projection {
    let mut lines = Vec::new();
    let hidden = |id: NodeId| store.beneath_settled(id.side, id.index, false);

    let mut link_lines: Vec<(String, Option<String>, ActionLine)> = Vec::new();
    for (index, link) in store.links.iter() {
        let left_id = NodeId {
            side: TreeSide::Left,
            index: link.left,
        };
        let right_id = NodeId {
            side: TreeSide::Right,
            index: link.right,
        };
        if hidden(left_id) || hidden(right_id) {
            continue;
        }

        let left_path = &store.item(left_id).logical_path;
        let right_path = &store.item(right_id).logical_path;
        let edits = edit_lists.get(&index).cloned().unwrap_or_default();
        let visible_edits: Vec<Edit> = edits
            .iter()
            .filter(|edit| edit.projection.visible)
            .cloned()
            .collect();
        let mut projection = store.projection(right_id).clone();
        projection.merge_from(store.projection(left_id));
        overlay_projection(&mut projection, &link.projection);
        for edit in &edits {
            overlay_projection(&mut projection, &edit.projection.hint);
        }
        let carried = carried_path_change(store, link.left, link.right);
        let copied = projection.action.as_deref() == Some("copy");
        let moved = left_path != right_path && !carried && !copied;
        let changed = !edits.is_empty();
        let line_is_container = !store
            .tree(TreeSide::Right)
            .node(link.right)
            .children
            .is_empty()
            || !store
                .tree(TreeSide::Left)
                .node(link.left)
                .children
                .is_empty();
        let derived_action = match (copied, moved, changed) {
            (true, _, _) => "copy",
            (false, true, _) => "move",
            (false, false, true) => "modify",
            (false, false, false) => "identical",
        };
        let action = projection
            .action
            .clone()
            .unwrap_or_else(|| derived_action.to_string());
        let item_type = projection.item_type.clone().unwrap_or_else(|| {
            if line_is_container {
                "container"
            } else {
                "item"
            }
            .into()
        });
        apply_annotations(
            &mut projection,
            annotators,
            &ProjectionAnnotationContext {
                action: &action,
                item_type: &item_type,
                path: right_path,
                source_path: if moved || copied {
                    Some(left_path.as_str())
                } else {
                    None
                },
                evidence: Some(&link.evidence),
                edits: &visible_edits,
                container: line_is_container,
                unlinked_side: None,
            },
        );

        let summary = projection
            .summary
            .clone()
            .unwrap_or_else(|| default_summary(&action, left_path, &visible_edits));
        let line = ActionLine {
            link_index: Some(index),
            action: action.clone(),
            path: right_path.clone(),
            sources: vec![Source::new(left_path.clone(), Side::From)
                .with_evidence(link.evidence.clone())
                .with_action(action)],
            evidence: Some(link.evidence.clone()),
            verbs: edits.iter().map(|edit| edit.verb.clone()).collect(),
            edits: visible_edits,
            container: line_is_container,
            depth: store.tree(TreeSide::Right).ancestors(link.right).len(),
            summary,
            projection,
        };
        link_lines.push((right_path.clone(), Some(left_path.clone()), line));
    }
    link_lines.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    lines.extend(link_lines.into_iter().map(|(_, _, line)| line));

    for (side, action) in [(TreeSide::Left, "remove"), (TreeSide::Right, "add")] {
        let tree = store.tree(side);
        let mut node_lines = Vec::new();
        for index in 0..tree.len() as u32 {
            let id = NodeId { side, index };
            if hidden(id) || !store.links.of_node(id).is_empty() {
                continue;
            }
            let path = store.item(id).logical_path.clone();
            let summary = Summary::from(if action == "add" { "Added" } else { "Removed" });
            let container = !tree.node(index).children.is_empty();
            let mut projection = store.projection(id).clone();
            let item_type = projection
                .item_type
                .clone()
                .unwrap_or_else(|| if container { "container" } else { "item" }.into());
            apply_annotations(
                &mut projection,
                annotators,
                &ProjectionAnnotationContext {
                    action,
                    item_type: &item_type,
                    path: &path,
                    source_path: None,
                    evidence: None,
                    edits: &[],
                    container,
                    unlinked_side: Some(side),
                },
            );
            node_lines.push((
                path.clone(),
                ActionLine {
                    link_index: None,
                    action: action.to_string(),
                    path: path.clone(),
                    sources: vec![Source::new(
                        path,
                        if side == TreeSide::Left {
                            Side::From
                        } else {
                            Side::To
                        },
                    )
                    .with_action(action)],
                    evidence: None,
                    verbs: Vec::new(),
                    edits: Vec::new(),
                    container,
                    depth: tree.ancestors(index).len(),
                    summary,
                    projection,
                },
            ));
        }
        node_lines.sort_by(|a, b| a.0.cmp(&b.0));
        lines.extend(node_lines.into_iter().map(|(_, line)| line));
    }

    // Record the item_type of every container node (one with children) so the
    // tree builder can label interior ancestor nodes that have no change-line.
    // The right side wins where a path exists on both.
    let mut container_item_types = BTreeMap::new();
    for side in [TreeSide::Left, TreeSide::Right] {
        let tree = store.tree(side);
        for index in 0..tree.len() as u32 {
            let node = tree.node(index);
            if node.children.is_empty() {
                continue;
            }
            let item_type = node
                .projection
                .item_type
                .clone()
                .unwrap_or_else(|| "container".into());
            match side {
                TreeSide::Left => {
                    container_item_types
                        .entry(node.item.logical_path.clone())
                        .or_insert(item_type);
                }
                TreeSide::Right => {
                    container_item_types.insert(node.item.logical_path.clone(), item_type);
                }
            }
        }
    }

    Projection {
        lines,
        container_item_types,
    }
}

fn default_summary(action: &str, left_path: &str, edits: &[Edit]) -> Summary {
    match action {
        "copy" => Summary::new()
            .text("Copied from ")
            .path(left_path.to_string(), Side::From),
        "move" => {
            let mut summary = Summary::new()
                .text("Moved from ")
                .path(left_path.to_string(), Side::From);
            if !edits.is_empty() {
                summary = summary.text(" (modified)");
            }
            summary
        }
        "modify" => edit_count_summary(edits.len()),
        _ => Summary::new(),
    }
}

fn overlay_projection(target: &mut ProjectionHint, source: &ProjectionHint) {
    target.overlay_from(source);
}

fn apply_annotations(
    projection: &mut ProjectionHint,
    annotators: &[Arc<dyn ProjectionAnnotator>],
    ctx: &ProjectionAnnotationContext<'_>,
) {
    for annotator in annotators {
        overlay_projection(projection, &annotator.annotate(ctx));
    }
}

fn carried_path_change(store: &Store, left: u32, right: u32) -> bool {
    let left_parent = store.tree(TreeSide::Left).node(left).parent;
    let right_parent = store.tree(TreeSide::Right).node(right).parent;
    match (left_parent, right_parent) {
        (Some(left_parent), Some(right_parent)) => {
            store.links.linked(left_parent, right_parent)
                && file_name(&store.left.node(left).item.logical_path)
                    == file_name(&store.right.node(right).item.logical_path)
        }
        _ => false,
    }
}

fn insert_line(
    root: &mut DiffNode,
    line: &ActionLine,
    merged_paths: &mut BTreeSet<String>,
    container_item_types: &BTreeMap<String, String>,
) {
    // Each yielded `cumulative` preserves the original separators (`/` for
    // membership, `/>` for decompose boundaries), so it matches node paths and
    // can be used directly as the projection node key.
    let segments: Vec<&str> = binoc_sdk::segments(&line.path)
        .into_iter()
        .map(|(cumulative, _name)| cumulative)
        .collect();
    let had_projected_line = !merged_paths.insert(line.path.clone());
    if segments.is_empty() {
        merge_line_into_node(root, line, had_projected_line);
    } else {
        insert_segments(
            root,
            &segments,
            line,
            had_projected_line,
            container_item_types,
        );
    }
}

fn insert_segments(
    parent: &mut DiffNode,
    segments: &[&str],
    line: &ActionLine,
    had_projected_line: bool,
    container_item_types: &BTreeMap<String, String>,
) {
    let current_path = segments[0].to_string();
    let is_leaf = segments.len() == 1;
    let position = parent
        .children
        .iter()
        .position(|child| child.path == current_path)
        .unwrap_or_else(|| {
            // A node skeleton created only to host descendants is an interior
            // container. Use the kind the producing rule named for this path
            // (e.g. "SQLite database"); fall back to the generic "container".
            // A leaf gets its real item_type from the line via
            // `merge_line_into_node` below.
            let item_type = if is_leaf {
                "item"
            } else {
                container_item_types
                    .get(&current_path)
                    .map(String::as_str)
                    .unwrap_or("container")
            };
            parent
                .children
                .push(DiffNode::new("modify", item_type, current_path.clone()));
            parent.children.len() - 1
        });
    if is_leaf {
        merge_line_into_node(&mut parent.children[position], line, had_projected_line);
    } else {
        insert_segments(
            &mut parent.children[position],
            &segments[1..],
            line,
            had_projected_line,
            container_item_types,
        );
    }
}

fn merge_line_into_node(node: &mut DiffNode, line: &ActionLine, had_projected_line: bool) {
    if had_projected_line {
        merge_projected_collision(node, line);
        return;
    }

    node.action = line.action.clone();
    node.item_type = line
        .projection
        .item_type
        .clone()
        .unwrap_or_else(|| if line.container { "container" } else { "item" }.into());
    node.sources = line.sources.clone();
    node.summary = Some(line.summary.clone());
    node.tags.extend(line.projection.tags.iter().cloned());
    if !line.edits.is_empty() {
        node.details.insert(
            "edits".into(),
            json!(line
                .edits
                .iter()
                .map(|edit| json!({ "verb": edit.verb, "params": edit.params }))
                .collect::<Vec<_>>()),
        );
    }
}

fn merge_projected_collision(node: &mut DiffNode, line: &ActionLine) {
    node.action = merge_action(&node.action, &line.action).to_string();
    if node.item_type == "item" {
        node.item_type = line
            .projection
            .item_type
            .clone()
            .unwrap_or_else(|| if line.container { "container" } else { "item" }.into());
    }
    merge_sources(node, &line.sources);
    node.tags.extend(line.projection.tags.iter().cloned());
    append_visible_edits(node, &line.edits);
    node.summary = Some(merged_summary(&node.sources));
}

fn merge_action(left: &str, right: &str) -> &'static str {
    if left == "copy" || right == "copy" {
        "copy"
    } else if left == "move" || right == "move" {
        "move"
    } else if left == "modify" || right == "modify" {
        "modify"
    } else if left == "add" || right == "add" {
        "add"
    } else if left == "remove" || right == "remove" {
        "remove"
    } else {
        "identical"
    }
}

fn merge_sources(node: &mut DiffNode, sources: &[Source]) {
    node.sources.extend(sources.iter().cloned());
    node.sources.sort();
    node.sources.dedup();
}

fn merged_summary(sources: &[Source]) -> Summary {
    let mut summary = Summary::new().text("Merged from ");
    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            summary = summary.text(", ");
        }
        summary = summary.path(source.path.clone(), source.side);
    }
    summary
}

fn append_visible_edits(node: &mut DiffNode, edits: &[Edit]) {
    if edits.is_empty() {
        return;
    }
    let mut current = node
        .details
        .remove("edits")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    current.extend(
        edits
            .iter()
            .map(|edit| json!({ "verb": edit.verb, "params": edit.params })),
    );
    node.details.insert("edits".into(), json!(current));
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::correspondence::store::Store;
    use binoc_sdk::{ItemRef, LinkProposal};

    fn item(path: &str) -> ItemRef {
        ItemRef {
            logical_path: path.into(),
            is_dir: false,
            content_hash: None,
            size: None,
            media_type: None,
            projection_hint: ProjectionHint::default().item_type("leaf"),
            handle: path.into(),
        }
    }

    #[test]
    fn projection_uses_rule_supplied_metadata_for_visible_output() {
        let mut store = Store::new(
            ItemRef {
                is_dir: true,
                projection_hint: ProjectionHint::default().item_type("tree"),
                ..item("")
            },
            ItemRef {
                is_dir: true,
                projection_hint: ProjectionHint::default().item_type("tree"),
                ..item("")
            },
            ProjectionHint::default().item_type("tree"),
        );
        let left =
            store
                .left
                .add_child(0, item("data"), ProjectionHint::default().item_type("leaf"));
        let right =
            store
                .right
                .add_child(0, item("data"), ProjectionHint::default().item_type("leaf"));
        store.links.apply(
            LinkProposal {
                left,
                right,
                evidence: "test.evidence".into(),
                settled: false,
                projection: ProjectionHint::default(),
            },
            "test-rule",
            1,
        );
        let mut edits = BTreeMap::new();
        edits.insert(
            0,
            vec![Edit::new("test.edit", json!({ "field": "value" }))
                .with_item_type("custom-item")
                .with_tag("test.tag")],
        );

        let changeset = project(&store, &edits, &[]).to_changeset("left", "right");
        let root = changeset.root.expect("root");
        let node = root
            .children
            .iter()
            .find(|node| node.path == "data")
            .unwrap();
        assert_eq!(node.action, "modify");
        assert_eq!(node.item_type, "custom-item");
        assert!(node.tags.contains("test.tag"));
        assert_eq!(node.details["edits"][0]["verb"], json!("test.edit"));
    }

    #[test]
    fn projected_collision_keeps_first_class_sources() {
        let mut store = Store::new(
            ItemRef {
                is_dir: true,
                projection_hint: ProjectionHint::default().item_type("tree"),
                ..item("")
            },
            ItemRef {
                is_dir: true,
                projection_hint: ProjectionHint::default().item_type("tree"),
                ..item("")
            },
            ProjectionHint::default().item_type("tree"),
        );
        let left_a = store.left.add_child(
            0,
            item("a.txt"),
            ProjectionHint::default().item_type("leaf"),
        );
        let left_b = store.left.add_child(
            0,
            item("b.txt"),
            ProjectionHint::default().item_type("leaf"),
        );
        let right = store.right.add_child(
            0,
            item("merged.txt"),
            ProjectionHint::default().item_type("leaf"),
        );
        store.links.apply(
            LinkProposal {
                left: left_a,
                right,
                evidence: "test.merge".into(),
                settled: false,
                projection: ProjectionHint::default(),
            },
            "test-rule",
            1,
        );
        store.links.apply(
            LinkProposal {
                left: left_b,
                right,
                evidence: "test.merge".into(),
                settled: false,
                projection: ProjectionHint::default(),
            },
            "test-rule",
            1,
        );

        let changeset = project(&store, &BTreeMap::new(), &[]).to_changeset("left", "right");
        let root = changeset.root.expect("root");
        let node = root
            .children
            .iter()
            .find(|node| node.path == "merged.txt")
            .unwrap();

        assert_eq!(node.sources.len(), 2);
        assert!(node
            .sources
            .iter()
            .any(|source| source.path == "a.txt" && source.side == Side::From));
        assert!(node
            .sources
            .iter()
            .any(|source| source.path == "b.txt" && source.side == Side::From));
        assert_eq!(
            node.summary.as_ref().map(Summary::plain_text).as_deref(),
            Some("Merged from a.txt, b.txt")
        );
        assert!(!node.details.contains_key("projection_line_count"));
        assert!(!node.details.contains_key("source_paths"));
    }
}
