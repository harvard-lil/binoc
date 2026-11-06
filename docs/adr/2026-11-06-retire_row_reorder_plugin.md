# Retire the binoc-row-reorder Plugin

**Date:** 2026-11-06
**Status:** Implemented

## Context

The [inline pure-reorder judgment ADR](2026-06-11-inline_pure_reorder_judgment.md)
kept `binoc-row-reorder` as a separate plugin because detecting a row reorder
needed a multiset scan that the tabular writer did not perform. Since then the
standard library has taken on row reorders at the layers the correspondence
engine assigns them to:

- The tabular writer explains keyed links. Under an inferred key it reports a
  `tabular.reorder_rows` edit when the keyed comparison finds no row changes.
- The `binoc.compact.sorted_row_alignment` compaction rule rewrites positional
  cell churn into a sorted-content plan, which becomes a `tabular.reorder_rows`
  edit when the rows are only re-sorted. This moved out of the writer on
  2026-07-09 (see the
  [inference disclosure ADR](2026-06-29-inference_disclosure_and_high_churn_guardrail.md)).

Both emit the `binoc.row-reorder` tag and the `tabular.reorder_rows` verb, the
same as the plugin. In the 0.3.0 bundle the plugin's only visible effect on a
pure reorder was its summary text.

The plugin also sat outside that layering. It registered its writer at index 0,
ahead of the tabular writer, so whenever it claimed a link it bypassed
configured row keys, the high-churn guardrail, and the inference disclosure that
the stdlib attaches to an inferred key. Its presence also hid one stdlib gap: with
a configured row key, a pure reorder produced no edits and fell through to
"no visible edit explained the difference".

## Decision

Remove `model-plugins/binoc-row-reorder`, its `row-reorder` CLI feature, and its
registry entry. Row reorders are the standard library's job.

Close the configured-key gap in the tabular writer: when the keyed comparison
finds no added, removed, or changed rows and some key's position differs, it
emits `tabular.reorder_rows` with `alignment: "configured_key"`.

The plugin's role as the in-tree example of a specialized writer is not
replaced. `binoc-sqlite` remains the reference for a parser plus writer.

## Alternatives Considered

**Keep the plugin and remove the stdlib detection.** The stdlib detection
covers cases the plugin does not (reorders alongside cell changes, keyed and
inferred-key alignment, guardrailed high churn) and runs inside the writer and
compaction passes that own those judgments. Removing it would move a core
tabular judgment into an optional pack.

**Keep both.** Two producers of the same tag and verb, with the plugin
pre-empting the writer, means output depends on which features are compiled in
and the plugin can hide stdlib behaviour, as it hid the configured-key gap.
