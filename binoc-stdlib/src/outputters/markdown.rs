use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use binoc_core::ir::{DiffNode, Migration};
use binoc_core::traits::{BinocResult, Outputter};

/// Config for the standard Markdown outputter. Parsed from the
/// `output.markdown` (or `output."binoc.markdown"`) section of the
/// dataset config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownOutputterConfig {
    /// Significance classification: maps category names to lists of tags.
    #[serde(default = "default_significance")]
    pub significance: BTreeMap<String, Vec<String>>,
}

impl Default for MarkdownOutputterConfig {
    fn default() -> Self {
        Self {
            significance: default_significance(),
        }
    }
}

fn default_significance() -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    map.insert(
        "ministerial".into(),
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

/// Standard markdown outputter. Groups changes by significance category.
pub struct MarkdownOutputter;

impl Outputter for MarkdownOutputter {
    fn name(&self) -> &str {
        "binoc.markdown"
    }
    fn file_extension(&self) -> &str {
        "md"
    }

    fn render(&self, migrations: &[Migration], config: &serde_json::Value) -> BinocResult<String> {
        let md_config: MarkdownOutputterConfig =
            serde_json::from_value(config.clone()).unwrap_or_default();
        Ok(render_markdown(migrations, &md_config))
    }
}

/// Generate a Markdown changelog from one or more migrations.
/// Public so other outputters can reuse or wrap it.
pub fn render_markdown(migrations: &[Migration], config: &MarkdownOutputterConfig) -> String {
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
                format_node(&mut out, node, 0);
            }
            out.push('\n');
        }

        if !uncategorized.is_empty() {
            out.push_str("## Other Changes\n\n");
            for node in &uncategorized {
                format_node(&mut out, node, 0);
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

fn format_node(out: &mut String, node: &DiffNode, _depth: usize) {
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
    use binoc_core::ir::{DiffNode, Migration};

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
        let config = MarkdownOutputterConfig::default();
        let md = render_markdown(&[migration], &config);
        assert!(md.contains("# Changelog: v1 → v2"));
        assert!(md.contains("## Substantive Changes"));
        assert!(md.contains("**data.csv**"), "path should be bold in bullet");
        assert!(
            md.contains("Column added: 'email'"),
            "summary should appear"
        );
    }

    #[test]
    fn to_markdown_no_changes_shows_message() {
        let migration = Migration::new("v1", "v2", None);
        let config = MarkdownOutputterConfig::default();
        let md = render_markdown(&[migration], &config);
        assert!(md.contains("No changes detected"));
    }

    #[test]
    fn significance_classification_maps_tags_to_categories() {
        let ministerial = DiffNode::new("modify", "csv", "a.csv").with_tag("binoc.column-reorder");
        let substantive = DiffNode::new("modify", "csv", "b.csv").with_tag("binoc.schema-change");
        let config = MarkdownOutputterConfig::default();

        let md_ministerial =
            render_markdown(&[Migration::new("v1", "v2", Some(ministerial))], &config);
        assert!(md_ministerial.contains("## Ministerial Changes"));
        assert!(md_ministerial.contains("**a.csv**"));

        let md_substantive =
            render_markdown(&[Migration::new("v1", "v2", Some(substantive))], &config);
        assert!(md_substantive.contains("## Substantive Changes"));
        assert!(md_substantive.contains("**b.csv**"));
    }

    #[test]
    fn parent_node_with_summary_is_rendered_alongside_children() {
        let root = DiffNode::new("modify", "directory", "data/")
            .with_summary("Directory restructured")
            .with_tag("binoc.schema-change")
            .with_children(vec![
                DiffNode::new("modify", "csv", "data/a.csv")
                    .with_summary("Columns reordered")
                    .with_tag("binoc.column-reorder"),
                DiffNode::new("add", "csv", "data/b.csv").with_summary("New table"),
            ]);
        let config = MarkdownOutputterConfig::default();
        let md = render_markdown(&[Migration::new("v1", "v2", Some(root))], &config);
        assert!(md.contains("**data/**"), "parent node should be rendered");
        assert!(
            md.contains("Directory restructured"),
            "parent summary should appear"
        );
        assert!(
            md.contains("**data/a.csv**"),
            "child should also be rendered"
        );
        assert!(
            md.contains("**data/b.csv**"),
            "second child should also be rendered"
        );
    }

    #[test]
    fn bare_container_without_summary_is_not_rendered() {
        let root =
            DiffNode::new("modify", "directory", "data/").with_children(vec![DiffNode::new(
                "add",
                "csv",
                "data/a.csv",
            )
            .with_summary("New table")
            .with_tag("binoc.column-addition")]);
        let config = MarkdownOutputterConfig::default();
        let md = render_markdown(&[Migration::new("v1", "v2", Some(root))], &config);
        assert!(
            !md.contains("**data/**"),
            "bare container should not be rendered"
        );
        assert!(md.contains("**data/a.csv**"), "child should be rendered");
    }

    #[test]
    fn node_without_summary_uses_fallback_description() {
        let node = DiffNode::new("add", "file", "new.txt").with_tag("binoc.content-changed");
        let migration = Migration::new("v1", "v2", Some(node));
        let config = MarkdownOutputterConfig::default();
        let md = render_markdown(&[migration], &config);
        assert!(md.contains("New file"), "fallback should describe the kind");
    }
}
