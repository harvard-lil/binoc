pub mod compact;
pub mod expand;
pub mod pair;
pub mod parse;
pub(crate) mod tabular;
pub mod writers;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use binoc_sdk::{
    structured_document_v1, tabular_v1, ArtifactFormat, BinocError, BinocResult, CoreRule,
    CorrespondenceDatasetConfigurator, CorrespondenceEngineConfig, DataAccess, DatasetSemanticsV1,
    Diagnostic, DispatchResolver, Edit, FileSelector, ItemRef, NodeIdentity, PathConfigEntry,
    ProjectionAnnotationContext, ProjectionAnnotator, ProjectionHint, RowIdentity,
    RowIdentityPatch, RowIdentityPolicies, Summary, TableConfig, TabularParseConfig,
};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct CorrespondenceOptions {
    /// When true, exact-hash links for renamed identical files/collections are
    /// left unsettled so expansion can recover copy/move-out provenance under
    /// the renamed collection. Set false for the fast short-circuit posture.
    pub expand_renamed_unchanged_collections: bool,
    /// Byte threshold above which stdlib tabular rules stop materializing full
    /// in-memory `tabular_v1` artifacts and rely on the bounded streaming path.
    pub large_tabular_threshold_bytes: u64,
    /// Decompression-bomb size caps for the archive/gzip expand rules. Defaults
    /// to [`expand::ExpandCaps::default`]; per-dataset config overrides
    /// individual caps (see [`engine_config_for_dataset_config`]).
    pub expand_caps: expand::ExpandCaps,
}

impl Default for CorrespondenceOptions {
    fn default() -> Self {
        Self {
            expand_renamed_unchanged_collections: true,
            large_tabular_threshold_bytes: parse::LARGE_TABULAR_THRESHOLD_BYTES,
            expand_caps: expand::ExpandCaps::default(),
        }
    }
}

pub fn default_engine_config() -> CorrespondenceEngineConfig {
    engine_config_with_options(CorrespondenceOptions::default())
}

pub fn engine_config_for_dataset_config(dataset: &serde_json::Value) -> CorrespondenceEngineConfig {
    let mut options = CorrespondenceOptions::default();
    let mut reduced_precision = compact::ReducedPrecision::default();
    if let Ok(semantics) = serde_json::from_value::<DatasetSemanticsV1>(dataset.clone()) {
        let correspondence = &semantics.correspondence;
        if let Some(value) = correspondence.expand_renamed_unchanged_collections {
            options.expand_renamed_unchanged_collections = value;
        }
        options.large_tabular_threshold_bytes = large_tabular_threshold_bytes(&semantics);
        if let Some(bytes) = correspondence.max_gzip_bytes {
            options.expand_caps.gzip_max_bytes = bytes;
        }
        if let Some(bytes) = correspondence.max_archive_entry_bytes {
            options.expand_caps.archive_max_entry_bytes = bytes;
        }
        if let Some(bytes) = correspondence.max_archive_total_bytes {
            options.expand_caps.archive_max_total_bytes = bytes;
        }
        reduced_precision =
            compact::ReducedPrecision::new(semantics.reduced_precision.suppression_sentinels);
    }
    engine_config_with_options_and_reduced_precision(options, reduced_precision)
}

pub fn engine_config_with_options(options: CorrespondenceOptions) -> CorrespondenceEngineConfig {
    engine_config_with_options_and_reduced_precision(options, compact::ReducedPrecision::default())
}

fn engine_config_with_options_and_reduced_precision(
    options: CorrespondenceOptions,
    reduced_precision: compact::ReducedPrecision,
) -> CorrespondenceEngineConfig {
    CorrespondenceEngineConfig {
        rules: vec![
            CoreRule::Pair(Arc::new(pair::HashPair {
                settle_renames: !options.expand_renamed_unchanged_collections,
            })),
            CoreRule::Pair(Arc::new(pair::CopyPair)),
            CoreRule::Pair(Arc::new(pair::DeclaredPair::default())),
            CoreRule::Pair(Arc::new(pair::NameUnderPairedParent)),
            // Partition (split/merge) runs on the residue the exact rules leave,
            // and *before* the fuzzy tabular/file rules so a clean partition is
            // claimed before `TabularPair` can mis-link a split as a 1:1 move.
            CoreRule::Pair(Arc::new(pair::PartitionPair::default())),
            CoreRule::Pair(Arc::new(pair::TabularPair::default())),
            CoreRule::Pair(Arc::new(pair::FuzzyPair::default())),
            CoreRule::Pair(Arc::new(pair::ContainerFromChildEvidence::default())),
            CoreRule::Expand(Arc::new(expand::ZipExpand {
                caps: options.expand_caps,
            })),
            CoreRule::Expand(Arc::new(expand::TarExpand {
                caps: options.expand_caps,
            })),
            CoreRule::Expand(Arc::new(expand::GzipExpand {
                caps: options.expand_caps,
            })),
            CoreRule::Expand(Arc::new(expand::DirectoryExpand)),
            CoreRule::Parse(Arc::new(parse::JsonRecordsParse)),
            CoreRule::Parse(Arc::new(parse::JsonMediaRecordsParse)),
            CoreRule::Parse(Arc::new(parse::JsonParse)),
            CoreRule::Parse(Arc::new(parse::JsonMediaParse)),
            CoreRule::Parse(Arc::new(parse::CsvParse {
                large_tabular_threshold_bytes: options.large_tabular_threshold_bytes,
            })),
            CoreRule::Parse(Arc::new(parse::CsvMediaParse {
                large_tabular_threshold_bytes: options.large_tabular_threshold_bytes,
            })),
            CoreRule::Parse(Arc::new(parse::YamlParse)),
            CoreRule::Parse(Arc::new(parse::YamlMediaParse)),
            CoreRule::Parse(Arc::new(parse::TomlParse)),
            CoreRule::Parse(Arc::new(parse::IniParse)),
            CoreRule::Pair(Arc::new(pair::RootPair)),
        ],
        writers: vec![
            Arc::new(writers::TabularWriter),
            Arc::new(writers::LargeTabularStreamWriter {
                threshold_bytes: options.large_tabular_threshold_bytes,
            }),
            Arc::new(writers::ParserMetadataWriter),
            Arc::new(writers::StructuredDocumentWriter),
            Arc::new(writers::TextWriter),
            Arc::new(writers::TextMediaWriter),
            Arc::new(writers::ContainerWriter),
            Arc::new(writers::BinaryChunkWriter),
            Arc::new(writers::FallbackWriter),
        ],
        // Pass 2 is deliberately a single ordered sweep. Keep the dependency
        // chain explicit: rename must see set_headers and alignment bases before
        // reorder/alignment consume them; sorted alignment competes for the raw
        // positional basis; row alignment must remove inserted-row noise before
        // reduced-precision grouping; row-addition consolidation runs last.
        compaction: vec![
            Arc::new(compact::ColumnRename),
            Arc::new(compact::ColumnReorder),
            Arc::new(compact::TypeOnlyColumnChange),
            Arc::new(compact::SortedRowAlignment),
            Arc::new(compact::RowAlignment),
            Arc::new(reduced_precision),
            Arc::new(compact::RowAdditionConsolidation),
        ],
        annotators: vec![Arc::new(StdlibProjectionAnnotator)],
        // The `tabular_v1` identity extractor gives all six tabular producers
        // partition (split/merge) capability for free (CFM-72).
        identity_extractors: vec![Arc::new(binoc_sdk::TabularIdentityExtractor)],
        row_keys: BTreeMap::new(),
        row_identity_policies: BTreeMap::new(),
        node_identities: Default::default(),
        root_projection: ProjectionHint::default().item_type("directory"),
        dataset_configurator: Some(Arc::new(StdlibDatasetConfigurator)),
        dispatch_resolver: None,
    }
}

struct StdlibDatasetConfigurator;

impl CorrespondenceDatasetConfigurator for StdlibDatasetConfigurator {
    fn configure(
        &self,
        config: &mut CorrespondenceEngineConfig,
        dataset: &serde_json::Value,
        left_root: &ItemRef,
        right_root: &ItemRef,
        data: &dyn DataAccess,
    ) -> BinocResult<Vec<Diagnostic>> {
        if dataset.is_null() {
            return Ok(Vec::new());
        }
        let semantics = match serde_json::from_value::<DatasetSemanticsV1>(dataset.clone()) {
            Ok(semantics) => semantics,
            Err(err) => {
                return Ok(vec![Diagnostic::warning(
                    "binoc.dataset_config.invalid",
                    Summary::new()
                        .text("Ignored malformed dataset semantics config: ")
                        .text(err.to_string()),
                )]);
            }
        };

        configure_reduced_precision(config, &semantics);

        let parse_capabilities = ParseCapabilities::from_rules(&config.rules);
        let left_paths = logical_paths_for_root(left_root, data)?;
        let right_paths = logical_paths_for_root(right_root, data)?;
        let diagnostics = validate_path_entries(&semantics, &parse_capabilities);
        if !semantics.paths.is_empty()
            || table_config_has_parse_overrides(&semantics.tables)
            || dataset_has_row_identity_config(&semantics)
        {
            config.dispatch_resolver = Some(Arc::new(StdlibPathDispatchResolver {
                entries: semantics.paths.clone(),
                default_row_identity: dataset_default_row_identity(&semantics).clone(),
                tables: semantics.tables.clone(),
                parse_capabilities: parse_capabilities.clone(),
            }));
        }
        let row_identity =
            row_identity_for_paths(&semantics, &left_paths, &right_paths, &parse_capabilities);
        config.row_keys = row_identity
            .iter()
            .map(|(path, identity)| (path.clone(), identity.columns.clone()))
            .collect();
        config.row_identity_policies = row_identity
            .into_iter()
            .map(|(path, identity)| {
                (
                    path,
                    RowIdentityPolicies {
                        on_null_key: identity.on_null_key,
                        on_duplicate_key: identity.on_duplicate_key,
                    },
                )
            })
            .collect();
        config.node_identities =
            node_identity_for_paths(&semantics, &left_paths, &right_paths, &parse_capabilities);
        if !semantics.files.correspondences.is_empty() {
            config.rules.insert(
                0,
                CoreRule::Pair(Arc::new(pair::DeclaredPair {
                    pairs: Vec::new(),
                    rules: semantics.files.correspondences,
                })),
            );
        }
        Ok(diagnostics)
    }
}

#[derive(Debug, Clone, Default)]
struct ParseCapabilities {
    known_rule_names: BTreeSet<String>,
    output_by_rule: BTreeMap<String, ArtifactFormat>,
    extensions_by_output: BTreeMap<ArtifactFormat, BTreeSet<String>>,
    media_types_by_output: BTreeMap<ArtifactFormat, BTreeSet<String>>,
}

impl ParseCapabilities {
    fn from_rules(rules: &[CoreRule]) -> Self {
        let mut capabilities = Self::default();
        for rule in rules {
            let CoreRule::Parse(rule) = rule else {
                if let CoreRule::Expand(rule) = rule {
                    capabilities.known_rule_names.insert(rule.descriptor().name);
                }
                continue;
            };
            let descriptor = rule.descriptor();
            capabilities
                .known_rule_names
                .insert(descriptor.name.clone());
            capabilities
                .output_by_rule
                .insert(descriptor.name, descriptor.output.clone());
            capabilities
                .extensions_by_output
                .entry(descriptor.output.clone())
                .or_default()
                .extend(
                    descriptor
                        .input
                        .extensions
                        .into_iter()
                        .map(|extension| extension.to_ascii_lowercase()),
                );
            capabilities
                .media_types_by_output
                .entry(descriptor.output)
                .or_default()
                .extend(descriptor.input.media_types);
        }
        capabilities
    }

    fn knows_rule(&self, name: &str) -> bool {
        self.known_rule_names.contains(name)
    }

    fn rule_outputs(&self, name: &str, output: &ArtifactFormat) -> bool {
        self.output_by_rule.get(name) == Some(output)
    }

    fn pattern_can_emit(&self, pattern: &str, output: &ArtifactFormat) -> bool {
        let pattern = pattern.to_ascii_lowercase();
        self.extensions_by_output
            .get(output)
            .is_some_and(|extensions| {
                extensions
                    .iter()
                    .any(|extension| pattern.ends_with(extension))
            })
    }

    fn media_type_can_emit(&self, media_type: &str, output: &ArtifactFormat) -> bool {
        self.media_types_by_output
            .get(output)
            .is_some_and(|media_types| media_types.contains(media_type))
    }

    fn entry_can_emit(&self, entry: &PathConfigEntry, output: &ArtifactFormat) -> bool {
        self.pattern_can_emit(&entry.match_, output)
            || entry
                .content_type
                .as_deref()
                .is_some_and(|media_type| self.media_type_can_emit(media_type, output))
            || entry
                .rule
                .as_deref()
                .is_some_and(|rule| self.rule_outputs(rule, output))
    }
}

fn configure_reduced_precision(
    config: &mut CorrespondenceEngineConfig,
    semantics: &DatasetSemanticsV1,
) {
    config
        .compaction
        .retain(|rule| rule.name() != "binoc.compact.reduced_precision");
    let index = config
        .compaction
        .iter()
        .position(|rule| rule.name() == "binoc.compact.row_addition")
        .unwrap_or(config.compaction.len());
    config.compaction.insert(
        index,
        Arc::new(compact::ReducedPrecision::new(
            semantics.reduced_precision.suppression_sentinels.clone(),
        )),
    );
}

#[derive(Debug, Clone)]
struct StdlibPathDispatchResolver {
    entries: Vec<PathConfigEntry>,
    default_row_identity: RowIdentity,
    tables: TableConfig,
    parse_capabilities: ParseCapabilities,
}

impl DispatchResolver for StdlibPathDispatchResolver {
    fn configure_item(&self, item: &mut ItemRef) -> BinocResult<Vec<Diagnostic>> {
        let Some(entry) = first_path_entry(&self.entries, &item.logical_path) else {
            if let Some(parse) = legacy_tabular_parse_for_path(
                &self.tables,
                &item.logical_path,
                &self.parse_capabilities,
            ) {
                item.tabular_parse = Some(parse);
            }
            return Ok(Vec::new());
        };
        if let Some(content_type) = &entry.content_type {
            item.media_type = Some(content_type.clone());
            if !item.is_dir {
                item.projection_hint =
                    projection_for_content_type(content_type, &self.parse_capabilities);
            }
        } else if entry
            .rule
            .as_deref()
            .is_some_and(|rule| self.parse_capabilities.rule_outputs(rule, &tabular_v1()))
            && !item.is_dir
        {
            item.media_type = None;
            item.projection_hint = ProjectionHint::default().item_type("tabular");
        }
        if let Some(parse) = path_tabular_parse_for_path(
            &self.tables,
            entry,
            &item.logical_path,
            &self.parse_capabilities,
        ) {
            item.tabular_parse = Some(parse);
            if !item.is_dir {
                item.projection_hint = ProjectionHint::default().item_type("tabular");
            }
        }
        Ok(Vec::new())
    }

    fn forced_rule_for(&self, item: &ItemRef) -> Option<String> {
        first_path_entry(&self.entries, &item.logical_path).and_then(|entry| entry.rule.clone())
    }

    fn row_identity_for(&self, path: &str) -> Option<RowIdentity> {
        resolve_row_identity_for_path(
            &self.default_row_identity,
            &self.tables,
            first_path_entry(&self.entries, path),
            path,
            &self.parse_capabilities,
        )
    }

    fn node_identity_for(&self, path: &str) -> Option<NodeIdentity> {
        first_path_entry(&self.entries, path)
            .and_then(|entry| entry.node_identity.clone())
            .filter(node_identity_configured)
    }
}

struct StdlibProjectionAnnotator;

impl ProjectionAnnotator for StdlibProjectionAnnotator {
    fn name(&self) -> &str {
        "binoc.annotate.stdlib_projection"
    }

    fn annotate(&self, ctx: &ProjectionAnnotationContext<'_>) -> ProjectionHint {
        let mut hint = ProjectionHint::default();
        // Container reshape (CFM-71): a linked container whose representation
        // changed across the snapshot — e.g. a directory of CSVs serialized into a
        // SQLite database. Core hands us the two endpoints' raw item_type strings
        // and the source path; we (stdlib, the format-aware layer) decide that a
        // differing container kind reads as a reshape rather than a bare move, and
        // own all the wording. The reconciled members already render underneath
        // this node via ordinary projection, so this stays a single coherent
        // container change instead of move + add/remove of the members.
        if let Some(reshape) = container_reshape_hint(ctx) {
            return reshape;
        }
        if ctx.action == "move" && !ctx.edits.is_empty() {
            hint = hint.tag("binoc.move.modified").tag("binoc.content-changed");
            if let Some(content_summary) = summarize_known_edits(ctx.edits) {
                hint = hint.annotate(
                    "binoc",
                    "content_summary",
                    serde_json::Value::String(content_summary.plain_text()),
                );
            }
            if let Some(source_path) = ctx.source_path {
                hint = hint.summary(
                    Summary::new()
                        .text("Moved from ")
                        .path(source_path.to_string(), binoc_sdk::Side::From),
                );
            }
        }
        if ctx.unlinked_side.is_some() && !ctx.container {
            hint = hint.tag("binoc.content-changed");
        }
        if ctx.action != "move" {
            if let Some(summary) = summarize_known_edits(ctx.edits) {
                hint = hint.summary(summary);
            }
        }
        hint
    }
}

/// Recognize a container whose *representation* changed across the snapshot and
/// describe it as a single reshape. Returns `None` for every line that is not
/// such a reshape, so ordinary moves/modifies fall through untouched.
///
/// Decision inputs (all supplied by core, none format-specific in core):
/// - `container`: the line has children on at least one side.
/// - `source_item_type` vs `item_type`: the two endpoints' named kinds. A reshape
///   is exactly the case where a container's kind differs across the link
///   (e.g. "directory" -> "SQLite database"). Equal kinds, or a missing source
///   kind (an unlinked add/remove), are not reshapes.
///
/// The wording and tags live here in stdlib; core stays type-ignorant.
fn container_reshape_hint(ctx: &ProjectionAnnotationContext<'_>) -> Option<ProjectionHint> {
    if !ctx.container {
        return None;
    }
    let from_kind = ctx.source_item_type?;
    let to_kind = ctx.item_type;
    // A reshape needs two *distinct, specific* container kinds. Treat the generic
    // placeholders as "no opinion": if neither endpoint names a real kind, or the
    // kinds match, this is an ordinary move/modify, not a representation change.
    if from_kind == to_kind || is_generic_kind(from_kind) && is_generic_kind(to_kind) {
        return None;
    }
    let source_path = ctx.source_path?;
    let mut summary = binoc_sdk::Summary::new()
        .text("Reshaped from ")
        .path(source_path.to_string(), binoc_sdk::Side::From)
        .text(format!(" ({from_kind} → {to_kind})"));
    if let Some(detail) = summarize_known_edits(ctx.edits) {
        summary = summary.text("; ");
        summary.extend(detail);
    }
    let mut hint = ProjectionHint::default()
        .action("container_representation_change")
        .tag("binoc.container-reshape")
        .tag("binoc.serialization-change")
        // A reshape supersedes the pair-time move framing: the container did not
        // simply move, its representation changed. Retract the now-stale move-family
        // tags so the IR's tag set is coherent, not just the rendering.
        .retract_tag("binoc.move")
        .retract_tag("binoc.move.modified")
        .retract_tag("binoc.folder-move")
        .summary(summary);
    if !ctx.edits.is_empty() {
        hint = hint.tag("binoc.content-changed");
    }
    Some(hint)
}

/// A container kind carrying no real format identity. These come from core's
/// fallbacks when a node set no explicit `item_type`, so a change between two of
/// them is not a meaningful representation change.
fn is_generic_kind(kind: &str) -> bool {
    matches!(kind, "container" | "directory" | "tree" | "item" | "")
}

fn summarize_known_edits(edits: &[Edit]) -> Option<Summary> {
    let mut parts = Vec::new();

    let added_columns = column_names(edits, "tabular.add_column");
    let removed_columns = column_names(edits, "tabular.remove_column");
    let renamed_columns = edits
        .iter()
        .filter(|edit| edit.verb == "tabular.rename_column")
        .collect::<Vec<_>>();
    if added_columns.len() == 1 {
        parts.push(Summary::from(format!(
            "Column added: '{}'",
            added_columns[0]
        )));
    } else if !added_columns.is_empty() {
        parts.push(Summary::from(count_phrase(
            added_columns.len(),
            "column added",
            "columns added",
        )));
    }
    if removed_columns.len() == 1 {
        parts.push(Summary::from(format!(
            "Column removed: '{}'",
            removed_columns[0]
        )));
    } else if !removed_columns.is_empty() {
        parts.push(Summary::from(count_phrase(
            removed_columns.len(),
            "column removed",
            "columns removed",
        )));
    }
    if renamed_columns.len() == 1 {
        let params = &renamed_columns[0].params;
        if let (Some(from), Some(to)) = (
            params.get("from").and_then(|value| value.as_str()),
            params.get("to").and_then(|value| value.as_str()),
        ) {
            parts.push(Summary::from(format!("Column renamed: '{from}' -> '{to}'")));
        }
    } else if !renamed_columns.is_empty() {
        parts.push(Summary::from(count_phrase(
            renamed_columns.len(),
            "column renamed",
            "columns renamed",
        )));
    }
    if edits
        .iter()
        .any(|edit| edit.verb == "tabular.reorder_columns")
    {
        parts.push(Summary::from("Columns reordered"));
    }

    let rows_added = count_verb(edits, "tabular.add_row") + count_appended_rows(edits);
    let rows_removed = count_verb(edits, "tabular.remove_row");
    if rows_added > 0 {
        parts.push(Summary::from(count_phrase(
            rows_added,
            "row added",
            "rows added",
        )));
    }
    if rows_removed > 0 {
        parts.push(Summary::from(count_phrase(
            rows_removed,
            "row removed",
            "rows removed",
        )));
    }

    let cell_edits = edits
        .iter()
        .filter(|edit| edit.verb == "tabular.edit_cell")
        .collect::<Vec<_>>();
    let keyed_rows = unique_keyed_rows(&cell_edits);
    if !keyed_rows.is_empty() {
        parts.push(Summary::from(format!(
            "{} modified by key",
            count_phrase(keyed_rows.len(), "row", "rows")
        )));
    } else if !cell_edits.is_empty() {
        parts.push(Summary::from(count_phrase(
            cell_edits.len(),
            "cell changed",
            "cells changed",
        )));
    }

    parts.extend(text_fact_summaries(edits));
    parts.extend(document_node_summaries(edits));
    parts.extend(metadata_summaries(edits));
    parts.extend(edit_summary_facts(edits, &parts));

    join_summary_parts(parts)
}

fn column_names(edits: &[Edit], verb: &str) -> Vec<String> {
    edits
        .iter()
        .filter(|edit| edit.verb == verb)
        .filter_map(|edit| {
            edit.params
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .collect()
}

fn count_verb(edits: &[Edit], verb: &str) -> usize {
    edits.iter().filter(|edit| edit.verb == verb).count()
}

fn count_appended_rows(edits: &[Edit]) -> usize {
    edits
        .iter()
        .filter(|edit| edit.verb == "tabular.append_rows")
        .filter_map(|edit| edit.params.get("rows").and_then(|value| value.as_array()))
        .map(Vec::len)
        .sum()
}

fn unique_keyed_rows(edits: &[&Edit]) -> BTreeSet<String> {
    edits
        .iter()
        .filter_map(|edit| edit.params.get("key"))
        .map(|key| key.to_string())
        .collect()
}

fn text_fact_summaries(edits: &[Edit]) -> Vec<Summary> {
    let mut parts = Vec::new();
    for edit in edits {
        match edit.verb.as_str() {
            "text.line_endings_changed" => parts.push(Summary::from("Line endings changed")),
            "text.bom_changed" => parts.push(Summary::from("UTF-8 BOM changed")),
            "text.encoding_changed" => parts.push(Summary::from("Encoding changed")),
            "text.whitespace_only_changed" => {
                parts.push(Summary::from("Whitespace-only text change"))
            }
            "text.replace_lines" => {
                let added = edit
                    .params
                    .get("lines_added")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let removed = edit
                    .params
                    .get("lines_removed")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                match (added, removed) {
                    (0, 0) => parts.push(Summary::from("Text changed")),
                    (added, 0) => parts.push(Summary::from(count_phrase(
                        added as usize,
                        "line added",
                        "lines added",
                    ))),
                    (0, removed) => parts.push(Summary::from(count_phrase(
                        removed as usize,
                        "line removed",
                        "lines removed",
                    ))),
                    (added, removed) => parts.push(Summary::from(format!(
                        "{}; {}",
                        count_phrase(added as usize, "line added", "lines added"),
                        count_phrase(removed as usize, "line removed", "lines removed")
                    ))),
                }
            }
            _ => {}
        }
    }
    parts
}

fn document_node_summaries(edits: &[Edit]) -> Vec<Summary> {
    let mut parts = Vec::new();
    let nodes_added = count_verb(edits, "document.add_node");
    let nodes_removed = count_verb(edits, "document.remove_node");
    let nodes_edited = count_verb(edits, "document.edit_node");
    if nodes_added > 0 {
        parts.push(Summary::from(count_phrase(
            nodes_added,
            "keyed node added",
            "keyed nodes added",
        )));
    }
    if nodes_removed > 0 {
        parts.push(Summary::from(count_phrase(
            nodes_removed,
            "keyed node removed",
            "keyed nodes removed",
        )));
    }
    if nodes_edited > 0 {
        parts.push(Summary::from(count_phrase(
            nodes_edited,
            "keyed node edited",
            "keyed nodes edited",
        )));
    }
    parts
}

/// Summarize `metadata.value_change` edits into prose, grouped by scope so a
/// column-label change, a table-label change, and a file-level provenance change
/// each read distinctly.
fn metadata_summaries(edits: &[Edit]) -> Vec<Summary> {
    let mut column = 0usize;
    let mut table = 0usize;
    let mut file = 0usize;
    for edit in edits
        .iter()
        .filter(|edit| edit.verb == "metadata.value_change")
    {
        match edit.params.get("scope").and_then(|v| v.as_str()) {
            Some("column") => column += 1,
            Some("table") => table += 1,
            Some("file") => file += 1,
            _ => {}
        }
    }
    let mut parts = Vec::new();
    if column > 0 {
        parts.push(Summary::from(count_phrase(
            column,
            "column metadata change",
            "column metadata changes",
        )));
    }
    if table > 0 {
        parts.push(Summary::from("Table metadata changed"));
    }
    if file > 0 {
        parts.push(Summary::from("File metadata changed"));
    }
    parts
}

fn edit_summary_facts(edits: &[Edit], existing: &[Summary]) -> Vec<Summary> {
    let mut parts = Vec::new();
    for edit in edits {
        let Some(summary) = edit.projection.hint.summary.as_ref() else {
            continue;
        };
        if summary.is_empty()
            || existing
                .iter()
                .any(|part| part.plain_text() == summary.plain_text())
            || parts
                .iter()
                .any(|part: &Summary| part.plain_text() == summary.plain_text())
        {
            continue;
        }
        parts.push(summary.clone());
    }
    parts
}

fn join_summary_parts(parts: Vec<Summary>) -> Option<Summary> {
    if parts.is_empty() {
        return None;
    }
    let mut summary = Summary::new();
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            summary = summary.text("; ");
        }
        summary.extend(part);
    }
    Some(summary)
}

fn count_phrase(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn row_identity_for_paths(
    semantics: &DatasetSemanticsV1,
    left_paths: &[String],
    right_paths: &[String],
    parse_capabilities: &ParseCapabilities,
) -> BTreeMap<String, RowIdentity> {
    if !dataset_has_row_identity_config(semantics) {
        return BTreeMap::new();
    }

    let mut paths = left_paths
        .iter()
        .chain(right_paths)
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();

    let defaults = dataset_default_row_identity(semantics);
    let mut row_identity = BTreeMap::new();
    for path in paths {
        let path_entry = first_path_entry(&semantics.paths, &path);
        if let Some(identity) = resolve_row_identity_for_path(
            defaults,
            &semantics.tables,
            path_entry,
            &path,
            parse_capabilities,
        ) {
            row_identity.insert(path, identity);
        }
    }
    row_identity
}

fn resolve_row_identity_for_path(
    defaults: &RowIdentity,
    tables: &TableConfig,
    path_entry: Option<&PathConfigEntry>,
    path: &str,
    parse_capabilities: &ParseCapabilities,
) -> Option<RowIdentity> {
    if !path_can_emit_tabular_artifact(path, path_entry, parse_capabilities) {
        return None;
    }

    let identity =
        if let Some(path_identity) = path_entry.and_then(|entry| entry.row_identity.as_ref()) {
            merge_row_identity(defaults, Some(path_identity))
        } else {
            let mut identity = defaults.clone();
            if let Some(entry) = tables
                .entries
                .iter()
                .find(|entry| table_entry_matches_path(entry, path))
            {
                identity = merge_row_identity(&identity, Some(&entry.row_identity));
            }
            canonicalize_row_identity(identity)
        };
    row_identity_configured(&identity).then_some(identity)
}

fn dataset_has_row_identity_config(semantics: &DatasetSemanticsV1) -> bool {
    row_identity_configured(&semantics.defaults.row_identity)
        || row_identity_configured(&semantics.tables.defaults.row_identity)
        || semantics
            .paths
            .iter()
            .filter_map(|entry| entry.row_identity.as_ref())
            .any(row_identity_patch_configured)
        || semantics
            .tables
            .entries
            .iter()
            .any(|entry| row_identity_patch_configured(&entry.row_identity))
}

fn dataset_default_row_identity(semantics: &DatasetSemanticsV1) -> &RowIdentity {
    if row_identity_configured(&semantics.defaults.row_identity) {
        &semantics.defaults.row_identity
    } else {
        &semantics.tables.defaults.row_identity
    }
}

fn large_tabular_threshold_bytes(semantics: &DatasetSemanticsV1) -> u64 {
    semantics
        .correspondence
        .large_tabular_threshold_bytes
        .unwrap_or(parse::LARGE_TABULAR_THRESHOLD_BYTES)
}

fn merge_row_identity(defaults: &RowIdentity, entry: Option<&RowIdentityPatch>) -> RowIdentity {
    let Some(entry) = entry else {
        return canonicalize_row_identity(defaults.clone());
    };
    let mut identity = defaults.clone();
    entry.apply_to(&mut identity);
    canonicalize_row_identity(identity)
}

fn canonicalize_row_identity(mut identity: RowIdentity) -> RowIdentity {
    if identity.columns.is_empty() {
        identity.columns = identity
            .by_position
            .iter()
            .copied()
            .filter(|position| *position > 0)
            .map(positional_column_name)
            .collect();
    }
    identity.by_position.clear();
    identity
}

fn row_identity_configured(identity: &RowIdentity) -> bool {
    !identity.columns.is_empty() || !identity.by_position.is_empty()
}

fn row_identity_patch_configured(identity: &RowIdentityPatch) -> bool {
    identity.has_key_selector()
}

fn node_identity_for_paths(
    semantics: &DatasetSemanticsV1,
    left_paths: &[String],
    right_paths: &[String],
    parse_capabilities: &ParseCapabilities,
) -> BTreeMap<String, NodeIdentity> {
    let mut paths = left_paths
        .iter()
        .chain(right_paths)
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();

    let mut node_identities = BTreeMap::new();
    for path in paths {
        let path_entry = first_path_entry(&semantics.paths, &path);
        let Some(identity) = path_entry
            .and_then(|entry| entry.node_identity.clone())
            .filter(node_identity_configured)
        else {
            continue;
        };
        if path_entry.is_some_and(|entry| {
            parse_capabilities.entry_can_emit(entry, &structured_document_v1())
        }) || parse_capabilities.pattern_can_emit(&path, &structured_document_v1())
        {
            node_identities.insert(path, identity);
        }
    }
    node_identities
}

fn node_identity_configured(identity: &NodeIdentity) -> bool {
    !identity.key_attribute.trim().is_empty()
}

fn positional_column_name(position: usize) -> String {
    format!("column_{position}")
}

fn first_path_entry<'a>(entries: &'a [PathConfigEntry], path: &str) -> Option<&'a PathConfigEntry> {
    entries
        .iter()
        .find(|entry| glob_matches_path(&entry.match_, path))
}

fn validate_path_entries(
    semantics: &DatasetSemanticsV1,
    parse_capabilities: &ParseCapabilities,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for entry in &semantics.paths {
        let location = if entry.match_.is_empty() {
            "dataset.paths[]".to_string()
        } else {
            format!("dataset.paths[match={}]", entry.match_)
        };
        if entry.match_.is_empty() {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.path_match_empty",
                    Summary::new().text("A dataset.paths entry is missing a non-empty match glob"),
                )
                .with_location(location.clone()),
            );
        }
        if entry.content_type.is_none()
            && entry.rule.is_none()
            && entry.dialect.is_none()
            && entry.shape.is_none()
            && entry.records_path.is_none()
            && entry.row_identity.is_none()
            && entry.node_identity.is_none()
        {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.path_entry_empty",
                    Summary::new().text("A dataset.paths entry must set at least one facet"),
                )
                .with_location(location.clone()),
            );
        }
        if entry.content_type.is_some() && entry.rule.is_some() {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.dispatch_ambiguous",
                    Summary::new()
                        .text("A dataset.paths entry cannot set both content_type and rule"),
                )
                .with_location(location.clone()),
            );
        }
        if let Some(rule) = &entry.rule {
            if !parse_capabilities.knows_rule(rule) {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.rule_unknown",
                        Summary::new()
                            .text("Unknown dataset.paths rule: ")
                            .text(rule.clone()),
                    )
                    .with_location(location.clone()),
                );
            }
        }
        for field in &entry.unknown_fields {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.unknown_facet",
                    Summary::new()
                        .text("Unknown dataset.paths facet: ")
                        .text(field.clone()),
                )
                .with_location(location.clone()),
            );
        }
        if entry.row_identity.is_some() && !parse_capabilities.entry_can_emit(entry, &tabular_v1())
        {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.facet_kind_mismatch",
                    Summary::new()
                        .text("row_identity is only meaningful for paths that can parse as ")
                        .text("tabular data; use a tabular-producing match, content_type, or rule"),
                )
                .with_location(location.clone()),
            );
        }
        if let Some(identity) = &entry.node_identity {
            if identity.key_attribute.trim().is_empty() {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.node_identity_key_empty",
                        Summary::new()
                            .text("node_identity.key_attribute must be a non-empty attribute name"),
                    )
                    .with_location(location.clone()),
                );
            }
            if !parse_capabilities.entry_can_emit(entry, &structured_document_v1()) {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.facet_kind_mismatch",
                        Summary::new()
                            .text("node_identity is only meaningful for paths that can parse as ")
                            .text("structured documents; use a structured-document-producing ")
                            .text("match, content_type, or rule"),
                    )
                    .with_location(location.clone()),
                );
            }
        }
        if let Some(records_path) = &entry.records_path {
            if records_path.trim().is_empty() {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.records_path_empty",
                        Summary::new().text("records_path must be a non-empty JSON path"),
                    )
                    .with_location(location.clone()),
                );
            }
            if !parse_capabilities.entry_can_emit(entry, &tabular_v1()) {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.facet_kind_mismatch",
                        Summary::new()
                            .text("records_path is only meaningful for JSON paths that can parse ")
                            .text("as tabular data; use a JSON match, content_type, or rule"),
                    )
                    .with_location(location.clone()),
                );
            }
        }
        if entry.dialect.is_some() && !parse_capabilities.entry_can_emit(entry, &tabular_v1()) {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.facet_kind_mismatch",
                    Summary::new()
                        .text("dialect is only meaningful for tabular paths; add ")
                        .text("content_type: text/csv or rule: binoc.parse.csv for this match"),
                )
                .with_location(location.clone()),
            );
        }
        if entry.shape.is_some() && !parse_capabilities.entry_can_emit(entry, &tabular_v1()) {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.facet_kind_mismatch",
                    Summary::new()
                        .text("shape is only meaningful for tabular paths; add ")
                        .text("content_type: text/csv or rule: binoc.parse.csv for this match"),
                )
                .with_location(location.clone()),
            );
        }
        if let Some(shape) = &entry.shape {
            if shape.header_line.is_some() && shape.skip_lines.is_some() {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.shape_header_position_ambiguous",
                        Summary::new().text("shape cannot set both header_line and skip_lines"),
                    )
                    .with_location(location.clone()),
                );
            }
            if shape.has_header == Some(false) && shape.header_line.is_some() {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.shape_header_ambiguous",
                        Summary::new()
                            .text("shape cannot set header_line when has_header is false"),
                    )
                    .with_location(location.clone()),
                );
            }
            if shape.header_line == Some(0) {
                diagnostics.push(
                    Diagnostic::error(
                        "binoc.dataset_config.shape_header_line_invalid",
                        Summary::new().text("shape.header_line is 1-based and must be at least 1"),
                    )
                    .with_location(location.clone()),
                );
            }
        }
    }
    diagnostics
}

fn path_tabular_parse_for_path(
    tables: &TableConfig,
    entry: &PathConfigEntry,
    path: &str,
    parse_capabilities: &ParseCapabilities,
) -> Option<TabularParseConfig> {
    let mut parse =
        legacy_tabular_parse_for_path(tables, path, parse_capabilities).unwrap_or_default();
    if let Some(shape) = &entry.shape {
        shape.apply_to_parse_config(&mut parse);
    }
    if let Some(dialect) = &entry.dialect {
        merge_csv_dialect(&mut parse, dialect);
    }
    if entry.records_path.is_some() {
        parse.records_path = entry.records_path.clone();
    }
    normalize_tabular_parse(parse)
}

fn legacy_tabular_parse_for_path(
    tables: &TableConfig,
    path: &str,
    parse_capabilities: &ParseCapabilities,
) -> Option<TabularParseConfig> {
    if !parse_capabilities.pattern_can_emit(path, &tabular_v1()) {
        return None;
    }
    let mut parse = tables.defaults.parse.clone();
    for entry in &tables.entries {
        if table_entry_matches_path(entry, path) {
            if !entry.parse.header {
                parse.header = false;
            }
            if entry.parse.delimiter.is_some() {
                parse.delimiter = entry.parse.delimiter.clone();
            }
            if let Some(dialect) = &entry.parse.dialect {
                merge_csv_dialect(&mut parse, dialect);
            }
            if entry.parse.records_path.is_some() {
                parse.records_path = entry.parse.records_path.clone();
            }
            break;
        }
    }
    normalize_tabular_parse(parse)
}

fn merge_csv_dialect(parse: &mut TabularParseConfig, dialect: &binoc_sdk::CsvDialectConfig) {
    let merged = parse.dialect.get_or_insert_with(Default::default);
    if dialect.delimiter.is_some() {
        merged.delimiter = dialect.delimiter.clone();
    }
    if dialect.quote.is_some() {
        merged.quote = dialect.quote.clone();
    }
    if dialect.escape.is_some() {
        merged.escape = dialect.escape.clone();
    }
    if dialect.bom.is_some() {
        merged.bom = dialect.bom;
    }
    if dialect.newline.is_some() {
        merged.newline = dialect.newline.clone();
    }
}

fn normalize_tabular_parse(mut parse: TabularParseConfig) -> Option<TabularParseConfig> {
    if let Some(delimiter) = parse.delimiter.clone() {
        let dialect = parse.dialect.get_or_insert_with(Default::default);
        if dialect.delimiter.is_none() {
            dialect.delimiter = Some(delimiter);
        }
    }
    if parse.header != TabularParseConfig::default().header
        || parse.delimiter.is_some()
        || parse.dialect.is_some()
        || parse.header_line.is_some()
        || parse.skip_lines.is_some()
        || parse.records_path.is_some()
    {
        Some(parse)
    } else {
        None
    }
}

fn table_config_has_parse_overrides(tables: &TableConfig) -> bool {
    normalize_tabular_parse(tables.defaults.parse.clone()).is_some()
        || tables
            .entries
            .iter()
            .any(|entry| normalize_tabular_parse(entry.parse.clone()).is_some())
}

fn projection_for_content_type(
    content_type: &str,
    parse_capabilities: &ParseCapabilities,
) -> ProjectionHint {
    if parse_capabilities.media_type_can_emit(content_type, &tabular_v1()) {
        ProjectionHint::default().item_type("tabular")
    } else if content_type.starts_with("text/") {
        ProjectionHint::default().item_type("text")
    } else {
        ProjectionHint::default()
    }
}

fn path_can_emit_tabular_artifact(
    path: &str,
    path_entry: Option<&PathConfigEntry>,
    parse_capabilities: &ParseCapabilities,
) -> bool {
    path_entry.is_some_and(|entry| parse_capabilities.entry_can_emit(entry, &tabular_v1()))
        || parse_capabilities.pattern_can_emit(path, &tabular_v1())
}

fn logical_paths_for_root(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<Vec<String>> {
    let physical = data.local_path(item)?;
    let mut paths = Vec::new();
    if physical.is_file() {
        if !item.logical_path.is_empty() {
            paths.push(item.logical_path.clone());
        }
        return Ok(paths);
    }
    for entry in walkdir::WalkDir::new(&physical)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
    {
        let entry = walk_path_entry(entry)?;
        let Ok(rel) = entry.path().strip_prefix(&physical) else {
            continue;
        };
        let logical = rel.to_string_lossy().replace('\\', "/");
        if !logical.is_empty() {
            paths.push(logical);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn walk_path_entry(
    entry: Result<walkdir::DirEntry, walkdir::Error>,
) -> BinocResult<walkdir::DirEntry> {
    entry.map_err(|err| BinocError::Other(format!("walk dataset paths: {err}")))
}

fn table_entry_matches_path(entry: &binoc_sdk::TableEntry, path: &str) -> bool {
    entry
        .match_
        .source
        .as_ref()
        .is_some_and(|selector| selector_matches_path(selector, path))
}

fn selector_matches_path(selector: &FileSelector, path: &str) -> bool {
    selector_captures(selector, path).is_some()
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

fn glob_matches_path(pattern: &str, path: &str) -> bool {
    glob_matches_path_exact(pattern, path)
        || path
            .contains("/>")
            .then(|| path.replace("/>", "/"))
            .is_some_and(|normalized| glob_matches_path_exact(pattern, &normalized))
}

fn glob_matches_path_exact(pattern: &str, path: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let mut regex = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    regex.push_str("(?:.*/)?");
                } else {
                    regex.push_str(".*");
                }
            }
            '/' => {
                regex.push('/');
            }
            '*' => regex.push_str("[^/]*"),
            '?' => regex.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            other => regex.push(other),
        }
    }
    regex.push('$');
    Regex::new(&regex)
        .map(|regex| regex.is_match(path))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::{Cardinality, IdentityFailurePolicy};

    fn parse_capabilities() -> ParseCapabilities {
        ParseCapabilities::from_rules(&default_engine_config().rules)
    }

    fn ctx<'a>(
        container: bool,
        source_item_type: Option<&'a str>,
        item_type: &'a str,
        source_path: Option<&'a str>,
    ) -> ProjectionAnnotationContext<'a> {
        ProjectionAnnotationContext {
            action: "move",
            item_type,
            path: "data.sqlite",
            source_path,
            source_item_type,
            evidence: Some("binoc.pair.container_from_children"),
            edits: &[],
            container,
            unlinked_side: None,
        }
    }

    #[test]
    fn reshape_fires_on_differing_container_kinds_and_names_them_verbatim() {
        // Wording is derived ONLY from the two endpoint kind strings core hands
        // us — no format knowledge, no path sniffing. Swap in any kinds and the
        // summary echoes them.
        let hint = container_reshape_hint(&ctx(
            true,
            Some("directory"),
            "SQLite database",
            Some("data"),
        ))
        .expect("differing container kinds should reshape");
        assert_eq!(
            hint.action.as_deref(),
            Some("container_representation_change")
        );
        assert!(hint.tags.contains(&"binoc.container-reshape".to_string()));
        assert!(hint
            .tags
            .contains(&"binoc.serialization-change".to_string()));
        let summary = hint.summary.expect("reshape summary").plain_text();
        assert!(
            summary.contains("Reshaped from")
                && summary.contains("directory")
                && summary.contains("SQLite database"),
            "summary should name both kinds verbatim: {summary:?}"
        );
    }

    #[test]
    fn reshape_does_not_fire_on_same_kind() {
        // A plain directory rename stays an ordinary move, not a reshape.
        assert!(
            container_reshape_hint(&ctx(true, Some("directory"), "directory", Some("old")))
                .is_none()
        );
    }

    #[test]
    fn reshape_does_not_fire_when_both_kinds_are_generic() {
        // "container" vs "directory" are both core fallbacks: no real
        // representation change to report.
        assert!(
            container_reshape_hint(&ctx(true, Some("container"), "directory", Some("old")))
                .is_none()
        );
    }

    #[test]
    fn reshape_requires_a_container() {
        // A leaf with differing kinds (e.g. a parsed file) is not a container
        // reshape; only nodes with members reconcile this way.
        assert!(container_reshape_hint(&ctx(
            false,
            Some("CSV file"),
            "SQLite database",
            Some("x")
        ))
        .is_none());
    }

    #[test]
    fn reshape_requires_a_known_source_kind() {
        // An unlinked add/remove carries no source kind, so it never reshapes.
        assert!(container_reshape_hint(&ctx(true, None, "SQLite database", None)).is_none());
    }

    #[test]
    fn summarize_known_edits_keeps_summary_bearing_edits_visible_with_cell_edits() {
        let edits = vec![
            Edit::new(
                "tabular.edit_cell",
                serde_json::json!({
                    "row": 0,
                    "column": "score",
                    "from": "1",
                    "to": "2"
                }),
            ),
            Edit::new(
                "tabular.column_type_changed",
                serde_json::json!({
                    "column": "score",
                    "from_type": "number",
                    "to_type": "string",
                    "cells": 1
                }),
            )
            .with_summary("Column type changed: 'score' number -> string"),
        ];

        assert_eq!(
            summarize_known_edits(&edits).map(|summary| summary.plain_text()),
            Some("1 cell changed; Column type changed: 'score' number -> string".into())
        );
    }

    #[test]
    fn summarize_known_edits_preserves_structured_edit_summary_segments() {
        let typed_summary = Summary::new()
            .text("Byte range changed at ")
            .uint(1234)
            .text(" (")
            .float(66.66666666666666)
            .text("%)");
        let edits = vec![
            Edit::new(
                "tabular.edit_cell",
                serde_json::json!({
                    "row": 0,
                    "column": "score",
                    "from": "1",
                    "to": "2"
                }),
            ),
            Edit::new("binary.byte_range_changed", serde_json::json!({}))
                .with_summary(typed_summary),
        ];

        let summary = summarize_known_edits(&edits).expect("summary");
        assert_eq!(
            summary.plain_text(),
            "1 cell changed; Byte range changed at 1234 (66.66666666666666%)"
        );
        assert!(summary
            .segments()
            .iter()
            .any(|segment| matches!(segment, binoc_sdk::Segment::Uint(1234))));
        assert!(summary.segments().iter().any(
            |segment| matches!(segment, binoc_sdk::Segment::Float(value) if *value == 66.66666666666666)
        ));
    }

    #[test]
    fn modified_move_keeps_origin_and_content_summaries() {
        let edits = [Edit::new(
            "text.replace_lines",
            serde_json::json!({
                "lines_added": 2,
                "lines_removed": 0,
            }),
        )];
        let context = ProjectionAnnotationContext {
            action: "move",
            item_type: "text",
            path: "notes-v2.txt",
            source_path: Some("notes.txt"),
            source_item_type: Some("text"),
            evidence: Some("binoc.pair.content_similarity"),
            edits: &edits,
            container: false,
            unlinked_side: None,
        };

        let hint = StdlibProjectionAnnotator.annotate(&context);

        assert_eq!(
            hint.summary.map(|summary| summary.plain_text()).as_deref(),
            Some("Moved from notes.txt")
        );
        assert!(hint.annotations.iter().any(|annotation| {
            annotation.package == "binoc"
                && annotation.key == "content_summary"
                && annotation.value == "2 lines added"
        }));
    }

    #[test]
    fn glob_matching_treats_decompose_boundaries_like_path_boundaries() {
        assert!(glob_matches_path(
            "**/num.tsv",
            "2026_03_notes.zip/>num.tsv"
        ));
        assert!(glob_matches_path("**/num.tsv", "expanded/num.tsv"));
    }

    #[test]
    fn logical_path_discovery_propagates_walk_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let walk_error = walkdir::WalkDir::new(temp.path().join("missing"))
            .into_iter()
            .next()
            .expect("walk result")
            .expect_err("missing path must fail");
        let error = walk_path_entry(Err(walk_error)).expect_err("walk must fail");

        assert!(error.to_string().contains("walk dataset paths"));
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn row_identity_probe_skips_trial_parse_when_no_key_is_configured_anywhere() {
        let semantics: DatasetSemanticsV1 = serde_json::from_value(serde_json::json!({
            "paths": [{
                "match": "**/*.json",
                "content_type": "application/json",
                "records_path": "$.records"
            }]
        }))
        .expect("dataset semantics");

        let row_identity = row_identity_for_paths(
            &semantics,
            &[String::from("records.json")],
            &[String::from("records.json")],
            &parse_capabilities(),
        );

        assert!(row_identity.is_empty());
    }

    #[test]
    fn merge_row_identity_keeps_default_policy_when_entry_only_overrides_columns() {
        let defaults = RowIdentity {
            columns: vec!["id".into()],
            by_position: vec![1],
            cardinality: Cardinality::default(),
            on_null_key: IdentityFailurePolicy::Error,
            on_duplicate_key: IdentityFailurePolicy::Ignore,
        };
        let entry = RowIdentityPatch {
            columns: Some(vec!["email".into()]),
            ..RowIdentityPatch::default()
        };

        let merged = merge_row_identity(&defaults, Some(&entry));

        assert_eq!(merged.columns, vec!["email"]);
        assert!(merged.by_position.is_empty());
        assert_eq!(merged.cardinality, Cardinality::default());
        assert_eq!(merged.on_null_key, IdentityFailurePolicy::Error);
        assert_eq!(merged.on_duplicate_key, IdentityFailurePolicy::Ignore);
    }

    #[test]
    fn merge_row_identity_accepts_explicit_default_policy_override() {
        let defaults = RowIdentity {
            columns: vec!["id".into()],
            on_null_key: IdentityFailurePolicy::Error,
            on_duplicate_key: IdentityFailurePolicy::Ignore,
            ..RowIdentity::default()
        };
        let entry = RowIdentityPatch {
            on_null_key: Some(IdentityFailurePolicy::Diagnostic),
            on_duplicate_key: Some(IdentityFailurePolicy::Diagnostic),
            ..RowIdentityPatch::default()
        };

        let merged = merge_row_identity(&defaults, Some(&entry));

        assert_eq!(merged.on_null_key, IdentityFailurePolicy::Diagnostic);
        assert_eq!(merged.on_duplicate_key, IdentityFailurePolicy::Diagnostic);
    }

    #[test]
    fn merge_row_identity_replaces_inherited_selector_in_both_directions() {
        let named_defaults = RowIdentity {
            columns: vec!["id".into()],
            ..RowIdentity::default()
        };
        let by_position = RowIdentityPatch {
            by_position: Some(vec![2]),
            ..RowIdentityPatch::default()
        };
        let positional = merge_row_identity(&named_defaults, Some(&by_position));
        assert_eq!(positional.columns, vec!["column_2"]);
        assert!(positional.by_position.is_empty());

        let positional_defaults = RowIdentity {
            by_position: vec![1],
            ..RowIdentity::default()
        };
        let by_name = RowIdentityPatch {
            columns: Some(vec!["email".into()]),
            ..RowIdentityPatch::default()
        };
        let named = merge_row_identity(&positional_defaults, Some(&by_name));
        assert_eq!(named.columns, vec!["email"]);
        assert!(named.by_position.is_empty());
    }

    #[test]
    fn table_row_identity_entry_columns_inherit_default_policy() {
        let semantics: DatasetSemanticsV1 = serde_json::from_value(serde_json::json!({
            "tables": {
                "defaults": {
                    "row_identity": {
                        "on_null_key": "error",
                        "on_duplicate_key": "ignore"
                    }
                },
                "entries": [{
                    "path_regex": "^data\\.csv$",
                    "columns": ["email"]
                }]
            }
        }))
        .expect("dataset semantics");

        let identities = row_identity_for_paths(
            &semantics,
            &[String::from("data.csv")],
            &[String::from("data.csv")],
            &parse_capabilities(),
        );

        let identity = identities.get("data.csv").expect("data.csv identity");
        assert_eq!(identity.columns, vec!["email"]);
        assert_eq!(identity.on_null_key, IdentityFailurePolicy::Error);
        assert_eq!(identity.on_duplicate_key, IdentityFailurePolicy::Ignore);
    }

    #[test]
    fn flat_path_policy_override_keeps_inherited_key_columns() {
        let semantics: DatasetSemanticsV1 = serde_json::from_value(serde_json::json!({
            "defaults": {
                "row_identity": {
                    "columns": ["id"],
                    "on_null_key": "error"
                }
            },
            "paths": [{
                "match": "data.csv",
                "on_null_key": "diagnostic"
            }]
        }))
        .expect("dataset semantics");

        let identities = row_identity_for_paths(
            &semantics,
            &[String::from("data.csv")],
            &[String::from("data.csv")],
            &parse_capabilities(),
        );

        let identity = identities.get("data.csv").expect("data.csv identity");
        assert_eq!(identity.columns, vec!["id"]);
        assert_eq!(identity.on_null_key, IdentityFailurePolicy::Diagnostic);
    }
}
