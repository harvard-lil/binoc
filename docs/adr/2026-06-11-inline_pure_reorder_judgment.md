# Inline Pure-Reorder Judgment; Retire Tag-Handoff Layering

**Date:** 2026-06-11
**Status:** Implemented

## Context

The [transformer composition ADR](2026-03-20-transformer_composition_and_artifact_flow.md)
established a two-layer tabular pipeline: `TabularAnalyzer` enumerates
structural facts and sets tags; refinement transformers dispatch on those
tags and reclassify specific patterns. `ColumnReorderDetector` was the
in-tree example: it matched `binoc.column-reorder` (set only by the
analyzer), re-read the same `tabular_v1` artifacts, re-scanned every cell
to verify "pure reorder", and upgraded the action from `modify` to
`reorder`.

Three months of use showed how that handoff actually behaved:

- The analyzer's single pass already computed every fact the detector
  needed (columns added/removed, order changed, rows added/removed, cells
  changed) and discarded them; the detector's full O(cells) re-scan
  re-derived a boolean the analyzer had just thrown away.
- `binoc.column-reorder` was a single-producer/single-consumer tag *as a
  dispatch channel*: one plugin set it so that exactly one other plugin
  would wake up. That is a function call drawn slowly, with the
  registration-order dependency made load-bearing on the side.
- The detector contained a real bug enabled by the split:
  `node.tags.clear()` erased tags owned by other plugins (e.g.
  `binoc.path-change` on recompared move nodes) before re-asserting its
  own tag.

Meanwhile the other refinement transformer, `RowReorderDetector`, lived
happily out of tree as the `binoc-row-reorder` model plugin: it matches
`tabular_v1` + `binoc.cell-change` and runs its own multiset scan that the
analyzer's pass genuinely does not subsume.

## Decision

Fold the pure-reorder judgment into `TabularAnalyzer` and delete
`ColumnReorderDetector`.

- In the unkeyed path the analyzer's facts (order changed; no columns
  added/removed; no rows added/removed; no cells changed) *are* the
  detector's positional judgment, so the upgrade costs nothing extra. In
  the keyed path the facts are matched by key, not position, so the
  analyzer runs the ported positional check — only when column order
  changed with no schema change.
- The `binoc.column-reorder` tag is still emitted. Tags-as-facts stay:
  renderer groups dispatch on the tag for significance classification.
  What's gone is only the internal tag-handoff *dispatch*.
- The `column_order` extract aspect moved to `TabularAnalyzer.extract()`,
  since the extract chain routes to the last transformer that touched the
  node.
- The `tags.clear()` bug is deleted along with the detector; the upgrade
  now touches only `action` and `summary`.

Scorecard for the two extension mechanisms this pipeline has trialed:

- **Artifact dispatch as the extension point: proven.** `binoc-row-reorder`
  demonstrates the intended shape — a third-party transformer matches an
  artifact format (plus a tag as a cheap pre-filter), runs its own scan,
  and adds its own facts. Judgments that need their own pass over the data
  remain separate transformers.
- **Tag-handoff layering between stdlib transformers: retired.** One
  consumer in three months, a redundant O(cells) re-scan per dispatch, a
  load-bearing registration-order dependency, and the `clear()` bug. When
  a judgment is derivable from facts the producer already computed, it
  belongs inline in the producer.

One observable side effect: transformers that match `action: "modify"` and
ran between the analyzer and the detector (in stdlib, only
`TabularStatsAnnotator`) no longer dispatch on pure-reorder nodes, because
the action becomes `reorder` before they see it. The annotator was a no-op
on such nodes — a pure column reorder cannot change any column's
distribution — so output is unchanged.

## Alternatives Considered

**Keep the detector but fix the `clear()` bug.** Treats the symptom. The
re-scan, the ordering dependency, and the single-consumer dispatch tag all
remain for no benefit, and the next contributor reasonably copies the
two-transformer pattern for the next derivable judgment.

**Generalize: let transformers pass computed facts to later transformers
(beyond tags).** A typed inter-transformer side channel would solve the
"facts discarded, re-derived downstream" problem in general, but invents a
new ABI surface for exactly one known use. If more derivable judgments
appear, inlining them in the analyzer stays cheaper than a protocol.

**Move `RowReorderDetector` inline too.** Rejected — it needs its own
multiset scan that the analyzer's positional/keyed pass does not produce,
and it deliberately stays out of tree as the model for third-party
artifact-format consumers.
