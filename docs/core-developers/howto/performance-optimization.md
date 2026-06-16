---
audience: core contributor
---

# Work on performance optimization

**Goal.** Measure Binoc performance with fixtures large enough to expose real
engine costs, then make optimizations only when the measurements show a net
win and the projected changeset remains deterministic.

This page is for core contributors changing the correspondence engine or
stdlib rule pack. For the original CFM-44 decision record, see
[CFM-44 Measured Correspondence Performance](../../adr/2026-06-13-cfm_44_measured_correspondence_performance.md).

## Ground rules

Performance work is gated by two checks:

1. **Outputs are invariant.** Serial and optimized runs must produce the same
   projected changeset. The perf report exposes a `changeset_json_hash`; hash
   changes are regressions unless the code change intentionally changes
   semantics and the test vectors are updated with that rationale.
2. **Structural metrics are exact; timing is a trend.** Compare `rounds`,
   per-rule invocations, fires, links, compaction counts, writer usage, and
   `description_cost` exactly. Treat wall time and per-rule elapsed time as
   noisy evidence that needs repeated runs or a before/after gap large enough
   to survive noise.

Do not optimize against tiny fixtures. They are useful for correctness tests,
but they hide the costs that dominate real snapshots: filesystem traversal,
hashing, parse work, and cross-tree pairing.

## Run the harness

The reusable report command emits one JSON object per execution mode:

```bash
just perf --groups 1 --files-per-group 200 --rows-per-file 1000
```

With no arguments, `just perf` uses the same default synthetic shape:

```bash
just perf
```

The same binary can measure real snapshot pairs:

```bash
just perf --left snapshot-a --right snapshot-b
```

The underlying binary is:

```bash
cargo run --release -q -p binoc-stdlib --bin perf_report -- \
  --groups 1 --files-per-group 200 --rows-per-file 1000
```

Run a single execution mode when you need attributable wall time, CPU, and RSS:

```bash
just perf --mode serial --groups 1 --files-per-group 200 --rows-per-file 1000
just perf --mode parallel_parse --groups 1 --files-per-group 200 --rows-per-file 1000
```

Use named fixture families for the standard scaling matrix:

```bash
just perf --family row-scale
just perf --family file-count-scale
just perf --family directory-scale
just perf --family fuzzy-threshold --mode serial
```

The ignored baseline test is useful when you want human-readable stderr and a
hard serial-vs-parallel equality assertion:

```bash
cargo test --release -p binoc-stdlib --test performance_baseline \
  performance_baseline_reports_driver_hotspots -- --ignored --nocapture
```

Override its fixture shape with environment variables:

```bash
BINOC_PERF_GROUPS=80 BINOC_PERF_FILES_PER_GROUP=20 BINOC_PERF_ROWS_PER_FILE=25 \
  cargo test --release -p binoc-stdlib --test performance_baseline \
  performance_baseline_reports_driver_hotspots -- --ignored --nocapture
```

The report includes `resources.user_cpu_ms`, `resources.system_cpu_ms`,
`resources.max_rss_kb`, and `resources.max_rss_delta_kb` on Unix/macOS.
`max_rss_kb` is the process high-water mark after the run; use `--mode serial`
or `--mode parallel_parse` when you need single-mode attribution.

## Drill into a hot phase with a sampling profiler

`perf_report` attributes time to *phases* (`expand_ms`, `parse_ms`, `pair_ms`)
and to each rule (`rule_elapsed_nanos`). That is enough to tell you *which phase*
dominates, but not *which function* inside it. When a phase looks pathological,
switch to a sampling profiler for function-level attribution. The two tools
compose: **`just perf` finds the hot phase; the profiler finds the hot
function.**

```bash
just profile-diff data/snapshots/foo/2025-01 data/snapshots/foo/2025-02
just profile-diff LEFT RIGHT path/to/dataset.binoc.yaml   # with a config
```

`profile-diff` builds the native `binoc-cli` under the `profiling` Cargo profile
(release optimizations plus line-table symbols, so the flame graph is readable
without distorting timings) and records it with
[samply](https://github.com/mstange/samply), which opens the Firefox Profiler
UI. Install once with `cargo install samply`.

Profile the **real CLI command**, not `perf_report`: `perf_report` always runs
the default engine config, so it cannot measure a keyed/`--config` run — the
exact path most likely to be slow on real data. Reach for sampling when phase
metrics point at one phase but not at one rule, when the cost is suspected in
shared helpers (CSV field parsing, row-key construction, changeset
serialization) that span rules, or when wall time is large but per-rule numbers
look unremarkable.

## Scaling dimensions

The synthetic fixture has three knobs:

| Dimension | Fixture control | What it stresses |
|---|---|---|
| Rows per file | `--rows-per-file` | CSV parse CPU, bytes read, row/field artifact construction. |
| Flat file count | Increase `--files-per-group` while keeping `--groups 1` | File registration, hashing, sibling scans, and whole-view pair proposal overhead. |
| Directory/group count | Increase `--groups` while holding files per group mostly constant | Directory expansion, child registration, path handling, and repeated directory-level work. |

Fuzzy rename detection needs a separate fixture family because it is driven by
unmatched remove/add candidates, not CSV row volume. The current stdlib guard
allows fuzzy scoring through 20 files per side, or 400 candidate pairs, and
skips scoring once a 21x21 candidate set would exceed that cap.

For each axis, hold the others constant and run two or three sizes. A useful
matrix is:

| Axis | Example sequence |
|---|---|
| Rows per file | `1x200x250`, `1x200x1000`, `1x200x8000` |
| Flat file count | `1x50x1000`, `1x200x250`, `1x1000x100` |
| Directory/group count | `10x20x250`, `80x20x250`, `320x20x250` |
| Fuzzy candidates | 5, 10, 20, 21, and 50 renamed text files per side |

The shorthand is `groups x files_per_group x rows_per_file`.

## Current measurements

The June 13, 2026 follow-up pass found:

| Scaling axis | Current result |
|---|---|
| Rows per file | CSV parsing scales roughly with bytes. Parallel parse stayed deterministic and improved wall time by 11-37%; the largest row-heavy run was 420 ms serial vs 265 ms parallel over 61 MB total input. |
| Flat file count | Whole-view pairing did not become the bottleneck. Pair time stayed around 0-4 ms; directory expansion and file registration/hash cost dominated many-file cases. The `1x1000x100` tiny-row run regressed under parallel parse, 329 ms serial vs 384 ms parallel, because saved parse time was outweighed by expansion and system noise. |
| Directory/group count | Expansion is the clearest current limit. At `320x20x250`, or 6,400 files per side and 13,442 total nodes, serial wall time was 937 ms with 622 ms in expand and 30 ms in pair. Parallel parse was 879 ms with 635 ms expand and 30 ms pair. |
| Fuzzy rename candidates | The fuzzy cap is effective. At 20x20 candidates, scoring and linking took about 1.3 ms; at 21x21, fuzzy scoring skipped entirely. No unbounded fuzzy-path behavior appeared. |
| Duplicate-hash renames | Whole-view scheduling was not the issue, but `HashPair` candidate selection was. A 4,000-file identical-content rename stress case initially spent 385 ms in pair rules; pre-sorted hash buckets and cursors reduced that to 13 ms with the same rounds and link counts. |
| Resource posture | No immediate memory issue showed up. A 61 MB row-heavy two-mode run reported 118 MB max RSS; a 13k-node directory-heavy two-mode run reported 69 MB max RSS. Row-heavy runs were user-CPU dominated; directory-heavy runs spent more time in system calls. |

Every serial/parallel pair in that pass produced identical changeset hashes.

## Optimization priority

Work through this order unless a new fixture clearly changes the profile:

1. **Improve the harness first.** Add single-mode execution or per-mode
   resource capture so reports can include max RSS plus user and system CPU
   without wrapping the whole two-mode process. Add named fixture families for
   row-scale, file-count-scale, directory-scale, and fuzzy-threshold runs.
   This is implemented in `perf_report`; preserve it when extending the
   harness.
2. **Prototype cheaper directory expansion and child registration.** This is
   the current top hotspot. Start with replacing depth-1 `WalkDir` use in
   `DirectoryExpand` with `std::fs::read_dir` plus a stable sort, then measure
   whether traversal, metadata, hashing, or registration is actually dominant.
   That first `read_dir` spike regressed on both flat and directory-heavy
   fixtures, so keep `WalkDir` for now. Next likely spike: deterministic
   batched hashing or registration for large sibling sets.
3. **Keep parallel parse, but tune only with the matrix.** It helps row-heavy
   CSV fixtures and is mostly harmless elsewhere, but tiny-file shapes can
   regress overall. The current `32..=1024` job guard held up under threshold
   experiments; do not change it until the named fixture matrix shows a net
   wall-time win.
4. **Keep dirty-set/frontier pairing parked.** Pair time is currently a small
   share of stdlib runs, and the current pair trait has no declared read set.
   Revisit when scheduler time becomes material or while redesigning the
   `EngineView` transit shape. If pair time spikes inside a specific rule,
   optimize that rule first, as with the duplicate-hash `HashPair` bucket fix.
5. **Keep fuzzy caps.** They are part of the performance contract, not merely a
   temporary guard. Raising the cap needs a better prefilter or a fixture that
   proves the larger candidate set is safe.

After an optimization, run the relevant matrix, the ignored baseline test, and
the normal verification loop:

```bash
just fmt
just check
just test
```
