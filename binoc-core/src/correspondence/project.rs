use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use binoc_sdk::{
    edit_count_summary, file_name, Changeset, DiffNode, Edit, NodeId, ProjectionAnnotationContext,
    ProjectionAnnotator, ProjectionHint, Side, Summary, TreeSide,
};
use serde_json::json;

use super::store::Store;

#[derive(Debug, Clone)]
pub struct ActionLine {
    pub link_index: Option<usize>,
    pub action: String,
    pub path: String,
    pub source_path: Option<String>,
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
            if let Some(source) = &line.source_path {
                out.push_str(" <- ");
                out.push_str(if source.is_empty() { "<root>" } else { source });
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
            insert_line(&mut root, line, &mut merged_paths);
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
            action,
            path: right_path.clone(),
            source_path: if moved || copied {
                Some(left_path.clone())
            } else {
                None
            },
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
                    path,
                    source_path: None,
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

    Projection { lines }
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
    if source.action.is_some() {
        target.action = source.action.clone();
    }
    if source.item_type.is_some() {
        target.item_type = source.item_type.clone();
    }
    if source.summary.is_some() {
        target.summary = source.summary.clone();
    }
    target.tags.extend(source.tags.iter().cloned());
    target.tags.sort();
    target.tags.dedup();
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

fn insert_line(root: &mut DiffNode, line: &ActionLine, merged_paths: &mut BTreeSet<String>) {
    let segments: Vec<&str> = line
        .path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let had_projected_line = !merged_paths.insert(line.path.clone());
    if segments.is_empty() {
        merge_line_into_node(root, line, had_projected_line);
    } else {
        insert_segments(root, &segments, line, had_projected_line);
    }
}

fn insert_segments(
    parent: &mut DiffNode,
    segments: &[&str],
    line: &ActionLine,
    had_projected_line: bool,
) {
    let current_path = if parent.path.is_empty() {
        segments[0].to_string()
    } else {
        format!("{}/{}", parent.path, segments[0])
    };
    let is_leaf = segments.len() == 1;
    let position = parent
        .children
        .iter()
        .position(|child| child.path == current_path)
        .unwrap_or_else(|| {
            parent
                .children
                .push(DiffNode::new("modify", "item", current_path.clone()));
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
    node.source_path = line.source_path.clone();
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
    if let Some(source_path) = &line.source_path {
        merge_source_path(node, source_path);
    }
    node.tags.extend(line.projection.tags.iter().cloned());
    append_visible_edits(node, &line.edits);
    node.summary = Some(
        Summary::new().count(
            node.details
                .get("projection_line_count")
                .and_then(|value| value.as_u64())
                .unwrap_or(1)
                + 1,
            "projected change",
        ),
    );
    let count = node
        .details
        .get("projection_line_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(1)
        + 1;
    node.details
        .insert("projection_line_count".into(), serde_json::json!(count));
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

fn merge_source_path(node: &mut DiffNode, source_path: &str) {
    let mut sources: Vec<String> = node
        .details
        .get("source_paths")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if let Some(existing) = &node.source_path {
        sources.push(existing.clone());
    }
    sources.push(source_path.to_string());
    sources.sort();
    sources.dedup();
    node.source_path = sources.first().cloned();
    if sources.len() > 1 {
        node.details
            .insert("source_paths".into(), serde_json::json!(sources));
    }
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
}
