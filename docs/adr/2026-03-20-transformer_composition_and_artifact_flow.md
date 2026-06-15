# Transformer Composition and Artifact Flow

**Date:** 2026-03-20
**Status:** Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md)

> **Supersession note (2026-06-12):** the public artifact lesson survives, but
> the engine no longer composes a sequence of IR transformers over a completed
> merged tree. Parse rules publish artifacts, writers emit edit lists, and
> compaction rules rewrite those edit lists under a strict cost-decrease check.
> The CFM-40 LCS row-alignment proof also supersedes this ADR's deferred
> "sequential undo" escape hatch: a mid-table row insertion can be inferred
> after independent column reorder/addition writes without mutating either
> side into a normalized intermediate artifact.

## Context

With [versioned artifacts](2026-03-19-published_artifacts_for_cross_plugin_composition.md) and [artifact-aware transformer dispatch](2026-03-20-transformer_dispatch_refinement.md) in place, the remaining question was how to structure the analysis pipeline between comparators and transformers.

The CSV comparator previously did all analysis itself: it parsed both files, detected column changes, counted rows, computed cell diffs, and packed everything into details and tags on a single `DiffNode`. Transformers like `ColumnReorderDetector` and `RowReorderDetector` ran afterward as refinements — upgrading "modify" to "reorder" when the change was pure — but the baseline analysis was format-specific. Every new tabular comparator (Parquet, Excel, TSV) would need to replicate the same detection logic.

Artifacts made a different split possible: comparators parse and publish typed data; transformers analyze the typed data. A Parquet comparator and a CSV comparator both emit `tabular_v1` artifacts; a single set of transformers handles both.

The design question was how multiple transformers compose on the same node — a CSV file might have columns reordered *and* rows added *and* cells changed, all at once.

## Decision

### Parallel analysis

Transformers independently read the original (immutable) artifacts and annotate the node with what they observe. The `TabularAnalyzer` says "column added: 'email'; 2 rows added; 3 cells changed." The `ColumnReorderDetector` says "this is a pure column reorder." Neither modifies the underlying data. The node accumulates tags, details, and summary text from successive transformers.

This avoids data amplification (transformers read but don't write artifacts) and keeps a simple mental model: each transformer is a function from (node, artifacts) → tags/details/summary. Ordering between layers *is* load-bearing — refinement transformers dispatch on tags set by the baseline analyzer — but independent observations within a layer are order-insensitive.

The one requirement is that each transformer must be robust to concurrent changes in the data. A row addition detector analyzing data where columns are also reordered must compare by column name, not position. The `tabular_v1` format has named columns, so this works but requires care in implementation.

### The thin comparator pattern

Comparators are responsible for parsing and identity:

1. Parse both sides of the source format into the domain-neutral data shape
2. Publish artifacts (e.g. `tabular_v1` for CSV, Parquet, Excel)
3. Check logical identity — if the parsed data is equivalent, return `Identical`
4. Emit a bare `DiffNode` with action, item_type, and artifacts — no tags, no details, no summary

All semantic analysis — column changes, row counts, cell diffs, summary text — is handled by transformers that match on the artifact format.

The CSV comparator implements this pattern. A future Parquet or Excel comparator need only parse its format and publish `tabular_v1` artifacts to get the same analysis pipeline for free.

### Two-layer transformer pipeline

> **Superseded in part (2026-06-11):** the in-tree layer split described
> below was collapsed — `ColumnReorderDetector` is gone and
> `TabularAnalyzer` performs the pure-reorder upgrade inline. The
> refinement layer survives as the extension point for judgments that need
> their own scan (e.g. the out-of-tree `RowReorderDetector`). See
> [Inline Pure-Reorder Judgment](2026-06-11-inline_pure_reorder_judgment.md).

Tabular analysis uses two layers of transformers:

1. **`TabularAnalyzer`** — matches any node with `tabular_v1` artifacts. Detects column additions/removals/reorder, row additions/removals, cell changes. Sets tags (`binoc.column-addition`, `binoc.row-addition`, `binoc.cell-change`, etc.), details, and summary text. Handles add/remove nodes as well as modify.

2. **Refinement transformers** — `ColumnReorderDetector` and `RowReorderDetector` run after `TabularAnalyzer`. They read the same artifacts independently and reclassify specific patterns (e.g. upgrading a "modify" with column-reorder tag to a "reorder" action when the change is pure, or adding a `binoc.row-reorder` tag when rows are the same multiset in a different order).

Registration order matters: `TabularAnalyzer` must precede the refinement transformers. Refinement transformers dispatch on tags set by `TabularAnalyzer` (e.g. `ColumnReorderDetector` matches `binoc.column-reorder`, `RowReorderDetector` matches `binoc.cell-change`), so without `TabularAnalyzer` running first they would not be dispatched at all. This is a real ordering dependency, not a soft convention. Each refinement transformer *could* match on artifacts alone and check the condition internally, but the tag-based narrowing avoids unnecessary work on nodes where the condition is definitely absent.

The conceptual boundary between the layers is observation vs. pattern recognition. `TabularAnalyzer` enumerates cheap structural facts (which columns exist, how many rows, how many cells differ) and is always wanted. Refinement transformers verify more expensive patterns (is this a *pure* column reorder? are rows a permutation?) and are the natural extension point for third-party plugins — `RowReorderDetector` is already a separate plugin crate.

## What we're leaving open

1. **Transformers can already publish artifacts** via `data.publish_artifact()` — the door is open for derived artifacts if a use case emerges.
2. **`ArtifactFormat` versioning** handles evolution — a future `tabular_v2` could carry normalization metadata.
3. **Multiple artifacts per node** — already supported (`Vec<ArtifactDescriptor>`). A transformer could add a derived artifact alongside the originals without breaking existing consumers.
4. **`ArtifactSubject`** is an enum we can extend (e.g. `NormalizedRight`, `DiffPatch`) without breaking the existing `Left`/`Right`/`Pair` variants.

## Alternatives Considered

**Sequential undo ("back out and re-analyze").** Instead of parallel observation, each transformer could "back out" a recognized change by writing a normalized artifact for the next transformer. The column reorder transformer detects the reorder, writes a new artifact with columns in the original order, and the row addition transformer then sees clean data where only rows differ.

This would let each transformer see a simpler input, but has significant downsides:

- **Data amplification.** Each normalizing transformer writes a new copy of potentially large data to disk.
- **Ordering becomes doubly load-bearing.** Transformer ordering between layers is already load-bearing (refinement transformers dispatch on tags set by the baseline analyzer). With artifact mutation, ordering *within* a layer also becomes a correctness concern — "reorder then row-add" vs. "row-add then reorder" may produce different narratives.
- **Semantic ambiguity.** Decomposing a combined change into an ordered sequence of atomic changes is a human interpretation, not a mathematical fact. Two users may reasonably disagree about whether "reorder + rename + add rows" or "create new spreadsheet" is the right narrative.
- **API complexity.** Needs conventions for artifact supersession, provenance tracking through transformations, and careful handling of `ArtifactSubject`.

The artifact API does not prevent this model — transformers have access to `data.publish_artifact()` and `node.with_artifact()` — but we don't optimize for it or provide conventions for derived artifacts. A compelling change-decomposition use case did emerge in CFM-40: mid-table row insertion combined with column reorder/addition. The correspondence engine handles it as bounded LCS edit-list compaction (`binoc.compact.row_alignment`), so no normalized artifact subjects or supersession conventions are needed for that case.

**Analysis in the comparator.** The previous design: comparators do full analysis and emit enriched nodes with tags, details, and summary. Simpler pipeline (no dependency on transformers for basic output) but couples every format-specific parser to the downstream analysis vocabulary and forces duplication across tabular emitters.
