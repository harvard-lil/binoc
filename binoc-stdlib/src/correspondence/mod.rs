pub mod compact;
pub mod expand;
pub mod pair;
pub mod parse;
mod tabular;
pub mod writers;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use binoc_sdk::{
    tabular_v1, BinocResult, CoreRule, CorrespondenceDatasetConfigurator,
    CorrespondenceEngineConfig, DataAccess, DatasetSemanticsV1, Diagnostic, DispatchResolver, Edit,
    FileSelector, ItemRef, ParseRule, PathConfigEntry, ProjectionAnnotationContext,
    ProjectionAnnotator, ProjectionHint, RowIdentity, RowIdentityPolicies, Summary, TableConfig,
    TabularParseConfig,
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
            Arc::new(writers::LargeTabularStreamWriter),
            Arc::new(writers::ParserMetadataWriter),
            Arc::new(writers::StructuredDocumentWriter),
            Arc::new(writers::TextWriter),
            Arc::new(writers::TextMediaWriter),
            Arc::new(writers::ContainerWriter),
            Arc::new(writers::BinaryChunkWriter),
            Arc::new(writers::FallbackWriter),
        ],
        compaction: vec![
            Arc::new(compact::ColumnReorder),
            Arc::new(compact::ColumnRename),
            Arc::new(compact::TypeOnlyColumnChange),
            Arc::new(compact::RowAlignment),
            Arc::new(compact::ReducedPrecision),
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
        if !semantics.paths.is_empty() || table_config_has_parse_overrides(&semantics.tables) {
            config.dispatch_resolver = Some(Arc::new(StdlibPathDispatchResolver {
                entries: semantics.paths.clone(),
                default_row_identity: dataset_default_row_identity(&semantics).clone(),
                tables: semantics.tables.clone(),
            }));
        }
        let row_identity = row_identity_for_paths(
            &semantics,
            &left_paths,
            &right_paths,
            left_root,
            right_root,
            data,
        )?;
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
    default_row_identity: RowIdentity,
    tables: TableConfig,
}

impl DispatchResolver for StdlibPathDispatchResolver {
    fn configure_item(&self, item: &mut ItemRef) -> BinocResult<Vec<Diagnostic>> {
        let Some(entry) = first_path_entry(&self.entries, &item.logical_path) else {
            if let Some(parse) = legacy_tabular_parse_for_path(&self.tables, &item.logical_path) {
                item.tabular_parse = Some(parse);
            }
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
            item.media_type = None;
            item.projection_hint = ProjectionHint::default().item_type("tabular");
        }
        if let Some(parse) = path_tabular_parse_for_path(&self.tables, entry, &item.logical_path) {
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
        let entry = first_path_entry(&self.entries, path);
        let has_default_row_identity = !self.default_row_identity.columns.is_empty();
        let has_path_row_identity = entry
            .and_then(|entry| entry.row_identity.as_ref())
            .is_some();
        let path_can_be_tabular =
            glob_can_match_tabular_artifact(path) || entry.is_some_and(entry_declares_tabular);
        if !(has_default_row_identity || has_path_row_identity) || !path_can_be_tabular {
            return None;
        }
        let identity = merge_row_identity(
            &self.default_row_identity,
            entry.and_then(|entry| entry.row_identity.as_ref()),
        );
        (!identity.columns.is_empty()).then_some(identity)
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
    left_root: &ItemRef,
    right_root: &ItemRef,
    data: &dyn DataAccess,
) -> BinocResult<BTreeMap<String, RowIdentity>> {
    let left_root_physical = data.local_path(left_root)?;
    let right_root_physical = data.local_path(right_root)?;
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
        let path_is_tabular = path_emits_tabular_artifact(
            &path,
            left_root,
            &left_root_physical,
            right_root,
            &right_root_physical,
            path_entry,
            data,
        )?;
        if !semantics.paths.is_empty() {
            let has_path_row_identity = path_entry
                .and_then(|entry| entry.row_identity.as_ref())
                .is_some_and(row_identity_configured);
            let has_default_row_identity = row_identity_configured(defaults);
            if (has_default_row_identity || has_path_row_identity) && path_is_tabular {
                let identity = merge_row_identity(
                    defaults,
                    path_entry.and_then(|entry| entry.row_identity.as_ref()),
                );
                if row_identity_configured(&identity) {
                    row_identity.insert(path, identity);
                }
            }
            continue;
        }

        if !path_is_tabular {
            continue;
        }
        let mut identity = defaults.clone();
        for entry in &semantics.tables.entries {
            if table_entry_matches_path(entry, &path) {
                let mut entry_identity = entry.row_identity.clone();
                if !row_identity_configured(&entry_identity) {
                    entry_identity.columns = identity.columns.clone();
                    entry_identity.by_position = identity.by_position.clone();
                }
                identity = entry_identity;
                break;
            }
        }
        identity = canonicalize_row_identity(identity);
        if row_identity_configured(&identity) {
            row_identity.insert(path, identity);
        }
    }
    Ok(row_identity)
}

fn dataset_default_row_identity(semantics: &DatasetSemanticsV1) -> &RowIdentity {
    if row_identity_configured(&semantics.defaults.row_identity) {
        &semantics.defaults.row_identity
    } else {
        &semantics.tables.defaults.row_identity
    }
}

fn merge_row_identity(defaults: &RowIdentity, entry: Option<&RowIdentity>) -> RowIdentity {
    let Some(entry) = entry else {
        return canonicalize_row_identity(defaults.clone());
    };
    let mut identity = entry.clone();
    if !row_identity_configured(&identity) {
        identity.columns = defaults.columns.clone();
        identity.by_position = defaults.by_position.clone();
    }
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

fn positional_column_name(position: usize) -> String {
    format!("column_{position}")
}

fn parse_config_for_entry(entry: &PathConfigEntry) -> Option<TabularParseConfig> {
    if entry.shape.is_none() && entry.dialect.is_none() && entry.records_path.is_none() {
        return None;
    }
    let mut parse = TabularParseConfig::default();
    if let Some(shape) = &entry.shape {
        shape.apply_to_parse_config(&mut parse);
    }
    if let Some(dialect) = &entry.dialect {
        merge_csv_dialect(&mut parse, dialect);
    }
    parse.records_path = entry.records_path.clone();
    Some(parse)
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
        if entry.content_type.is_none()
            && entry.rule.is_none()
            && entry.dialect.is_none()
            && entry.shape.is_none()
            && entry.records_path.is_none()
            && entry.row_identity.is_none()
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
                        .text("row_identity is only meaningful for paths that can parse as ")
                        .text("tabular data; use a tabular-producing match, content_type, or rule"),
                )
                .with_location(location.clone()),
            );
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
            if !entry_declares_tabular(entry) {
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
        if entry.dialect.is_some() && !entry_declares_tabular(entry) {
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
        if entry.shape.is_some() && !entry_declares_tabular(entry) {
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
) -> Option<TabularParseConfig> {
    let mut parse = legacy_tabular_parse_for_path(tables, path).unwrap_or_default();
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

fn legacy_tabular_parse_for_path(tables: &TableConfig, path: &str) -> Option<TabularParseConfig> {
    if !is_tabular_path(path) {
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

fn entry_declares_tabular(entry: &PathConfigEntry) -> bool {
    glob_can_match_tabular_artifact(&entry.match_)
        || entry
            .content_type
            .as_deref()
            .is_some_and(content_type_can_emit_tabular_artifact)
        || entry
            .rule
            .as_deref()
            .is_some_and(rule_can_emit_tabular_artifact)
}

fn content_type_can_emit_tabular_artifact(content_type: &str) -> bool {
    matches!(
        content_type,
        "text/csv"
            | "text/tab-separated-values"
            | "application/json"
            | "application/ld+json"
            | "application/geo+json"
    )
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

fn glob_can_match_tabular_artifact(pattern: &str) -> bool {
    [
        ".csv", ".tsv", ".json", ".jsonl", ".ndjson", ".geojson", ".jsonld", ".json-ld",
    ]
    .iter()
    .any(|extension| pattern.ends_with(extension))
}

fn is_tabular_path(path: &str) -> bool {
    glob_can_match_tabular_artifact(path)
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

fn path_emits_tabular_artifact(
    path: &str,
    left_root: &ItemRef,
    left_root_physical: &Path,
    right_root: &ItemRef,
    right_root_physical: &Path,
    path_entry: Option<&PathConfigEntry>,
    data: &dyn DataAccess,
) -> BinocResult<bool> {
    for (root, physical) in [
        (left_root, left_root_physical),
        (right_root, right_root_physical),
    ] {
        let Some(item) = item_for_logical_path(root, physical, path, path_entry, data)? else {
            continue;
        };
        if item_emits_tabular_artifact(&item, path_entry, data) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn item_for_logical_path(
    root: &ItemRef,
    root_physical: &Path,
    logical_path: &str,
    path_entry: Option<&PathConfigEntry>,
    data: &dyn DataAccess,
) -> BinocResult<Option<ItemRef>> {
    if root_physical.is_file() {
        let Some(mut item) = (root.logical_path == logical_path).then_some(root.clone()) else {
            return Ok(None);
        };
        apply_path_dispatch_overrides(&mut item, path_entry);
        return Ok(Some(item));
    }
    let physical = root_physical.join(logical_path);
    if !physical.exists() {
        return Ok(None);
    }
    let mut item = data.register_local(&physical, logical_path)?;
    apply_path_dispatch_overrides(&mut item, path_entry);
    Ok(Some(item))
}

fn apply_path_dispatch_overrides(item: &mut ItemRef, path_entry: Option<&PathConfigEntry>) {
    let Some(path_entry) = path_entry else {
        return;
    };
    if let Some(content_type) = &path_entry.content_type {
        item.media_type = Some(content_type.clone());
    }
    if let Some(parse) = parse_config_for_entry(path_entry) {
        item.tabular_parse = Some(parse);
    }
}

fn item_emits_tabular_artifact(
    item: &ItemRef,
    path_entry: Option<&PathConfigEntry>,
    data: &dyn DataAccess,
) -> bool {
    let csv = parse::CsvParse;
    let csv_media = parse::CsvMediaParse;
    let json_records = parse::JsonRecordsParse;
    let json_media_records = parse::JsonMediaRecordsParse;
    if let Some(forced) = path_entry
        .and_then(|entry| entry.rule.as_deref())
        .and_then(forced_tabular_parse_rule)
    {
        if let Ok(output) = forced.parse(item, data) {
            return !output.bytes.is_empty();
        }
    }
    let rules = [
        &csv as &dyn ParseRule,
        &csv_media as &dyn ParseRule,
        &json_records as &dyn ParseRule,
        &json_media_records as &dyn ParseRule,
    ];
    for rule in rules {
        let descriptor = rule.descriptor();
        if descriptor.output != tabular_v1() || !descriptor.input.matches(item) {
            continue;
        }
        let Ok(output) = rule.parse(item, data) else {
            continue;
        };
        if !output.bytes.is_empty() {
            return true;
        }
    }
    false
}

fn forced_tabular_parse_rule(rule_name: &str) -> Option<&'static dyn ParseRule> {
    match rule_name {
        "binoc.parse.csv" => Some(&parse::CsvParse),
        "binoc.parse.csv_media" => Some(&parse::CsvMediaParse),
        "binoc.parse.json_records" => Some(&parse::JsonRecordsParse),
        "binoc.parse.json_media_records" => Some(&parse::JsonMediaRecordsParse),
        _ => None,
    }
}

fn rule_can_emit_tabular_artifact(rule_name: &str) -> bool {
    forced_tabular_parse_rule(rule_name).is_some()
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

    #[test]
    fn glob_matching_treats_decompose_boundaries_like_path_boundaries() {
        assert!(glob_matches_path(
            "**/num.tsv",
            "2026_03_notes.zip/>num.tsv"
        ));
        assert!(glob_matches_path("**/num.tsv", "expanded/num.tsv"));
    }
}
