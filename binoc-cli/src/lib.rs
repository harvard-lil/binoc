use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use binoc_core::config::{DatasetConfig, PluginRegistry, ResolvedPlugins};
use binoc_core::controller::Controller;
use binoc_core::ir::Migration;
use binoc_core::output;
use binoc_core::traits::{BinocError, Outputter};

#[derive(Parser)]
#[command(name = "binoc", about = "The missing changelog for datasets")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Diff two snapshots and produce a changelog.
    Diff {
        /// Path to snapshot A (the "before" state).
        snapshot_a: PathBuf,
        /// Path to snapshot B (the "after" state).
        snapshot_b: PathBuf,
        /// Path to dataset config file (YAML). Uses defaults if not provided.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write output to a file. Format is inferred from extension (.json for
        /// raw migration, .md for markdown, etc.) or set explicitly with
        /// format:path syntax (e.g. "json:output.dat"). Repeatable.
        #[arg(long, short)]
        output: Vec<String>,
        /// Format for stdout output. Default: "markdown". Use "json" for raw
        /// migration JSON.
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Suppress stdout output. Useful with -o to only write files.
        #[arg(long, short)]
        quiet: bool,
    },
    /// Generate a human-readable changelog from one or more migrations.
    Changelog {
        /// One or more migration JSON files.
        migrations: Vec<PathBuf>,
        /// Path to dataset config file (YAML). Uses defaults if not provided.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write output to a file. Format is inferred from extension or set
        /// explicitly with format:path syntax. Repeatable.
        #[arg(long, short)]
        output: Vec<String>,
        /// Format for stdout output. Default: "markdown".
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Suppress stdout output. Useful with -o to only write files.
        #[arg(long, short)]
        quiet: bool,
    },
    /// Extract actual changed data from a migration node.
    /// Requires access to the original snapshots.
    Extract {
        /// Migration JSON file.
        migration: PathBuf,
        /// Path of the node to extract from (logical path within the diff tree).
        node: String,
        /// What to extract: rows_added, rows_removed, cells_changed,
        /// columns_added, columns_removed, diff, content, column_order, etc.
        #[arg(default_value = "content")]
        aspect: String,
        /// Override path to snapshot A. Defaults to the path stored in the migration.
        #[arg(long)]
        snapshot_a: Option<PathBuf>,
        /// Override path to snapshot B. Defaults to the path stored in the migration.
        #[arg(long)]
        snapshot_b: Option<PathBuf>,
        /// Path to dataset config file (YAML). Uses defaults if not provided.
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
    Outputter(Arc<dyn Outputter>),
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
            match resolved.outputter_for_extension(ext)? {
                Some(o) => Ok(ResolvedFormat::Outputter(o)),
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
        .outputter_by_name(name)
        .map(ResolvedFormat::Outputter)
        .ok_or_else(|| BinocError::Config(format!("unknown output format: {name}")))
}

fn render(
    format: &ResolvedFormat,
    migrations: &[Migration],
    config: &DatasetConfig,
) -> Result<String, BinocError> {
    match format {
        ResolvedFormat::Json => {
            if migrations.len() == 1 {
                output::to_json(&migrations[0]).map_err(|e| BinocError::Other(e.to_string()))
            } else {
                serde_json::to_string_pretty(&migrations)
                    .map_err(|e| BinocError::Other(e.to_string()))
            }
        }
        ResolvedFormat::Outputter(o) => {
            let outputter_config = config.output.get_for_outputter(o.name());
            o.render(migrations, &outputter_config)
        }
    }
}

fn write_outputs(
    output_specs: &[String],
    stdout_format: &str,
    quiet: bool,
    migrations: &[Migration],
    config: &DatasetConfig,
    resolved: &ResolvedPlugins,
) -> Result<(), Box<dyn std::error::Error>> {
    if !quiet {
        let fmt = resolve_format_name(stdout_format, resolved)?;
        let text = render(&fmt, migrations, config)?;
        print!("{text}");
    }

    for raw in output_specs {
        let spec = OutputSpec::parse(raw);
        let fmt = resolve_format(&spec, resolved)?;
        let text = render(&fmt, migrations, config)?;
        if let Some(parent) = spec.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&spec.path, &text)?;
    }

    Ok(())
}

/// Run the binoc CLI with the given plugin registry and command-line arguments.
///
/// This is the main entry point for the CLI, parameterized on the registry so
/// that callers (the standalone Rust binary, the Python console_script) can
/// populate it with different plugin sets before invoking the CLI.
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
            snapshot_a,
            snapshot_b,
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
                Controller::new(resolved.comparators.clone(), resolved.transformers.clone());

            let snap_a = snapshot_a.to_string_lossy().to_string();
            let snap_b = snapshot_b.to_string_lossy().to_string();

            let migration = controller.diff(&snap_a, &snap_b)?;
            let migrations = [migration];

            write_outputs(
                &output,
                &format,
                quiet,
                &migrations,
                &dataset_config,
                &resolved,
            )?;
        }
        Commands::Changelog {
            migrations: migration_paths,
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

            let mut migrations: Vec<Migration> = Vec::new();
            for path in &migration_paths {
                let data = std::fs::read_to_string(path)?;
                let m: Migration = serde_json::from_str(&data)?;
                migrations.push(m);
            }

            write_outputs(
                &output,
                &format,
                quiet,
                &migrations,
                &dataset_config,
                &resolved,
            )?;
        }
        Commands::Extract {
            migration: migration_path,
            node,
            aspect,
            snapshot_a,
            snapshot_b,
            config,
        } => {
            let data = std::fs::read_to_string(&migration_path)?;
            let migration: Migration = serde_json::from_str(&data)?;

            let snap_a = snapshot_a
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| migration.from_snapshot.clone());
            let snap_b = snapshot_b
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| migration.to_snapshot.clone());

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
            let controller = Controller::new(resolved.comparators, resolved.transformers);

            match controller.extract(&migration, &node, &aspect, &snap_a, &snap_b) {
                Ok(result) => match result {
                    binoc_core::types::ExtractResult::Text(text) => {
                        print!("{text}");
                    }
                    binoc_core::types::ExtractResult::Binary(bytes) => {
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
