use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use binoc_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownGroup {
    pub heading: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownRendererConfig {
    #[serde(default)]
    pub groups: Vec<MarkdownGroup>,
    #[serde(default = "default_max_diagnostics")]
    pub max_diagnostics: usize,
}

impl Default for MarkdownRendererConfig {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            max_diagnostics: default_max_diagnostics(),
        }
    }
}

fn default_max_diagnostics() -> usize {
    8
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

        if let Some(root) = &changeset.root {
            let tag_to_group = build_tag_map(&config.groups);
            let mut by_group: Vec<Vec<&DiffNode>> = vec![Vec::new(); config.groups.len()];
            let mut uncategorized: Vec<&DiffNode> = Vec::new();
            collect_reportable_nodes(root, &tag_to_group, &mut by_group, &mut uncategorized);

            if config.groups.is_empty() {
                for node in &uncategorized {
                    format_node(&mut out, node);
                }
                out.push('\n');
            } else {
                for (group, nodes) in config.groups.iter().zip(by_group.iter()) {
                    out.push_str(&format!("## {}\n\n", group.heading));
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
        } else {
            out.push_str("No changes detected.\n\n");
        }

        let diagnostics = display_diagnostics(&changeset.diagnostics, config.max_diagnostics);
        format_diagnostics_section(
            &mut out,
            "Warnings",
            DiagnosticSeverity::Warning,
            &diagnostics,
        );
        format_diagnostics_section(
            &mut out,
            "Suggestions",
            DiagnosticSeverity::Suggestion,
            &diagnostics,
        );
    }

    out
}

fn display_diagnostics(diagnostics: &[Diagnostic], max_diagnostics: usize) -> Vec<&Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for diagnostic in diagnostics {
        let key = (&diagnostic.code, diagnostic.location.as_deref());
        if seen.insert(key) {
            out.push(diagnostic);
            if out.len() >= max_diagnostics {
                break;
            }
        }
    }
    out
}

fn format_diagnostics_section(
    out: &mut String,
    title: &str,
    severity: DiagnosticSeverity,
    diagnostics: &[&Diagnostic],
) {
    let matching: Vec<&Diagnostic> = diagnostics
        .iter()
        .copied()
        .filter(|diagnostic| diagnostic.severity == severity)
        .collect();
    if matching.is_empty() {
        return;
    }

    out.push_str(&format!("## {title}\n\n"));
    for diagnostic in matching {
        out.push_str("- ");
        out.push_str(&humanize_numbers(&diagnostic.message));
        if let Some(location) = &diagnostic.location {
            out.push_str(&format!(" (`{location}`)"));
        }
        out.push_str(&format!(" [{}]\n", diagnostic.code));
    }
    out.push('\n');
}

fn build_tag_map(groups: &[MarkdownGroup]) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for (index, group) in groups.iter().enumerate() {
        for tag in &group.tags {
            map.entry(tag.clone()).or_insert(index);
        }
    }
    map
}

fn classify_tags(
    tags: impl IntoIterator<Item = impl AsRef<str>>,
    tag_map: &BTreeMap<String, usize>,
) -> Option<usize> {
    let mut best: Option<usize> = None;
    for tag in tags {
        if let Some(index) = tag_map.get(tag.as_ref()) {
            best = Some(match best {
                Some(current) => current.min(*index),
                None => *index,
            });
        }
    }
    best
}

fn collect_reportable_nodes<'a>(
    node: &'a DiffNode,
    tag_map: &BTreeMap<String, usize>,
    by_group: &mut [Vec<&'a DiffNode>],
    uncategorized: &mut Vec<&'a DiffNode>,
) {
    let is_reportable = node.summary.is_some()
        || !node.tags.is_empty()
        || (node.children.is_empty() && node.action != "identical");

    // A `move` node with content detail (from fuzzy correlation +
    // re-dispatch) is reported as one unit: the move headline plus a
    // trailing content summary. Detail can live in children, in
    // `annotations.tabular_summary` (TabularAnalyzer), or in
    // `annotations.content_summary` (comparator leaf summary captured
    // during inflate). Without this grouping, the move and its content
    // detail would land in different sections, hiding the
    // relationship.
    let group_as_move = should_group_move_children(node);

    if is_reportable {
        let group_index = if group_as_move {
            // Promote to the highest-priority group among the move
            // node's own tags and any descendant tags. Priority is the
            // first matching configured group.
            classify_tags(node.all_tags().iter(), tag_map)
        } else {
            classify_tags(node.tags.iter(), tag_map)
        };

        match group_index {
            Some(index) => by_group[index].push(node),
            None => uncategorized.push(node),
        }
    }

    // Don't descend into a move-with-children's content children — they're
    // surfaced inline by format_node.
    if group_as_move {
        return;
    }

    for child in &node.children {
        collect_reportable_nodes(child, tag_map, by_group, uncategorized);
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
        out.push_str(&humanize_numbers(summary));
    } else {
        out.push_str(&humanize_numbers(&fallback_description(node)));
    }
    out.push('\n');

    // For a move with content detail, emit the detail as a second
    // top-level bullet under the same path. The two-bullet layout keeps
    // the rename and the content change visually grouped (they share a
    // path and stay in the same significance section) without needing
    // inline punctuation or capitalization fixups.
    if should_group_move_children(node) {
        if let Some(detail) = move_trailer(node) {
            out.push_str(&format!("- **{path}**: {}\n", humanize_numbers(&detail)));
        }
    }
}

fn should_group_move_children(node: &DiffNode) -> bool {
    node.action == "move"
        && !node.tags.contains("binoc.folder-move")
        && move_trailer(node).is_some()
}

/// Build the trailing description for a move bullet, if any.
///
/// Priority (first match wins):
/// 1. `annotations.tabular_summary` — rich, from TabularAnalyzer.
/// 2. `annotations.content_summary` — generic, captured during the
///    controller's re-dispatch merge.
/// 3. A join of non-identical child summaries.
fn move_trailer(node: &DiffNode) -> Option<String> {
    if let Some(s) = annotation_str(node, "tabular_summary") {
        return Some(s);
    }
    if let Some(s) = annotation_str(node, "content_summary") {
        return Some(s);
    }
    if !node.children.is_empty() {
        let parts: Vec<String> = node
            .children
            .iter()
            .filter(|c| c.action != "identical")
            .map(|c| c.summary.clone().unwrap_or_else(|| fallback_description(c)))
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("; "));
        }
    }
    None
}

fn annotation_str(node: &DiffNode, key: &str) -> Option<String> {
    node.annotations
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
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

fn humanize_numbers(input: &str) -> String {
    // Locale-aware formatting is intentionally out of scope for now; this is US-style grouping.
    let mut out = String::with_capacity(input.len() + input.len() / 3);
    let mut digits = String::new();

    let flush_digits = |out: &mut String, digits: &mut String| {
        if digits.len() > 3 {
            let first_group = digits.len() % 3;
            let mut idx = 0;
            if first_group != 0 {
                out.push_str(&digits[..first_group]);
                idx = first_group;
                if idx < digits.len() {
                    out.push(',');
                }
            }
            while idx < digits.len() {
                let end = (idx + 3).min(digits.len());
                out.push_str(&digits[idx..end]);
                idx = end;
                if idx < digits.len() {
                    out.push(',');
                }
            }
        } else {
            out.push_str(digits);
        }
        digits.clear();
    };

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            flush_digits(&mut out, &mut digits);
            out.push(ch);
        }
    }
    flush_digits(&mut out, &mut digits);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_markdown_default_is_flat_list() {
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
        assert!(!md.contains("## "));
        assert!(md.contains("**data.csv**"));
        assert!(md.contains("Column added: 'email'"));
    }

    #[test]
    fn to_markdown_respects_explicit_group_sections() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "csv", "data.csv")
                    .with_summary("Column added: 'email'")
                    .with_tag("binoc.column-addition"),
            ),
        );
        let config = MarkdownRendererConfig {
            groups: vec![MarkdownGroup {
                heading: "Substantive changes".into(),
                tags: vec!["binoc.column-addition".into()],
            }],
            ..Default::default()
        };
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("## Substantive changes"));
    }

    #[test]
    fn to_markdown_uses_declared_group_order_and_first_match_priority() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "csv", "data.csv")
                    .with_summary("Column added: 'email'")
                    .with_tag("binoc.column-addition")
                    .with_tag("binoc.content-changed"),
            ),
        );
        let config = MarkdownRendererConfig {
            groups: vec![
                MarkdownGroup {
                    heading: "Critical".into(),
                    tags: vec!["binoc.content-changed".into()],
                },
                MarkdownGroup {
                    heading: "Substantive".into(),
                    tags: vec!["binoc.column-addition".into()],
                },
            ],
            ..Default::default()
        };
        let md = render_markdown(&[changeset], &config);
        let critical = md.find("## Critical").expect("missing Critical heading");
        let substantive = md
            .find("## Substantive")
            .expect("missing Substantive heading");
        assert!(
            critical < substantive,
            "group headings should keep declaration order; got:\n{md}"
        );
        let substantive_section = md
            .split("## Substantive")
            .nth(1)
            .expect("expected substantive section");
        assert!(md.contains("## Critical\n\n- **data.csv**: Column added: 'email'"));
        assert!(
            !substantive_section.contains("**data.csv**"),
            "node should land in first matching group; got:\n{md}"
        );
    }

    #[test]
    fn to_markdown_no_changes_shows_message() {
        let changeset = Changeset::new("v1", "v2", None);
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("No changes detected"));
    }

    #[test]
    fn renders_structured_diagnostics_sections() {
        let mut changeset = Changeset::new(
            "v1",
            "v2",
            Some(DiffNode::new("modify", "file", "data.bin").with_summary("Content changed")),
        );
        changeset.push_diagnostic(
            Diagnostic::warning("binoc.test-warning", "A parser setting was ignored")
                .with_location("data.bin"),
        );
        changeset.push_diagnostic(
            Diagnostic::suggestion(
                "binoc.binary-fallback",
                "Compared as binary; a plugin may provide a more semantic diff.",
            )
            .with_location("data.bin"),
        );

        let md = render_markdown(&[changeset], &MarkdownRendererConfig::default());
        assert!(md.contains("## Warnings"));
        assert!(md.contains("## Suggestions"));
        assert!(md.contains("[binoc.binary-fallback]"));
        assert!(md.contains("(`data.bin`)"));
    }

    #[test]
    fn diagnostics_are_shown_even_without_changes() {
        let mut changeset = Changeset::new("v1", "v2", None);
        changeset.push_diagnostic(Diagnostic::suggestion(
            "binoc.test",
            "A plugin may provide more context.",
        ));

        let md = render_markdown(&[changeset], &MarkdownRendererConfig::default());
        assert!(md.contains("No changes detected."));
        assert!(md.contains("## Suggestions"));
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
    fn move_with_children_renders_as_paired_bullets() {
        // A `move` node carrying its own content-change children should
        // be reported as two stacked top-level bullets under the same
        // path (move headline + content detail), classified together by
        // the highest-significance descendant tag. Children must NOT
        // also appear as separate enumerated entries elsewhere.
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
            &MarkdownRendererConfig {
                groups: vec![MarkdownGroup {
                    heading: "Substantive changes".into(),
                    tags: vec!["binoc.column-addition".into()],
                }],
                ..Default::default()
            },
        );

        assert!(
            md.contains("## Substantive changes"),
            "should land in substantive section (promoted from child tag)"
        );
        assert!(md.contains("- **data_v2.csv**: Moved from data.csv (modified)\n"));
        assert!(md.contains("- **data_v2.csv**: Column added: 'email'\n"));
        // The child detail should appear exactly once, never as its own
        // separately-categorized entry.
        assert_eq!(md.matches("Column added: 'email'").count(), 1);
    }

    #[test]
    fn move_with_tabular_summary_annotation_renders_as_paired_bullets() {
        // A CSV rename+modify produces a move node with no children but
        // `annotations.tabular_summary` set by TabularAnalyzer.
        let mut move_node = DiffNode::new("move", "tabular", "data_v2.csv")
            .with_source_path("data.csv")
            .with_summary("Moved from data.csv (modified)")
            .with_tag("binoc.move")
            .with_tag("binoc.move.modified")
            .with_tag("binoc.column-addition")
            .with_tag("binoc.schema-change");
        move_node.annotations.insert(
            "tabular_summary".into(),
            serde_json::json!("Column added: 'email'"),
        );
        let root = DiffNode::new("modify", "directory", "").with_children(vec![move_node]);

        let md = render_markdown(
            &[Changeset::new("v1", "v2", Some(root))],
            &MarkdownRendererConfig {
                groups: vec![MarkdownGroup {
                    heading: "Substantive changes".into(),
                    tags: vec!["binoc.column-addition".into(), "binoc.schema-change".into()],
                }],
                ..Default::default()
            },
        );

        assert!(md.contains("## Substantive changes"));
        assert!(
            md.contains("- **data_v2.csv**: Moved from data.csv (modified)\n"),
            "move headline bullet missing; got:\n{md}"
        );
        assert!(
            md.contains("- **data_v2.csv**: Column added: 'email'\n"),
            "tabular_summary must render as its own bullet under the same path; got:\n{md}"
        );
    }

    #[test]
    fn move_with_content_summary_annotation_renders_as_paired_bullets() {
        // A text rename+modify produces a move node with no children,
        // no tabular_summary, but `annotations.content_summary` from
        // the controller's re-dispatch merge.
        let mut move_node = DiffNode::new("move", "text", "meeting-notes-v2.txt")
            .with_source_path("notes.txt")
            .with_summary("Moved from notes.txt (modified)")
            .with_tag("binoc.move")
            .with_tag("binoc.move.modified")
            .with_tag("binoc.content-changed")
            .with_tag("binoc.lines-added");
        move_node
            .annotations
            .insert("content_summary".into(), serde_json::json!("2 lines added"));
        let root = DiffNode::new("modify", "directory", "").with_children(vec![move_node]);

        let md = render_markdown(
            &[Changeset::new("v1", "v2", Some(root))],
            &MarkdownRendererConfig::default(),
        );

        assert!(
            md.contains("- **meeting-notes-v2.txt**: Moved from notes.txt (modified)\n"),
            "move headline bullet missing; got:\n{md}"
        );
        assert!(
            md.contains("- **meeting-notes-v2.txt**: 2 lines added\n"),
            "content_summary must render as its own bullet under the same path; got:\n{md}"
        );
    }

    #[test]
    fn move_trailer_prefers_tabular_over_content_summary() {
        let mut move_node = DiffNode::new("move", "tabular", "data_v2.csv")
            .with_source_path("data.csv")
            .with_summary("Moved from data.csv (modified)")
            .with_tag("binoc.move");
        move_node.annotations.insert(
            "tabular_summary".into(),
            serde_json::json!("Column added: 'email'"),
        );
        move_node
            .annotations
            .insert("content_summary".into(), serde_json::json!("CSV modified"));
        let root = DiffNode::new("modify", "directory", "").with_children(vec![move_node]);

        let md = render_markdown(
            &[Changeset::new("v1", "v2", Some(root))],
            &MarkdownRendererConfig::default(),
        );

        assert!(md.contains("- **data_v2.csv**: Column added: 'email'\n"));
        assert!(
            !md.contains("CSV modified"),
            "content_summary should be shadowed by tabular_summary"
        );
    }

    #[test]
    fn folder_move_descends_into_children_instead_of_grouping() {
        let node = DiffNode::new("move", "directory", "docs-v2")
            .with_source_path("docs-v1")
            .with_summary("Folder moved from docs-v1")
            .with_tag("binoc.move")
            .with_tag("binoc.folder-move")
            .with_children(vec![DiffNode::new("add", "file", "docs-v2/new.txt")
                .with_summary("New file")
                .with_tag("binoc.content-changed")]);

        let md = render_markdown(
            &[Changeset::new(
                "a",
                "b",
                Some(DiffNode::new("modify", "directory", "").with_children(vec![node])),
            )],
            &MarkdownRendererConfig::default(),
        );

        assert!(md.contains("- **docs-v2**: Folder moved from docs-v1\n"));
        assert!(md.contains("- **docs-v2/new.txt**: New file\n"));
        assert!(
            !md.contains("- **docs-v2**: New file"),
            "folder-move children should render as their own bullets"
        );
    }

    #[test]
    fn humanizes_large_numbers_in_summaries() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "csv", "data.csv")
                    .with_summary("5975 rows added; 18133333 cells changed"),
            ),
        );
        let config = MarkdownRendererConfig::default();
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("5,975 rows added; 18,133,333 cells changed"));
    }
}
