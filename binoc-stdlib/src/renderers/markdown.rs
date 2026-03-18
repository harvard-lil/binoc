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

    fn render(&self, migrations: &[Migration], config: &serde_json::Value) -> BinocResult<String> {
        let md_config: MarkdownRendererConfig =
            serde_json::from_value(config.clone()).unwrap_or_default();
        Ok(render_markdown(migrations, &md_config))
    }
}

pub fn render_markdown(migrations: &[Migration], config: &MarkdownRendererConfig) -> String {
    let mut out = String::new();

    for migration in migrations {
        out.push_str(&format!(
            "# Changelog: {} → {}\n\n",
            migration.from_snapshot, migration.to_snapshot
        ));

        let root = match &migration.root {
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
        || (node.children.is_empty() && node.kind != "identical");

    if is_reportable {
        let category = node.tags.iter().find_map(|tag| tag_map.get(tag)).cloned();

        match category {
            Some(cat) => by_significance.entry(cat).or_default().push(node),
            None => uncategorized.push(node),
        }
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

    out.push('\n');
}

fn fallback_description(node: &DiffNode) -> String {
    let kind = &node.kind;
    let item_type = if node.item_type.is_empty() {
        "item"
    } else {
        &node.item_type
    };

    match kind.as_str() {
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
        _ => format!("{kind} ({item_type})"),
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
        let migration = Migration::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "csv", "data.csv")
                    .with_summary("Column added: 'email'")
                    .with_tag("binoc.column-addition"),
            ),
        );
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[migration], &config);
        assert!(md.contains("# Changelog: v1 → v2"));
        assert!(md.contains("## Substantive Changes"));
        assert!(md.contains("**data.csv**"));
        assert!(md.contains("Column added: 'email'"));
    }

    #[test]
    fn to_markdown_no_changes_shows_message() {
        let migration = Migration::new("v1", "v2", None);
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[migration], &config);
        assert!(md.contains("No changes detected"));
    }

    #[test]
    fn node_without_summary_uses_fallback() {
        let node = DiffNode::new("add", "file", "new.txt").with_tag("binoc.content-changed");
        let migration = Migration::new("v1", "v2", Some(node));
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[migration], &config);
        assert!(md.contains("New file"));
    }
}
