# CFM-44 Measured Correspondence Performance

**Date:** 2026-06-13
**Status:** Implemented

## Context

CFM-44 asked whether the correspondence driver should earn back performance
lost during the migration by narrowing repeated pair proposals and by
parallelizing per-node expand/parse work. The migration tracker identified
two suspected costs: pair rules re-propose over the whole `EngineView` every
round, and expand/parse rules run serially even though each node's rule call
is independent.

Before changing the scheduling model, we added a focused run-report path and
`performance-baseline` harness. `just perf` generates deterministic
directory/CSV fixtures, runs the driver in serial and candidate modes, and
prints one JSON object per run. The report keeps input facts, deterministic
structural metrics, and noisy timing metrics in separate top-level groups so
tools can compare them differently. The test harness asserts byte-identical
projected changesets.

Measured fixtures:

- `1 x 200 x 1000`: 1 directory, 200 CSV files, 1,000 rows per file. This
  is the default `performance-baseline` fixture and stresses CPU-heavy CSV
  parse work without thousands of tiny filesystem entries.
- `80 x 20 x 25`: 80 directories, 20 CSV files per directory, 25 rows per
  file, 1,600 files per side. This stress variant exercises directory
  expansion and many small parse candidates.

Reusable report command:

```bash
just perf --groups 1 --files-per-group 200 --rows-per-file 1000
```

The same runner can point at existing snapshots:

```bash
just perf --left snapshot-a --right snapshot-b
```

Release-mode measurement command:

```bash
cargo test --release -p binoc-stdlib --test performance_baseline \
  performance_baseline_reports_driver_hotspots -- --ignored --nocapture
```

Many-small-file stress variant:

```bash
BINOC_PERF_GROUPS=80 BINOC_PERF_FILES_PER_GROUP=20 BINOC_PERF_ROWS_PER_FILE=25 \
  cargo test --release -p binoc-stdlib --test performance_baseline \
  performance_baseline_reports_driver_hotspots -- --ignored --nocapture
```

## Decision

Land guarded parallel parse execution only. Keep expansion serial and do not
change pair-rule scheduling in CFM-44.

The driver now has a measured execution mode seam:

- `ExecutionMode::Serial` for tests and measurement.
- `ExecutionMode::ParallelParse` as the default production path.

Parse jobs are collected in stable side/index order, run through rayon only
for medium-sized batches, and then applied back to the store in the original
order. Artifact publication, child insertion, diagnostics, and event
recording remain serial and deterministic. The guard is deliberately
conservative: fewer than 32 jobs stays serial to avoid rayon overhead, and
more than 1,024 jobs stays serial because the many-tiny-file fixture showed
parallel filesystem pressure can dominate.

Measured result:

| Fixture | Serial | Parallel parse | Result |
|---|---:|---:|---|
| `1 x 200 x 1000` | 79 ms | 61 ms | 23% faster overall; parse time fell from 30 ms to 8 ms. |
| `80 x 20 x 25` | 193 ms | 207 ms | Tiny-file stress did not justify expand parallelism; parse work fell from 12 ms to 7 ms but total time stayed dominated by filesystem and expansion noise. |

Both runs reported `deterministic_json_equal=true`. A normal regression test
also compares serial and parallel projected JSON on a representative fixture.

Pair scheduling is parked. On the default CPU-heavy fixture, all pair rules
together ran 28 times over 4 rounds and rounded to 0 ms of 79 ms. On the
1,600-file stress fixture they accounted for 7 ms of 193 ms serial and 6 ms
of 207 ms parallel-parse. A
dirty-set/frontier implementation would need a new pair-rule contract because
the current `PairRule::propose` receives a whole `EngineView`; filtering that
view without declared reads would either be unsound for third-party rules or
mostly skip nothing in the current rule order.

Expansion is also parked. The expand prototype showed directory expansion is
I/O-heavy and can regress when parallelized across many directories. Keeping
expand serial preserves deterministic behavior and avoids filesystem
contention until a future materialized-large-archive benchmark proves a
different case.

## Alternatives Considered

- **Parallelize expand and parse unconditionally.** Rejected. It preserved
  output determinism but regressed the many-small-file fixture and inflated
  system time.
- **Add a dirty-set pair frontier now.** Rejected. The current pair trait is
  whole-view by design; a correct frontier needs declared pair-rule read
  semantics or a new batch/snapshot protocol, which belongs with the later
  `EngineView` transit-shape work.
- **Leave all execution serial.** Rejected. Medium CPU-heavy parse batches
  show a measurable speedup with a narrow deterministic implementation.
