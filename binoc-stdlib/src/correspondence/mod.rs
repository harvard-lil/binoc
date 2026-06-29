pub mod compact;
pub mod expand;
pub mod pair;
pub mod parse;
pub mod writers;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use binoc_sdk::{
    BinocResult, CoreRule, CorrespondenceDatasetConfigurator, CorrespondenceEngineConfig,
    DataAccess, DatasetSemanticsV1, Diagnostic, DispatchResolver, Edit, FileSelector, ItemRef,
    PathConfigEntry, ProjectionAnnotationContext, ProjectionAnnotator, ProjectionHint, RowIdentity,
    RowIdentityPolicies, Summary,
};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct CorrespondenceOptions {
    /// When true, exact-hash links for renamed identical files/collections are
    /// left unsettled so expansion can recover copy/move-out provenance under
    /// the renamed collection. Set false for the fast short-circuit posture.
    pub expand_renamed_unchanged_collections: bool,
    /// Decompression-bomb size caps for the archive/gzip expand rules. Defaults
    /// to [`expand::ExpandCaps::default`]; per-dataset config overrides
    /// individual caps (see [`engine_config_for_dataset_config`]).
    pub expand_caps: expand::ExpandCaps,
}

impl Default for CorrespondenceOptions {
    fn default() -> Self {
        Self {
            expand_renamed_unchanged_collections: true,
            expand_caps: expand::ExpandCaps::default(),
        }
    }
}

pub fn default_engine_config() -> CorrespondenceEngineConfig {
    engine_config_with_options(CorrespondenceOptions::default())
}

pub fn engine_config_for_dataset_config(dataset: &serde_json::Value) -> CorrespondenceEngineConfig {
    let mut options = CorrespondenceOptions::default();
    if let Ok(semantics) = serde_json::from_value::<DatasetSemanticsV1>(dataset.clone()) {
        let correspondence = &semantics.correspondence;
        if let Some(value) = correspondence.expand_renamed_unchanged_collections {
            options.expand_renamed_unchanged_collections = value;
        }
        if let Some(bytes) = correspondence.max_gzip_bytes {
            options.expand_caps.gzip_max_bytes = bytes;
        }
        if let Some(bytes) = correspondence.max_archive_entry_bytes {
            options.expand_caps.archive_max_entry_bytes = bytes;
        }
        if let Some(bytes) = correspondence.max_archive_total_bytes {
            options.expand_caps.archive_max_total_bytes = bytes;
        }
    }
    engine_config_with_options(options)
}

pub fn engine_config_with_options(options: CorrespondenceOptions) -> CorrespondenceEngineConfig {
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
            CoreRule::Parse(Arc::new(parse::CsvParse)),
            CoreRule::Parse(Arc::new(parse::CsvMediaParse)),
            CoreRule::Parse(Arc::new(parse::YamlParse)),
            CoreRule::Parse(Arc::new(parse::YamlMediaParse)),
            CoreRule::Parse(Arc::new(parse::TomlParse)),
            CoreRule::Parse(Arc::new(parse::IniParse)),
            CoreRule::Pair(Arc::new(pair::RootPair)),
        ],
        writers: vec![
            Arc::new(writers::TabularWriter),
            Arc::new(writers::ParserMetadataWriter),
            Arc::new(writers::StructuredDocumentWriter),
            Arc::new(writers::TextWriter),
            Arc::new(writers::ContainerWriter),
            Arc::new(writers::FallbackWriter),
        ],
        compaction: vec![
            Arc::new(compact::ColumnReorder),
            Arc::new(compact::ColumnRename),
            Arc::new(compact::RowAlignment),
            Arc::new(compact::RowAdditionConsolidation),
        ],
        annotators: vec![Arc::new(StdlibProjectionAnnotator)],
        // The `tabular_v1` identity extractor gives all six tabular producers
        // partition (split/merge) capability for free (CFM-72).
        identity_extractors: vec![Arc::new(binoc_sdk::TabularIdentityExtractor)],
        row_keys: BTreeMap::new(),
        row_identity_policies: BTreeMap::new(),
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

        let left_paths = logical_paths_for_root(left_root, data)?;
        let right_paths = logical_paths_for_root(right_root, data)?;
        let diagnostics = validate_path_entries(&semantics);
        if !semantics.paths.is_empty() {
            config.dispatch_resolver = Some(Arc::new(StdlibPathDispatchResolver {
                entries: semantics.paths.clone(),
            }));
        }
        let row_identity = row_identity_for_paths(&semantics, &left_paths, &right_paths);
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

#[derive(Debug, Clone)]
struct StdlibPathDispatchResolver {
    entries: Vec<PathConfigEntry>,
}

impl DispatchResolver for StdlibPathDispatchResolver {
    fn configure_item(&self, item: &mut ItemRef) -> BinocResult<Vec<Diagnostic>> {
        let Some(entry) = first_path_entry(&self.entries, &item.logical_path) else {
            return Ok(Vec::new());
        };
        if let Some(content_type) = &entry.content_type {
            item.media_type = Some(content_type.clone());
            if !item.is_dir {
                item.projection_hint = projection_for_content_type(content_type);
            }
        } else if matches!(
            entry.rule.as_deref(),
            Some("binoc.parse.csv" | "binoc.parse.csv_media")
        ) && !item.is_dir
        {
            item.projection_hint = ProjectionHint::default().item_type("tabular");
        }
        Ok(Vec::new())
    }

    fn forced_rule_for(&self, item: &ItemRef) -> Option<String> {
        first_path_entry(&self.entries, &item.logical_path).and_then(|entry| entry.rule.clone())
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
        }
        if ctx.unlinked_side.is_some() && !ctx.container {
            hint = hint.tag("binoc.content-changed");
        }
        if let Some(summary) = summarize_known_edits(ctx.edits) {
            hint = hint.summary(summary);
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
        summary = summary.text(format!("; {detail}"));
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

fn summarize_known_edits(edits: &[Edit]) -> Option<String> {
    let mut parts = Vec::new();

    let added_columns = column_names(edits, "tabular.add_column");
    let removed_columns = column_names(edits, "tabular.remove_column");
    let renamed_columns = edits
        .iter()
        .filter(|edit| edit.verb == "tabular.rename_column")
        .collect::<Vec<_>>();
    if added_columns.len() == 1 {
        parts.push(format!("Column added: '{}'", added_columns[0]));
    } else if !added_columns.is_empty() {
        parts.push(count_phrase(
            added_columns.len(),
            "column added",
            "columns added",
        ));
    }
    if removed_columns.len() == 1 {
        parts.push(format!("Column removed: '{}'", removed_columns[0]));
    } else if !removed_columns.is_empty() {
        parts.push(count_phrase(
            removed_columns.len(),
            "column removed",
            "columns removed",
        ));
    }
    if renamed_columns.len() == 1 {
        let params = &renamed_columns[0].params;
        if let (Some(from), Some(to)) = (
            params.get("from").and_then(|value| value.as_str()),
            params.get("to").and_then(|value| value.as_str()),
        ) {
            parts.push(format!("Column renamed: '{from}' -> '{to}'"));
        }
    } else if !renamed_columns.is_empty() {
        parts.push(count_phrase(
            renamed_columns.len(),
            "column renamed",
            "columns renamed",
        ));
    }
    if edits
        .iter()
        .any(|edit| edit.verb == "tabular.reorder_columns")
    {
        parts.push("Columns reordered".into());
    }

    let rows_added = count_verb(edits, "tabular.add_row") + count_appended_rows(edits);
    let rows_removed = count_verb(edits, "tabular.remove_row");
    if rows_added > 0 {
        parts.push(count_phrase(rows_added, "row added", "rows added"));
    }
    if rows_removed > 0 {
        parts.push(count_phrase(rows_removed, "row removed", "rows removed"));
    }

    let cell_edits = edits
        .iter()
        .filter(|edit| edit.verb == "tabular.edit_cell")
        .collect::<Vec<_>>();
    let keyed_rows = unique_keyed_rows(&cell_edits);
    if !keyed_rows.is_empty() {
        parts.push(format!(
            "{} modified by key",
            count_phrase(keyed_rows.len(), "row", "rows")
        ));
    } else if !cell_edits.is_empty() {
        parts.push(count_phrase(
            cell_edits.len(),
            "cell changed",
            "cells changed",
        ));
    }

    parts.extend(text_fact_summaries(edits));
    parts.extend(metadata_summaries(edits));

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
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

fn text_fact_summaries(edits: &[Edit]) -> Vec<String> {
    let mut parts = Vec::new();
    for edit in edits {
        match edit.verb.as_str() {
            "text.line_endings_changed" => parts.push("Line endings changed".into()),
            "text.bom_changed" => parts.push("UTF-8 BOM changed".into()),
            "text.encoding_changed" => parts.push("Encoding changed".into()),
            "text.whitespace_only_changed" => parts.push("Whitespace-only text change".into()),
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
                    (0, 0) => parts.push("Text changed".into()),
                    (added, 0) => {
                        parts.push(count_phrase(added as usize, "line added", "lines added"))
                    }
                    (0, removed) => parts.push(count_phrase(
                        removed as usize,
                        "line removed",
                        "lines removed",
                    )),
                    (added, removed) => parts.push(format!(
                        "{}; {}",
                        count_phrase(added as usize, "line added", "lines added"),
                        count_phrase(removed as usize, "line removed", "lines removed")
                    )),
                }
            }
            _ => {}
        }
    }
    parts
}

/// Summarize `metadata.value_change` edits into prose, grouped by scope so a
/// column-label change, a table-label change, and a file-level provenance change
/// each read distinctly.
fn metadata_summaries(edits: &[Edit]) -> Vec<String> {
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
        parts.push(count_phrase(
            column,
            "column metadata change",
            "column metadata changes",
        ));
    }
    if table > 0 {
        parts.push("Table metadata changed".into());
    }
    if file > 0 {
        parts.push("File metadata changed".into());
    }
    parts
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
) -> BTreeMap<String, RowIdentity> {
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
        if !semantics.paths.is_empty() {
            let entry = first_path_entry(&semantics.paths, &path);
            let has_path_row_identity = entry
                .and_then(|entry| entry.row_identity.as_ref())
                .is_some();
            let has_default_row_identity = !defaults.columns.is_empty();
            if (has_default_row_identity || has_path_row_identity)
                && path_resolves_to_tabular(&path, entry)
            {
                let identity = merge_row_identity(
                    defaults,
                    entry.and_then(|entry| entry.row_identity.as_ref()),
                );
                if !identity.columns.is_empty() {
                    row_identity.insert(path, identity);
                }
            }
            continue;
        }

        if !is_tabular_path(&path) {
            continue;
        }
        let mut identity = defaults.clone();
        for entry in &semantics.tables.entries {
            if table_entry_matches_path(entry, &path) {
                let mut entry_identity = entry.row_identity.clone();
                if entry_identity.columns.is_empty() {
                    entry_identity.columns = identity.columns;
                }
                identity = entry_identity;
                break;
            }
        }
        if !identity.columns.is_empty() {
            row_identity.insert(path, identity);
        }
    }
    row_identity
}

fn dataset_default_row_identity(semantics: &DatasetSemanticsV1) -> &RowIdentity {
    if !semantics.defaults.row_identity.columns.is_empty() {
        &semantics.defaults.row_identity
    } else {
        &semantics.tables.defaults.row_identity
    }
}

fn merge_row_identity(defaults: &RowIdentity, entry: Option<&RowIdentity>) -> RowIdentity {
    let Some(entry) = entry else {
        return defaults.clone();
    };
    let mut identity = entry.clone();
    if identity.columns.is_empty() {
        identity.columns = defaults.columns.clone();
    }
    identity
}

fn path_resolves_to_tabular(path: &str, entry: Option<&PathConfigEntry>) -> bool {
    is_tabular_path(path)
        || entry.is_some_and(|entry| {
            entry
                .content_type
                .as_deref()
                .is_some_and(content_type_is_tabular)
                || entry.rule.as_deref() == Some("binoc.parse.csv")
                || entry.rule.as_deref() == Some("binoc.parse.csv_media")
        })
}

fn first_path_entry<'a>(entries: &'a [PathConfigEntry], path: &str) -> Option<&'a PathConfigEntry> {
    entries
        .iter()
        .find(|entry| glob_matches_path(&entry.match_, path))
}

fn validate_path_entries(semantics: &DatasetSemanticsV1) -> Vec<Diagnostic> {
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
        if entry.content_type.is_none() && entry.rule.is_none() && entry.row_identity.is_none() {
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
        if entry.row_identity.is_some() && !entry_declares_tabular(entry) {
            diagnostics.push(
                Diagnostic::error(
                    "binoc.dataset_config.facet_kind_mismatch",
                    Summary::new()
                        .text("row_identity is only meaningful for tabular paths; add ")
                        .text("content_type: text/csv or rule: binoc.parse.csv for this match"),
                )
                .with_location(location),
            );
        }
    }
    diagnostics
}

fn entry_declares_tabular(entry: &PathConfigEntry) -> bool {
    glob_can_match_tabular_extension(&entry.match_)
        || entry
            .content_type
            .as_deref()
            .is_some_and(content_type_is_tabular)
        || entry.rule.as_deref() == Some("binoc.parse.csv")
        || entry.rule.as_deref() == Some("binoc.parse.csv_media")
}

fn content_type_is_tabular(content_type: &str) -> bool {
    matches!(content_type, "text/csv" | "text/tab-separated-values")
}

fn projection_for_content_type(content_type: &str) -> ProjectionHint {
    match content_type {
        "text/csv" | "text/tab-separated-values" => ProjectionHint::default().item_type("tabular"),
        content_type if content_type.starts_with("text/") => {
            ProjectionHint::default().item_type("text")
        }
        _ => ProjectionHint::default(),
    }
}

fn glob_can_match_tabular_extension(pattern: &str) -> bool {
    pattern.ends_with(".csv") || pattern.ends_with(".tsv")
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
        .filter_map(|entry| entry.ok())
    {
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

fn is_tabular_path(path: &str) -> bool {
    path.ends_with(".csv") || path.ends_with(".tsv")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
