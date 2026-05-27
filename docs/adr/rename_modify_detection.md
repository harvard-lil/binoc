# Rename-and-modify detection: fuzzy correlation + transformer-initiated re-dispatch

**Date:** 2026-05-13
**Status:** Implemented

## Context

`CorrelationDetector` ([binoc-stdlib/src/transformers/correlation_detector.rs](../../binoc-stdlib/src/transformers/correlation_detector.rs)) pairs `add` and `remove` leaves across the diff tree by **exact content hash**. When a file is renamed without modification, this catches the move. When a file is modified without rename, comparator dispatch handles it. When both happen at once — a CSV is renamed *and* gets a new column — neither path fires: the hashes diverge, the paths differ, and Binoc reports a separate `remove` + `add` with no surfaced relationship.

This is the most common false-negative for archivists tracking versioned datasets, where files routinely accumulate version suffixes and schema tweaks together.

## Decision

Two cooperating mechanisms.

### 1. `FuzzyCorrelationDetector` — residual leaf pairing by similarity

A new tree-wide root-scope transformer, [binoc-stdlib/src/transformers/fuzzy_correlation_detector.rs](../../binoc-stdlib/src/transformers/fuzzy_correlation_detector.rs), runs **after** `CorrelationDetector` and only looks at the `add`/`remove` leaves the exact-hash pass couldn't pair. For each surviving (remove, add) pair:

1. **Extension match** — different extensions rarely indicate the same logical file. Cheap reject.
2. **Size ratio cap** — pairs whose byte sizes differ by more than 10x are unlikely to be the same logical file. Uses `ItemRef::resolve_size` so the metadata may already be cached.
3. **Skip binary** — the first 8 KB are scanned for null bytes; we don't fuzzy-match binaries in v1.
4. **Token-set Jaccard** over the file bytes (delimited by newlines, carriage returns, commas, tabs, spaces). Greedy assignment in descending score order, threshold 0.5.

The matcher reuses the `RewritePlan` / `apply_rewrite` machinery from [binoc-stdlib/src/transformers/correlation.rs](../../binoc-stdlib/src/transformers/correlation.rs) — the same plumbing exact-hash correlation uses to atomically remove leaves and insert move nodes at the destination's parent container.

A rename-limit cap of `400` candidate pairs (matching Git's `diff.renameLimit` default) bounds worst-case `O(adds × removes)` cost.

### 2. `pending_recompare` — controller-mediated re-dispatch

The new move node represents the rename, but its content is still unknown. The fuzzy detector cannot synthesize a content diff itself: comparators are the parser, transformers are an optimization pass, and the IR enforces that boundary (see [transformer composition and artifact flow](transformer_composition_and_artifact_flow.md)).

To bridge the gap, `DiffNode` gains a transient field [`pending_recompare: Option<ItemPair>`](../../binoc-sdk/src/ir.rs). When set on a node, it signals to the controller: "*re-dispatch this `ItemPair` through the comparator pipeline and merge the result into me before the next transformer sees the tree.*" After each `apply_transformer` call, the controller walks the result, takes any `pending_recompare`, calls `process_pair`, and merges:

- `item_type`, `comparator`, `source_items`, `artifacts` — replace/extend on the host.
- `details` — merge with `entry().or_insert()` (host wins, so move-level fields like `source_path` aren't trampled).
- `tags` — union, so content-derived tags (`binoc.content-changed`, `binoc.lines-added`) appear alongside `binoc.move`/`binoc.move.modified`.
- `summary` — captured into `annotations.content_summary` (with `entry().or_insert()` so a later transformer like TabularAnalyzer can supply a richer phrasing without being overridden). The host's `summary` ("Moved from … (modified)") is preserved; renderers surface the annotation as a trailing clause.
- `children` — replaced wholesale. The merge does **not** recursively re-apply the current transformer to inflated children; today's only caller (`FuzzyCorrelationDetector`) is `NodeShapeFilter::Root`, so recursion would be a no-op. Subsequent transformers in the pipeline pick up the inflated subtree on later iterations either way. A splice-point comment in the controller marks where a non-Root transformer could opt into single-pass nested correlation if a future use case warrants it.

The result is a single `move` node that records both the rename (its `action` + `source_path`) and the content change (the merged tabular/text/whatever diff the comparator produced), feeding the rest of the pipeline (TabularAnalyzer, ColumnReorderDetector, …) the same shape they expect for an in-place modification.

`pending_recompare` follows the wire-visible-but-stripped-at-session-exit pattern from [transient fields on wire](transient_fields_on_wire.md): `#[serde(default, skip_serializing_if = "Option::is_none")]` for the ABI, cleared by `DiffNode::strip_transient` for user-facing output.

### 3. TabularAnalyzer and Markdown renderer learn the new node shape

`TabularAnalyzer` ([binoc-stdlib/src/transformers/tabular_analyzer.rs](../../binoc-stdlib/src/transformers/tabular_analyzer.rs)) treats `action == "move"` the same as `"modify"` when tabular artifacts are present, so a renamed CSV with new columns still gets `binoc.column-addition` / `binoc.schema-change` and friends. The rename summary is preserved; the column/row description is stashed in `annotations.tabular_summary` instead of overwriting "Moved from …".

The Markdown renderer ([binoc-stdlib/src/renderers/markdown.rs](../../binoc-stdlib/src/renderers/markdown.rs)) treats a `move` node with content detail — children, `annotations.tabular_summary`, or `annotations.content_summary` — as a single reportable unit: classified by the highest-significance tag, rendered as two stacked top-level bullets under the same path. The first carries the move headline ("Moved from data.csv (modified)"); the second carries the content detail ("Column added: 'email'"). Without this grouping, the move and its content diff would land in different significance sections and the rename-edit relationship would be invisible to the reader. Two flat bullets were chosen over an inline trailing clause to avoid sentence-fragment capitalization fixups, and over a nested sub-bullet to keep the renderer's output structurally flat.

## Non-goals

**M:N matching.** If a single source file is copied and both copies are then edited, fuzzy correlation reports exactly one of them as a `move` and the other as a fresh `add` — never two `move`s sharing one source. `greedy_assign` enforces this via `used_removes` / `used_adds` sets ([fuzzy_correlation_detector.rs](../../binoc-stdlib/src/transformers/fuzzy_correlation_detector.rs)). The 1:1 framing reads more naturally for the user ("renamed and modified" + "new file") than reporting the same source as the origin of two different destinations would, and avoids combinatorial assignment complexity for a case Binoc is not optimizing for.

**Binary fuzzy matching.** Deferred (see Alternatives below).

## Alternatives Considered

**Extend `CorrelationDetector` with a fuzzy second pass.** Tempting because it would share the existing `collect_and_hydrate` walk and keep all correlation logic in one place. Rejected because exact-hash and fuzzy-similarity have meaningfully different cost profiles, accuracy trade-offs, and likely-to-evolve config knobs, and main's recent split of move/copy into separate `CorrelationDetector` + `FolderMoveDetector` transformers ([transformer scope YAGNI](transformer_scope_yagni.md)) set a precedent for *narrow correlation transformers sharing a `correlation` module*. The second tree walk costs `O(tree size)` iteration — no I/O, no rehashing — so the single-walk argument doesn't recoup much.

**`TransformResult::Recompare(Box<DiffNode>, Vec<ItemPair>)` variant.** Originally proposed. Rejected because the fuzzy detector mutates a container's *children*: the rewritten tree is a mix of exact-match moves (no re-comparison needed), fuzzy-match moves (need re-comparison), and unmatched residuals (pass through). A single `TransformResult` cannot express this heterogeneous output. A per-node field decouples the request from the return value.

**Line-set Jaccard.** Simpler implementation, and the obvious first thing to try. Rejected because for structured formats — especially CSV with column additions — every line changes but most cell *values* are shared. Token-set Jaccard with newline/comma/tab/space delimiters picks up the shared cell values and produces useful similarity scores (≥0.5) on the exact case this feature targets.

**Hungarian assignment for optimal pairing.** Replace greedy assignment with optimal bipartite matching. Deferred. Greedy is enough for v1; greedy mistakes are rare in practice and any future switch is a local change inside `FuzzyCorrelationDetector::transform` that does not affect the public API.

**Fuzzy match binaries too.** Deferred. Binary diffing wants content-defined chunks or MinHash sketches, not Jaccard on raw bytes. The exact-hash pass already catches binary renames-without-modification, which is the more common case; binary rename-and-modify is a follow-up.

## Consequences

- A new transient field on `DiffNode` follows the established wire-visible-but-stripped pattern. Plugins can set `pending_recompare` from across the ABI boundary and the controller handles it transparently.
- `inflate_pending_recompares` adds one mechanism the controller has to support, but the surface area is small and self-contained — the rest of the controller is unchanged.
- The fuzzy detector is enable/disabled via plugin registration: leave it out of the registry (or set `enable: false` in its config) to opt out entirely.
- Existing test vector `abi-log` snapshots gain one extra no-op `transform` call per diff (for the new transformer). Changeset snapshots are unchanged.
