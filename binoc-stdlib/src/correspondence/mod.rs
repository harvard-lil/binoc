pub mod compact;
pub mod expand;
pub mod pair;
pub mod parse;
pub mod writers;

use std::collections::BTreeMap;
use std::sync::Arc;

use binoc_sdk::{
    BinocResult, CoreRule, CorrespondenceDatasetConfigurator, CorrespondenceEngineConfig,
    DataAccess, DatasetSemanticsV1, Diagnostic, FileSelector, ItemRef, ProjectionAnnotationContext,
    ProjectionAnnotator, ProjectionHint, RowIdentity, RowIdentityPolicies,
};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct CorrespondenceOptions {
    /// When true, exact-hash links for renamed identical files/collections are
    /// left unsettled so expansion can recover copy/move-out provenance under
    /// the renamed collection. Set false for the fast short-circuit posture.
    pub expand_renamed_unchanged_collections: bool,
}

impl Default for CorrespondenceOptions {
    fn default() -> Self {
        Self {
            expand_renamed_unchanged_collections: true,
        }
    }
}

pub fn default_engine_config() -> CorrespondenceEngineConfig {
    engine_config_with_options(CorrespondenceOptions::default())
}

pub fn engine_config_for_dataset_config(dataset: &serde_json::Value) -> CorrespondenceEngineConfig {
    let mut options = CorrespondenceOptions::default();
    if let Ok(semantics) = serde_json::from_value::<DatasetSemanticsV1>(dataset.clone()) {
        if let Some(value) = semantics
            .correspondence
            .expand_renamed_unchanged_collections
        {
            options.expand_renamed_unchanged_collections = value;
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
            CoreRule::Pair(Arc::new(pair::FuzzyPair::default())),
            CoreRule::Pair(Arc::new(pair::ContainerFromChildEvidence::default())),
            CoreRule::Expand(Arc::new(expand::ZipExpand)),
            CoreRule::Expand(Arc::new(expand::TarExpand)),
            CoreRule::Expand(Arc::new(expand::GzipExpand)),
            CoreRule::Expand(Arc::new(expand::DirectoryExpand)),
            CoreRule::Parse(Arc::new(parse::CsvParse)),
            CoreRule::Pair(Arc::new(pair::RootPair)),
        ],
        writers: vec![
            Arc::new(writers::TabularWriter),
            Arc::new(writers::TabularCollectionWriter),
            Arc::new(writers::TextWriter),
            Arc::new(writers::ContainerWriter),
            Arc::new(writers::FallbackWriter),
        ],
        compaction: vec![
            Arc::new(compact::ColumnReorder),
            Arc::new(compact::RowAlignment),
            Arc::new(compact::RowAdditionConsolidation),
        ],
        annotators: vec![Arc::new(StdlibProjectionAnnotator)],
        row_keys: BTreeMap::new(),
        row_identity_policies: BTreeMap::new(),
        root_projection: ProjectionHint::default().item_type("directory"),
        dataset_configurator: Some(Arc::new(StdlibDatasetConfigurator)),
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
                    format!("Ignored malformed dataset semantics config: {err}"),
                )]);
            }
        };

        let left_paths = logical_paths_for_root(left_root, data)?;
        let right_paths = logical_paths_for_root(right_root, data)?;
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
        Ok(Vec::new())
    }
}

struct StdlibProjectionAnnotator;

impl ProjectionAnnotator for StdlibProjectionAnnotator {
    fn name(&self) -> &str {
        "binoc.annotate.stdlib_projection"
    }

    fn annotate(&self, ctx: &ProjectionAnnotationContext<'_>) -> ProjectionHint {
        let mut hint = ProjectionHint::default();
        if ctx.action == "move" && !ctx.edits.is_empty() {
            hint = hint.tag("binoc.move.modified").tag("binoc.content-changed");
        }
        if ctx.unlinked_side.is_some() && !ctx.container {
            hint = hint.tag("binoc.content-changed");
        }
        hint
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
        .filter(|path| is_tabular_path(path))
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();

    let defaults = &semantics.tables.defaults.row_identity;
    let mut row_identity = BTreeMap::new();
    for path in paths {
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

fn is_tabular_path(path: &str) -> bool {
    path.ends_with(".csv") || path.ends_with(".tsv")
}
