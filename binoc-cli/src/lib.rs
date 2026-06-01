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
    /// Reopens both snapshots through the comparator chain and returns the
    /// real bytes or text for a given node and aspect (e.g. `rows_added`,
    /// `diff`, `content`). Use this to recover data that changesets only
    /// summarize.
    Extract {
        /// Path to a changeset JSON file.
        changeset: PathBuf,
        /// Node path within the changeset (e.g. `/path/to/file.csv`).
        node: String,
        /// Named aspect of the node to extract. Which aspects are available
        /// depends on the comparator that produced the node.
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
            o.render(changesets, &renderer_config)
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
        print!("{text}");
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

/// Return the underlying `clap::Command` tree so external tooling (e.g. the
/// `emit-cli-markdown` binary that regenerates `docs/reference/cli.md`) can
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
        } => {
            let dataset_config = match config {
                Some(path) => DatasetConfig::from_file(&path)?,
                None => registry.default_config(),
            };

            let resolved = registry.resolve(&dataset_config)?;
            let controller =
                Controller::new(resolved.comparators.clone(), resolved.transformers.clone())
                    .with_transformer_configs(dataset_config.transformer_config.as_map());

            let snapshots: Vec<String> = snapshots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
            let changesets = controller.diff_many(&snapshots)?;

            write_outputs(
                &output,
                &format,
                quiet,
                &changesets,
                &dataset_config,
                &resolved,
            )?;
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
                None => registry.default_config(),
            };

            let resolved = registry.resolve(&dataset_config)?;

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
                None => registry.default_config(),
            };

            let resolved = registry.resolve(&dataset_config)?;
            let controller = Controller::new(resolved.comparators, resolved.transformers)
                .with_transformer_configs(dataset_config.transformer_config.as_map());

            match controller.extract(&changeset, &node, &aspect, &snap_a, &snap_b) {
                Ok(result) => match result {
                    ExtractResult::Text(text) => {
                        print!("{text}");
                    }
                    ExtractResult::Binary(bytes) => {
                        use std::io::Write;
                        std::io::stdout().write_all(&bytes)?;
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
