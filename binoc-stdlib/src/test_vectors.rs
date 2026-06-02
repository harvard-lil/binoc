//! Shared harness for running test vectors. Used by binoc-stdlib’s own vectors
//! and by plugins (e.g. binoc-sqlite) so they don’t duplicate manifest parsing,
//! copy/build/snapshot logic, or assertions. Vectors live in a `test-vectors/`
//! directory; a root `manifest.toml` there provides default `[config]`/`[expected]`;
//! each vector’s manifest overrides.
//!
//! Test vectors commit *source* trees like `archive.zip.d/` instead of opaque
//! binary archives. A [`VectorMaterializer`] turns each staging directory into
//! the real artifact (`.zip`, `.tar.gz`, `.sqlite`, ...). The stdlib ships
//! [`ZipMaterializer`] and [`TarMaterializer`] ([`stdlib_materializers`]);
//! plugins contribute their own (see `binoc-sqlite` for SQLite). Both
//! [`run_vector`] and [`materialize_snapshots`] go through the same walker so
//! tests and `just materialize` build identical trees.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use binoc_core::config::{DatasetConfig, PluginRegistry, TransformerConfig};
use binoc_core::controller::Controller;
use binoc_sdk::ir::DiffNode;
use binoc_sdk::test_support::{AbiCall, AbiComparator, AbiLogCollector, AbiTransformer};
use binoc_sdk::Changeset;
use serde::Deserialize;

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
struct ManifestConfig {
    #[serde(default)]
    comparators: Option<Vec<String>>,
    #[serde(default)]
    transformers: Option<Vec<String>>,
    #[serde(default)]
    transformer_config: Option<TransformerConfig>,
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

/// Stdlib-provided materializers: [`ZipMaterializer`] and [`TarMaterializer`].
/// Plugin materialize binaries should start from this list and push their own.
pub fn stdlib_materializers() -> Vec<Box<dyn VectorMaterializer>> {
    vec![Box::new(ZipMaterializer), Box::new(TarMaterializer)]
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

/// Build a default stdlib registry with all plugins wrapped in ABI wrappers.
/// Returns the registry, a vec of log collectors for snapshotting, and the
/// shared sequence counter (pass to additional plugin wrappers to keep a
/// single global ordering).
pub fn abi_wrapped_default_registry() -> (
    PluginRegistry,
    Vec<Arc<dyn AbiLogCollector>>,
    Arc<AtomicU64>,
) {
    use crate::comparators::*;
    use crate::transformers::*;

    let counter = Arc::new(AtomicU64::new(0));
    let mut registry = PluginRegistry::new();
    let mut collectors: Vec<Arc<dyn AbiLogCollector>> = Vec::new();

    macro_rules! wrap_comparator {
        ($ty:expr) => {{
            let w = Arc::new(AbiComparator::new($ty, counter.clone()));
            collectors.push(w.clone());
            registry.register_comparator(w).expect("same-build plugin");
        }};
    }
    macro_rules! wrap_transformer {
        ($ty:expr) => {{
            let w = Arc::new(AbiTransformer::new($ty, counter.clone()));
            collectors.push(w.clone());
            registry.register_transformer(w).expect("same-build plugin");
        }};
    }

    wrap_comparator!(zip_compare::ZipComparator);
    wrap_comparator!(tar_compare::TarComparator);
    wrap_comparator!(directory::DirectoryComparator);
    wrap_comparator!(csv_compare::CsvComparator);
    wrap_comparator!(text::TextComparator);
    wrap_comparator!(binary::BinaryComparator);

    wrap_transformer!(correlation_detector::CorrelationDetector);
    wrap_transformer!(fuzzy_correlation_detector::FuzzyCorrelationDetector);
    wrap_transformer!(folder_move_detector::FolderMoveDetector);
    wrap_transformer!(tabular_analyzer::TabularAnalyzer);
    wrap_transformer!(column_reorder::ColumnReorderDetector);

    registry
        .register_renderer(Arc::new(markdown::MarkdownRenderer))
        .expect("same-build plugin");

    (registry, collectors, counter)
}

/// Run one vector: materialize snapshots into a temp dir using
/// `materializers`, then resolve config, run diff, run assertions, and
/// snapshot. Pass [`stdlib_materializers`] plus any plugin-specific builders.
/// Snapshot paths in the changeset are normalized to `snapshot-a` / `snapshot-b`.
pub fn run_vector(
    vector_dir: &Path,
    vectors_root: &Path,
    registry_builder: impl FnOnce() -> PluginRegistry,
    materializers: &[&dyn VectorMaterializer],
) {
    let manifest = load_manifest(vectors_root, vector_dir);
    let config = build_config(&manifest);

    let tmp = tempfile::tempdir().expect("temp dir");
    let snap_a = tmp.path().join("snapshot-a");
    let snap_b = tmp.path().join("snapshot-b");
    copy_dir_all(&vector_dir.join("snapshot-a"), &snap_a);
    copy_dir_all(&vector_dir.join("snapshot-b"), &snap_b);
    run_materializers(&snap_a, materializers);
    run_materializers(&snap_b, materializers);

    let (diff_a, diff_b) = diff_roots(&snap_a, &snap_b, &manifest.vector.root_file);

    let registry = registry_builder();
    let resolved = registry.resolve(&config).unwrap_or_else(|e| {
        panic!(
            "Failed to resolve plugins for {}: {e}",
            manifest.vector.name
        )
    });
    let controller = Controller::new(resolved.comparators, resolved.transformers)
        .with_transformer_configs(config.transformer_config.as_map());

    let changeset = controller
        .diff(diff_a.to_str().unwrap(), diff_b.to_str().unwrap())
        .unwrap_or_else(|e| panic!("Diff failed for {}: {e}", manifest.vector.name));

    check_invariants(&manifest.vector.name, &changeset);

    if let Some(expected) = &manifest.expected {
        check_assertions(&manifest.vector.name, &changeset, expected, &config);
    }

    let mut stable_changeset = changeset.clone();
    stable_changeset.from_snapshot = "snapshot-a".into();
    stable_changeset.to_snapshot = "snapshot-b".into();
    let md = markdown::render_markdown(
        &[stable_changeset.clone()],
        &markdown::MarkdownRendererConfig::default(),
    );

    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(vector_dir.join("expected-output"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("changeset", &stable_changeset);
        insta::assert_snapshot!("changelog", &md);
    });
}

/// Like [`run_vector`], but also runs the same vector through an ABI-wrapped
/// registry, asserts that the two runs produce byte-identical changesets, and
/// snapshots the ABI call log as `abi-log`.
///
/// The parity check is the core guarantee: anything a plugin can "cheat" by
/// exploiting in-process conveniences (shared memory, non-serializable fields,
/// raw filesystem access outside `DataAccess`) will make the two changesets
/// diverge and fail the test. Callers pass two registry builders that must
/// construct logically equivalent sets of plugins — one with raw
/// implementations, one with every plugin wrapped in
/// [`AbiComparator`]/[`AbiTransformer`].
pub fn run_vector_with_abi_log(
    vector_dir: &Path,
    vectors_root: &Path,
    direct_registry_builder: impl FnOnce() -> PluginRegistry,
    abi_registry_builder: impl FnOnce() -> PluginRegistry,
    materializers: &[&dyn VectorMaterializer],
    abi_collectors: &[&dyn AbiLogCollector],
) {
    let manifest = load_manifest(vectors_root, vector_dir);
    let config = build_config(&manifest);

    let tmp = tempfile::tempdir().expect("temp dir");
    let snap_a = tmp.path().join("snapshot-a");
    let snap_b = tmp.path().join("snapshot-b");
    copy_dir_all(&vector_dir.join("snapshot-a"), &snap_a);
    copy_dir_all(&vector_dir.join("snapshot-b"), &snap_b);
    run_materializers(&snap_a, materializers);
    run_materializers(&snap_b, materializers);

    let (diff_a, diff_b) = diff_roots(&snap_a, &snap_b, &manifest.vector.root_file);

    // Single-threaded rayon pool so traversal order is deterministic (both for
    // the ABI seq counter and for stable parity comparison).
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("rayon pool");

    // Run 1: direct (unwrapped) registry — what an in-process deployment does.
    let direct_registry = direct_registry_builder();
    let direct_resolved = direct_registry.resolve(&config).unwrap_or_else(|e| {
        panic!(
            "Failed to resolve direct plugins for {}: {e}",
            manifest.vector.name
        )
    });
    let direct_controller =
        Controller::new(direct_resolved.comparators, direct_resolved.transformers)
            .with_transformer_configs(config.transformer_config.as_map());
    let direct_changeset = pool
        .install(|| direct_controller.diff(diff_a.to_str().unwrap(), diff_b.to_str().unwrap()))
        .unwrap_or_else(|e| panic!("Direct diff failed for {}: {e}", manifest.vector.name));

    // Run 2: ABI-wrapped registry — what a separately-compiled plugin via the
    // C ABI will see. Must produce an identical changeset to Run 1.
    let abi_registry = abi_registry_builder();
    let abi_resolved = abi_registry.resolve(&config).unwrap_or_else(|e| {
        panic!(
            "Failed to resolve ABI plugins for {}: {e}",
            manifest.vector.name
        )
    });
    let abi_controller = Controller::new(abi_resolved.comparators, abi_resolved.transformers)
        .with_transformer_configs(config.transformer_config.as_map());
    let abi_changeset = pool
        .install(|| abi_controller.diff(diff_a.to_str().unwrap(), diff_b.to_str().unwrap()))
        .unwrap_or_else(|e| panic!("ABI diff failed for {}: {e}", manifest.vector.name));

    assert_changesets_equal(&manifest.vector.name, &direct_changeset, &abi_changeset);

    let mut abi_log: Vec<AbiCall> = Vec::new();
    for collector in abi_collectors {
        abi_log.extend(collector.take_abi_log());
    }
    abi_log.sort_by_key(|c| c.seq);

    // Use the ABI run as the canonical changeset for downstream checks and
    // snapshots — parity with the direct run is already asserted.
    let changeset = abi_changeset;
    check_invariants(&manifest.vector.name, &changeset);

    if let Some(expected) = &manifest.expected {
        check_assertions(&manifest.vector.name, &changeset, expected, &config);
    }

    let mut stable_changeset = changeset.clone();
    stable_changeset.from_snapshot = "snapshot-a".into();
    stable_changeset.to_snapshot = "snapshot-b".into();
    let md = markdown::render_markdown(
        &[stable_changeset.clone()],
        &markdown::MarkdownRendererConfig::default(),
    );

    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(vector_dir.join("expected-output"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("changeset", &stable_changeset);
        insta::assert_snapshot!("changelog", &md);
        if !abi_log.is_empty() {
            insta::assert_json_snapshot!("abi-log", &abi_log);
        }
    });
}

/// Assert that two changesets are byte-identical as JSON. Used to enforce that
/// direct and ABI-wrapped runs of the same vector produce the same result.
fn assert_changesets_equal(name: &str, direct: &Changeset, abi: &Changeset) {
    let direct_json = serde_json::to_string_pretty(direct).expect("serialize direct changeset");
    let abi_json = serde_json::to_string_pretty(abi).expect("serialize ABI changeset");
    if direct_json != abi_json {
        // Print a compact diff hint so the failure points at the first
        // diverging line rather than dumping two large blobs.
        let first_diff = direct_json
            .lines()
            .zip(abi_json.lines())
            .enumerate()
            .find(|(_, (d, a))| d != a)
            .map(|(i, (d, a))| format!("  line {i}:\n    direct: {d}\n    abi:    {a}"))
            .unwrap_or_else(|| "  (lengths differ, no shared prefix)".into());
        panic!(
            "[{name}] direct-dispatch and ABI-wrapped runs produced different changesets.\n\
             This means a plugin is relying on in-process conveniences that won't survive\n\
             the real C ABI boundary (e.g. non-serializable fields, shared memory, raw fs\n\
             access outside DataAccess). First divergence:\n{first_diff}"
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

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
                comparators: cfg.comparators.clone().unwrap_or(default.comparators),
                transformers: cfg.transformers.clone().unwrap_or(default.transformers),
                renderers: default.renderers,
                output: default.output,
                transformer_config: cfg
                    .transformer_config
                    .clone()
                    .unwrap_or(default.transformer_config),
            }
        }
        None => DatasetConfig::default_config(),
    }
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

// ── Invariant checker ─────────────────────────────────────────────────

/// Validate semantic invariants that must hold for every changeset,
/// regardless of which vector produced it. Runs after diff+transform
/// and before snapshot comparison so that buggy output is caught even
/// when the snapshot would silently enshrine it.
fn check_invariants(name: &str, changeset: &Changeset) {
    let Some(root) = &changeset.root else {
        return;
    };
    let mut leaf_paths: Vec<&str> = Vec::new();
    collect_invariant_violations(name, root, &mut leaf_paths);

    let mut seen = HashSet::new();
    for path in &leaf_paths {
        if !path.is_empty() {
            assert!(seen.insert(*path), "[{name}] Duplicate leaf path: '{path}'");
        }
    }
}

fn collect_invariant_violations<'a>(name: &str, node: &'a DiffNode, leaf_paths: &mut Vec<&'a str>) {
    if node.children.is_empty() {
        leaf_paths.push(&node.path);
    }

    check_tabular_tag_detail_consistency(name, node);

    for child in &node.children {
        collect_invariant_violations(name, child, leaf_paths);
    }
}

/// If a tabular node has both tags and details set by TabularAnalyzer,
/// the tags must be consistent with the detail values. A tag like
/// `binoc.column-addition` implies `columns_added` is non-empty, and
/// a non-empty `columns_added` implies the tag is present.
fn check_tabular_tag_detail_consistency(name: &str, node: &DiffNode) {
    if !node
        .transformed_by
        .contains(&"binoc.tabular_analyzer".to_string())
    {
        return;
    }
    if node.action == "add" || node.action == "remove" {
        return;
    }

    let tag_detail_pairs: &[(&str, &str)] = &[
        ("binoc.column-addition", "columns_added"),
        ("binoc.column-removal", "columns_removed"),
        ("binoc.row-addition", "rows_added"),
        ("binoc.row-removal", "rows_removed"),
        ("binoc.cell-change", "cells_changed"),
    ];

    for &(tag, detail_key) in tag_detail_pairs {
        let has_tag = node.tags.contains(tag);
        let detail_is_positive = node.details.get(detail_key).is_some_and(|v| match v {
            serde_json::Value::Number(n) => n.as_u64().unwrap_or(0) > 0,
            serde_json::Value::Array(a) => !a.is_empty(),
            _ => false,
        });

        assert_eq!(
            has_tag,
            detail_is_positive,
            "[{name}] node '{}': tag '{tag}' is {}, but detail '{detail_key}' is {} \
             (tags={:?}, details={:?})",
            node.path,
            if has_tag { "present" } else { "absent" },
            if detail_is_positive {
                "positive/non-empty"
            } else {
                "zero/empty/missing"
            },
            node.tags,
            node.details,
        );
    }

    check_tabular_column_detail_consistency(name, node);
}

/// Cross-check: `columns_left` and `columns_right` details must be
/// consistent with `columns_added` and `columns_removed`. Every column
/// in `columns_added` must appear in `columns_right` but not `columns_left`,
/// and vice versa for `columns_removed`.
fn check_tabular_column_detail_consistency(name: &str, node: &DiffNode) {
    let get_string_array = |key: &str| -> Option<Vec<String>> {
        node.details.get(key).and_then(|v| {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
        })
    };

    let Some(cols_left) = get_string_array("columns_left") else {
        return;
    };
    let Some(cols_right) = get_string_array("columns_right") else {
        return;
    };
    let cols_added = get_string_array("columns_added").unwrap_or_default();
    let cols_removed = get_string_array("columns_removed").unwrap_or_default();

    let left_set: HashSet<&str> = cols_left.iter().map(|s| s.as_str()).collect();
    let right_set: HashSet<&str> = cols_right.iter().map(|s| s.as_str()).collect();

    for col in &cols_added {
        assert!(
            right_set.contains(col.as_str()) && !left_set.contains(col.as_str()),
            "[{name}] node '{}': column '{col}' listed as added but \
             columns_left={cols_left:?}, columns_right={cols_right:?}",
            node.path,
        );
    }
    for col in &cols_removed {
        assert!(
            left_set.contains(col.as_str()) && !right_set.contains(col.as_str()),
            "[{name}] node '{}': column '{col}' listed as removed but \
             columns_left={cols_left:?}, columns_right={cols_right:?}",
            node.path,
        );
    }

    if let Some(rows_left) = node.details.get("rows_left").and_then(|v| v.as_u64()) {
        if let Some(rows_right) = node.details.get("rows_right").and_then(|v| v.as_u64()) {
            let rows_added = node
                .details
                .get("rows_added")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let rows_removed = node
                .details
                .get("rows_removed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let expected_right = rows_left + rows_added - rows_removed;
            assert_eq!(
                rows_right, expected_right,
                "[{name}] node '{}': rows_left({rows_left}) + rows_added({rows_added}) \
                 - rows_removed({rows_removed}) = {expected_right}, but rows_right = {rows_right}",
                node.path,
            );
        }
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
