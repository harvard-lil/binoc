# Provenance Tracking and the Extract Chain

**Date:** 2026-03-05
**Status:** Implemented

## Problem

Binoc can tell you *what* changed — "2 rows were added to data.csv" — but not show you the actual data. Users (archivists, data scientists) need to pull out the changed content: which rows were added, what does the text diff look like, what columns were reordered. This is the `extract` verb.

The hard question isn't "how do we read a CSV" — it's **who is responsible for formatting the extracted data?** A node in the changeset tree may have been created by one plugin and then rewritten by another. A CSV comparator produces a generic `modify` node with row/column stats. A column reorder transformer then rewrites that node to `action: "reorder"`. If you ask to extract that node, do you get CSV data (from the comparator) or a column order summary (from the transformer)?

This interacts with container nesting. A file `archive.zip/data/records.csv` was reached by the directory comparator (expanding the root), then the zip comparator (extracting the archive), then another directory comparator (expanding the extracted contents), then the CSV comparator (parsing the file). At extract time, we need to reconstruct that physical access chain from the changeset JSON alone.

## Decision

**Each DiffNode records its provenance: which comparator created it (`comparator`) and which transformers modified it (`transformed_by`, in order). The last plugin to touch a node owns its extraction.**

Concretely:

1. `DiffNode` gains two fields: `comparator: Option<String>` and `transformed_by: Vec<String>`. These are serialized into the changeset JSON.

2. Comparators implement `reopen(pair, child_path, data)` to reconstruct physical access to a child item (directory resolves a path, zip re-extracts to a temp dir). Container comparators override this; leaf comparators use the default (error).

3. Comparators and transformers implement `extract(node, aspect, data)` to format cached data for the end user. The comparator populates the cache during `compare()` (e.g., CSV comparator calls `data.store("tabular:path", ...)`) and reads it back during `extract()` via `data.load()`. Transformers do the same if they have custom extraction logic.

4. `Controller::extract()` reconstructs the scratchpad by walking the ancestor chain from root to the target node:
   - For each ancestor: call `reopen()` on its comparator to reconstruct physical access to the next level (directory → zip → directory → csv)
   - At the leaf: call `compare()` on the node's comparator to re-derive and cache the data
   - Finally: call `extract()` on the last transformer (or the comparator itself if no transformer modified the node)

The rule is simple: **whoever last touched the node understands it best and is responsible for explaining it to the user.**

## Why "last toucher" and not explicit registration?

The alternative was a separate `extract_registry` where plugins explicitly register which `(item_type, action)` combinations they can extract. We rejected this because:

- It's redundant. The transformer already declared what it matches via `match_types`/`match_tags`/`match_actions`. If it rewrites a node, it understands the node.
- It creates a coordination problem. A transformer author would need to register extraction handlers separately from the transform itself, and the two could drift out of sync.
- It doesn't handle the common case where no transformer fires. If the CSV comparator produces a `modify` node and no transformer touches it, the comparator should extract — but a registry-based approach would need the comparator to register as *both* a comparator and an extractor.

The `transformed_by` list makes this automatic: if the list is empty, the comparator extracts; otherwise, the last entry extracts.

## Why record provenance in the serialized changeset?

Extract must work on a saved changeset file, potentially on a different machine or at a later time. The changeset JSON must contain enough information to reconstruct the access chain without re-running the diff. Storing `comparator` and `transformed_by` as strings (plugin names) makes this possible — the extract command looks up the named plugins in the current registry and calls their `reopen`/`extract` methods.

This does mean extract requires the same plugin set that produced the changeset. A changeset produced with a custom BioBinoc plugin can only be extracted if BioBinoc is installed. This is acceptable — the changeset JSON itself is always readable (it's just JSON), only the `extract` verb requires the plugins.

## The reopen chain

Container comparators (directory, zip, tar) implement `reopen` to reconstruct physical access. This is distinct from `compare` — `reopen` doesn't diff anything, it just resolves a child's physical path within the container. For directories, this is trivial (join the path). For zips and tars, it re-extracts to a workspace directory via `data.workspace()`.

The chain is walked from root to target: directory → zip → directory → csv. Each `reopen` call produces an `ItemPair` pointing at the next level's physical files. At the leaf, `compare()` is called to re-derive the data. The controller then sets `source_items` on the target node with the reconstructed `ItemPair`, so `extract()` can re-parse source files directly.

## Cross-phase data sharing

The primary mechanism is `DiffNode.source_items`: the controller sets it on every node during the diff, and again during the extract chain after reconstructing physical access via the reopen walk. Transformers and extractors that need the original data re-parse it from these source references. The field is `#[serde(skip)]` (transient, not in changeset JSON) and carried explicitly in ABI request types.

For plugins where re-parsing is expensive (e.g., SQLite schema introspection) or where the cached format is genuinely more efficient than the source (e.g., Arrow IPC for large columnar data), `DataAccess::store(key, bytes)` / `load(key)` provides a filesystem-backed cache under `<data_root>/.cache/`. The cache survives across plugin boundaries — the host and a separately-compiled native plugin share the same `data_root`. This replaced the old `ReopenedData` closed enum, which couldn't be extended by third-party plugins.

## Alternatives considered

- **Re-run the diff and intercept intermediate data:** Simpler to implement (no new traits), but wasteful for large datasets and doesn't work on saved changesets.
- **Store extracted data in the changeset JSON:** Would balloon the changeset size. The whole point of extract is on-demand access.
- **Generic `Extractor` trait separate from Comparator/Transformer:** Adds a third plugin axis. The "last toucher" rule achieves the same dispatch without a new concept.
- **`reopen_data` as a separate method:** The original design had comparators implement `reopen_data(pair, ctx)` to parse leaf content into a `ReopenedData` enum. Replaced by `store()`/`load()` during the normal `compare()` call, which is more general (plugins choose their own serialization format) and works across the C ABI boundary.
