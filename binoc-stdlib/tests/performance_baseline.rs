use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use binoc_core::correspondence::driver::{self, ExecutionMode, RunStats};
use binoc_core::data_access::LocalDataAccess;
use binoc_sdk::{Changeset, DataAccess};
use binoc_stdlib::correspondence::default_engine_config;

#[test]
fn parallel_parse_matches_serial_on_representative_fixture() {
    let fixture = FixtureShape {
        groups: 4,
        files_per_group: 6,
        rows_per_file: 12,
    };
    let temp = tempfile::tempdir().expect("tempdir");
    let (left, right) = write_fixture(temp.path(), fixture);

    let serial = run_fixture(&left, &right, ExecutionMode::Serial);
    let parallel = run_fixture(&left, &right, ExecutionMode::ParallelParse);

    assert_eq!(serial.json, parallel.json);
}

#[test]
#[ignore = "focused CFM-44 measurement; run with --ignored --nocapture"]
fn performance_baseline_reports_driver_hotspots() {
    let fixture = FixtureShape {
        groups: std::env::var("BINOC_PERF_GROUPS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
        files_per_group: std::env::var("BINOC_PERF_FILES_PER_GROUP")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(200),
        rows_per_file: std::env::var("BINOC_PERF_ROWS_PER_FILE")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1000),
    };
    let temp = tempfile::tempdir().expect("tempdir");
    let (left, right) = write_fixture(temp.path(), fixture);

    let serial = run_fixture(&left, &right, ExecutionMode::Serial);
    let parallel = run_fixture(&left, &right, ExecutionMode::ParallelParse);

    eprintln!(
        "performance-baseline fixture: groups={} files_per_group={} rows_per_file={} files_per_side={}",
        fixture.groups,
        fixture.files_per_group,
        fixture.rows_per_file,
        fixture.files_per_side()
    );
    report("serial", &serial);
    report("parallel_parse", &parallel);
    eprintln!(
        "deterministic_json_equal={}",
        if serial.json == parallel.json {
            "true"
        } else {
            "false"
        }
    );

    assert_eq!(serial.json, parallel.json);
}

#[derive(Debug, Clone, Copy)]
struct FixtureShape {
    groups: usize,
    files_per_group: usize,
    rows_per_file: usize,
}

impl FixtureShape {
    fn files_per_side(self) -> usize {
        self.groups * self.files_per_group
    }
}

struct RunMeasurement {
    elapsed_ms: u128,
    json: String,
    stats: RunStats,
}

fn run_fixture(left_root: &Path, right_root: &Path, execution: ExecutionMode) -> RunMeasurement {
    let data = Arc::new(
        LocalDataAccess::new_for_diff(left_root, right_root).expect("local data access for diff"),
    );
    let left = data
        .register_local(left_root, "")
        .expect("register left root");
    let right = data
        .register_local(right_root, "")
        .expect("register right root");
    let config = default_engine_config();

    let started = Instant::now();
    let run = driver::run_with_execution(&config, left, right, data.as_ref(), execution)
        .expect("correspondence run");
    let elapsed_ms = started.elapsed().as_millis();
    let mut changeset = run.project().to_changeset(
        left_root.display().to_string(),
        right_root.display().to_string(),
    );
    changeset.diagnostics.extend(run.diagnostics.clone());
    changeset.hoist_node_diagnostics();
    changeset.dedupe_and_cap_diagnostics(16);
    changeset.strip_transient();
    let json = serde_json::to_string(&stable_changeset(changeset)).expect("serialize changeset");

    RunMeasurement {
        elapsed_ms,
        json,
        stats: run.stats,
    }
}

fn stable_changeset(mut changeset: Changeset) -> Changeset {
    changeset.from_snapshot = "snapshot-a".into();
    changeset.to_snapshot = "snapshot-b".into();
    changeset
}

fn report(label: &str, run: &RunMeasurement) {
    eprintln!(
        "{label}: elapsed_ms={} pair_ms={} expand_ms={} parse_ms={} rounds={} pair_invocations={} expand_invocations={} parse_invocations={}",
        run.elapsed_ms,
        elapsed_ms_matching(&run.stats, ".pair."),
        elapsed_ms_matching(&run.stats, ".expand."),
        elapsed_ms_matching(&run.stats, ".parse."),
        run.stats.rounds,
        invocations_matching(&run.stats, ".pair."),
        invocations_matching(&run.stats, ".expand."),
        invocations_matching(&run.stats, ".parse.")
    );
    for (rule, count) in &run.stats.invocations {
        eprintln!("{label}: invocation {rule}={count}");
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

fn invocations_matching(stats: &RunStats, needle: &str) -> u64 {
    stats
        .invocations
        .iter()
        .filter(|(rule, _)| rule.contains(needle))
        .map(|(_, count)| *count)
        .sum()
}

fn write_fixture(root: &Path, shape: FixtureShape) -> (std::path::PathBuf, std::path::PathBuf) {
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

    (left, right)
}
