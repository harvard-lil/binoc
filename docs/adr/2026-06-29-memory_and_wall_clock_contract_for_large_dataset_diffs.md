# Memory and Wall-Clock Contract for Large Dataset Diffs

**Date:** 2026-06-29
**Status:** Proposed

## Context

Issue #56 asks for the performance contract binoc has been missing: whether
large public-data diffs are expected to work when the input is larger than RAM,
and what wall-clock time counts as practical for a large-to-large comparison.
This is not a new optimization by itself. It is the standard that #110, #111,
and the SEC showcase capstone are measured against.

The motivating shape is ordinary data for binoc's audience, not a stress-only
case. SEC Financial Statement and Notes monthly bundles are roughly 300 MB zip
files; the target 2026-02 -> 2026-03 showcase includes `txt.tsv` at roughly
118 MB and `num.tsv` at roughly 5 million rows. CDC BRFSS is the other current
large keyed-row reference. The replayable BRFSS showcase pair used for the
2026-07-01 remeasurement has 1,773,430 data rows per side (1,773,431 lines
including the header), 1,066,296,995 total bytes across both CSV snapshots,
and the same 9-column composite key that motivated #111.

The current implementation has two known memory-scaling sites:

- Archive and gzip expansion in
  `binoc-stdlib/src/correspondence/expand.rs` uses `read_capped`, which reads a
  whole decompressed member into `Vec<u8>` before writing it. The defaults are
  GiB-scale bomb-defense caps, so an in-cap large member can drive RSS by the
  member size and an over-cap member can allocate up to the cap before failing.
  This is the #110 target.
- Tabular writers in `binoc-stdlib/src/correspondence/writers.rs` load full
  `tabular_v1` artifacts through `load_tabular`, then keyed-row helpers such as
  `unique_rows_by_key` build side maps over fully materialized `TabularData`
  row vectors. Keyed-row comparison therefore holds both sides, plus derived
  indexes, in memory. This is the #111 target.

The measurement basis already exists. The
[CFM-44 performance ADR](2026-06-13-cfm_44_measured_correspondence_performance.md)
established the measure-first loop and the `performance-baseline` harness.
`just perf` now emits JSONL with structural metrics, wall time, phase/rule time,
CPU, and RSS. `just profile-diff LEFT RIGHT [CONFIG]` builds the profiling CLI
and records a real diff with `samply` when phase metrics need function-level
attribution. #56's "measure first" requirement is satisfied by that harness and
ADR; later Track D tasks should extend the same reports rather than inventing a
second benchmark story.

Existing measurements set the scale:

| Run | Input shape | Wall-clock | Peak RSS |
|---|---:|---:|---:|
| CFM-44 row-heavy synthetic | 61 MB total CSV input | 265 ms parallel parse | 118 MB |
| CFM-44 directory-heavy synthetic | 6,400 files per side, 13,442 nodes | 879 ms parallel parse | 69 MB |
| #56 pre-streaming SEC-shaped baseline | 36.0 MiB total tabular input | 0.863 s | 463.4 MiB |
| #56 pre-streaming SEC-shaped baseline | 145.0 MiB total tabular input | 2.022 s | 1,869.3 MiB |
| #56 streaming spike, summary-only path | 730.0 MiB total tabular input | 1.992 s | 369.5 MiB |
| CDC BRFSS keyed-row baseline (`c03c4f7`, pre-#111) | 1.07 GB total CSV input, 1.77M rows/side, 9-column key | 78.51 s | 10.3 GiB |
| CDC BRFSS keyed-row current tip (2026-07-01) | 1.07 GB total CSV input, 1.77M rows/side, 9-column key | 18.49 s | 511 MiB |

The pre-streaming SEC-shaped baseline proves the old whole-table path is not a
valid large-data posture. The streaming spike is not a guarantee for current
mainline behavior, but it proves that a bounded-memory path is practical for
SEC-scale input when the large table is not retained as one JSON artifact.

On 2026-07-01, #111 was remeasured against the replayable BRFSS showcase pair
using the same input snapshots and the same 9-column row identity on both the
pre-#111 baseline commit (`c03c4f7`) and current tip. The direct release
`binoc diff` wall-clock dropped from 78.51 s to 18.49 s, a 4.25x speedup
(76.4% less wall time), while peak memory footprint fell from 10.3 GiB to
511 MiB (95.2% lower). On current tip, `just perf` over the same snapshots
reports `binoc.write.tabular_stream` as the selected writer for the BRFSS CSV
and 11,672 ms driver wall time with a 209 MiB high-water RSS increase during
the measured run. `just profile-diff` on the same pair shows the old
materializing writer dominated the hot path through
`binoc_stdlib::correspondence::writers::write_keyed_row_edits`, while the new
path reduces writer-module leaf self samples from 5.2% to 1.8%, with the
remaining leaf time spread across streaming digest helpers
(`record_digest`, `record_key_digest`) instead of row materialization.

## Decision

Binoc's large-data contract is bounded RSS for core and standard-library paths
that claim large dataset formats. It is not "load the dataset into memory and
document a bigger machine." Users should be able to compare snapshots whose raw
or decompressed bytes exceed RAM when the requested semantics can be expressed
through streaming reads, bounded summaries, compact indexes, spill files, or
reopenable extracts.

RSS may scale with:

- the comparison tree and projected changeset,
- configured concurrency,
- bounded read/write buffers,
- small artifacts,
- compact indexes for reported or potentially changed records,
- renderer output that the caller explicitly asks to materialize.

RSS must not scale with:

- the largest decompressed archive member,
- the total decompressed size of an archive,
- both full sides of a large tabular artifact,
- every row in a large keyed-row table when only a bounded index, summary, or
  extract handle is needed.

The concrete cap for this round is:

- The SEC 2026-02 -> 2026-03 capstone must complete under **1 GiB peak RSS** on
  the reference developer-machine class used for the existing measurements.
- Full in-memory `TabularData` is a small-input implementation strategy only.
  It is acceptable when each side's tabular source is at or below **32 MiB**,
  or **64 MiB total for the pair**. Above that, stdlib tabular rules need a
  bounded path: streaming summaries, chunk/spill indexes, compact keyed indexes,
  or source reopen for extraction. This ceiling is grounded in the measured
  amplification above: 36.0 MiB total input used 463.4 MiB RSS, while 145.0 MiB
  total input used 1,869.3 MiB RSS.
- #110 must remove whole-member decompression buffers. Zip, tar, and gzip
  expansion should copy through fixed buffers and enforce caps incrementally.
- #111 may keep full `TabularData` for the small-input path, but large keyed-row
  writers must not hold both complete row vectors plus unbounded derived maps.
  The replayable CDC BRFSS showcase pair above is now the measured regression
  fixture for that work: 78.51 s / 10.3 GiB at `c03c4f7`, 18.49 s / 511 MiB on
  current tip, with `binoc.write.tabular_stream` selected by `just perf`.

The wall-clock expectation is order-of-magnitude, not a service-level
guarantee: a large-to-large semantic diff should take **minutes, not hours**.
For this round, "large" means the SEC monthly bundle shape above and the CDC
BRFSS keyed-row shape. The SEC capstone passes the contract if it completes in
**10 minutes or less** with the memory bound above. That is roughly
single-digit MiB/s end-to-end on the SEC-sized input after decompression and
semantic processing. Faster paths should be kept, but the capstone standard is
practical completion, not matching the summary-only streaming spike.

Progress reporting is out of scope for this round beyond measurement output.
`just perf` already records `wall_ms`, CPU, RSS, phase times, and structural
metrics; `just profile-diff` is the drill-down path for slow real runs. A
user-visible progress protocol needs declared units from plugins and the
controller, and adding ad hoc spinners or elapsed-time side effects would cut
against the no-global-side-effects rule. If a large diff exceeds the 10-minute
cap, the Track D response is to fix the scaling path or revise this ADR with new
measurements, not to mask the wait with progress UI.

All performance follow-ups use the existing measurement path:

- Run `just perf` for synthetic matrix or real snapshot reports.
- Run `just profile-diff LEFT RIGHT [CONFIG]` when `just perf` identifies a hot
  phase but not the function-level cause.
- Preserve structural outputs while comparing timing and RSS. Timing is noisy;
  structural metrics and changeset hashes are exact.
- Record the machine class, command, input shape, wall-clock, peak RSS, and
  relevant phase/rule time for #110, #111, and the SEC capstone.
- Do not commit large real benchmark inputs to this repo. The BRFSS replayable
  snapshots remain in the external showcase workspace; this round records the
  measured numbers but does not add a CI wall-clock threshold because the
  representative input is out-of-tree and shared-runner timing would be noisy.

## Alternatives Considered

**Document an in-memory ceiling as the product stance.** Rejected. The baseline
RSS amplification makes the ceiling too low to cover the motivating datasets,
and "use a larger workstation" is not a credible default for public archives
and multi-million-row tables.

**Promise bounded memory for every possible plugin and comparison.** Rejected.
The core can keep plugin-facing APIs serializable and streaming-friendly, but a
third-party rule can still choose to retain input. The contract applies to core
and standard-library paths that binoc presents as large-data capable.

**Require progress reporting before doing more performance work.** Rejected for
this round. Progress is useful UX, but the immediate blockers are unbounded
retention sites with known code owners and measurement commands. Adding progress
does not make an unbounded path acceptable.

**Treat the summary-only streaming spike as the capstone target.** Rejected.
It is valuable evidence that bounded RSS is feasible, but real keyed diffs,
archive expansion, extraction, rendering, and large edit-list writing do more
work than the summary-only path. The contract therefore sets a practical upper
bound and lets later measurements ratchet it down.
