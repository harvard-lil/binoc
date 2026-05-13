use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use binoc_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownRendererConfig {
    #[serde(default = "default_significance")]
    pub significance: BTreeMap<String, Vec<String>>,
}

impl Default for MarkdownRendererConfig {
    fn default() -> Self {
        Self {
            significance: default_significance(),
        }
    }
}

fn default_significance() -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    map.insert(
        "clerical".into(),
        vec![
            "binoc.column-reorder".into(),
            "binoc.whitespace-change".into(),
            "binoc.folder-rename".into(),
            "binoc.encoding-change".into(),
        ],
    );
    map.insert(
        "substantive".into(),
        vec![
            "binoc.column-addition".into(),
            "binoc.column-removal".into(),
            "binoc.schema-change".into(),
            "binoc.row-addition".into(),
            "binoc.row-removal".into(),
            "binoc.content-changed".into(),
        ],
    );
    map
}

pub struct MarkdownRenderer;

impl Renderer for MarkdownRenderer {
    fn descriptor(&self) -> RendererDescriptor {
        RendererDescriptor::new("binoc.markdown", "md")
    }

    fn render(&self, changesets: &[Changeset], config: &serde_json::Value) -> BinocResult<String> {
        let md_config: MarkdownRendererConfig =
            serde_json::from_value(config.clone()).unwrap_or_default();
        Ok(render_markdown(changesets, &md_config))
    }
}

pub fn render_markdown(changesets: &[Changeset], config: &MarkdownRendererConfig) -> String {
    let mut out = String::new();

    for changeset in changesets {
        out.push_str(&format!(
            "# Changelog: {} → {}\n\n",
            changeset.from_snapshot, changeset.to_snapshot
        ));

        let root = match &changeset.root {
            Some(r) => r,
            None => {
                out.push_str("No changes detected.\n\n");
                continue;
            }
        };

        let tag_to_significance = build_tag_map(&config.significance);
        let mut by_significance: BTreeMap<String, Vec<&DiffNode>> = BTreeMap::new();
        let mut uncategorized: Vec<&DiffNode> = Vec::new();
        collect_reportable_nodes(
            root,
            &tag_to_significance,
            &mut by_significance,
            &mut uncategorized,
        );

        for (category, nodes) in &by_significance {
            let title = capitalize(category);
            out.push_str(&format!("## {title} Changes\n\n"));
            for node in nodes {
                format_node(&mut out, node);
            }
            out.push('\n');
        }

        if !uncategorized.is_empty() {
            out.push_str("## Other Changes\n\n");
            for node in &uncategorized {
                format_node(&mut out, node);
            }
            out.push('\n');
        }
    }

    out
}

fn build_tag_map(significance: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (category, tags) in significance {
        for tag in tags {
            map.insert(tag.clone(), category.clone());
        }
    }
    map
}

fn collect_reportable_nodes<'a>(
    node: &'a DiffNode,
    tag_map: &BTreeMap<String, String>,
    by_significance: &mut BTreeMap<String, Vec<&'a DiffNode>>,
    uncategorized: &mut Vec<&'a DiffNode>,
) {
    let is_reportable = node.summary.is_some()
        || !node.tags.is_empty()
        || (node.children.is_empty() && node.action != "identical");

    // A `move` node with its own children (rename+modify from fuzzy
    // correlation) is reported as one unit: the move headline plus an
    // inline summary of each child change. Without this, the move and
    // its content children would land in different significance sections,
    // hiding the relationship.
    let group_as_move = node.action == "move" && !node.children.is_empty();

    if is_reportable {
        let category = if group_as_move {
            // Promote to the highest-significance category among the move
            // node's own tags and any descendant tags.
            node.all_tags()
                .iter()
                .find_map(|tag| tag_map.get(tag))
                .cloned()
        } else {
            node.tags.iter().find_map(|tag| tag_map.get(tag)).cloned()
        };

        match category {
            Some(cat) => by_significance.entry(cat).or_default().push(node),
            None => uncategorized.push(node),
        }
    }

    // Don't descend into a move-with-children's content children — they're
    // surfaced inline by format_node.
    if group_as_move {
        return;
    }

    for child in &node.children {
        collect_reportable_nodes(child, tag_map, by_significance, uncategorized);
    }
}

fn format_node(out: &mut String, node: &DiffNode) {
    let path = if node.path.is_empty() {
        "(root)"
    } else {
        &node.path
    };

    out.push_str(&format!("- **{path}**: "));

    if let Some(summary) = &node.summary {
        out.push_str(summary);
    } else {
        out.push_str(&fallback_description(node));
    }

    // For move-with-children, fold each child's summary in on the same
    // bullet so the rename and the content changes read as one event.
    if node.action == "move" && !node.children.is_empty() {
        let parts: Vec<String> = node
            .children
            .iter()
            .filter(|c| c.action != "identical")
            .map(|c| c.summary.clone().unwrap_or_else(|| fallback_description(c)))
            .collect();
        if !parts.is_empty() {
            out.push_str(" — ");
            out.push_str(&parts.join("; "));
        }
    }

    out.push('\n');
}

fn fallback_description(node: &DiffNode) -> String {
    let action = &node.action;
    let item_type = if node.item_type.is_empty() {
        "item"
    } else {
        &node.item_type
    };

    match action.as_str() {
        "add" => format!("New {item_type}"),
        "remove" => format!("{} removed", capitalize(item_type)),
        "modify" => format!("{} modified", capitalize(item_type)),
        "move" => {
            if let Some(src) = &node.source_path {
                format!("Moved from {src}")
            } else {
                format!("{} moved", capitalize(item_type))
            }
        }
        "copy" => {
            if let Some(src) = &node.source_path {
                format!("Copied from {src}")
            } else {
                format!("{} copied", capitalize(item_type))
            }
        }
        "reorder" => format!("{} reordered", capitalize(item_type)),
        _ => format!("{action} ({item_type})"),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_markdown_includes_significance_sections() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "csv", "data.csv")
                    .with_summary("Column added: 'email'")
                    .with_tag("binoc.column-addition"),
            ),
        );
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("# Changelog: v1 → v2"));
        assert!(md.contains("## Substantive Changes"));
        assert!(md.contains("**data.csv**"));
        assert!(md.contains("Column added: 'email'"));
    }

    #[test]
    fn to_markdown_no_changes_shows_message() {
        let changeset = Changeset::new("v1", "v2", None);
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("No changes detected"));
    }

    #[test]
    fn node_without_summary_uses_fallback() {
        let node = DiffNode::new("add", "file", "new.txt").with_tag("binoc.content-changed");
        let changeset = Changeset::new("v1", "v2", Some(node));
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("New file"));
    }

    #[test]
    fn move_with_children_renders_as_one_unit() {
        // A `move` node carrying its own content-change children should
        // be reported as a single bullet (rename headline + inline child
        // summaries), classified by the highest-significance descendant
        // tag. Children must NOT also appear as separate entries.
        let child = DiffNode::new("modify", "column", "email")
            .with_summary("Column added: 'email'")
            .with_tag("binoc.column-addition");
        let move_node = DiffNode::new("move", "tabular", "data_v2.csv")
            .with_source_path("data.csv")
            .with_summary("Moved from data.csv (modified)")
            .with_tag("binoc.move")
            .with_tag("binoc.move.modified")
            .with_children(vec![child]);
        let root = DiffNode::new("modify", "directory", "").with_children(vec![move_node]);

        let md = render_markdown(
            &[Changeset::new("v1", "v2", Some(root))],
            &MarkdownRendererConfig::default(),
        );

        assert!(
            md.contains("## Substantive Changes"),
            "should land in substantive section (promoted from child tag)"
        );
        assert!(md.contains("Moved from data.csv (modified)"));
        assert!(md.contains("Column added: 'email'"));
        // The child should appear exactly once, inline under the move.
        assert_eq!(md.matches("Column added: 'email'").count(), 1);
    }
}
