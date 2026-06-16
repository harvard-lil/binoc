use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use binoc_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownGroup {
    pub heading: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    Summary,
    #[default]
    Examples,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownRendererConfig {
    #[serde(default)]
    pub groups: Vec<MarkdownGroup>,
    #[serde(default)]
    pub verbosity: Verbosity,
    #[serde(default = "default_max_examples_per_block")]
    pub max_examples_per_block: usize,
    #[serde(default = "default_max_detail_blocks_per_node")]
    pub max_detail_blocks_per_node: usize,
    #[serde(default = "default_max_value_chars")]
    pub max_value_chars: usize,
    #[serde(default = "default_max_rendered_detail_bytes")]
    pub max_rendered_detail_bytes: usize,
    #[serde(default = "default_max_diagnostics")]
    pub max_diagnostics: usize,
}

impl Default for MarkdownRendererConfig {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            verbosity: Verbosity::Examples,
            max_examples_per_block: default_max_examples_per_block(),
            max_detail_blocks_per_node: default_max_detail_blocks_per_node(),
            max_value_chars: default_max_value_chars(),
            max_rendered_detail_bytes: default_max_rendered_detail_bytes(),
            max_diagnostics: default_max_diagnostics(),
        }
    }
}

fn default_max_examples_per_block() -> usize {
    3
}

fn default_max_detail_blocks_per_node() -> usize {
    4
}

fn default_max_value_chars() -> usize {
    160
}

fn default_max_rendered_detail_bytes() -> usize {
    200_000
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
    let mut detail_budget = DetailBudget::new(config.max_rendered_detail_bytes);

    for changeset in changesets {
        out.push_str(&format!(
            "# Changelog: {} → {}\n\n",
            changeset.from_snapshot, changeset.to_snapshot
        ));

        format_claims_section(&mut out, changeset);

        if let Some(root) = &changeset.root {
            let tag_to_group = build_tag_map(&config.groups);
            let mut by_group: Vec<Vec<&DiffNode>> = vec![Vec::new(); config.groups.len()];
            let mut uncategorized: Vec<&DiffNode> = Vec::new();
            collect_reportable_nodes(root, &tag_to_group, &mut by_group, &mut uncategorized);

            if config.groups.is_empty() {
                for node in &uncategorized {
                    format_node(&mut out, node, config, &mut detail_budget);
                }
                out.push('\n');
            } else {
                for (group, nodes) in config.groups.iter().zip(by_group.iter()) {
                    out.push_str(&format!("## {}\n\n", group.heading));
                    for node in nodes {
                        format_node(&mut out, node, config, &mut detail_budget);
                    }
                    out.push('\n');
                }

                if !uncategorized.is_empty() {
                    out.push_str("## Other Changes\n\n");
                    for node in &uncategorized {
                        format_node(&mut out, node, config, &mut detail_budget);
                    }
                    out.push('\n');
                }
            }
        } else {
            out.push_str("No changes detected.\n\n");
        }

        let diagnostics = display_diagnostics(&changeset.diagnostics, config.max_diagnostics);
        format_diagnostics_section(&mut out, "Errors", DiagnosticSeverity::Error, &diagnostics);
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

fn format_claims_section(out: &mut String, changeset: &Changeset) {
    if changeset.claims.is_empty() {
        return;
    }

    out.push_str("Claims\n\n");
    for claim in &changeset.claims {
        out.push_str("- ");
        match &claim.summary {
            Some(summary) => out.push_str(&render_summary(summary)),
            None => out.push_str(&claim.verb),
        }
        out.push('\n');
    }
    out.push('\n');
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
        out.push_str(&render_summary(&diagnostic.message));
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
    let is_reportable = is_reportable && !is_pure_bookkeeping_container(node);

    // A `move` node with content detail is reported as one unit: the move
    // headline plus a trailing content summary. Detail can live in children or
    // renderer annotations. Without this grouping, the move and its content
    // detail would land in different sections, hiding the relationship.
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

fn is_pure_bookkeeping_container(node: &DiffNode) -> bool {
    if node.children.is_empty() || node.action != "modify" {
        return false;
    }
    let has_visible_edits = node
        .details
        .get("edits")
        .and_then(|value| value.as_array())
        .is_some_and(|edits| !edits.is_empty());
    if has_visible_edits {
        return false;
    }
    node.summary
        .as_ref()
        .is_none_or(|summary| summary.plain_text() == "0 edits")
}

fn format_node(
    out: &mut String,
    node: &DiffNode,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    let path = if node.path.is_empty() {
        "(root)"
    } else {
        &node.path
    };

    // A moved or copied node always names its origin, even when the pairing
    // also carried a content change. When both are present they read as an
    // indented pair under one path — origin first, then the change — matching
    // the sub-bullet style used for edit detail. A move with no separate
    // content (a pure rename, a copy, a merge whose summary already states the
    // origin) stays a single inline line.
    if is_move_node(node) {
        match move_content(node) {
            Some(content) => {
                out.push_str(&format!("- **{path}**:\n"));
                out.push_str(&format!("  - {}\n", render_summary(&move_origin(node))));
                out.push_str(&format!("  - {}\n", render_summary(&content)));
            }
            None => {
                out.push_str(&format!(
                    "- **{path}**: {}\n",
                    render_summary(&move_origin(node))
                ));
            }
        }
    } else {
        out.push_str(&format!(
            "- **{path}**: {}\n",
            render_summary(&node_summary(node))
        ));
    }

    render_sources(out, node, config);
    render_annotations(out, node, config, detail_budget);
    render_detail_blocks(out, node, path, config, detail_budget);
    render_known_edit_details(out, node, config, detail_budget);
}

fn render_sources(out: &mut String, node: &DiffNode, config: &MarkdownRendererConfig) {
    if config.verbosity != Verbosity::Full || node.sources.is_empty() {
        return;
    }

    out.push_str("  - Sources\n");
    for source in &node.sources {
        out.push_str("    - ");
        out.push_str(&source.path);
        out.push_str(" (");
        out.push_str(source_side_label(source.side));
        if let Some(action) = &source.action {
            out.push_str(", ");
            out.push_str(action);
        }
        if let Some(evidence) = &source.evidence {
            out.push_str(", ");
            out.push_str(evidence);
        }
        out.push_str(")\n");
    }
}

fn source_side_label(side: Side) -> &'static str {
    match side {
        Side::From => "from",
        Side::To => "to",
    }
}

/// Whether a node represents a move or a copy (and so names its origin).
fn is_move_node(node: &DiffNode) -> bool {
    matches!(node.action.as_str(), "move" | "copy")
}

/// The origin line for a moved or copied node: "Moved from X" / "Copied from X".
///
/// When the projection tagged the node `binoc.move.modified`, its `summary`
/// holds the *content* change (see [`move_content`]) and the origin must be
/// derived from the node's sources. Otherwise the producer's own summary is
/// already an origin statement — a bare rename, a copy, or a multi-source
/// "Merged from A, B" that names every source — and we use it verbatim, falling
/// back to the synthesized phrasing only when no summary was supplied.
fn move_origin(node: &DiffNode) -> Summary {
    if node.tags.contains("binoc.move.modified") {
        return synthesized_move_origin(node);
    }
    node_summary(node)
}

/// The synthesized "Moved from X" / "Copied from X" origin, reusing the same
/// wording as [`fallback_summary`]'s move/copy arms.
fn synthesized_move_origin(node: &DiffNode) -> Summary {
    let item_type = if node.item_type.is_empty() {
        "item"
    } else {
        &node.item_type
    };
    let (verb_phrase, fallback) = if node.action == "copy" {
        ("Copied from ", "copied")
    } else {
        ("Moved from ", "moved")
    };
    match node.primary_from_source() {
        Some(src) => Summary::new()
            .text(verb_phrase)
            .path(src.path.clone(), src.side),
        None => format!("{} {fallback}", capitalize(item_type)).into(),
    }
}

/// Whether a move/copy node carries a content change folded inline beneath the
/// origin line (so its content children, if any, are not reported separately).
/// Pure moves, copies, merges, and folder moves report normally.
fn should_group_move_children(node: &DiffNode) -> bool {
    is_move_node(node) && move_content(node).is_some()
}

/// The content change carried by a moved/copied node, shown beneath its origin
/// line, or `None` for a pure move/copy. Folder moves describe their content
/// through separately-rendered child nodes, so they contribute none here.
///
/// Priority (first match wins):
/// 1. `annotations.tabular_summary` — rich, from tabular writers.
/// 2. `annotations.content_summary` — generic content detail.
/// 3. The node's own `summary`, when it was tagged `binoc.move.modified` (its
///    summary is the content change, not the origin).
/// 4. A join of non-identical child summaries.
fn move_content(node: &DiffNode) -> Option<Summary> {
    if node.tags.contains("binoc.folder-move") {
        return None;
    }
    // The annotation trailers are carried as plain strings and render
    // verbatim as a single text segment.
    if let Some(s) = annotation_str(node, "tabular_summary") {
        return Some(s.into());
    }
    if let Some(s) = annotation_str(node, "content_summary") {
        return Some(s.into());
    }
    if node.tags.contains("binoc.move.modified") {
        if let Some(summary) = &node.summary {
            return Some(summary.clone());
        }
    }
    if !node.children.is_empty() {
        let mut trailer = Summary::new();
        let mut any = false;
        for child in node.children.iter().filter(|c| c.action != "identical") {
            if any {
                trailer = trailer.text("; ");
            }
            trailer.extend(node_summary(child));
            any = true;
        }
        if any {
            return Some(trailer);
        }
    }
    None
}

fn annotation_str(node: &DiffNode, key: &str) -> Option<String> {
    node.binoc_annotation(key)
        .and_then(Annotation::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn render_annotations(
    out: &mut String,
    node: &DiffNode,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    if config.verbosity == Verbosity::Summary {
        return;
    }
    for annotation in node.annotations.iter().filter(|annotation| {
        !(annotation.package == "binoc"
            && matches!(
                annotation.key.as_str(),
                "content_summary" | "tabular_summary"
            ))
    }) {
        if !render_annotation(out, annotation, config, detail_budget) {
            return;
        }
    }
}

fn render_annotation(
    out: &mut String,
    annotation: &Annotation,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) -> bool {
    let label = annotation_label(annotation);
    match &annotation.value {
        serde_json::Value::Null => detail_budget.push_line(out, format!("  - {label}: null\n")),
        serde_json::Value::Bool(value) => {
            detail_budget.push_line(out, format!("  - {label}: {value}\n"))
        }
        serde_json::Value::Number(value) => {
            detail_budget.push_line(out, format!("  - {label}: {value}\n"))
        }
        serde_json::Value::String(value) => {
            let (value, truncated) = truncate_text(value, config.max_value_chars);
            detail_budget.push_line(
                out,
                format!(
                    "  - {label}: {}{}\n",
                    value,
                    if truncated { "..." } else { "" }
                ),
            )
        }
        serde_json::Value::Array(values) if values.iter().all(|value| value.as_str().is_some()) => {
            render_string_list_annotation(out, &label, values, config, detail_budget)
        }
        value => detail_budget.push_line(
            out,
            format!(
                "  - {label}: {}\n",
                format_json_annotation_value(value, config)
            ),
        ),
    }
}

fn render_string_list_annotation(
    out: &mut String,
    label: &str,
    values: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) -> bool {
    let shown = if config.verbosity == Verbosity::Full {
        values.len()
    } else {
        values.len().min(config.max_examples_per_block)
    };
    if shown == 0 {
        return true;
    }

    let mut header = label.to_string();
    if shown < values.len() {
        header.push_str(&format!(" (showing {shown} of {})", values.len()));
    }
    if !detail_budget.push_line(out, format!("  - {header}\n")) {
        return false;
    }

    for value in values.iter().take(shown).filter_map(|value| value.as_str()) {
        let (value, truncated) = truncate_text(value, config.max_value_chars);
        if !detail_budget.push_line(
            out,
            format!("    - {}{}\n", value, if truncated { "..." } else { "" }),
        ) {
            return false;
        }
    }
    true
}

fn format_json_annotation_value(
    value: &serde_json::Value,
    config: &MarkdownRendererConfig,
) -> String {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
    let (value, truncated) = truncate_text(&raw, config.max_value_chars);
    if truncated {
        format!("{value}...")
    } else {
        value
    }
}

fn annotation_label(annotation: &Annotation) -> String {
    let mut label = humanize_annotation_key(&annotation.key);
    if annotation.package != "binoc" {
        label = format!("{} {label}", annotation.package);
    }
    label
}

fn humanize_annotation_key(key: &str) -> String {
    let text = key
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    capitalize(&text)
}

/// This node's summary, or a generic fallback when the producer supplied none.
fn node_summary(node: &DiffNode) -> Summary {
    node.summary
        .clone()
        .unwrap_or_else(|| fallback_summary(node))
}

/// Render a structured summary by formatting each segment according to its
/// type. The renderer never parses prose: a `Uint` is digit-grouped, a `Float`
/// gets decimal policy, `Text` is verbatim, and a `Path` is emitted as-is (a
/// richer renderer could hyperlink it, using `Segment::Path`'s snapshot side to
/// resolve against the correct tree). See ADR
/// 2026-06-03-structured-summary-segments.
fn render_summary(summary: &Summary) -> String {
    let mut out = String::new();
    for segment in summary.segments() {
        match segment {
            Segment::Text(text) => out.push_str(text),
            Segment::Path { value, .. } => out.push_str(value),
            // Reuse the prose grouper on the bare decimal form: a `Uint` is
            // always a count, so it is always grouped — no context guessing.
            Segment::Uint(value) => out.push_str(&humanize_numbers(&value.to_string())),
            Segment::Float(value) => out.push_str(&format_float(*value)),
        }
    }
    out
}

/// Format a real-valued quantity: trimmed fixed/scientific notation, with the
/// integer part digit-grouped. (No producer emits `Float` yet; this keeps the
/// segment type renderable for plugins that do.)
fn format_float(value: f64) -> String {
    let raw = if value.abs() < 1_000_000.0 {
        format!("{value:.3}")
    } else {
        format!("{value:.6e}")
    };
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
    match trimmed.split_once('.') {
        Some((int_part, frac)) => format!("{}.{frac}", humanize_numbers(int_part)),
        None => humanize_numbers(trimmed),
    }
}

/// Generic last-resort summary for a node whose producer supplied none.
///
/// Producers own the wording of their concepts; this fallback only covers the
/// built-in actions so a summary-less node still renders something. The
/// move/copy cases emit a [`Segment::Path`] for the source so the path is typed
/// (linkable, never digit-grouped) even on this degraded path.
fn fallback_summary(node: &DiffNode) -> Summary {
    let item_type = if node.item_type.is_empty() {
        "item"
    } else {
        &node.item_type
    };

    match node.action.as_str() {
        "add" => format!("New {item_type}").into(),
        "remove" => format!("{} removed", capitalize(item_type)).into(),
        "modify" => format!("{} modified", capitalize(item_type)).into(),
        "move" => match node.primary_from_source() {
            Some(src) => Summary::new()
                .text("Moved from ")
                .path(src.path.clone(), src.side),
            None => format!("{} moved", capitalize(item_type)).into(),
        },
        "copy" => match node.primary_from_source() {
            Some(src) => Summary::new()
                .text("Copied from ")
                .path(src.path.clone(), src.side),
            None => format!("{} copied", capitalize(item_type)).into(),
        },
        "reorder" => format!("{} reordered", capitalize(item_type)).into(),
        action => format!("{action} ({item_type})").into(),
    }
}

fn render_detail_blocks(
    out: &mut String,
    node: &DiffNode,
    path: &str,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    if config.verbosity == Verbosity::Summary || node.detail_blocks.is_empty() {
        return;
    }

    let block_limit = if config.verbosity == Verbosity::Full {
        node.detail_blocks.len()
    } else {
        node.detail_blocks
            .len()
            .min(config.max_detail_blocks_per_node)
    };
    for block in node.detail_blocks.iter().take(block_limit) {
        let example_limit = if config.verbosity == Verbosity::Full {
            block.examples.len()
        } else {
            block.examples.len().min(config.max_examples_per_block)
        };
        if example_limit == 0 && block.examples.is_empty() && block.extract.is_empty() {
            continue;
        }

        let shown = example_limit as u64;
        let total = block.total_count.unwrap_or(block.examples.len() as u64);
        let omitted_by_renderer = shown < block.examples.len() as u64
            || (config.verbosity != Verbosity::Full
                && node.detail_blocks.len() > config.max_detail_blocks_per_node);
        let truncated = block.truncated || omitted_by_renderer || shown < total;

        let mut header = block.label.clone().unwrap_or_else(|| block.id.clone());
        if truncated {
            if shown == 0 && block.examples.is_empty() && total > 0 {
                header.push_str(&format!(" ({total} total)"));
            } else if total > 0 {
                header.push_str(&format!(" (showing {shown} of {total})"));
            } else {
                header.push_str(&format!(" (showing {shown})"));
            }
        }
        if let Some(extract) = block.extract.first() {
            let label = extract
                .label
                .as_deref()
                .unwrap_or("all matching data")
                .to_lowercase();
            header.push_str(&format!(
                "; use `binoc extract CHANGESET \"{path}\" {}` for {label}",
                extract.aspect
            ));
        }

        if !detail_budget.push_line(out, format!("  - {header}\n")) {
            return;
        }

        for example in block.examples.iter().take(example_limit) {
            let line = format_detail_example(block, example, config);
            if !detail_budget.push_line(out, format!("    - {line}\n")) {
                return;
            }
        }
    }
}

fn render_known_edit_details(
    out: &mut String,
    node: &DiffNode,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    if config.verbosity == Verbosity::Summary || !node.detail_blocks.is_empty() {
        return;
    }
    let Some(edits) = node.details.get("edits").and_then(|value| value.as_array()) else {
        return;
    };
    render_tabular_cell_details(out, edits, config, detail_budget);
    render_tabular_row_details(out, edits, config, detail_budget);
    render_text_line_details(out, edits, config, detail_budget);
    render_binary_strings_details(out, edits, config, detail_budget);
    // Metadata reads AFTER the primary table/content edits above, so a changelog
    // says "what the table did" then "what its metadata did" (CFM-82).
    render_metadata_details(out, edits, config, detail_budget);
    render_generic_edit_details(out, node, edits, config, detail_budget);
}

fn render_generic_edit_details(
    out: &mut String,
    node: &DiffNode,
    edits: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    let generic: Vec<&serde_json::Value> = edits
        .iter()
        .filter(|edit| {
            edit.get("verb")
                .and_then(|value| value.as_str())
                .is_none_or(|verb| !specialized_detail_verb(verb))
                && !summary_covered_generic_verb(node, edit)
        })
        .collect();
    if generic.is_empty() {
        return;
    }
    let total = generic.len();
    let shown = example_count(generic.len(), config);
    for edit in generic.into_iter().take(shown) {
        if !render_generic_edit_detail(out, edit, config, detail_budget) {
            return;
        }
    }
    if shown < total {
        let _ = detail_budget.push_line(
            out,
            format!(
                "  - Additional edits omitted{}\n",
                showing_suffix(shown, total)
            ),
        );
    }
}

fn render_generic_edit_detail(
    out: &mut String,
    edit: &serde_json::Value,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) -> bool {
    let verb = edit
        .get("verb")
        .and_then(|value| value.as_str())
        .unwrap_or("edit");
    let params = edit.get("params").unwrap_or(&serde_json::Value::Null);
    if verb == "tabular.append_rows" {
        return render_append_rows_detail(out, params, config, detail_budget);
    }

    let title = humanize_edit_verb(verb);
    let detail = format_generic_edit_params(params, config);
    let line = if detail.is_empty() {
        title
    } else {
        format!("{title}: {detail}")
    };
    detail_budget.push_line(out, format!("  - {line}\n"))
}

fn render_append_rows_detail(
    out: &mut String,
    params: &serde_json::Value,
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) -> bool {
    let rows = params
        .get("rows")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        return detail_budget.push_line(out, "  - Rows added\n".to_string());
    }
    let shown = example_count(rows.len(), config);
    if !detail_budget.push_line(
        out,
        format!("  - Rows added{}\n", showing_suffix(shown, rows.len())),
    ) {
        return false;
    }
    let start = params.get("start").and_then(|value| value.as_u64());
    for (offset, row) in rows.into_iter().take(shown).enumerate() {
        let locator = start
            .map(|start| format!("row {}", start + offset as u64 + 1))
            .unwrap_or_else(|| "row".into());
        let values = captured_row_values_text(&row, config);
        if !detail_budget.push_line(out, format!("    - {locator}: {values}\n")) {
            return false;
        }
    }
    true
}

fn specialized_detail_verb(verb: &str) -> bool {
    matches!(
        verb,
        "tabular.edit_cell"
            | "tabular.add_row"
            | "tabular.remove_row"
            | "text.replace_lines"
            | "binary.contents-differ"
            | "metadata.value_change"
    )
}

fn summary_covered_generic_verb(node: &DiffNode, edit: &serde_json::Value) -> bool {
    let Some(verb) = edit.get("verb").and_then(|value| value.as_str()) else {
        return false;
    };
    matches!(verb, "tabular.rename_column") && node.tags.contains("binoc.column-rename")
}

fn humanize_edit_verb(verb: &str) -> String {
    let tail = verb.rsplit_once('.').map(|(_, tail)| tail).unwrap_or(verb);
    tail.replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn captured_row_values_text(row: &serde_json::Value, config: &MarkdownRendererConfig) -> String {
    row.get("values")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .take(config.max_examples_per_block.max(1))
                .map(|value| format_scalar_value(value, config))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| "values omitted".into())
}

fn format_generic_edit_params(
    params: &serde_json::Value,
    config: &MarkdownRendererConfig,
) -> String {
    match params {
        serde_json::Value::Object(map) => map
            .iter()
            .take(config.max_examples_per_block.max(1))
            .map(|(key, value)| format!("{key}: {}", compact_json_value(value, config)))
            .collect::<Vec<_>>()
            .join("; "),
        serde_json::Value::Null => String::new(),
        other => compact_json_value(other, config),
    }
}

fn compact_json_value(value: &serde_json::Value, config: &MarkdownRendererConfig) -> String {
    let rendered = match value {
        serde_json::Value::String(text) => format!("'{text}'"),
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".into()),
    };
    let (rendered, truncated) = truncate_text(&rendered, config.max_value_chars);
    format!("{rendered}{}", if truncated { "..." } else { "" })
}

/// Render `metadata.value_change` edits (column/table/file metadata) as human
/// prose: a relabeled column, a changed display format, a dropped value-label
/// set, or a file-level provenance/version/encoding change.
fn render_metadata_details(
    out: &mut String,
    edits: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    let metadata: Vec<&serde_json::Value> = edits
        .iter()
        .filter(|edit| edit.get("verb").and_then(|v| v.as_str()) == Some("metadata.value_change"))
        .collect();
    if metadata.is_empty() {
        return;
    }
    if !detail_budget.push_line(out, "  - Metadata changed\n".to_string()) {
        return;
    }
    for edit in metadata {
        let params = edit.get("params").unwrap_or(&serde_json::Value::Null);
        let scope = params.get("scope").and_then(|v| v.as_str()).unwrap_or("");
        let where_ = match scope {
            "column" => params
                .get("locator")
                .and_then(|l| l.get("column"))
                .and_then(|c| c.as_str())
                .map(|name| format!("column '{name}'"))
                .unwrap_or_else(|| "column".into()),
            "table" => "table".into(),
            "file" => "file".into(),
            other => other.to_string(),
        };
        let changes = params
            .get("changes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for change in changes.iter().take(config.max_examples_per_block.max(1)) {
            let line = format_metadata_change(&where_, change, config);
            if !detail_budget.push_line(out, format!("    - {line}\n")) {
                return;
            }
        }
    }
}

/// One metadata key change as a single prose line.
fn format_metadata_change(
    where_: &str,
    change: &serde_json::Value,
    config: &MarkdownRendererConfig,
) -> String {
    let kind = change
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("changed");
    let key = humanize_metadata_key(
        change
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("value"),
    );
    let from = change.get("from").map(|v| format_metadata_value(v, config));
    let to = change.get("to").map(|v| format_metadata_value(v, config));
    match kind {
        "added" => format!(
            "{where_} {key} set to {}",
            to.unwrap_or_else(|| "(value)".into())
        ),
        "removed" => format!(
            "{where_} {key} removed (was {})",
            from.unwrap_or_else(|| "(value)".into())
        ),
        _ => match (from, to) {
            (Some(from), Some(to)) => format!("{where_} {key} changed from {from} to {to}"),
            (_, Some(to)) => format!("{where_} {key} set to {to}"),
            (Some(from), _) => format!("{where_} {key} removed (was {from})"),
            _ => format!("{where_} {key} changed"),
        },
    }
}

/// Format a metadata value (string bare-quoted, other JSON compacted) with
/// truncation, for prose lines.
fn format_metadata_value(value: &serde_json::Value, config: &MarkdownRendererConfig) -> String {
    match value {
        serde_json::Value::String(text) => {
            let (text, truncated) = truncate_text(text, config.max_value_chars);
            format!("'{text}'{}", if truncated { "..." } else { "" })
        }
        other => {
            let rendered = serde_json::to_string(other).unwrap_or_else(|_| "null".into());
            let (rendered, truncated) = truncate_text(&rendered, config.max_value_chars);
            format!("{rendered}{}", if truncated { "..." } else { "" })
        }
    }
}

/// Turn a metadata bag key into a readable noun phrase.
fn humanize_metadata_key(key: &str) -> String {
    match key {
        "label" => "label".into(),
        "format" => "display format".into(),
        "value_label_set" => "value-label set".into(),
        "value_labels" => "value-label dictionary".into(),
        "dataset_label" => "dataset label".into(),
        "dataset_name" => "dataset name".into(),
        "source_format" => "source format".into(),
        "file_encoding" | "cell_encoding" => "encoding".into(),
        other => other.replace('_', " "),
    }
}

/// Render the additive extracted-strings projection attached to a
/// `binary.contents-differ` edit. This is layered on top of the
/// "binary content changed" fact (the summary already states it); here we list
/// the added/removed printable runs so an otherwise-unreadable file gets a
/// strings-level diff.
fn render_binary_strings_details(
    out: &mut String,
    edits: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    let Some(strings) = edits
        .iter()
        .find(|edit| edit.get("verb").and_then(|v| v.as_str()) == Some("binary.contents-differ"))
        .and_then(|edit| edit.get("params"))
        .and_then(|params| params.get("strings"))
    else {
        return;
    };
    for (key, count_key, label) in [
        ("added", "added_count", "Extracted strings added"),
        ("removed", "removed_count", "Extracted strings removed"),
    ] {
        let examples = strings
            .get(key)
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        if examples.is_empty() {
            continue;
        }
        let total = strings
            .get(count_key)
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(examples.len());
        let shown = example_count(examples.len(), config);
        if !detail_budget.push_line(
            out,
            format!("  - {label}{}\n", showing_suffix(shown, total)),
        ) {
            return;
        }
        for example in examples.into_iter().take(shown) {
            let text = example.as_str().unwrap_or("");
            let (text, truncated) = truncate_text(text, config.max_value_chars);
            let suffix = if truncated { "..." } else { "" };
            if !detail_budget.push_line(out, format!("    - '{text}'{suffix}\n")) {
                return;
            }
        }
    }
}

fn render_tabular_cell_details(
    out: &mut String,
    edits: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    let cells: Vec<&serde_json::Value> = edits
        .iter()
        .filter(|edit| edit.get("verb").and_then(|v| v.as_str()) == Some("tabular.edit_cell"))
        .collect();
    if cells.is_empty() {
        return;
    }
    let shown = example_count(cells.len(), config);
    if !detail_budget.push_line(
        out,
        format!("  - Changed cells{}\n", showing_suffix(shown, cells.len())),
    ) {
        return;
    }
    for edit in cells.into_iter().take(shown) {
        let params = edit.get("params").unwrap_or(&serde_json::Value::Null);
        let example = DetailExample {
            locator: edit_locator(params, &["key", "row", "column"]),
            before: params.get("from").map(value_preview_from_json),
            after: params.get("to").map(value_preview_from_json),
            fields: BTreeMap::new(),
        };
        let line = format_tabular_cell_example(&example, config);
        if !detail_budget.push_line(out, format!("    - {line}\n")) {
            return;
        }
    }
}

fn render_tabular_row_details(
    out: &mut String,
    edits: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    for (verb, label) in [
        ("tabular.add_row", "Rows added"),
        ("tabular.remove_row", "Rows removed"),
    ] {
        let rows: Vec<&serde_json::Value> = edits
            .iter()
            .filter(|edit| edit.get("verb").and_then(|v| v.as_str()) == Some(verb))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let shown = example_count(rows.len(), config);
        if !detail_budget.push_line(
            out,
            format!("  - {label}{}\n", showing_suffix(shown, rows.len())),
        ) {
            return;
        }
        for edit in rows.into_iter().take(shown) {
            let params = edit.get("params").unwrap_or(&serde_json::Value::Null);
            let locator = row_locator_text(params, config);
            let values = params
                .get("values")
                .and_then(|value| value.get("values"))
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .take(config.max_examples_per_block.max(1))
                        .map(|value| format_scalar_value(value, config))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|values| !values.is_empty())
                .unwrap_or_else(|| "values omitted".into());
            let line = if locator.is_empty() {
                values
            } else {
                format!("{locator}: {values}")
            };
            if !detail_budget.push_line(out, format!("    - {line}\n")) {
                return;
            }
        }
    }
}

fn render_text_line_details(
    out: &mut String,
    edits: &[serde_json::Value],
    config: &MarkdownRendererConfig,
    detail_budget: &mut DetailBudget,
) {
    let Some(edit) = edits
        .iter()
        .find(|edit| edit.get("verb").and_then(|v| v.as_str()) == Some("text.replace_lines"))
    else {
        return;
    };
    let examples = edit
        .get("params")
        .and_then(|params| params.get("examples"))
        .and_then(|examples| examples.as_array())
        .cloned()
        .unwrap_or_default();
    if examples.is_empty() {
        return;
    }
    let shown = example_count(examples.len(), config);
    if !detail_budget.push_line(
        out,
        format!(
            "  - Line changes{}\n",
            showing_suffix(shown, examples.len())
        ),
    ) {
        return;
    }
    for example in examples.into_iter().take(shown) {
        let line = example.get("line").and_then(|value| value.as_u64());
        let from = example
            .get("from")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let to = example
            .get("to")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let prefix = line
            .map(|line| format!("line {line}: "))
            .unwrap_or_default();
        let (from, from_truncated) = truncate_text(from, config.max_value_chars);
        let (to, to_truncated) = truncate_text(to, config.max_value_chars);
        let suffix = if from_truncated || to_truncated {
            "..."
        } else {
            ""
        };
        if !detail_budget.push_line(out, format!("    - {prefix}'{from}' -> '{to}'{suffix}\n")) {
            return;
        }
    }
}

fn example_count(total: usize, config: &MarkdownRendererConfig) -> usize {
    if config.verbosity == Verbosity::Full {
        total
    } else {
        total.min(config.max_examples_per_block)
    }
}

fn showing_suffix(shown: usize, total: usize) -> String {
    if shown < total {
        format!(" (showing {shown} of {total})")
    } else {
        String::new()
    }
}

fn edit_locator(params: &serde_json::Value, keys: &[&str]) -> BTreeMap<String, serde_json::Value> {
    keys.iter()
        .filter_map(|key| {
            params
                .get(*key)
                .map(|value| ((*key).to_string(), value.clone()))
        })
        .collect()
}

fn value_preview_from_json(value: &serde_json::Value) -> ValuePreview {
    ValuePreview {
        value: value.clone(),
        media_type: value.as_str().map(|_| "text/plain".into()),
        truncated: false,
    }
}

fn row_locator_text(params: &serde_json::Value, config: &MarkdownRendererConfig) -> String {
    if let Some(key) = params.get("key").and_then(|value| value.as_object()) {
        let parts = key
            .iter()
            .map(|(column, value)| {
                format!(
                    "{} {}",
                    truncate_text(column, config.max_value_chars).0,
                    format_key_value(value, config)
                )
            })
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            return format!("key {}", parts.join(", "));
        }
    }
    params
        .get("index")
        .and_then(|value| value.as_u64())
        .map(|index| format!("row {}", index + 1))
        .unwrap_or_default()
}

fn format_detail_example(
    block: &DetailBlock,
    example: &DetailExample,
    config: &MarkdownRendererConfig,
) -> String {
    if block.kind == "binoc.tabular.cell_changes.v1" {
        return format_tabular_cell_example(example, config);
    }

    let locator = if example.locator.is_empty() {
        None
    } else {
        Some(compact_json_map(&example.locator))
    };
    let before = example
        .before
        .as_ref()
        .map(|v| format_value_preview(v, config));
    let after = example
        .after
        .as_ref()
        .map(|v| format_value_preview(v, config));

    match (locator, before, after) {
        (Some(locator), Some(before), Some(after)) => format!("{locator}: {before} -> {after}"),
        (Some(locator), None, Some(after)) => format!("{locator}: -> {after}"),
        (Some(locator), Some(before), None) => format!("{locator}: {before} ->"),
        (Some(locator), None, None) => locator,
        (None, Some(before), Some(after)) => format!("{before} -> {after}"),
        (None, None, Some(after)) => format!("-> {after}"),
        (None, Some(before), None) => format!("{before} ->"),
        (None, None, None) => "example".into(),
    }
}

fn format_tabular_cell_example(example: &DetailExample, config: &MarkdownRendererConfig) -> String {
    let key = example.locator.get("key").and_then(|value| {
        let map = value.as_object()?;
        if map.is_empty() {
            return None;
        }
        let parts: Vec<String> = map
            .iter()
            .map(|(column, value)| {
                format!(
                    "{} {}",
                    truncate_text(column, config.max_value_chars).0,
                    format_key_value(value, config)
                )
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(format!("key {}", parts.join(", ")))
        }
    });
    let row = example
        .locator
        .get("row")
        .and_then(|value| value.as_u64())
        .map(|row| row + 1)
        .map(|row| format!("row {row}"));
    let column = example
        .locator
        .get("column")
        .and_then(|value| value.as_str())
        .map(|column| {
            format!(
                "column '{}'",
                truncate_text(column, config.max_value_chars).0
            )
        });
    let locator = match (key, row, column) {
        (Some(key), _, Some(column)) => format!("{key}, {column}"),
        (Some(key), _, None) => key,
        (None, Some(row), Some(column)) => format!("{row}, {column}"),
        (None, Some(row), None) => row,
        (None, None, Some(column)) => column,
        (None, None, None) => "cell".into(),
    };
    let before = example
        .before
        .as_ref()
        .map(|v| format_value_preview(v, config))
        .unwrap_or_else(|| "(none)".into());
    let after = example
        .after
        .as_ref()
        .map(|v| format_value_preview(v, config))
        .unwrap_or_else(|| "(none)".into());
    format!("{locator}: {before} -> {after}")
}

fn format_value_preview(value: &ValuePreview, config: &MarkdownRendererConfig) -> String {
    match &value.value {
        serde_json::Value::String(text) => {
            let (truncated_text, render_truncated) = truncate_text(text, config.max_value_chars);
            let mut rendered = format!("'{}'", truncated_text.replace('\'', "\\'"));
            if value.truncated || render_truncated {
                rendered.push_str("...");
            }
            rendered
        }
        other => {
            let raw = serde_json::to_string(other).unwrap_or_else(|_| "null".into());
            let (truncated, render_truncated) = truncate_text(&raw, config.max_value_chars);
            if value.truncated || render_truncated {
                format!("{truncated}...")
            } else {
                truncated
            }
        }
    }
}

/// Render a JSON scalar for inline display. Strings are quoted and truncated
/// (matching the legacy all-string output); other scalars render bare so typed
/// cells (numbers, bools, null, nested) do not vanish.
fn format_scalar_value(value: &serde_json::Value, config: &MarkdownRendererConfig) -> String {
    match value {
        serde_json::Value::String(text) => {
            format!("'{}'", truncate_text(text, config.max_value_chars).0)
        }
        other => {
            let raw = serde_json::to_string(other).unwrap_or_else(|_| "null".into());
            truncate_text(&raw, config.max_value_chars).0
        }
    }
}

/// Render a key column's value (string bare, other scalars via their JSON form)
/// for locator text like `key id '5'` / `key id 5`.
fn format_key_value(value: &serde_json::Value, config: &MarkdownRendererConfig) -> String {
    match value {
        serde_json::Value::String(text) => {
            format!("'{}'", truncate_text(text, config.max_value_chars).0)
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".into()),
    }
}

fn truncate_text(input: &str, max_chars: usize) -> (String, bool) {
    if input.chars().count() <= max_chars {
        return (input.to_string(), false);
    }
    (input.chars().take(max_chars).collect(), true)
}

fn compact_json_map(map: &BTreeMap<String, serde_json::Value>) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".into())
}

struct DetailBudget {
    remaining_bytes: usize,
}

impl DetailBudget {
    fn new(max_bytes: usize) -> Self {
        Self {
            remaining_bytes: max_bytes,
        }
    }

    fn push_line(&mut self, out: &mut String, line: String) -> bool {
        if line.len() > self.remaining_bytes {
            if self.remaining_bytes > 0 {
                out.push_str("  - Additional detail omitted (renderer detail budget reached)\n");
                self.remaining_bytes = 0;
            }
            return false;
        }
        self.remaining_bytes -= line.len();
        out.push_str(&line);
        true
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Insert US-style thousands separators into every run of more than three
/// digits in `input`. Called only on bare numeric strings produced by the typed
/// render path (`render_summary`'s `Uint` arm and `format_float`), so no
/// identifier/filename guarding is needed — `Path`/`Text` segments are rendered
/// verbatim and never reach this function.
fn humanize_numbers(input: &str) -> String {
    // Locale-aware formatting is intentionally out of scope for now; this is US-style grouping.
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + input.len() / 3);

    let mut idx = 0;
    while idx < chars.len() {
        if chars[idx].is_ascii_digit() {
            let start = idx;
            while idx < chars.len() && chars[idx].is_ascii_digit() {
                idx += 1;
            }
            let digits: String = chars[start..idx].iter().collect();
            if digits.len() > 3 {
                out.push_str(&group_thousands(&digits));
            } else {
                out.push_str(&digits);
            }
        } else {
            out.push(chars[idx]);
            idx += 1;
        }
    }
    out
}

/// Insert US-style thousands separators into a run of ASCII digits.
fn group_thousands(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_numbers_groups_runs_over_three_digits() {
        assert_eq!(humanize_numbers("12345"), "12,345");
        assert_eq!(humanize_numbers("1234 56789"), "1,234 56,789");
        assert_eq!(humanize_numbers("123"), "123");
    }

    #[test]
    fn diagnostic_summary_renders_filename_safely_with_grouped_count() {
        // A diagnostic that embeds a year-bearing filename (as a typed `Path`)
        // alongside a genuine standalone count (a `Uint`): the filename's digits
        // must survive verbatim while the count is digit-grouped. This is the
        // case the old `humanize_numbers` identifier-guard existed to handle;
        // typing the channel makes it structural rather than a prose-scan rule.
        let diagnostic = Diagnostic::warning(
            "binoc.possible_split",
            Summary::new()
                .text("'")
                .path("actions_2023.csv", Side::From)
                .text("' shares ")
                .count(12345, "row")
                .text(" with the split"),
        );
        let rendered = render_summary(&diagnostic.message);
        assert_eq!(
            rendered,
            "'actions_2023.csv' shares 12,345 rows with the split"
        );
        // The filename's 2023 is untouched; only the standalone count is grouped.
        assert!(rendered.contains("actions_2023.csv"));
        assert!(!rendered.contains("2,023"));
        assert!(rendered.contains("12,345 rows"));
    }

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
    fn to_markdown_renders_unknown_visible_edit_details() {
        let mut node = DiffNode::new("modify", "tabular", "data.csv").with_summary("1 edit");
        node.details.insert(
            "edits".into(),
            serde_json::json!([
                {
                    "verb": "tabular.append_rows",
                    "params": {
                        "start": 2,
                        "rows": [
                            { "values": ["south", "2025", "9"] },
                            { "values": ["east", "2025", "21"] }
                        ]
                    }
                }
            ]),
        );
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(DiffNode::new("modify", "directory", "").with_children(vec![node])),
        );

        let md = render_markdown(&[changeset], &MarkdownRendererConfig::default());
        assert!(md.contains("  - Rows added\n"), "got:\n{md}");
        assert!(
            md.contains("    - row 3: 'south', '2025', '9'")
                && md.contains("    - row 4: 'east', '2025', '21'"),
            "got:\n{md}"
        );
    }

    #[test]
    fn to_markdown_does_not_duplicate_summary_covered_column_rename() {
        let mut node = DiffNode::new("modify", "tabular", "data.csv")
            .with_summary("Column renamed: 'count' -> 'total'")
            .with_tag("binoc.column-rename");
        node.details.insert(
            "edits".into(),
            serde_json::json!([
                {
                    "verb": "tabular.rename_column",
                    "params": { "from": "count", "to": "total" }
                },
                {
                    "verb": "tabular.append_rows",
                    "params": {
                        "start": 3,
                        "rows": [
                            { "values": ["south", "2025", "9"] },
                            { "values": ["east", "2025", "21"] }
                        ]
                    }
                }
            ]),
        );
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(DiffNode::new("modify", "directory", "").with_children(vec![node])),
        );

        let md = render_markdown(&[changeset], &MarkdownRendererConfig::default());
        assert!(
            md.contains("Column renamed: 'count' -> 'total'"),
            "got:\n{md}"
        );
        assert!(!md.contains("Rename Column"), "got:\n{md}");
        assert!(!md.contains("Other edits"), "got:\n{md}");
        assert!(md.contains("  - Rows added\n"), "got:\n{md}");
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
    fn binary_strings_projection_renders_added_and_removed_runs() {
        // The additive extracted-strings projection on a binary.contents-differ
        // edit should surface added/removed runs in Examples/Full verbosity,
        // layered under the "binary content changed" summary.
        let node = DiffNode::new("modify", "file", "firmware.bin")
            .with_summary(
                "Binary content changed; 1 extracted string added, 1 extracted string removed",
            )
            .with_tag("binoc.content-changed")
            .with_tag("binoc.strings-changed")
            .with_detail(
                "edits",
                serde_json::json!([{
                    "verb": "binary.contents-differ",
                    "params": {
                        "strings": {
                            "added": ["version=2.0.0"],
                            "removed": ["version=1.0.0"],
                            "added_count": 1,
                            "removed_count": 1,
                        }
                    }
                }]),
            );
        let changeset = Changeset::new("v1", "v2", Some(node));
        let config = MarkdownRendererConfig {
            verbosity: Verbosity::Full,
            ..Default::default()
        };
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("Binary content changed"), "got:\n{md}");
        assert!(md.contains("Extracted strings added"), "got:\n{md}");
        assert!(md.contains("'version=2.0.0'"), "got:\n{md}");
        assert!(md.contains("Extracted strings removed"), "got:\n{md}");
        assert!(md.contains("'version=1.0.0'"), "got:\n{md}");
    }

    #[test]
    fn move_with_children_renders_as_paired_bullets() {
        // A container `move` whose content change lives in a child (e.g. a
        // renamed archive holding one modified member) reports as one unit: an
        // origin line plus the joined child detail, indented under one path and
        // classified together by the highest-significance descendant tag.
        // Children must NOT also appear as separate enumerated entries.
        let child = DiffNode::new("modify", "column", "email")
            .with_summary("Column added: 'email'")
            .with_tag("binoc.column-addition");
        let move_node = DiffNode::new("move", "tabular", "data_v2.csv")
            .with_source(Source::new("data.csv", Side::From).with_action("move"))
            .with_summary("Moved from data.csv")
            .with_tag("binoc.move")
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
        assert!(md.contains("- **data_v2.csv**:\n"), "got:\n{md}");
        assert!(md.contains("  - Moved from data.csv\n"), "got:\n{md}");
        assert!(md.contains("  - Column added: 'email'\n"), "got:\n{md}");
        // The child detail should appear exactly once, never as its own
        // separately-categorized entry.
        assert_eq!(md.matches("Column added: 'email'").count(), 1);
    }

    #[test]
    fn move_with_tabular_summary_annotation_renders_as_paired_bullets() {
        // A CSV rename+modify produces a `binoc.move.modified` node whose
        // origin is synthesized from its source and whose content comes from
        // `annotations.tabular_summary` (set by TabularAnalyzer).
        let mut move_node = DiffNode::new("move", "tabular", "data_v2.csv")
            .with_source(Source::new("data.csv", Side::From).with_action("move"))
            .with_tag("binoc.move")
            .with_tag("binoc.move.modified")
            .with_tag("binoc.column-addition")
            .with_tag("binoc.schema-change");
        move_node.annotate_from(
            "binoc",
            "tabular_summary",
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
            md.contains("  - Moved from data.csv\n"),
            "origin line missing; got:\n{md}"
        );
        assert!(
            md.contains("  - Column added: 'email'\n"),
            "tabular_summary must render beneath the origin under the same path; got:\n{md}"
        );
    }

    #[test]
    fn move_with_content_summary_annotation_renders_as_paired_bullets() {
        // A text rename+modify produces a `binoc.move.modified` node with no
        // children, no tabular_summary, but `annotations.content_summary` from
        // the controller's re-dispatch merge.
        let mut move_node = DiffNode::new("move", "text", "meeting-notes-v2.txt")
            .with_source(Source::new("notes.txt", Side::From).with_action("move"))
            .with_tag("binoc.move")
            .with_tag("binoc.move.modified")
            .with_tag("binoc.content-changed")
            .with_tag("binoc.lines-added");
        move_node.annotate_from(
            "binoc",
            "content_summary",
            serde_json::json!("2 lines added"),
        );
        let root = DiffNode::new("modify", "directory", "").with_children(vec![move_node]);

        let md = render_markdown(
            &[Changeset::new("v1", "v2", Some(root))],
            &MarkdownRendererConfig::default(),
        );

        assert!(
            md.contains("  - Moved from notes.txt\n"),
            "origin line missing; got:\n{md}"
        );
        assert!(
            md.contains("  - 2 lines added\n"),
            "content_summary must render beneath the origin under the same path; got:\n{md}"
        );
    }

    #[test]
    fn move_trailer_prefers_tabular_over_content_summary() {
        let mut move_node = DiffNode::new("move", "tabular", "data_v2.csv")
            .with_source(Source::new("data.csv", Side::From).with_action("move"))
            .with_summary("Moved from data.csv")
            .with_tag("binoc.move");
        move_node.annotate_from(
            "binoc",
            "tabular_summary",
            serde_json::json!("Column added: 'email'"),
        );
        move_node.annotate_from(
            "binoc",
            "content_summary",
            serde_json::json!("CSV modified"),
        );
        let root = DiffNode::new("modify", "directory", "").with_children(vec![move_node]);

        let md = render_markdown(
            &[Changeset::new("v1", "v2", Some(root))],
            &MarkdownRendererConfig::default(),
        );

        assert!(md.contains("  - Column added: 'email'\n"), "got:\n{md}");
        assert!(
            !md.contains("CSV modified"),
            "content_summary should be shadowed by tabular_summary"
        );
    }

    #[test]
    fn folder_move_descends_into_children_instead_of_grouping() {
        let node = DiffNode::new("move", "directory", "docs-v2")
            .with_source(Source::new("docs-v1", Side::From).with_action("move"))
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
    fn groups_uint_segments_but_leaves_text_and_paths_verbatim() {
        // Grouping is a property of the `Uint` segment, not a prose scan: a
        // `Uint` is always grouped, while digits inside `Text`/`Path` (a year
        // in a folder name) are never touched.
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(DiffNode::new("modify", "directory", "").with_children(vec![
                DiffNode::new("move", "directory", "FoodData_Central_csv_2026-04-30")
                    .with_source(
                        Source::new("FoodData_Central_csv_2025-12-18", Side::From)
                            .with_action("move"),
                    )
                    .with_summary(
                        Summary::new()
                            .text("Folder moved from ")
                            .path("FoodData_Central_csv_2025-12-18", Side::From),
                    ),
                DiffNode::new("modify", "csv", "data.csv").with_summary(
                    Summary::new()
                        .uint(5975)
                        .text(" rows added; ")
                        .uint(18133333)
                        .text(" cells changed"),
                ),
            ])),
        );
        let md = render_markdown(&[changeset], &MarkdownRendererConfig::default());
        assert!(md.contains("5,975 rows added; 18,133,333 cells changed"));
        assert!(md.contains("Folder moved from FoodData_Central_csv_2025-12-18"));
        assert!(!md.contains("FoodData_Central_csv_2,025-12-18"));
    }

    #[test]
    fn summary_verbosity_hides_detail_blocks() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "tabular", "data.csv")
                    .with_summary("2 cells changed")
                    .with_detail_block(sample_detail_block(2)),
            ),
        );
        let config = MarkdownRendererConfig {
            verbosity: Verbosity::Summary,
            ..Default::default()
        };
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("- **data.csv**: 2 cells changed"));
        assert!(!md.contains("Changed cells"));
        assert!(!md.contains("binoc extract"));
    }

    #[test]
    fn examples_verbosity_renders_capped_tabular_examples() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "tabular", "data.csv")
                    .with_summary("4 cells changed")
                    .with_detail_block(sample_detail_block(4)),
            ),
        );
        let config = MarkdownRendererConfig {
            verbosity: Verbosity::Examples,
            max_examples_per_block: 2,
            ..Default::default()
        };
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("Changed cells (showing 2 of 4); use `binoc extract CHANGESET \"data.csv\" cells_changed` for all changed cells"));
        assert!(md.contains("row 1, column 'score': '10' -> '12'"));
        assert!(md.contains("row 2, column 'score': '20' -> '22'"));
        assert!(!md.contains("row 3, column 'score': '30' -> '32'"));
    }

    #[test]
    fn examples_verbosity_renders_extract_only_detail_blocks() {
        let block = DetailBlock::new("cells_changed", "binoc.tabular.cell_changes.v1")
            .with_label("Changed cells")
            .with_total_count(4)
            .with_extract_hint(ExtractHint::new("cells_changed").with_label("All changed cells"));
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "tabular", "large.csv")
                    .with_summary("4 cells changed")
                    .with_detail_block(block),
            ),
        );

        let md = render_markdown(&[changeset], &MarkdownRendererConfig::default());
        assert!(md.contains("Changed cells (4 total); use `binoc extract CHANGESET \"large.csv\" cells_changed` for all changed cells"));
    }

    #[test]
    fn full_verbosity_renders_all_captured_examples() {
        let changeset = Changeset::new(
            "v1",
            "v2",
            Some(
                DiffNode::new("modify", "tabular", "data.csv")
                    .with_summary("4 cells changed")
                    .with_detail_block(sample_detail_block(4)),
            ),
        );
        let config = MarkdownRendererConfig {
            verbosity: Verbosity::Full,
            max_examples_per_block: 1,
            ..Default::default()
        };
        let md = render_markdown(&[changeset], &config);
        assert!(md.contains("row 4, column 'score': '40' -> '42'"));
        assert!(!md.contains("showing 1 of 4"));
    }

    #[test]
    fn examples_verbosity_renders_string_list_annotations() {
        let mut node =
            DiffNode::new("modify", "tabular", "data.csv").with_summary("3 rows modified");
        node.annotate_from(
            "binoc",
            "distribution_shifts",
            serde_json::json!([
                "column 'score': mean 20 -> 35.5",
                "column 'rank': mean 2 -> 3",
                "column 'cost': mean 10 -> 12",
                "column 'height': mean 70 -> 72"
            ]),
        );

        let md = render_markdown(
            &[Changeset::new(
                "a",
                "b",
                Some(DiffNode::new("modify", "directory", "").with_children(vec![node])),
            )],
            &MarkdownRendererConfig::default(),
        );

        assert!(md.contains("Distribution shifts (showing 3 of 4)"));
        assert!(md.contains("column 'score': mean 20 -> 35.5"));
        assert!(!md.contains("column 'height': mean 70 -> 72"));
    }

    #[test]
    fn summary_verbosity_hides_annotations() {
        let mut node =
            DiffNode::new("modify", "tabular", "data.csv").with_summary("3 rows modified");
        node.annotate_from(
            "binoc",
            "distribution_shifts",
            serde_json::json!(["column 'score' changed"]),
        );

        let md = render_markdown(
            &[Changeset::new(
                "a",
                "b",
                Some(DiffNode::new("modify", "directory", "").with_children(vec![node])),
            )],
            &MarkdownRendererConfig {
                verbosity: Verbosity::Summary,
                ..Default::default()
            },
        );

        assert!(!md.contains("Distribution shifts"));
        assert!(!md.contains("column 'score'"));
    }

    fn sample_detail_block(total_count: u64) -> DetailBlock {
        let mut block = DetailBlock::new("cells_changed", "binoc.tabular.cell_changes.v1")
            .with_label("Changed cells")
            .with_total_count(total_count)
            .with_extract_hint(ExtractHint::new("cells_changed").with_label("All changed cells"));
        for row in 0..total_count {
            let mut example = DetailExample::new();
            example.locator.insert("row".into(), serde_json::json!(row));
            example
                .locator
                .insert("column".into(), serde_json::json!("score"));
            example.before = Some(ValuePreview {
                value: serde_json::json!(format!("{}", (row + 1) * 10)),
                media_type: Some("text/plain".into()),
                truncated: false,
            });
            example.after = Some(ValuePreview {
                value: serde_json::json!(format!("{}", (row + 1) * 10 + 2)),
                media_type: Some("text/plain".into()),
                truncated: false,
            });
            block.examples.push(example);
        }
        block
    }
}
