//! Shared harness for running test vectors. Used by binoc-stdlib’s own vectors
//! and by plugins (e.g. binoc-sqlite) so they don’t duplicate manifest parsing,
//! copy/build/snapshot logic, or assertions. Vectors live in a `test-vectors/`
//! directory; a root `manifest.toml` there provides default `[config]`/`[expected]`;
//! each vector's manifest overrides.
//!
//! Test vectors commit *source* trees like `archive.zip.d/` instead of opaque
//! binary archives. A [`VectorMaterializer`] turns each staging directory into
//! the real artifact (`.zip`, `.tar.gz`, `.sqlite`, ...). The stdlib ships
//! [`ZipMaterializer`], [`TarMaterializer`], and [`GzipMaterializer`]
//! ([`stdlib_materializers`]); plugins contribute their own (see
//! `binoc-sqlite` for SQLite). Both [`run_vector`] and
//! [`materialize_snapshots`] go through the same walker so tests and
//! `just materialize` build identical trees.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use binoc_core::config::{DatasetConfig, OutputConfig};
use binoc_core::controller::Controller;
use binoc_core::correspondence::{DescriptionCost, LinkDescriptionCost};
use binoc_sdk::ir::DiffNode;
use binoc_sdk::Changeset;
use serde::{Deserialize, Serialize};

use crate::renderers::markdown;

// ── Manifest schema ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Manifest {
    vector: VectorMeta,
    #[serde(default)]
    config: Option<ManifestConfig>,
    #[serde(default)]
    expected: Option<ExpectedAssertions>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct VectorMeta {
    name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    /// When set, diff this file inside each snapshot directory instead of
    /// the directories themselves. Tests file-level root dispatch.
    #[serde(default)]
    root_file: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct ManifestConfig {
    #[serde(default)]
    output: Option<OutputConfig>,
    #[serde(default)]
    dataset: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ExpectedAssertions {
    #[serde(default)]
    root_action: Option<String>,
    #[serde(default)]
    child_count: Option<usize>,
    #[serde(default)]
    has_tags: Option<Vec<String>>,
    #[serde(default)]
    significance: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RootManifest {
    #[serde(default)]
    config: Option<ManifestConfig>,
    #[serde(default)]
    expected: Option<ExpectedAssertions>,
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Discover vector directories under `vectors_dir`: subdirs that have
/// `manifest.toml`, `snapshot-a/`, and `snapshot-b/`. Sorted by name.
pub fn discover_vectors(vectors_dir: &Path) -> Vec<PathBuf> {
    if !vectors_dir.exists() {
        return Vec::new();
    }
    let mut vectors: Vec<PathBuf> = std::fs::read_dir(vectors_dir)
        .expect("test-vectors directory should be readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.join("manifest.toml").exists()
                && p.join("snapshot-a").exists()
                && p.join("snapshot-b").exists()
        })
        .collect();
    vectors.sort();
    vectors
}

/// Copy a vector directory (including `manifest.toml`, `snapshot-a/`,
/// `snapshot-b/`, and any other files) into `dest`, then run each
/// [`VectorMaterializer`] against both snapshot directories. The result is a
/// drop-in replacement for the source vector with every `.d` staging tree
/// replaced by the real artifact. `dest` is replaced if it already exists
/// (fresh materialization). Used by both [`run_vector`] and the
/// `materialize-test-vectors` binary so tests and `just materialize` see the
/// same tree.
pub fn materialize_snapshots(
    vector_dir: &Path,
    dest: &Path,
    materializers: &[&dyn VectorMaterializer],
) {
    if dest.exists() {
        std::fs::remove_dir_all(dest).expect("remove_dir_all dest");
    }
    copy_dir_all(vector_dir, dest);
    run_materializers(&dest.join("snapshot-a"), materializers);
    run_materializers(&dest.join("snapshot-b"), materializers);
}

// ── VectorMaterializer trait ──────────────────────────────────────────────

/// A builder that turns a staging directory (`name.ext.d/`) into a real
/// artifact (`name.ext`). Test-harness only — never shipped through the plugin
/// ABI or `PluginRegistry`. Plugins instantiate a `VectorMaterializer` and pass
/// it to [`materialize_snapshots`] or [`run_vector`] alongside
/// [`stdlib_materializers`].
pub trait VectorMaterializer: Send + Sync {
    /// Directory suffixes this builder claims, each including the leading dot
    /// (e.g. `".zip.d"`, `".sqlite.d"`). First match wins, in pipeline order.
    fn suffixes(&self) -> &[&'static str];

    /// Build `out_path` from the contents of `staging_dir`. Implementations
    /// should consult `all_staging_suffixes` to skip nested staging directories
    /// (the walker processes innermost-first, so their sibling artifacts already
    /// exist as regular files).
    fn build(&self, staging_dir: &Path, out_path: &Path, all_staging_suffixes: &[&str]);
}

/// Stdlib-provided materializers: [`ZipMaterializer`], [`TarMaterializer`],
/// and [`GzipMaterializer`].
/// Plugin materialize binaries should start from this list and push their own.
pub fn stdlib_materializers() -> Vec<Box<dyn VectorMaterializer>> {
    vec![
        Box::new(ZipMaterializer),
        Box::new(TarMaterializer),
        Box::new(GzipMaterializer),
    ]
}

/// Materializer for `.zip.d/` → `.zip`. Stored (uncompressed) zip so snapshots
/// are deterministic.
pub struct ZipMaterializer;

impl VectorMaterializer for ZipMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".zip.d"]
    }
    fn build(&self, staging_dir: &Path, out_path: &Path, all_suffixes: &[&str]) {
        create_zip_from_dir(staging_dir, out_path, all_suffixes);
    }
}

/// Materializer for `.tar.d/`, `.tar.gz.d/`, `.tgz.d/` → `.tar` / `.tar.gz` /
/// `.tgz`. Uses deterministic tar header mode and zero mtimes.
pub struct TarMaterializer;

impl VectorMaterializer for TarMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".tar.d", ".tar.gz.d", ".tgz.d"]
    }
    fn build(&self, staging_dir: &Path, out_path: &Path, all_suffixes: &[&str]) {
        create_tar_from_dir(staging_dir, out_path, all_suffixes);
    }
}

/// Materializer for `.gz.d/` -> `.gz`. The staging directory should contain
/// the uncompressed inner file, usually named like the output without `.gz`.
pub struct GzipMaterializer;

impl VectorMaterializer for GzipMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".gz.d"]
    }
    fn build(&self, staging_dir: &Path, out_path: &Path, _all_suffixes: &[&str]) {
        create_gzip_from_dir(staging_dir, out_path);
    }
}

/// Walk `root` depth-first; for each directory whose name ends in a registered
/// suffix, call the owning materializer (innermost first), then remove all
/// staging dirs. `materializers` is scanned in order on each name; first match
/// wins.
fn run_materializers(root: &Path, materializers: &[&dyn VectorMaterializer]) {
    if !root.exists() || materializers.is_empty() {
        return;
    }
    let all_suffixes: Vec<&str> = materializers
        .iter()
        .flat_map(|m| m.suffixes().iter().copied())
        .collect();
    build_materializers_recursive(root, materializers, &all_suffixes);
    remove_staging_dirs(root, &all_suffixes);
}

fn build_materializers_recursive(
    dir: &Path,
    materializers: &[&dyn VectorMaterializer],
    all_suffixes: &[&str],
) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flat_map(|rd| rd.into_iter())
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for entry in entries {
        if !entry.is_dir() {
            continue;
        }
        // Always recurse first so inner staging artifacts exist before an outer
        // materializer packages this directory.
        build_materializers_recursive(&entry, materializers, all_suffixes);

        let name = entry.file_name().unwrap().to_string_lossy().to_string();
        if let Some((m, suffix)) = find_materializer_for(materializers, &name) {
            let out_name = name.strip_suffix(".d").unwrap_or(&name);
            let out_path = dir.join(out_name);
            m.build(&entry, &out_path, all_suffixes);
            let _ = suffix; // informational
        }
    }
}

fn find_materializer_for<'a>(
    materializers: &'a [&'a dyn VectorMaterializer],
    name: &str,
) -> Option<(&'a dyn VectorMaterializer, &'a str)> {
    for m in materializers {
        for suffix in m.suffixes() {
            if name.ends_with(suffix) {
                return Some((*m, *suffix));
            }
        }
    }
    None
}

fn remove_staging_dirs(dir: &Path, all_suffixes: &[&str]) {
    if !dir.exists() {
        return;
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flat_map(|rd| rd.into_iter())
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    for entry in entries {
        if !entry.is_dir() {
            continue;
        }
        let name = entry.file_name().unwrap().to_string_lossy().to_string();
        if all_suffixes.iter().any(|s| name.ends_with(s)) {
            std::fs::remove_dir_all(&entry).ok();
        } else {
            remove_staging_dirs(&entry, all_suffixes);
        }
    }
}

/// True iff `name` ends with any registered staging-dir suffix. Exposed so
/// third-party materializers can skip nested staging dirs in their own walks.
pub fn is_staging_dir_name(name: &str, all_suffixes: &[&str]) -> bool {
    all_suffixes.iter().any(|s| name.ends_with(s))
}

/// Run one vector: materialize snapshots into a temp dir using
/// `materializers`, then resolve config, run diff, run assertions, and
/// snapshot. Pass [`stdlib_materializers`] plus any plugin-specific builders.
/// Snapshot paths in the changeset are normalized to `snapshot-a` / `snapshot-b`.
pub fn run_vector(
    vector_dir: &Path,
    vectors_root: &Path,
    materializers: &[&dyn VectorMaterializer],
) {
    run_vector_inner(
        vector_dir,
        vectors_root,
        materializers,
        crate::correspondence::engine_config_for_dataset_config,
        CostBaselineMode::CheckStdlibBaseline,
    );
}

/// Like [`run_vector`], with a caller-supplied correspondence config. Plugin
/// crates use this to register in-process rule packs while still reusing the
/// shared materialization/assertion/snapshot harness.
pub fn run_vector_with_correspondence_engine_config(
    vector_dir: &Path,
    vectors_root: &Path,
    materializers: &[&dyn VectorMaterializer],
    config_builder: impl FnOnce(&serde_json::Value) -> binoc_sdk::CorrespondenceEngineConfig,
) -> Changeset {
    run_vector_inner(
        vector_dir,
        vectors_root,
        materializers,
        config_builder,
        CostBaselineMode::ObserveOnly,
    )
}

fn run_vector_inner(
    vector_dir: &Path,
    vectors_root: &Path,
    materializers: &[&dyn VectorMaterializer],
    config_builder: impl FnOnce(&serde_json::Value) -> binoc_sdk::CorrespondenceEngineConfig,
    cost_baseline_mode: CostBaselineMode,
) -> Changeset {
    let manifest = load_manifest(vectors_root, vector_dir);
    let config = build_config(&manifest);

    let tmp = tempfile::tempdir().expect("temp dir");
    let materialized = tmp.path().join(&manifest.vector.name);
    materialize_snapshots(vector_dir, &materialized, materializers);
    let snap_a = materialized.join("snapshot-a");
    let snap_b = materialized.join("snapshot-b");

    let (diff_a, diff_b) = diff_roots(&snap_a, &snap_b, &manifest.vector.root_file);

    let controller = Controller::new(config_builder(&config.dataset))
        .with_dataset_config(config.dataset.clone());

    let run = controller
        .diff_with_metrics(diff_a.to_str().unwrap(), diff_b.to_str().unwrap())
        .unwrap_or_else(|e| {
            panic!(
                "Correspondence diff failed for {}: {e}",
                manifest.vector.name
            )
        });
    let changeset = run.changeset;
    record_description_cost(
        vectors_root,
        &manifest.vector.name,
        &run.metrics.description_cost,
        cost_baseline_mode,
    );

    check_changeset_invariants(&manifest.vector.name, &changeset);
    if let Some(expected) = &manifest.expected {
        check_assertions(&manifest.vector.name, &changeset, expected, &config);
    }

    let mut stable_changeset = changeset.clone();
    stable_changeset.from_snapshot = "snapshot-a".into();
    stable_changeset.to_snapshot = "snapshot-b".into();
    let md_config = markdown_config_for_dataset(&config);
    let md = markdown::render_markdown(&[stable_changeset.clone()], &md_config);

    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(vector_dir.join("expected-output"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("changeset", &stable_changeset);
        insta::assert_snapshot!("changelog", &md);
    });
    changeset
}

// ── Helpers ───────────────────────────────────────────────────────────────

const DESCRIPTION_COST_BASELINE_FILE: &str = "description-cost-baselines.toml";
const DESCRIPTION_COST_REGRESSION_MIN_UNITS: u64 = 2;
const DESCRIPTION_COST_REGRESSION_PCT: f64 = 0.05;
const UPDATE_DESCRIPTION_COST_BASELINE_ENV: &str = "BINOC_UPDATE_DESCRIPTION_COST_BASELINE";

#[derive(Debug, Clone, Copy)]
enum CostBaselineMode {
    CheckStdlibBaseline,
    ObserveOnly,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct DescriptionCostBaselineFile {
    #[serde(default)]
    vectors: Vec<NamedDescriptionCostBaseline>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NamedDescriptionCostBaseline {
    name: String,
    #[serde(flatten)]
    cost: DescriptionCost,
}

fn record_description_cost(
    vectors_root: &Path,
    vector_name: &str,
    observed: &DescriptionCost,
    mode: CostBaselineMode,
) {
    eprintln!(
        "description_cost: vector={vector_name} total={} edit_cost={} links={}",
        observed.description_cost, observed.edit_cost, observed.link_count
    );

    if !matches!(mode, CostBaselineMode::CheckStdlibBaseline) {
        return;
    }

    let path = vectors_root.join(DESCRIPTION_COST_BASELINE_FILE);
    if std::env::var_os(UPDATE_DESCRIPTION_COST_BASELINE_ENV).is_some() {
        update_description_cost_baseline(&path, vector_name, observed);
        return;
    }

    let baseline_file = load_description_cost_baseline(&path);
    let Some(baseline) = baseline_file
        .vectors
        .iter()
        .find(|entry| entry.name == vector_name)
    else {
        eprintln!(
            "warning: binoc-cost: [{vector_name}] missing description-cost baseline in {}",
            path.display()
        );
        return;
    };

    for warning in description_cost_warnings(vector_name, &baseline.cost, observed) {
        eprintln!("warning: binoc-cost: {warning}");
    }
}

fn load_description_cost_baseline(path: &Path) -> DescriptionCostBaselineFile {
    if !path.exists() {
        return DescriptionCostBaselineFile::default();
    }
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    toml::from_str(&contents).unwrap_or_else(|e| {
        panic!(
            "Failed to parse description-cost baseline {}: {e}",
            path.display()
        )
    })
}

fn update_description_cost_baseline(path: &Path, vector_name: &str, observed: &DescriptionCost) {
    let mut baseline_file = load_description_cost_baseline(path);
    baseline_file
        .vectors
        .retain(|entry| entry.name != vector_name);
    baseline_file.vectors.push(NamedDescriptionCostBaseline {
        name: vector_name.to_string(),
        cost: observed.clone(),
    });
    baseline_file
        .vectors
        .sort_by(|left, right| left.name.cmp(&right.name));
    let mut contents =
        "# Description-cost baselines for stdlib test vectors.\n# Cost = sum(binoc-core correspondence cost::cost(edit_list)) + link_count.\n# Refresh with BINOC_UPDATE_DESCRIPTION_COST_BASELINE=1 cargo test -p binoc-stdlib --test test_vectors.\n\n"
            .to_string();
    contents.push_str(
        &toml::to_string_pretty(&baseline_file)
            .expect("description-cost baseline should serialize"),
    );
    std::fs::write(path, contents)
        .unwrap_or_else(|e| panic!("Failed to write {}: {e}", path.display()));
}

fn description_cost_warnings(
    vector_name: &str,
    baseline: &DescriptionCost,
    observed: &DescriptionCost,
) -> Vec<String> {
    let allowed = description_cost_threshold(baseline.description_cost);
    if observed.description_cost <= baseline.description_cost + allowed {
        return Vec::new();
    }

    let mut warning = format!(
        "[{vector_name}] description_cost regressed from {} to {} (allowed +{}); edit_cost {} -> {}, link_count {} -> {}",
        baseline.description_cost,
        observed.description_cost,
        allowed,
        baseline.edit_cost,
        observed.edit_cost,
        baseline.link_count,
        observed.link_count
    );

    let link_details = link_change_details(&baseline.links, &observed.links);
    if !link_details.is_empty() {
        warning.push_str("; ");
        warning.push_str(&link_details.join("; "));
    }

    if baseline.compaction_accepted != observed.compaction_accepted {
        warning.push_str(&format!(
            "; compaction_accepted {:?} -> {:?}",
            baseline.compaction_accepted, observed.compaction_accepted
        ));
    }
    if baseline.compaction_rejected != observed.compaction_rejected {
        warning.push_str(&format!(
            "; compaction_rejected {:?} -> {:?}",
            baseline.compaction_rejected, observed.compaction_rejected
        ));
    }
    if baseline.unwritten_links != observed.unwritten_links {
        warning.push_str(&format!(
            "; unwritten_links {:?} -> {:?}",
            baseline.unwritten_links, observed.unwritten_links
        ));
    }

    vec![warning]
}

fn description_cost_threshold(baseline: u64) -> u64 {
    let relative = ((baseline as f64) * DESCRIPTION_COST_REGRESSION_PCT).ceil() as u64;
    DESCRIPTION_COST_REGRESSION_MIN_UNITS.max(relative)
}

fn link_change_details(
    baseline: &[LinkDescriptionCost],
    observed: &[LinkDescriptionCost],
) -> Vec<String> {
    let before = link_cost_map(baseline);
    let after = link_cost_map(observed);
    let mut details = Vec::new();

    for (key, old) in &before {
        match after.get(key) {
            Some(new) => {
                if old.edit_cost != new.edit_cost
                    || old.edit_count != new.edit_count
                    || old.writer != new.writer
                    || old.settled != new.settled
                {
                    details.push(format!(
                        "link changed {} edit_cost {} -> {}, edit_count {} -> {}, writer {:?} -> {:?}, settled {} -> {}",
                        link_label(new),
                        old.edit_cost,
                        new.edit_cost,
                        old.edit_count,
                        new.edit_count,
                        old.writer,
                        new.writer,
                        old.settled,
                        new.settled
                    ));
                }
            }
            None => details.push(format!("link removed {}", link_label(old))),
        }
    }
    for (key, new) in &after {
        if !before.contains_key(key) {
            details.push(format!("link added {}", link_label(new)));
        }
    }

    details
}

fn link_cost_map(links: &[LinkDescriptionCost]) -> BTreeMap<String, &LinkDescriptionCost> {
    links
        .iter()
        .map(|link| {
            (
                format!(
                    "{}\x1f{}\x1f{}\x1f{}",
                    link.left_path, link.right_path, link.evidence, link.proposer
                ),
                link,
            )
        })
        .collect()
}

fn link_label(link: &LinkDescriptionCost) -> String {
    format!(
        "'{}' -> '{}' [{} by {}]",
        link.left_path, link.right_path, link.evidence, link.proposer
    )
}

fn diff_roots(snap_a: &Path, snap_b: &Path, root_file: &Option<String>) -> (PathBuf, PathBuf) {
    match root_file {
        Some(f) => (snap_a.join(f), snap_b.join(f)),
        None => (snap_a.to_path_buf(), snap_b.to_path_buf()),
    }
}

fn load_root_manifest(vectors_dir: &Path) -> RootManifest {
    let path = vectors_dir.join("manifest.toml");
    if !path.exists() {
        return RootManifest::default();
    }
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    toml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
}

fn load_manifest(vectors_root: &Path, vector_dir: &Path) -> Manifest {
    let root = load_root_manifest(vectors_root);
    let content = std::fs::read_to_string(vector_dir.join("manifest.toml"))
        .expect("manifest.toml should be readable");
    let mut manifest: Manifest = toml::from_str(&content).unwrap_or_else(|e| {
        panic!(
            "Failed to parse {}/manifest.toml: {e}",
            vector_dir.display()
        )
    });
    if manifest.config.is_none() && root.config.is_some() {
        manifest.config = root.config;
    }
    if manifest.expected.is_none() && root.expected.is_some() {
        manifest.expected = root.expected;
    }
    manifest
}

fn build_config(manifest: &Manifest) -> DatasetConfig {
    match &manifest.config {
        Some(cfg) => {
            let default = DatasetConfig::default_config();
            DatasetConfig {
                renderers: default.renderers,
                output: cfg.output.clone().unwrap_or(default.output),
                dataset: cfg.dataset.clone().unwrap_or(default.dataset),
            }
        }
        None => DatasetConfig::default_config(),
    }
}

fn markdown_config_for_dataset(config: &DatasetConfig) -> markdown::MarkdownRendererConfig {
    let md_val = config.output.get_for_renderer("binoc.markdown");
    serde_json::from_value(md_val).unwrap_or_default()
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create_dir_all");
    for e in std::fs::read_dir(src).expect("read_dir") {
        let e = e.expect("entry");
        let path = e.path();
        let name = e.file_name();
        if name == ".gitkeep" {
            continue;
        }
        let dst_path = dst.join(&name);
        if path.is_dir() {
            copy_dir_all(&path, &dst_path);
        } else {
            std::fs::copy(&path, &dst_path).expect("copy");
        }
    }
}

fn create_tar_from_dir(source_dir: &Path, tar_path: &Path, all_suffixes: &[&str]) {
    let tar_name = tar_path.to_string_lossy();
    let is_gz = tar_name.ends_with(".tar.gz") || tar_name.ends_with(".tgz");

    let file = std::fs::File::create(tar_path)
        .unwrap_or_else(|e| panic!("Failed to create {}: {e}", tar_path.display()));

    if is_gz {
        let encoder = flate2::GzBuilder::new()
            .mtime(0)
            .write(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        builder.mode(tar::HeaderMode::Deterministic);
        add_dir_to_tar(&mut builder, source_dir, source_dir, all_suffixes);
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
    } else {
        let mut builder = tar::Builder::new(file);
        builder.mode(tar::HeaderMode::Deterministic);
        add_dir_to_tar(&mut builder, source_dir, source_dir, all_suffixes);
        builder.into_inner().unwrap();
    }
}

fn add_dir_to_tar<W: Write>(
    builder: &mut tar::Builder<W>,
    base: &Path,
    dir: &Path,
    all_suffixes: &[&str],
) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap();
        let name = rel.to_string_lossy();
        if path.is_dir() && is_staging_dir_name(&name, all_suffixes) {
            continue;
        }
        if path.is_dir() {
            add_dir_to_tar(builder, base, &path, all_suffixes);
        } else {
            builder.append_path_with_name(&path, &*name).unwrap();
        }
    }
}

fn create_zip_from_dir(source_dir: &Path, zip_path: &Path, all_suffixes: &[&str]) {
    let file = std::fs::File::create(zip_path)
        .unwrap_or_else(|e| panic!("Failed to create {}: {e}", zip_path.display()));
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    add_dir_to_zip(&mut zip, source_dir, source_dir, options, all_suffixes);
    zip.finish().unwrap();
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    dir: &Path,
    options: zip::write::SimpleFileOptions,
    all_suffixes: &[&str],
) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap();
        let name = rel.to_string_lossy().to_string();
        if path.is_dir() && is_staging_dir_name(&name, all_suffixes) {
            continue;
        }
        if path.is_dir() {
            zip.add_directory(format!("{name}/"), options).unwrap();
            add_dir_to_zip(zip, base, &path, options, all_suffixes);
        } else {
            zip.start_file(&name, options).unwrap();
            let data = std::fs::read(&path).unwrap();
            zip.write_all(&data).unwrap();
        }
    }
}

fn create_gzip_from_dir(staging_dir: &Path, gzip_path: &Path) {
    let source_path = gzip_source_path(staging_dir, gzip_path);
    let input = std::fs::read(&source_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", source_path.display()));
    let file = std::fs::File::create(gzip_path)
        .unwrap_or_else(|e| panic!("Failed to create {}: {e}", gzip_path.display()));
    let mut encoder = flate2::GzBuilder::new()
        .mtime(0)
        .write(file, flate2::Compression::fast());
    encoder.write_all(&input).unwrap();
    encoder.finish().unwrap();
}

fn gzip_source_path(staging_dir: &Path, gzip_path: &Path) -> PathBuf {
    let Some(output_name) = gzip_path.file_name().and_then(|n| n.to_str()) else {
        panic!("gzip output path has no filename: {}", gzip_path.display());
    };
    let Some(inner_name) = output_name.strip_suffix(".gz") else {
        panic!("gzip output path must end in .gz: {}", gzip_path.display());
    };

    let preferred = staging_dir.join(inner_name);
    if preferred.is_file() {
        return preferred;
    }

    let files: Vec<PathBuf> = std::fs::read_dir(staging_dir)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", staging_dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    if files.len() == 1 {
        return files[0].clone();
    }

    panic!(
        "{} must contain the inner file '{}' or exactly one regular file",
        staging_dir.display(),
        inner_name
    );
}

// ── Invariant checker ─────────────────────────────────────────────────

/// Validate semantic invariants that must hold for every changeset,
/// regardless of which vector produced it. Runs after correspondence projection
/// and before snapshot comparison so that buggy output is caught even when the
/// snapshot would silently enshrine it.
///
/// This is the canonical home for cheap changeset invariants (tier 1 of
/// the lint scheme — see `binoc_sdk::lints` for the tier overview).
/// Invariants added here run on every vector of every crate that uses
/// this harness, stdlib and plugins alike. Plugins with domain-specific
/// invariants should call this plus their own checks on changesets they
/// build in their own tests. `name` is only used to label failures.
pub fn check_changeset_invariants(name: &str, changeset: &Changeset) {
    let Some(root) = &changeset.root else {
        return;
    };
    let mut leaf_paths: Vec<&str> = Vec::new();
    collect_invariant_violations(root, &mut leaf_paths);

    let mut seen = HashSet::new();
    for path in &leaf_paths {
        if !path.is_empty() {
            assert!(seen.insert(*path), "[{name}] Duplicate leaf path: '{path}'");
        }
    }

    check_no_parent_child_edit_duplication(name, root);
}

/// Collect the JSON edit objects (`{ "verb", "params" }`) recorded under a
/// node's `details["edits"]`.
fn node_edits(node: &DiffNode) -> Vec<&serde_json::Value> {
    node.details
        .get("edits")
        .and_then(|edits| edits.as_array())
        .map(|edits| edits.iter().collect())
        .unwrap_or_default()
}

/// A parent node's own edits must not restate edits that already appear on any
/// of its descendants. Once a multi-table or container source decomposes into
/// child nodes, each child carries its own content edits; the parent should only
/// carry residual (membership-level) edits, never a copy of what a child already
/// reports. This guards the double-count the dropped collection manifest used to
/// cause (see the CFM-69 ADR invariant).
fn check_no_parent_child_edit_duplication(name: &str, node: &DiffNode) {
    let parent_edits = node_edits(node);
    if !parent_edits.is_empty() {
        let mut descendant_edits: HashSet<String> = HashSet::new();
        for child in &node.children {
            collect_descendant_edits(child, &mut descendant_edits);
        }
        for edit in parent_edits {
            assert!(
                !descendant_edits.contains(&edit.to_string()),
                "[{name}] Parent node '{}' restates a child edit: {edit}",
                node.path
            );
        }
    }
    for child in &node.children {
        check_no_parent_child_edit_duplication(name, child);
    }
}

fn collect_descendant_edits(node: &DiffNode, out: &mut HashSet<String>) {
    for edit in node_edits(node) {
        out.insert(edit.to_string());
    }
    for child in &node.children {
        collect_descendant_edits(child, out);
    }
}

fn collect_invariant_violations<'a>(node: &'a DiffNode, leaf_paths: &mut Vec<&'a str>) {
    if node.children.is_empty() {
        leaf_paths.push(&node.path);
    }

    for child in &node.children {
        collect_invariant_violations(child, leaf_paths);
    }
}

fn check_assertions(
    name: &str,
    changeset: &Changeset,
    expected: &ExpectedAssertions,
    config: &DatasetConfig,
) {
    if let Some(root_action) = &expected.root_action {
        let root = changeset.root.as_ref().unwrap_or_else(|| {
            panic!("[{name}] Expected root with action '{root_action}' but changeset has no root")
        });
        if root.item_type == "directory" && root.action != *root_action {
            let child_actions: Vec<&str> =
                root.children.iter().map(|c| c.action.as_str()).collect();
            assert!(
                child_actions.contains(&root_action.as_str()) || root.action == *root_action,
                "[{name}] Expected root_action '{root_action}', got root.action='{}' with child actions: {child_actions:?}",
                root.action
            );
        }
    }
    if let Some(child_count) = expected.child_count {
        let root = changeset.root.as_ref().unwrap_or_else(|| {
            panic!("[{name}] Expected child_count={child_count} but changeset has no root")
        });
        assert_eq!(
            root.children.len(),
            child_count,
            "[{name}] Expected child_count={child_count}, got {}. Children: {:?}",
            root.children.len(),
            root.children
                .iter()
                .map(|c| (&c.action, &c.path))
                .collect::<Vec<_>>()
        );
    }
    if let Some(has_tags) = &expected.has_tags {
        let root = changeset
            .root
            .as_ref()
            .unwrap_or_else(|| panic!("[{name}] Expected tags but changeset has no root"));
        let all_tags = root.all_tags();
        for tag in has_tags {
            assert!(
                all_tags.contains(tag),
                "[{name}] Expected tag '{tag}' not found. All tags in tree: {all_tags:?}"
            );
        }
    }
    if let Some(significance) = &expected.significance {
        let root = changeset
            .root
            .as_ref()
            .unwrap_or_else(|| panic!("[{name}] Expected significance but changeset has no root"));
        let all_tags = root.all_tags();
        let md_val = config.output.get_for_renderer("binoc.markdown");
        let md_config: markdown::MarkdownRendererConfig =
            serde_json::from_value(md_val).unwrap_or_default();
        let sig_tags = md_config
            .groups
            .iter()
            .find(|group| group.heading == *significance)
            .map(|group| &group.tags);
        assert!(
            sig_tags.is_some(),
            "[{name}] Significance group '{significance}' not in markdown renderer config"
        );
        let sig_tags = sig_tags.unwrap();
        let has_sig_tag = all_tags.iter().any(|t| sig_tags.contains(t));
        assert!(
            has_sig_tag,
            "[{name}] Expected significance '{significance}' but no matching tags. All tags: {all_tags:?}, sig_tags: {sig_tags:?}"
        );
    }
}

#[cfg(test)]
mod cost_baseline_tests {
    use super::*;

    fn link(
        left_path: &str,
        right_path: &str,
        edit_cost: u64,
        writer: Option<&str>,
    ) -> LinkDescriptionCost {
        LinkDescriptionCost {
            index: 0,
            left_path: left_path.into(),
            right_path: right_path.into(),
            evidence: "test.evidence".into(),
            proposer: "test.pair".into(),
            writer: writer.map(str::to_string),
            settled: false,
            edit_count: 1,
            edit_cost,
        }
    }

    fn cost(
        description_cost: u64,
        edit_cost: u64,
        links: Vec<LinkDescriptionCost>,
    ) -> DescriptionCost {
        DescriptionCost {
            description_cost,
            edit_cost,
            link_count: links.len() as u64,
            links,
            compaction_accepted: BTreeMap::new(),
            compaction_rejected: BTreeMap::new(),
            unwritten_links: Vec::new(),
        }
    }

    #[test]
    fn description_cost_within_threshold_does_not_warn() {
        let baseline = cost(100, 98, vec![link("a.csv", "a.csv", 98, Some("writer"))]);
        let observed = cost(105, 103, vec![link("a.csv", "a.csv", 103, Some("writer"))]);

        assert!(description_cost_warnings("small-change", &baseline, &observed).is_empty());
    }

    #[test]
    fn deliberately_cost_regressed_path_warns_with_link_and_compaction_context() {
        let mut baseline = cost(10, 9, vec![link("a.csv", "a.csv", 9, Some("old.writer"))]);
        baseline.compaction_accepted.insert("compact".into(), 1);

        let mut observed = cost(20, 19, vec![link("a.csv", "a.csv", 19, Some("new.writer"))]);
        observed.compaction_accepted.insert("compact".into(), 0);

        let warnings = description_cost_warnings("cost-regressed-fixture", &baseline, &observed);

        assert_eq!(warnings.len(), 1);
        let warning = &warnings[0];
        assert!(warning.contains("description_cost regressed from 10 to 20"));
        assert!(warning.contains("link changed 'a.csv' -> 'a.csv'"));
        assert!(warning.contains("edit_cost 9 -> 19"));
        assert!(warning.contains("writer Some(\"old.writer\") -> Some(\"new.writer\")"));
        assert!(warning.contains("compaction_accepted"));
    }
}
