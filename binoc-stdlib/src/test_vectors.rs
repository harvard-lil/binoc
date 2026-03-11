//! Shared harness for running test vectors. Used by binoc-stdlib’s own vectors
//! and by plugins (e.g. binoc-sqlite) so they don’t duplicate manifest parsing,
//! copy/build/snapshot logic, or assertions. Vectors live in a `test-vectors/`
//! directory; a root `manifest.toml` there provides default `[config]`/`[expected]`;
//! each vector’s manifest overrides. The harness copies snapshots to a temp dir
//! and builds `.zip` files from `.zip.d` and `.tar`/`.tar.gz` files from
//! `.tar.d`/`.tar.gz.d` there; plugins that need other artifacts (e.g. SQLite
//! from `.sqlite.d`) pass an optional `prepare` callback.

use std::io::Write;
use std::path::{Path, PathBuf};

use binoc_core::config::{DatasetConfig, PluginRegistry};
use binoc_core::controller::Controller;
use binoc_core::ir::Migration;
use serde::Deserialize;

use crate::outputters::markdown;

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
}

#[derive(Debug, Deserialize)]
struct ManifestConfig {
    #[serde(default)]
    comparators: Option<Vec<String>>,
    #[serde(default)]
    transformers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ExpectedAssertions {
    #[serde(default)]
    root_kind: Option<String>,
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

/// Run one vector: copy snapshots to temp, build zips from `.zip.d`, optionally
/// run `prepare(snap_a, snap_b)` for plugin-specific artifacts (e.g. SQLite from
/// `.sqlite.d`), then resolve config, run diff, run assertions, snapshot.
/// Snapshot paths in the migration are normalized to `snapshot-a` / `snapshot-b`.
pub fn run_vector(
    vector_dir: &Path,
    vectors_root: &Path,
    registry_builder: impl FnOnce() -> PluginRegistry,
    prepare: Option<impl FnOnce(&Path, &Path)>,
) {
    let manifest = load_manifest(vectors_root, vector_dir);
    let config = build_config(&manifest);

    let snap_a_src = vector_dir.join("snapshot-a");
    let snap_b_src = vector_dir.join("snapshot-b");
    let tmp = tempfile::tempdir().expect("temp dir");
    let snap_a = tmp.path().join("snapshot-a");
    let snap_b = tmp.path().join("snapshot-b");
    copy_dir_all(&snap_a_src, &snap_a);
    copy_dir_all(&snap_b_src, &snap_b);
    build_zips_in_dir(&snap_a);
    build_zips_in_dir(&snap_b);
    remove_zipd_dirs(&snap_a);
    remove_zipd_dirs(&snap_b);
    build_tars_in_dir(&snap_a);
    build_tars_in_dir(&snap_b);
    remove_tard_dirs(&snap_a);
    remove_tard_dirs(&snap_b);
    if let Some(f) = prepare {
        f(&snap_a, &snap_b);
    }

    let registry = registry_builder();
    let resolved = registry.resolve(&config).unwrap_or_else(|e| {
        panic!(
            "Failed to resolve plugins for {}: {e}",
            manifest.vector.name
        )
    });
    let controller = Controller::new(resolved.comparators, resolved.transformers);

    let migration = controller
        .diff(snap_a.to_str().unwrap(), snap_b.to_str().unwrap())
        .unwrap_or_else(|e| panic!("Diff failed for {}: {e}", manifest.vector.name));

    if let Some(expected) = &manifest.expected {
        check_assertions(&manifest.vector.name, &migration, expected, &config);
    }

    let mut stable_migration = migration.clone();
    stable_migration.from_snapshot = "snapshot-a".into();
    stable_migration.to_snapshot = "snapshot-b".into();
    let md = markdown::render_markdown(
        &[stable_migration.clone()],
        &markdown::MarkdownOutputterConfig::default(),
    );

    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(vector_dir.join("expected-output"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("migration", &stable_migration);
        insta::assert_snapshot!("changelog", &md);
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────

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
                outputters: default.outputters,
                output: default.output,
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

fn build_zips_in_dir(dir: &Path) {
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
        if entry.is_dir() {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".zip.d") {
                build_zips_in_dir(&entry);
                let zip_name = name.trim_end_matches(".d");
                let zip_path = dir.join(zip_name);
                create_zip_from_dir(&entry, &zip_path);
            } else {
                build_zips_in_dir(&entry);
            }
        }
    }
}

fn remove_zipd_dirs(dir: &Path) {
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
        if entry.is_dir() {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".zip.d") {
                std::fs::remove_dir_all(&entry).ok();
            } else {
                remove_zipd_dirs(&entry);
            }
        }
    }
}

fn build_tars_in_dir(dir: &Path) {
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
        if entry.is_dir() {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".tar.d") || name.ends_with(".tar.gz.d") || name.ends_with(".tgz.d") {
                build_tars_in_dir(&entry);
                let tar_name = name.trim_end_matches(".d");
                let tar_path = dir.join(tar_name);
                create_tar_from_dir(&entry, &tar_path);
            } else {
                build_tars_in_dir(&entry);
            }
        }
    }
}

fn remove_tard_dirs(dir: &Path) {
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
        if entry.is_dir() {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".tar.d") || name.ends_with(".tar.gz.d") || name.ends_with(".tgz.d") {
                std::fs::remove_dir_all(&entry).ok();
            } else {
                remove_tard_dirs(&entry);
            }
        }
    }
}

fn create_tar_from_dir(source_dir: &Path, tar_path: &Path) {
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
        add_dir_to_tar(&mut builder, source_dir, source_dir);
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
    } else {
        let mut builder = tar::Builder::new(file);
        builder.mode(tar::HeaderMode::Deterministic);
        add_dir_to_tar(&mut builder, source_dir, source_dir);
        builder.into_inner().unwrap();
    }
}

fn add_dir_to_tar<W: Write>(builder: &mut tar::Builder<W>, base: &Path, dir: &Path) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap();
        let name = rel.to_string_lossy();
        if path.is_dir()
            && (name.ends_with(".tar.d") || name.ends_with(".tar.gz.d") || name.ends_with(".tgz.d"))
        {
            continue;
        }
        if path.is_dir() {
            add_dir_to_tar(builder, base, &path);
        } else {
            builder.append_path_with_name(&path, &*name).unwrap();
        }
    }
}

fn create_zip_from_dir(source_dir: &Path, zip_path: &Path) {
    let file = std::fs::File::create(zip_path)
        .unwrap_or_else(|e| panic!("Failed to create {}: {e}", zip_path.display()));
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    add_dir_to_zip(&mut zip, source_dir, source_dir, options);
    zip.finish().unwrap();
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    dir: &Path,
    options: zip::write::SimpleFileOptions,
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
        if path.is_dir() && name.ends_with(".zip.d") {
            continue;
        }
        if path.is_dir() {
            zip.add_directory(format!("{name}/"), options).unwrap();
            add_dir_to_zip(zip, base, &path, options);
        } else {
            zip.start_file(&name, options).unwrap();
            let data = std::fs::read(&path).unwrap();
            zip.write_all(&data).unwrap();
        }
    }
}

fn check_assertions(
    name: &str,
    migration: &Migration,
    expected: &ExpectedAssertions,
    config: &DatasetConfig,
) {
    if let Some(root_kind) = &expected.root_kind {
        let root = migration.root.as_ref().unwrap_or_else(|| {
            panic!("[{name}] Expected root with kind '{root_kind}' but migration has no root")
        });
        if root.item_type == "directory" && root.kind != *root_kind {
            let child_kinds: Vec<&str> = root.children.iter().map(|c| c.kind.as_str()).collect();
            assert!(
                child_kinds.contains(&root_kind.as_str()) || root.kind == *root_kind,
                "[{name}] Expected root_kind '{root_kind}', got root.kind='{}' with child kinds: {child_kinds:?}",
                root.kind
            );
        }
    }
    if let Some(child_count) = expected.child_count {
        let root = migration.root.as_ref().unwrap_or_else(|| {
            panic!("[{name}] Expected child_count={child_count} but migration has no root")
        });
        assert_eq!(
            root.children.len(),
            child_count,
            "[{name}] Expected child_count={child_count}, got {}. Children: {:?}",
            root.children.len(),
            root.children
                .iter()
                .map(|c| (&c.kind, &c.path))
                .collect::<Vec<_>>()
        );
    }
    if let Some(has_tags) = &expected.has_tags {
        let root = migration
            .root
            .as_ref()
            .unwrap_or_else(|| panic!("[{name}] Expected tags but migration has no root"));
        let all_tags = root.all_tags();
        for tag in has_tags {
            assert!(
                all_tags.contains(tag),
                "[{name}] Expected tag '{tag}' not found. All tags in tree: {all_tags:?}"
            );
        }
    }
    if let Some(significance) = &expected.significance {
        let root = migration
            .root
            .as_ref()
            .unwrap_or_else(|| panic!("[{name}] Expected significance but migration has no root"));
        let all_tags = root.all_tags();
        let md_val = config.output.get_for_outputter("binoc.markdown");
        let md_config: markdown::MarkdownOutputterConfig =
            serde_json::from_value(md_val).unwrap_or_default();
        let sig_tags = md_config.significance.get(significance.as_str());
        assert!(
            sig_tags.is_some(),
            "[{name}] Significance category '{significance}' not in markdown outputter config"
        );
        let sig_tags = sig_tags.unwrap();
        let has_sig_tag = all_tags.iter().any(|t| sig_tags.contains(t));
        assert!(
            has_sig_tag,
            "[{name}] Expected significance '{significance}' but no matching tags. All tags: {all_tags:?}, sig_tags: {sig_tags:?}"
        );
    }
}
