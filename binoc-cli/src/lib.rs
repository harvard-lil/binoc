use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use binoc_core::config::{DatasetConfig, PluginRegistry, ResolvedPlugins};
use binoc_core::controller::Controller;
use binoc_core::output;
use binoc_sdk::{BinocError, Changeset, CorrespondenceEngineConfig, ExtractResult, Renderer};

const DEFAULT_REPORT_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Parser)]
#[command(
    name = "binoc",
    about = "The missing changelog for datasets",
    long_about = "Binoc produces the missing changelog for datasets. It \
                  detects, classifies, and renders changes across ordered \
                  dataset snapshots. The CLI is porcelain over the embeddable \
                  library; see the Python API and Rust SDK reference pages \
                  for programmatic use."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Diff an ordered sequence of snapshots and produce a changelog.
    ///
    /// Runs pairwise comparisons over each consecutive snapshot pair and emits
    /// the resulting changeset sequence. Defaults to human-readable Markdown
    /// on stdout; use --format json or -o for machine-readable or multi-output
    /// rendering.
    Diff {
        /// Ordered snapshot paths. For N inputs, binoc emits N-1 pairwise
        /// diffs (A→B, B→C, ...). Must provide at least two snapshots.
        #[arg(required = true, num_args = 2..)]
        snapshots: Vec<PathBuf>,
        /// Path to a dataset config YAML file. If omitted, the registry's
        /// default config is used.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write an additional rendered output to a file. Repeatable. Accepts
        /// `format:path` (e.g. `markdown:out.md`) or a bare path whose format
        /// is inferred from the extension.
        #[arg(long, short)]
        output: Vec<String>,
        /// Renderer used for stdout. Accepts `json`, `markdown`, or any
        /// registered renderer name (the `binoc.` prefix is optional).
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Suppress stdout. Useful when every rendered output is directed to
        /// a file via -o.
        #[arg(long, short)]
        quiet: bool,
        /// Write a detailed replay trace (JSON) of the correspondence run to
        /// this path: every expand/parse/link/write/compaction step plus the
        /// final side trees and links. Requires exactly two snapshots. Convert
        /// it to an interactive HTML replay with `binoc replay`.
        #[arg(long)]
        trace: Option<PathBuf>,
    },
    /// Render a saved run trace as a self-contained HTML replay.
    ///
    /// Reads a trace JSON produced by `binoc diff --trace` and writes a single
    /// standalone HTML file that animates the comparison: the two snapshot
    /// trees growing from the bottom up, links forming between them, and the
    /// per-link edit lists building and compacting, with play/step/scrub
    /// controls and a detail inspector.
    Replay {
        /// Path to a trace JSON file produced by `binoc diff --trace`.
        trace: PathBuf,
        /// Output HTML path. Defaults to the trace path with a `.html`
        /// extension.
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// Prepare a local bug-report bundle for an imperfect diff result.
    ///
    /// Re-runs a two-snapshot diff with trace capture and writes a local
    /// directory containing the snapshots, the resolved config, the replay
    /// trace, rendered output, and version metadata. The bundle is never
    /// uploaded anywhere; its purpose is to give a user one directory they can
    /// inspect and share if they choose.
    Report {
        /// "Before" snapshot path.
        snapshot_a: PathBuf,
        /// "After" snapshot path.
        snapshot_b: PathBuf,
        /// Path to a dataset config YAML file. If omitted, the registry's
        /// default config is used.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Output directory for the report bundle. The command refuses to
        /// reuse an existing path.
        #[arg(long, short = 'd')]
        output_dir: PathBuf,
        /// Whether to copy the snapshot bytes into the bundle or merely record
        /// their original paths. `copy` is reproducible but can be large;
        /// `reference` is lighter but not self-contained.
        #[arg(long, value_enum, default_value_t = SnapshotMode::Copy)]
        snapshot_mode: SnapshotMode,
        /// Refuse to copy more than this many bytes of snapshot payload unless
        /// explicitly raised. Applies only with `--snapshot-mode copy`.
        #[arg(long, default_value_t = DEFAULT_REPORT_MAX_BYTES)]
        max_snapshot_bytes: u64,
    },
    /// Generate a human-readable changelog from one or more saved changesets.
    ///
    /// Reads each changeset JSON file and renders the combined result.
    /// Supports the same -o / --format / -q flags as `binoc diff`.
    Changelog {
        /// One or more changeset JSON files, typically produced by earlier
        /// runs of `binoc diff -o changeset.json`.
        changesets: Vec<PathBuf>,
        /// Path to a dataset config YAML file. If omitted, the registry's
        /// default config is used.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write an additional rendered output to a file. Repeatable. Accepts
        /// `format:path` or a bare path whose format is inferred from the
        /// extension.
        #[arg(long, short)]
        output: Vec<String>,
        /// Renderer used for stdout. Accepts `json`, `markdown`, or any
        /// registered renderer name (the `binoc.` prefix is optional).
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Suppress stdout.
        #[arg(long, short)]
        quiet: bool,
    },
    /// Extract actual changed data from a changeset node.
    ///
    /// Reopens both snapshots through the correspondence engine and asks the
    /// rule that owns the projected node for the requested aspect (e.g.
    /// `rows_added`, `diff`, `content`). Use this to recover data that
    /// changesets only summarize.
    Extract {
        /// Path to a changeset JSON file.
        changeset: PathBuf,
        /// Node path within the changeset (e.g. `/path/to/file.csv`).
        node: String,
        /// Named aspect of the node to extract. Which aspects are available
        /// depends on the rule that owns the projected node.
        #[arg(default_value = "content")]
        aspect: String,
        /// Override the "before" snapshot path. Defaults to the one recorded
        /// in the changeset.
        #[arg(long)]
        snapshot_a: Option<PathBuf>,
        /// Override the "after" snapshot path. Defaults to the one recorded
        /// in the changeset.
        #[arg(long)]
        snapshot_b: Option<PathBuf>,
        /// Path to a dataset config YAML file. If omitted, the registry's
        /// default config is used.
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

struct OutputSpec {
    format: Option<String>,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotMode {
    Copy,
    Reference,
}

#[derive(Debug, Serialize)]
struct ReportMetadata {
    tool: &'static str,
    version: &'static str,
    created_unix_seconds: u64,
    snapshot_mode: SnapshotMode,
    snapshot_a: ReportSnapshotMetadata,
    snapshot_b: ReportSnapshotMetadata,
    artifacts: ReportArtifacts,
}

#[derive(Debug, Serialize)]
struct ReportSnapshotMetadata {
    original_path: String,
    file_count: u64,
    total_bytes: u64,
    bundled_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReportArtifacts {
    config: &'static str,
    changeset: &'static str,
    changelog: &'static str,
    trace: &'static str,
    metadata: &'static str,
    readme: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct SnapshotStats {
    file_count: u64,
    total_bytes: u64,
}

struct ReportBundleRequest<'a> {
    output_dir: &'a Path,
    snapshot_a: &'a Path,
    snapshot_b: &'a Path,
    dataset_config: &'a DatasetConfig,
    changeset: &'a Changeset,
    changelog: &'a str,
    run_trace: &'a binoc_core::correspondence::RunTrace,
    snapshot_mode: SnapshotMode,
    max_snapshot_bytes: u64,
}

impl OutputSpec {
    fn parse(s: &str) -> Self {
        if let Some((prefix, rest)) = s.split_once(':') {
            if !prefix.is_empty()
                && !rest.is_empty()
                && !prefix.contains('/')
                && !prefix.contains('\\')
            {
                return Self {
                    format: Some(prefix.to_string()),
                    path: PathBuf::from(rest),
                };
            }
        }
        Self {
            format: None,
            path: PathBuf::from(s),
        }
    }
}

enum ResolvedFormat {
    Json,
    Renderer(Arc<dyn Renderer>),
}

fn resolve_format(
    spec: &OutputSpec,
    resolved: &ResolvedPlugins,
) -> Result<ResolvedFormat, BinocError> {
    match &spec.format {
        Some(fmt) => resolve_format_name(fmt, resolved),
        None => {
            let ext = spec.path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "json" {
                return Ok(ResolvedFormat::Json);
            }
            match resolved.renderer_for_extension(ext)? {
                Some(o) => Ok(ResolvedFormat::Renderer(o)),
                None => Err(BinocError::Config(format!(
                    "cannot infer format for .{ext}; use format:path syntax (e.g. markdown:{path})",
                    path = spec.path.display(),
                ))),
            }
        }
    }
}

fn resolve_format_name(
    name: &str,
    resolved: &ResolvedPlugins,
) -> Result<ResolvedFormat, BinocError> {
    if name == "json" {
        return Ok(ResolvedFormat::Json);
    }
    resolved
        .renderer_by_name(name)
        .map(ResolvedFormat::Renderer)
        .ok_or_else(|| BinocError::Config(format!("unknown output format: {name}")))
}

fn render(
    format: &ResolvedFormat,
    changesets: &[Changeset],
    config: &DatasetConfig,
) -> Result<String, BinocError> {
    match format {
        ResolvedFormat::Json => {
            if changesets.len() == 1 {
                output::to_json(&changesets[0]).map_err(|e| BinocError::Other(e.to_string()))
            } else {
                serde_json::to_string_pretty(&changesets)
                    .map_err(|e| BinocError::Other(e.to_string()))
            }
        }
        ResolvedFormat::Renderer(o) => {
            let renderer_config = config.output.get_for_renderer(&o.descriptor().name);
            let mut augmented = changesets.to_vec();
            for changeset in &mut augmented {
                for diagnostic in o.diagnostics(changeset, &renderer_config) {
                    changeset.push_diagnostic(diagnostic);
                }
                changeset.dedupe_and_cap_diagnostics(16);
            }
            o.render(&augmented, &renderer_config)
        }
    }
}

fn parse_changesets_json(data: &str) -> Result<Vec<Changeset>, BinocError> {
    match serde_json::from_str::<Changeset>(data) {
        Ok(changeset) => Ok(vec![changeset]),
        Err(single_err) => match serde_json::from_str::<Vec<Changeset>>(data) {
            Ok(changesets) => Ok(changesets),
            Err(seq_err) => Err(BinocError::Other(format!(
                "failed to parse changeset JSON as object ({single_err}) or array ({seq_err})"
            ))),
        },
    }
}

fn write_outputs(
    output_specs: &[String],
    stdout_format: &str,
    quiet: bool,
    changesets: &[Changeset],
    config: &DatasetConfig,
    resolved: &ResolvedPlugins,
) -> Result<(), Box<dyn std::error::Error>> {
    if !quiet {
        let fmt = resolve_format_name(stdout_format, resolved)?;
        let text = render(&fmt, changesets, config)?;
        write_stdout_text(&text)?;
    }

    for raw in output_specs {
        let spec = OutputSpec::parse(raw);
        let fmt = resolve_format(&spec, resolved)?;
        let text = render(&fmt, changesets, config)?;
        if let Some(parent) = spec.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&spec.path, &text)?;
    }

    Ok(())
}

fn write_stdout_text(text: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()
}

/// Write `bytes` to `path`, creating parent directories as needed.
fn write_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, bytes)
}

fn ensure_new_directory(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        return Err(BinocError::Config(format!(
            "report output directory already exists: {}",
            path.display()
        ))
        .into());
    }
    std::fs::create_dir_all(path)?;
    Ok(())
}

fn snapshot_stats(path: &Path) -> Result<SnapshotStats, Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(BinocError::Other(format!(
            "report bundles do not support symlinks: {}",
            path.display()
        ))
        .into());
    }
    if metadata.is_file() {
        return Ok(SnapshotStats {
            file_count: 1,
            total_bytes: metadata.len(),
        });
    }
    if metadata.is_dir() {
        let mut stats = SnapshotStats {
            file_count: 0,
            total_bytes: 0,
        };
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let child = snapshot_stats(&entry.path())?;
            stats.file_count += child.file_count;
            stats.total_bytes += child.total_bytes;
        }
        return Ok(stats);
    }
    Err(BinocError::Other(format!(
        "unsupported snapshot kind for report bundle: {}",
        path.display()
    ))
    .into())
}

fn copy_snapshot(src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(src)?;
    if metadata.file_type().is_symlink() {
        return Err(BinocError::Other(format!(
            "report bundles do not support symlinks: {}",
            src.display()
        ))
        .into());
    }
    if metadata.is_file() {
        if let Some(parent) = dst.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::copy(src, dst)?;
        return Ok(());
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_snapshot(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(());
    }
    Err(BinocError::Other(format!(
        "unsupported snapshot kind for report bundle: {}",
        src.display()
    ))
    .into())
}

fn report_readme(snapshot_mode: SnapshotMode) -> String {
    let snapshot_note = match snapshot_mode {
        SnapshotMode::Copy => {
            "This bundle includes exact copies of both snapshots under `snapshots/`, so rerunning `binoc diff` inside the bundle should reproduce the saved output."
        }
        SnapshotMode::Reference => {
            "This bundle records the original snapshot paths in `metadata.json` but does not copy snapshot bytes, so it is not self-contained."
        }
    };
    format!(
        "# Binoc bug-report bundle\n\n\
         This directory was produced by `binoc report` for a user to inspect and share manually.\n\
         Nothing in this command uploads data or opens a network connection.\n\n\
         {snapshot_note}\n\n\
         Contents:\n\n\
         - `dataset-config.yaml`: resolved dataset config used for the run\n\
         - `changeset.json`: raw changeset IR\n\
         - `changelog.md`: rendered Markdown output\n\
         - `run.trace.json`: detailed correspondence replay trace\n\
         - `metadata.json`: tool version, source paths, and bundle layout\n"
    )
}

fn write_report_bundle(request: ReportBundleRequest<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let ReportBundleRequest {
        output_dir,
        snapshot_a,
        snapshot_b,
        dataset_config,
        changeset,
        changelog,
        run_trace,
        snapshot_mode,
        max_snapshot_bytes,
    } = request;
    let stats_a = snapshot_stats(snapshot_a)?;
    let stats_b = snapshot_stats(snapshot_b)?;
    if snapshot_mode == SnapshotMode::Copy
        && stats_a.total_bytes + stats_b.total_bytes > max_snapshot_bytes
    {
        return Err(BinocError::Config(format!(
            "snapshot payload is {} bytes, above --max-snapshot-bytes {}; rerun with a higher cap or --snapshot-mode reference",
            stats_a.total_bytes + stats_b.total_bytes,
            max_snapshot_bytes
        ))
        .into());
    }

    ensure_new_directory(output_dir)?;

    let snapshot_root = output_dir.join("snapshots");
    let bundled_a = match snapshot_mode {
        SnapshotMode::Copy => {
            let path = snapshot_root.join("snapshot-a");
            copy_snapshot(snapshot_a, &path)?;
            Some("snapshots/snapshot-a".to_string())
        }
        SnapshotMode::Reference => None,
    };
    let bundled_b = match snapshot_mode {
        SnapshotMode::Copy => {
            let path = snapshot_root.join("snapshot-b");
            copy_snapshot(snapshot_b, &path)?;
            Some("snapshots/snapshot-b".to_string())
        }
        SnapshotMode::Reference => None,
    };

    write_file(
        &output_dir.join("dataset-config.yaml"),
        serde_yaml::to_string(dataset_config)?.as_bytes(),
    )?;
    write_file(
        &output_dir.join("changeset.json"),
        output::to_json(changeset)?.as_bytes(),
    )?;
    write_file(&output_dir.join("changelog.md"), changelog.as_bytes())?;
    write_file(
        &output_dir.join("run.trace.json"),
        serde_json::to_string_pretty(run_trace)?.as_bytes(),
    )?;

    let created_unix_seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let metadata = ReportMetadata {
        tool: "binoc",
        version: env!("CARGO_PKG_VERSION"),
        created_unix_seconds,
        snapshot_mode,
        snapshot_a: ReportSnapshotMetadata {
            original_path: snapshot_a.display().to_string(),
            file_count: stats_a.file_count,
            total_bytes: stats_a.total_bytes,
            bundled_path: bundled_a,
        },
        snapshot_b: ReportSnapshotMetadata {
            original_path: snapshot_b.display().to_string(),
            file_count: stats_b.file_count,
            total_bytes: stats_b.total_bytes,
            bundled_path: bundled_b,
        },
        artifacts: ReportArtifacts {
            config: "dataset-config.yaml",
            changeset: "changeset.json",
            changelog: "changelog.md",
            trace: "run.trace.json",
            metadata: "metadata.json",
            readme: "README.md",
        },
    };
    write_file(
        &output_dir.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?.as_bytes(),
    )?;
    write_file(
        &output_dir.join("README.md"),
        report_readme(snapshot_mode).as_bytes(),
    )?;

    Ok(())
}

/// Embed a run-trace JSON into the standalone replay viewer template. The JSON
/// is escaped so a `</script>` (or any `</...`) inside string data — e.g. a
/// snapshot path — cannot terminate the embedding `<script>` element; JSON
/// treats `\/` as `/`, so parsing is unaffected.
fn render_replay_html(trace_json: &str) -> String {
    const TEMPLATE: &str = include_str!("../templates/replay.html");
    let escaped = trace_json.replace("</", "<\\/");
    TEMPLATE.replace("__BINOC_TRACE_JSON__", &escaped)
}

fn resolve_renderers_only(
    registry: &PluginRegistry,
    config: &DatasetConfig,
) -> Result<ResolvedPlugins, Box<dyn std::error::Error>> {
    Ok(registry.resolve(config)?)
}

fn diff_controller(config: &DatasetConfig) -> Controller {
    Controller::new(correspondence_engine_config(config))
        .with_dataset_config(config.dataset.clone())
}

fn correspondence_engine_config(config: &DatasetConfig) -> CorrespondenceEngineConfig {
    let mut engine =
        binoc_stdlib::correspondence::engine_config_for_dataset_config(&config.dataset);
    register_bundled_correspondence_rules(&mut engine);
    engine
}

/// Register the first-party format packs that were compiled in via cargo
/// features (the fat-binoc bundle). Shared by the standalone CLI and the Python
/// host (`binoc-python`) so both honour the same in-process registration seam
/// rather than any privileged shortcut.
///
/// See docs/adr/2026-06-30-fat_binoc_distribution_and_abi_canary.md.
pub fn register_bundled_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    // Keep `config` "used" even when no format features are enabled (e.g. the
    // lean standalone CLI build with `default = []`).
    let _ = &mut *config;
    #[cfg(feature = "sqlite")]
    binoc_sqlite::register_correspondence_rules(config);
    #[cfg(feature = "excel")]
    binoc_excel::register_correspondence_rules(config);
    #[cfg(feature = "parquet")]
    binoc_parquet::register_correspondence_rules(config);
    #[cfg(feature = "avro")]
    binoc_avro::register_correspondence_rules(config);
    #[cfg(feature = "dbf")]
    binoc_dbf::register_correspondence_rules(config);
    #[cfg(feature = "xml")]
    binoc_xml::register_correspondence_rules(config);
    #[cfg(feature = "shapefile")]
    binoc_shapefile::register_correspondence_rules(config);
    #[cfg(feature = "binformats")]
    binoc_binformats::register_correspondence_rules(config);
    #[cfg(feature = "stat-binary")]
    binoc_stat_binary::register_correspondence_rules(config);
    #[cfg(feature = "row-reorder")]
    binoc_row_reorder::register_correspondence_rules(config);
}

/// Return the underlying `clap::Command` tree so external tooling (e.g. the
/// `emit-cli-markdown` binary that regenerates `docs/users/reference/cli.md`) can
/// walk it without depending on private types.
pub fn command() -> clap::Command {
    Cli::command()
}

pub fn run(
    registry: PluginRegistry,
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            e.print()?;
            if e.use_stderr() {
                std::process::exit(2);
            } else {
                std::process::exit(0);
            }
        }
    };

    match cli.command {
        Commands::Diff {
            snapshots,
            config,
            output,
            format,
            quiet,
            trace,
        } => {
            let dataset_config = match config {
                Some(path) => DatasetConfig::from_file(&path)?,
                None => DatasetConfig::default_config(),
            };

            let resolved = resolve_renderers_only(&registry, &dataset_config)?;
            let controller = diff_controller(&dataset_config);

            let snapshots: Vec<String> = snapshots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();

            let changesets = match &trace {
                Some(trace_path) => {
                    if snapshots.len() != 2 {
                        return Err(BinocError::Config(format!(
                            "--trace requires exactly two snapshots, got {}",
                            snapshots.len()
                        ))
                        .into());
                    }
                    let (changeset, mut run_trace) =
                        controller.diff_with_trace(&snapshots[0], &snapshots[1])?;
                    // Attach the final rendered changelog so the replay can show
                    // the end product the run produced.
                    let md = resolve_format_name("markdown", &resolved)
                        .ok()
                        .and_then(|fmt| {
                            render(&fmt, std::slice::from_ref(&changeset), &dataset_config).ok()
                        });
                    run_trace.output = md;
                    write_file(
                        trace_path,
                        serde_json::to_string_pretty(&run_trace)?.as_bytes(),
                    )?;
                    vec![changeset]
                }
                None => controller.diff_many(&snapshots)?,
            };

            write_outputs(
                &output,
                &format,
                quiet,
                &changesets,
                &dataset_config,
                &resolved,
            )?;
        }
        Commands::Replay { trace, output } => {
            let data = std::fs::read_to_string(&trace)?;
            // Validate the input really is a trace so bad files fail loudly
            // rather than producing an empty viewer.
            let _: binoc_core::correspondence::RunTrace =
                serde_json::from_str(&data).map_err(|e| {
                    BinocError::Other(format!("{} is not a valid run trace: {e}", trace.display()))
                })?;
            let html = render_replay_html(&data);
            let out_path = output.unwrap_or_else(|| trace.with_extension("html"));
            write_file(&out_path, html.as_bytes())?;
            if !out_path.as_os_str().is_empty() {
                eprintln!("Wrote replay to {}", out_path.display());
            }
        }
        Commands::Report {
            snapshot_a,
            snapshot_b,
            config,
            output_dir,
            snapshot_mode,
            max_snapshot_bytes,
        } => {
            let dataset_config = match config {
                Some(path) => DatasetConfig::from_file(&path)?,
                None => DatasetConfig::default_config(),
            };
            let resolved = resolve_renderers_only(&registry, &dataset_config)?;
            let controller = diff_controller(&dataset_config);
            let snapshot_a_str = snapshot_a.to_string_lossy().to_string();
            let snapshot_b_str = snapshot_b.to_string_lossy().to_string();
            let (changeset, mut run_trace) =
                controller.diff_with_trace(&snapshot_a_str, &snapshot_b_str)?;
            let markdown = render(
                &resolve_format_name("markdown", &resolved)?,
                std::slice::from_ref(&changeset),
                &dataset_config,
            )?;
            run_trace.output = Some(markdown.clone());

            write_report_bundle(ReportBundleRequest {
                output_dir: &output_dir,
                snapshot_a: &snapshot_a,
                snapshot_b: &snapshot_b,
                dataset_config: &dataset_config,
                changeset: &changeset,
                changelog: &markdown,
                run_trace: &run_trace,
                snapshot_mode,
                max_snapshot_bytes,
            })?;
            eprintln!("Wrote report bundle to {}", output_dir.display());
        }
        Commands::Changelog {
            changesets: changeset_paths,
            config,
            output,
            format,
            quiet,
        } => {
            let dataset_config = match config {
                Some(path) => DatasetConfig::from_file(&path)?,
                None => DatasetConfig::default_config(),
            };

            let resolved = resolve_renderers_only(&registry, &dataset_config)?;

            let mut changesets: Vec<Changeset> = Vec::new();
            for path in &changeset_paths {
                let data = std::fs::read_to_string(path)?;
                changesets.extend(parse_changesets_json(&data)?);
            }

            write_outputs(
                &output,
                &format,
                quiet,
                &changesets,
                &dataset_config,
                &resolved,
            )?;
        }
        Commands::Extract {
            changeset: changeset_path,
            node,
            aspect,
            snapshot_a,
            snapshot_b,
            config,
        } => {
            let data = std::fs::read_to_string(&changeset_path)?;
            let changeset: Changeset = serde_json::from_str(&data)?;

            let snap_a = snapshot_a
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| changeset.from_snapshot.clone());
            let snap_b = snapshot_b
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| changeset.to_snapshot.clone());

            if !std::path::Path::new(&snap_a).exists() {
                eprintln!("Snapshot A not found: {snap_a}");
                eprintln!("Use --snapshot-a to specify the path.");
                std::process::exit(1);
            }
            if !std::path::Path::new(&snap_b).exists() {
                eprintln!("Snapshot B not found: {snap_b}");
                eprintln!("Use --snapshot-b to specify the path.");
                std::process::exit(1);
            }

            let dataset_config = match config {
                Some(path) => DatasetConfig::from_file(&path)?,
                None => DatasetConfig::default_config(),
            };

            let controller = diff_controller(&dataset_config);

            match controller.extract(&changeset, &node, &aspect, &snap_a, &snap_b) {
                Ok(result) => match result {
                    ExtractResult::Text(text) => {
                        write_stdout_text(&text)?;
                    }
                    ExtractResult::Binary(bytes) => {
                        let mut stdout = std::io::stdout().lock();
                        stdout.write_all(&bytes)?;
                        stdout.flush()?;
                    }
                },
                Err(e) => {
                    eprintln!("Extract error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
