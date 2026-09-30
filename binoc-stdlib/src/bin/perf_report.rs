use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use binoc_core::config::DatasetConfig;
use binoc_core::correspondence::driver::{self, ExecutionMode, RunStats};
use binoc_core::data_access::LocalDataAccess;
use binoc_sdk::{Changeset, DataAccess};
use binoc_stdlib::correspondence::{default_engine_config, engine_config_for_dataset_config};
use serde::Serialize;

const REPORT_VERSION: u32 = 1;

fn main() {
    let args = Args::parse();
    let temp = tempfile::tempdir().expect("create perf tempdir");
    let dataset_config = args.dataset_config();
    for fixture in args.fixtures {
        let prepared = fixture.prepare(temp.path());
        for execution in &args.execution_modes {
            let report = run_report(&prepared, *execution, &dataset_config, args.config.as_ref());
            println!(
                "{}",
                serde_json::to_string(&report).expect("serialize run report")
            );
        }
    }
}

#[derive(Debug, Clone)]
struct Args {
    fixtures: Vec<Fixture>,
    execution_modes: Vec<ExecutionMode>,
    config: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Fixture {
    Synthetic(SyntheticFixture),
    FuzzyThreshold(FuzzyFixture),
    Paths { left: PathBuf, right: PathBuf },
}

impl Args {
    fn parse() -> Self {
        let mut groups = 1;
        let mut files_per_group = 200;
        let mut rows_per_file = 1000;
        let mut family = None;
        let mut mode = ModeSelection::Both;
        let mut left = None;
        let mut right = None;
        let mut config = None;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--family" => {
                    family = Some(parse_fixture_family(required_arg("--family", args.next())))
                }
                "--mode" => mode = parse_mode(required_arg("--mode", args.next())),
                "--groups" => groups = parse_usize_arg("--groups", args.next()),
                "--files-per-group" => {
                    files_per_group = parse_usize_arg("--files-per-group", args.next());
                }
                "--rows-per-file" => {
                    rows_per_file = parse_usize_arg("--rows-per-file", args.next());
                }
                "--left" => left = Some(PathBuf::from(required_arg("--left", args.next()))),
                "--right" => right = Some(PathBuf::from(required_arg("--right", args.next()))),
                "--config" => config = Some(PathBuf::from(required_arg("--config", args.next()))),
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("unknown argument: {arg}");
                    print_help();
                    std::process::exit(2);
                }
            }
        }

        let fixtures = match (left, right, family) {
            (Some(left), Some(right), None) => vec![Fixture::Paths { left, right }],
            (None, None, Some(family)) => family.fixtures(),
            (None, None, None) => {
                let shape = FixtureShape {
                    groups,
                    files_per_group,
                    rows_per_file,
                };
                vec![Fixture::Synthetic(SyntheticFixture::new(
                    synthetic_fixture_name(shape),
                    Some("synthetic".into()),
                    shape,
                ))]
            }
            (Some(_), Some(_), Some(_)) => {
                eprintln!("--family cannot be combined with --left/--right");
                std::process::exit(2);
            }
            _ => {
                eprintln!("--left and --right must be provided together");
                std::process::exit(2);
            }
        };
        Self {
            fixtures,
            execution_modes: mode.execution_modes(),
            config,
        }
    }

    fn dataset_config(&self) -> DatasetConfig {
        self.config
            .as_deref()
            .map(DatasetConfig::from_file)
            .transpose()
            .unwrap_or_else(|err| {
                eprintln!("failed to read --config: {err}");
                std::process::exit(2);
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy)]
enum ModeSelection {
    Both,
    Serial,
    ParallelParse,
}

impl ModeSelection {
    fn execution_modes(self) -> Vec<ExecutionMode> {
        match self {
            Self::Both => vec![ExecutionMode::Serial, ExecutionMode::ParallelParse],
            Self::Serial => vec![ExecutionMode::Serial],
            Self::ParallelParse => vec![ExecutionMode::ParallelParse],
        }
    }
}

fn parse_mode(value: String) -> ModeSelection {
    match value.as_str() {
        "both" => ModeSelection::Both,
        "serial" => ModeSelection::Serial,
        "parallel_parse" | "parallel-parse" => ModeSelection::ParallelParse,
        _ => {
            eprintln!("--mode must be one of: both, serial, parallel_parse");
            std::process::exit(2);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FixtureFamily {
    RowScale,
    FileCountScale,
    DirectoryScale,
    FuzzyThreshold,
}

impl FixtureFamily {
    fn fixtures(self) -> Vec<Fixture> {
        match self {
            Self::RowScale => synthetic_family(
                "row-scale",
                &[
                    FixtureShape::new(1, 200, 250),
                    FixtureShape::new(1, 200, 1000),
                    FixtureShape::new(1, 200, 4000),
                    FixtureShape::new(1, 200, 8000),
                ],
            ),
            Self::FileCountScale => synthetic_family(
                "file-count-scale",
                &[
                    FixtureShape::new(1, 50, 1000),
                    FixtureShape::new(1, 200, 250),
                    FixtureShape::new(1, 500, 100),
                    FixtureShape::new(1, 1000, 100),
                ],
            ),
            Self::DirectoryScale => synthetic_family(
                "directory-scale",
                &[
                    FixtureShape::new(10, 20, 250),
                    FixtureShape::new(40, 20, 250),
                    FixtureShape::new(160, 20, 250),
                    FixtureShape::new(320, 20, 250),
                ],
            ),
            Self::FuzzyThreshold => [5, 10, 20, 21, 50]
                .into_iter()
                .map(|renamed_files| {
                    Fixture::FuzzyThreshold(FuzzyFixture {
                        name: format!("fuzzy-threshold:{renamed_files}x{renamed_files}"),
                        family: "fuzzy-threshold".into(),
                        renamed_files,
                    })
                })
                .collect(),
        }
    }
}

fn synthetic_family(family: &str, shapes: &[FixtureShape]) -> Vec<Fixture> {
    shapes
        .iter()
        .copied()
        .map(|shape| {
            Fixture::Synthetic(SyntheticFixture::new(
                format!(
                    "{family}:{}x{}x{}",
                    shape.groups, shape.files_per_group, shape.rows_per_file
                ),
                Some(family.into()),
                shape,
            ))
        })
        .collect()
}

fn synthetic_fixture_name(shape: FixtureShape) -> String {
    format!(
        "synthetic:{}x{}x{}",
        shape.groups, shape.files_per_group, shape.rows_per_file
    )
}

fn parse_fixture_family(value: String) -> FixtureFamily {
    match value.as_str() {
        "row-scale" => FixtureFamily::RowScale,
        "file-count-scale" => FixtureFamily::FileCountScale,
        "directory-scale" => FixtureFamily::DirectoryScale,
        "fuzzy-threshold" => FixtureFamily::FuzzyThreshold,
        _ => {
            eprintln!(
                "--family must be one of: row-scale, file-count-scale, directory-scale, fuzzy-threshold"
            );
            std::process::exit(2);
        }
    }
}

fn parse_usize_arg(flag: &str, value: Option<String>) -> usize {
    required_arg(flag, value).parse().unwrap_or_else(|err| {
        eprintln!("{flag} must be a positive integer: {err}");
        std::process::exit(2);
    })
}

fn required_arg(flag: &str, value: Option<String>) -> String {
    value.unwrap_or_else(|| {
        eprintln!("{flag} requires a value");
        std::process::exit(2);
    })
}

fn print_help() {
    eprintln!(
        "Usage: just perf [--mode both|serial|parallel_parse] [--groups N --files-per-group N --rows-per-file N]\n       just perf --family row-scale|file-count-scale|directory-scale|fuzzy-threshold [--mode both|serial|parallel_parse]\n       just perf --left SNAPSHOT_A --right SNAPSHOT_B [--config DATASET.yaml] [--mode both|serial|parallel_parse]"
    );
}

#[derive(Debug, Clone, Copy)]
struct FixtureShape {
    groups: usize,
    files_per_group: usize,
    rows_per_file: usize,
}

impl FixtureShape {
    fn new(groups: usize, files_per_group: usize, rows_per_file: usize) -> Self {
        Self {
            groups,
            files_per_group,
            rows_per_file,
        }
    }

    fn files_per_side(self) -> usize {
        self.groups * self.files_per_group
    }
}

#[derive(Debug, Clone)]
struct SyntheticFixture {
    name: String,
    family: Option<String>,
    shape: FixtureShape,
}

impl SyntheticFixture {
    fn new(name: String, family: Option<String>, shape: FixtureShape) -> Self {
        Self {
            name,
            family,
            shape,
        }
    }
}

#[derive(Debug, Clone)]
struct FuzzyFixture {
    name: String,
    family: String,
    renamed_files: usize,
}

struct PreparedFixture {
    name: String,
    family: Option<String>,
    shape: Option<FixtureShape>,
    fuzzy_candidates_per_side: Option<usize>,
    left: PathBuf,
    right: PathBuf,
}

impl Fixture {
    fn prepare(self, temp_root: &Path) -> PreparedFixture {
        match self {
            Self::Synthetic(fixture) => {
                let root = temp_root.join(sanitize_path_component(&fixture.name));
                let (left, right) = write_synthetic_fixture(&root, fixture.shape);
                PreparedFixture {
                    name: fixture.name,
                    family: fixture.family,
                    shape: Some(fixture.shape),
                    fuzzy_candidates_per_side: None,
                    left,
                    right,
                }
            }
            Self::FuzzyThreshold(fixture) => {
                let root = temp_root.join(sanitize_path_component(&fixture.name));
                let (left, right) = write_fuzzy_threshold_fixture(&root, fixture.renamed_files);
                PreparedFixture {
                    name: fixture.name,
                    family: Some(fixture.family),
                    shape: None,
                    fuzzy_candidates_per_side: Some(fixture.renamed_files),
                    left,
                    right,
                }
            }
            Self::Paths { left, right } => PreparedFixture {
                name: "paths".into(),
                family: None,
                shape: None,
                fuzzy_candidates_per_side: None,
                left,
                right,
            },
        }
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Debug, Serialize)]
struct RunReport {
    version: u32,
    execution_mode: &'static str,
    input: InputFacts,
    structural: StructuralMetrics,
    timing: TimingMetrics,
    resources: ResourceMetrics,
    determinism: DeterminismMetrics,
}

#[derive(Debug, Serialize)]
struct InputFacts {
    fixture: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixture_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    groups: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_per_group: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rows_per_file: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_per_side: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fuzzy_candidates_per_side: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fuzzy_candidate_pairs: Option<usize>,
    left: String,
    right: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<String>,
    left_nodes: usize,
    right_nodes: usize,
    total_bytes: u64,
}

#[derive(Debug, Serialize)]
struct StructuralMetrics {
    rounds: u32,
    invocations: BTreeMap<String, u64>,
    fires: BTreeMap<String, u64>,
    suppressed: BTreeMap<String, u64>,
    fires_beneath_settled: BTreeMap<String, u64>,
    links_added: u64,
    links_upgraded: u64,
    priorities: BTreeMap<String, u32>,
    writer_used: BTreeMap<usize, std::collections::BTreeSet<String>>,
    unwritten_links: Vec<usize>,
    compaction_accepted: BTreeMap<String, u64>,
    compaction_rejected: BTreeMap<String, u64>,
    description_cost: driver::DescriptionCost,
}

#[derive(Debug, Serialize)]
struct TimingMetrics {
    wall_ms: u128,
    rule_elapsed_nanos: BTreeMap<String, u128>,
    pair_ms: u128,
    expand_ms: u128,
    parse_ms: u128,
}

#[derive(Debug, Serialize)]
struct ResourceMetrics {
    supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_cpu_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_cpu_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_rss_kb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_rss_delta_kb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct DeterminismMetrics {
    changeset_json_hash: String,
}

fn run_report(
    fixture: &PreparedFixture,
    execution: ExecutionMode,
    dataset_config: &DatasetConfig,
    config_path: Option<&PathBuf>,
) -> RunReport {
    let data = Arc::new(
        LocalDataAccess::new_for_diff(&fixture.left, &fixture.right)
            .expect("local data access for diff"),
    );
    let root_logical = root_logical_path(&fixture.left, &fixture.right);
    let left = data
        .register_local(&fixture.left, &root_logical)
        .expect("register left root");
    let right = data
        .register_local(&fixture.right, &root_logical)
        .expect("register right root");
    let mut config = if dataset_config.dataset.is_null() {
        default_engine_config()
    } else {
        engine_config_for_dataset_config(&dataset_config.dataset)
    };
    let setup_diagnostics = if let Some(configurator) = config.dataset_configurator.clone() {
        configurator
            .configure(
                &mut config,
                &dataset_config.dataset,
                &left,
                &right,
                data.as_ref(),
            )
            .expect("configure dataset semantics")
    } else {
        Vec::new()
    };

    let resource_start = resource_snapshot();
    let started = Instant::now();
    let run = driver::run_with_execution(&config, left, right, data.as_ref(), execution)
        .expect("correspondence run");
    let wall_ms = started.elapsed().as_millis();
    let resource_end = resource_snapshot();
    let description_cost = run.description_cost();
    let left_nodes = run.store.left.len();
    let right_nodes = run.store.right.len();
    let mut changeset = run.project().to_changeset(
        fixture.left.display().to_string(),
        fixture.right.display().to_string(),
    );
    changeset.diagnostics.extend(setup_diagnostics);
    changeset.diagnostics.extend(run.diagnostics);
    changeset.hoist_node_diagnostics();
    changeset.dedupe_and_cap_diagnostics(16);
    changeset.strip_transient();
    let hash = changeset_hash(stable_changeset(changeset));
    let stats = run.stats;

    RunReport {
        version: REPORT_VERSION,
        execution_mode: execution_label(execution),
        input: InputFacts {
            fixture: fixture.name.clone(),
            fixture_family: fixture.family.clone(),
            groups: fixture.shape.map(|shape| shape.groups),
            files_per_group: fixture.shape.map(|shape| shape.files_per_group),
            rows_per_file: fixture.shape.map(|shape| shape.rows_per_file),
            files_per_side: fixture.shape.map(|shape| shape.files_per_side()),
            fuzzy_candidates_per_side: fixture.fuzzy_candidates_per_side,
            fuzzy_candidate_pairs: fixture
                .fuzzy_candidates_per_side
                .map(|candidates| candidates * candidates),
            left: fixture.left.display().to_string(),
            right: fixture.right.display().to_string(),
            config: config_path.map(|path| path.display().to_string()),
            left_nodes,
            right_nodes,
            total_bytes: total_bytes(&fixture.left) + total_bytes(&fixture.right),
        },
        structural: structural_metrics(&stats, description_cost),
        timing: timing_metrics(&stats, wall_ms),
        resources: resource_metrics(resource_start, resource_end),
        determinism: DeterminismMetrics {
            changeset_json_hash: hash,
        },
    }
}

fn structural_metrics(
    stats: &RunStats,
    description_cost: driver::DescriptionCost,
) -> StructuralMetrics {
    StructuralMetrics {
        rounds: stats.rounds,
        invocations: stats.invocations.clone(),
        fires: stats.fires.clone(),
        suppressed: stats.suppressed.clone(),
        fires_beneath_settled: stats.fires_beneath_settled.clone(),
        links_added: stats.links_added,
        links_upgraded: stats.links_upgraded,
        priorities: stats.priorities.clone(),
        writer_used: stats.writer_used.clone(),
        unwritten_links: stats.unwritten_links.clone(),
        compaction_accepted: stats.compaction_accepted.clone(),
        compaction_rejected: stats.compaction_rejected.clone(),
        description_cost,
    }
}

fn timing_metrics(stats: &RunStats, wall_ms: u128) -> TimingMetrics {
    TimingMetrics {
        wall_ms,
        rule_elapsed_nanos: stats.rule_elapsed_nanos.clone(),
        pair_ms: elapsed_ms_matching(stats, ".pair."),
        expand_ms: elapsed_ms_matching(stats, ".expand."),
        parse_ms: elapsed_ms_matching(stats, ".parse."),
    }
}

fn elapsed_ms_matching(stats: &RunStats, needle: &str) -> u128 {
    stats
        .rule_elapsed_nanos
        .iter()
        .filter(|(rule, _)| rule.contains(needle))
        .map(|(_, nanos)| *nanos)
        .sum::<u128>()
        / 1_000_000
}

fn execution_label(execution: ExecutionMode) -> &'static str {
    match execution {
        ExecutionMode::Serial => "serial",
        ExecutionMode::ParallelParse => "parallel_parse",
    }
}

fn stable_changeset(mut changeset: Changeset) -> Changeset {
    changeset.from_snapshot = "snapshot-a".into();
    changeset.to_snapshot = "snapshot-b".into();
    changeset
}

fn changeset_hash(changeset: Changeset) -> String {
    let json = serde_json::to_vec(&changeset).expect("serialize stable changeset");
    blake3::hash(&json).to_hex().to_string()
}

fn total_bytes(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

fn root_logical_path(left: &Path, right: &Path) -> String {
    if left.is_dir() && right.is_dir() {
        return String::new();
    }
    right
        .file_name()
        .or_else(|| left.file_name())
        .and_then(|name| name.to_str())
        .map(binoc_sdk::escape_segment)
        .unwrap_or_default()
}

fn write_synthetic_fixture(root: &Path, shape: FixtureShape) -> (PathBuf, PathBuf) {
    let left = root.join("snapshot-a");
    let right = root.join("snapshot-b");
    fs::create_dir_all(&left).expect("left root");
    fs::create_dir_all(&right).expect("right root");

    for group in 0..shape.groups {
        let left_group = left.join(format!("group-{group:03}"));
        let right_group = right.join(format!("group-{group:03}"));
        fs::create_dir_all(&left_group).expect("left group");
        fs::create_dir_all(&right_group).expect("right group");
        for file in 0..shape.files_per_group {
            let name = format!("table-{file:03}.csv");
            let mut before = String::from("id,value,status\n");
            let mut after = String::from("id,value,status\n");
            for row in 0..shape.rows_per_file {
                before.push_str(&format!("{row},{group}-{file}-{row},old\n"));
                let status = if row == shape.rows_per_file / 2 && group % 4 == 0 && file % 5 == 0 {
                    "new"
                } else {
                    "old"
                };
                after.push_str(&format!("{row},{group}-{file}-{row},{status}\n"));
            }
            fs::write(left_group.join(&name), before).expect("left csv");
            fs::write(right_group.join(&name), after).expect("right csv");
        }
    }

    assert_eq!(shape.files_per_side(), count_files(&left));
    assert_eq!(shape.files_per_side(), count_files(&right));
    (left, right)
}

fn write_fuzzy_threshold_fixture(root: &Path, renamed_files: usize) -> (PathBuf, PathBuf) {
    let left = root.join("snapshot-a");
    let right = root.join("snapshot-b");
    fs::create_dir_all(&left).expect("left root");
    fs::create_dir_all(&right).expect("right root");

    for file in 0..renamed_files {
        let before = format!(
            "record {file:03}\nshared body for fuzzy threshold measurements\nleft marker {file:03}\n"
        );
        let after = format!(
            "record {file:03}\nshared body for fuzzy threshold measurements\nright marker {file:03}\n"
        );
        fs::write(left.join(format!("note-{file:03}.txt")), before).expect("left text");
        fs::write(right.join(format!("renamed-{file:03}.txt")), after).expect("right text");
    }

    assert_eq!(renamed_files, count_files(&left));
    assert_eq!(renamed_files, count_files(&right));
    (left, right)
}

fn count_files(root: &Path) -> usize {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .count()
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
struct ResourceSnapshot {
    user_cpu_micros: u128,
    system_cpu_micros: u128,
    max_rss_kb: u64,
}

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy)]
struct TimeVal {
    tv_sec: std::os::raw::c_long,
    tv_usec: std::os::raw::c_long,
}

#[cfg(unix)]
#[repr(C)]
struct RUsage {
    ru_utime: TimeVal,
    ru_stime: TimeVal,
    ru_maxrss: std::os::raw::c_long,
    ru_ixrss: std::os::raw::c_long,
    ru_idrss: std::os::raw::c_long,
    ru_isrss: std::os::raw::c_long,
    ru_minflt: std::os::raw::c_long,
    ru_majflt: std::os::raw::c_long,
    ru_nswap: std::os::raw::c_long,
    ru_inblock: std::os::raw::c_long,
    ru_oublock: std::os::raw::c_long,
    ru_msgsnd: std::os::raw::c_long,
    ru_msgrcv: std::os::raw::c_long,
    ru_nsignals: std::os::raw::c_long,
    ru_nvcsw: std::os::raw::c_long,
    ru_nivcsw: std::os::raw::c_long,
}

#[cfg(not(unix))]
#[derive(Debug, Clone, Copy)]
struct ResourceSnapshot;

#[cfg(unix)]
fn resource_snapshot() -> Option<ResourceSnapshot> {
    unix_resource_snapshot()
}

#[cfg(not(unix))]
fn resource_snapshot() -> Option<ResourceSnapshot> {
    None
}

#[cfg(unix)]
fn resource_metrics(
    start: Option<ResourceSnapshot>,
    end: Option<ResourceSnapshot>,
) -> ResourceMetrics {
    match (start, end) {
        (Some(start), Some(end)) => ResourceMetrics {
            supported: true,
            user_cpu_ms: Some(end.user_cpu_micros.saturating_sub(start.user_cpu_micros) / 1000),
            system_cpu_ms: Some(
                end.system_cpu_micros
                    .saturating_sub(start.system_cpu_micros)
                    / 1000,
            ),
            max_rss_kb: Some(end.max_rss_kb),
            max_rss_delta_kb: Some(end.max_rss_kb.saturating_sub(start.max_rss_kb)),
            note: Some(
                "max_rss_kb is process high-water RSS after this run; max_rss_delta_kb is the high-water increase during the measured driver run",
            ),
        },
        _ => ResourceMetrics {
            supported: false,
            user_cpu_ms: None,
            system_cpu_ms: None,
            max_rss_kb: None,
            max_rss_delta_kb: None,
            note: Some("getrusage failed"),
        },
    }
}

#[cfg(not(unix))]
fn resource_metrics(
    _start: Option<ResourceSnapshot>,
    _end: Option<ResourceSnapshot>,
) -> ResourceMetrics {
    ResourceMetrics {
        supported: false,
        user_cpu_ms: None,
        system_cpu_ms: None,
        max_rss_kb: None,
        max_rss_delta_kb: None,
        note: Some("resource reporting is currently implemented only for Unix getrusage"),
    }
}

#[cfg(unix)]
fn unix_resource_snapshot() -> Option<ResourceSnapshot> {
    unsafe extern "C" {
        fn getrusage(who: std::os::raw::c_int, usage: *mut RUsage) -> std::os::raw::c_int;
    }

    const RUSAGE_SELF: std::os::raw::c_int = 0;

    let mut usage = std::mem::MaybeUninit::<RUsage>::uninit();
    let status = unsafe { getrusage(RUSAGE_SELF, usage.as_mut_ptr()) };
    if status != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    Some(ResourceSnapshot {
        user_cpu_micros: timeval_micros(usage.ru_utime),
        system_cpu_micros: timeval_micros(usage.ru_stime),
        max_rss_kb: max_rss_kb(usage.ru_maxrss),
    })
}

#[cfg(unix)]
fn timeval_micros(value: TimeVal) -> u128 {
    let seconds: u128 = value.tv_sec.try_into().unwrap_or(0);
    let micros: u128 = value.tv_usec.try_into().unwrap_or(0);
    seconds.saturating_mul(1_000_000) + micros
}

#[cfg(all(unix, target_os = "macos"))]
fn max_rss_kb(max_rss: std::os::raw::c_long) -> u64 {
    let bytes: u64 = max_rss.try_into().unwrap_or(0);
    bytes.div_ceil(1024)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn max_rss_kb(max_rss: std::os::raw::c_long) -> u64 {
    max_rss.try_into().unwrap_or(0)
}
