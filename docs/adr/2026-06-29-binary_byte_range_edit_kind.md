# Binary Byte-Range Edit Kind

**Date:** 2026-06-29
**Status:** Proposed

## Context

Opaque binary files currently bottom out in the fallback writer. If the two
leaf hashes differ, `FallbackWriter` emits the flat fact
`binary.contents-differ`; if bounded string extraction finds printable runs, it
adds an extracted-strings projection to that same fact. This is intentionally
conservative: the BLAKE3 byte hash is the sole equality oracle, and every other
binary explanation is additive.

That leaves a gap for the long tail of opaque binaries whose changed bytes are
localized but whose strings do not explain the change. A steward may need to know
whether a 500 MiB file changed everywhere or only in a few byte ranges. Today the
edit vocabulary has no place to carry that answer.

This ADR settles the edit shape before implementation. It must respect the
renderer-ignorance contract from ADR
2026-06-29-renderer_ecosystem_ignorance: a new binary explanation must be
self-describing through `Segment` summaries and configurable tags, not a new
Markdown renderer special case.

## Decision

Add an open-vocabulary stdlib edit verb named `binary.byte_ranges_changed`.
It is a byte-range explanation for opaque binary leaves whose hashes already
differ. It is not a core enum, not a change to equality semantics, and not a
renderer-owned concept.

The edit is emitted alongside the existing `binary.contents-differ` fact:

- `binary.contents-differ` remains the durable low-information fact that the
  bytes differ.
- `binary.byte_ranges_changed` explains where the difference appears when the
  bounded byte-range analyzer can produce a useful result.
- If the analyzer declines or exhausts its configured budget, the fallback still
  reports `binary.contents-differ`.
- A byte-range result never suppresses `binary.contents-differ`, never proves
  equality, and never overrules the BLAKE3 hash check.

The edit parameters use half-open byte intervals on each side:

```json
{
  "left_size": 524288000,
  "right_size": 524288128,
  "unchanged_bytes": 524280000,
  "unchanged_ratio": 0.99998462,
  "changed_region_count": 2,
  "regions_truncated": false,
  "regions": [
    {
      "left_start": 4096,
      "left_len": 64,
      "right_start": 4096,
      "right_len": 64
    },
    {
      "left_start": 1048576,
      "left_len": 0,
      "right_start": 1048576,
      "right_len": 128
    }
  ]
}
```

Each region maps a changed left span
`[left_start, left_start + left_len)` to a changed right span
`[right_start, right_start + right_len)`. `left_len == 0` is an insertion,
`right_len == 0` is a deletion, and both non-zero is a replacement. The list is
ordered by left offset, then right offset.

`unchanged_bytes` is the count of matched bytes common to both versions after
alignment. `unchanged_ratio` is symmetric:

```text
2 * unchanged_bytes / (left_size + right_size)
```

Identical files would therefore have ratio `1.0`, but this edit is only emitted
after the hash oracle has already established that the files are not identical.
Completely unmatched files have ratio `0.0`.

`regions` is a bounded display/detail payload. When the full region list exceeds
the implementation cap, `changed_region_count` still records the full count,
`regions` carries the deterministic prefix that fit the budget, and
`regions_truncated` is `true`. Implementations may add explicitly named
diagnostic fields such as an analyzer version or budget reason, but they must not
change the meaning of these base fields.

The edit carries projection hints rather than asking renderers to learn the verb:

- `item_type`: `file`
- tags: `binoc.content-changed` and `binoc.binary-byte-range-change`
- summary: a `Segment` sequence that states the human story, for example:
  `2 changed byte ranges; 99.998% unchanged; first range left [4,096, 4,160) to
  right [4,096, 4,160)`.

The summary is built from `Segment::Text`, `Segment::Uint`, and
`Segment::Float`; renderers only format segments generically. Significance is
also generic: a renderer config may map `binoc.binary-byte-range-change` or the
broader `binoc.content-changed` tag through `tag_map`/`classify_tags`. No
renderer branch on `binary.byte_ranges_changed` is required or allowed for the
baseline Markdown story. JSON output naturally exposes the structured params.

This analyzer is scoped to the opaque fallback path only. Parsed formats already
own their semantic explanations through their expand/parse rules and edit-list
writers: CSV, text, SQLite, structured documents, archives, and other claimed
formats should not get an extra byte-range edit beneath their semantic edits.
The byte-range writer is for the unparsed long tail where the alternative is only
`binary.contents-differ`.

The implementation in `c-binary-cdc-impl` must keep the same bounding posture as
the existing strings projection, but for binary scale: deterministic output,
bounded retained regions, bounded summary size, and no requirement to hold both
large files plus an unbounded edit graph in memory. It may stream, window, or
content-define chunks internally, but its public output is the stable interval
shape above.

## Alternatives Considered

**Put byte ranges inside `binary.contents-differ`.** Rejected. That edit is the
minimal content-differ fact and is already the stable fallback story. Keeping the
byte-range analysis as a separate additive edit makes it clear that the analysis
can be absent, budget-truncated, or improved without changing the equality fact.

**Make the renderer special-case `binary.byte_ranges_changed`.** Rejected by ADR
2026-06-29-renderer_ecosystem_ignorance. The writer owns the wording through a
self-describing `Segment` summary; the renderer owns only generic typography and
tag-to-category configuration.

**Bake significance into the edit or IR.** Rejected. The IR remains openly typed
and significance-free. `binoc.binary-byte-range-change` is a categorization tag
that users may map differently per renderer configuration.

**Run byte-range analysis for every changed file, including parsed formats.**
Rejected. Parsed formats have better semantic units and should not be diluted by
raw byte offsets. The byte-range edit is reserved for opaque leaves that no
parser or domain writer claimed.

**Require an exact global binary diff.** Rejected. Exact edit minimization can be
memory-expensive and is not necessary for the steward-facing question this edit
answers. The contract is deterministic, bounded byte-range explanation after a
hash mismatch, not a canonical minimum edit script.
