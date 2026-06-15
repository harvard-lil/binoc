use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};

use binoc_core::config::{DatasetConfig, PluginRegistry, ResolvedPlugins};
use binoc_core::controller::Controller;
use binoc_core::output;
use binoc_sdk::{BinocError, Changeset, ExtractResult, Renderer};

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
    Controller::new(binoc_stdlib::correspondence::engine_config_for_dataset_config(&config.dataset))
        .with_dataset_config(config.dataset.clone())
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
