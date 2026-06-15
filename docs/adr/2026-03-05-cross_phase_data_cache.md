# Cross-Phase Data Cache in CompareContext

**Date:** 2026-03-05
**Status:** Superseded by [Published artifacts for cross-plugin composition](2026-03-19-published_artifacts_for_cross_plugin_composition.md) and [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md)

> **Supersession note (2026-06-12):** artifacts are still the composition
> mechanism, but they now flow through correspondence rule families rather than
> a comparator-to-transformer phase boundary.

## Problem

The architecture has a clean separation: comparators parse raw data into IR, transformers rewrite IR without raw data access. But this breaks down for semantic transformers that need parsed data to do their job correctly.

The column reorder detector is the motivating example. To determine whether a CSV change is a *pure* column reorder (same columns, same data, different order), it needs to compare the actual column values positionally. The original implementation scraped summary statistics from `DiffNode::details` (`columns_added: []`, `rows_added: 0`, `cells_changed: 0`) — if all the "change" counters were zero and the reorder tag was present, it declared a pure reorder. This worked but was fragile: it relied on the exact detail schema of the CSV comparator, and it couldn't verify that cell values actually matched after reindexing by column name.

The same problem would recur for any transformer that needs to look at actual data: a "row reorder detector" would need row contents, a "column rename detector" would need to compare values under old and new column names.

Meanwhile, the CSV comparator already parsed both files into memory during `compare`. Throwing that away and forcing the transformer to re-read from disk (or scrape details) wastes work and creates coupling.

## Decision

**`CompareContext` gains a data cache. Comparators can cache parsed data during `compare`, and transformers can read it during `transform`.**

The cache is a `HashMap<String, ReopenedData>` keyed by logical path, behind a `Mutex` for thread safety. `ReopenedData` is a format-neutral enum:

```rust
enum ReopenedData {
    Tabular(TabularDataPair),
    Text { left: Option<String>, right: Option<String> },
    Binary { left: Option<Vec<u8>>, right: Option<Vec<u8>> },
}
```

The CSV comparator calls `ctx.cache_data(logical_path, data)` after parsing. The column reorder detector calls `ctx.get_cached_data(path)` during `transform` and falls back to details-scraping if the cache is empty (which happens when the changeset was loaded from JSON rather than computed live).

The `Transformer::transform` signature changes from `fn transform(&self, node: DiffNode)` to `fn transform(&self, node: DiffNode, ctx: &CompareContext)`. Transformers that don't need data ignore the parameter.

## Why not re-read from disk?

The transformer doesn't have file paths — it only has the `DiffNode`, which contains logical paths and summary details but not physical paths. Re-reading would require either storing physical paths in the IR (leaking implementation details) or passing `ItemPair`s through the transformer pipeline (breaking the "transformers see only IR" rule). The cache sidesteps this by making data available without file access.

## Why not store full data in DiffNode::details?

Putting entire CSV contents into the `details` map was considered. This would work but:

- It balloons the serialized changeset JSON for every CSV comparison, even when no transformer or extract needs the data.
- `details` values are `serde_json::Value`, so tabular data would need to be serialized to JSON arrays and deserialized back — pointless round-tripping.
- It conflates summary metadata (for output formatting) with raw data (for semantic analysis).

The cache is ephemeral — it exists during a live diff/extract session and is not serialized. This keeps changeset JSON lean.

## Trade-offs

- **Memory:** Parsed CSV data stays in memory for the duration of the diff session. For very large CSVs, this could be significant. A future improvement could use memory-mapped files or streaming access, but for the current target (archival datasets, typically < 1GB), in-memory caching is fine.
- **Coupling:** The transformer now depends on the comparator's caching behavior. If the CSV comparator stops caching, the column reorder detector silently falls back to details-scraping. This is acceptable because the fallback exists, and both plugins are in the same stdlib crate.
- **Thread safety:** The `Mutex` on the cache is adequate because cache writes happen during the compare phase (which is parallel per-subtree but each path is written once) and reads happen during the transform phase (which is sequential).
