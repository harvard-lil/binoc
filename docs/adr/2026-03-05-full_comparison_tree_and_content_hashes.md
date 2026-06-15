# Full Comparison Tree and Content Hash Propagation

**Date:** 2026-03-05
**Status:** Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md)

> **Supersession note (2026-06-12):** content hashes remain central pairing
> evidence, but the engine no longer builds a full merged comparison tree for
> transformers. It stores side trees and correspondence links, then projects the
> final changeset tree after edit writing and compaction.

## Problem

The original controller filtered identical items immediately: when a comparator returned `CompareResult::Identical`, the controller produced `None` and the item vanished from the IR tree. This caused two problems:

1. **Transformers couldn't see unchanged items.** A copy detector needs to correlate an `add` node with an existing unchanged file — but the unchanged file wasn't in the tree. Any transformer that reasons about what *didn't* change (copy detection, duplication analysis, coverage reporting) was structurally impossible.

2. **Content hashes were only available for binary files.** The binary comparator computed BLAKE3 hashes and stored them in `DiffNode::details`. But specialized comparators (text, CSV) didn't compute or propagate hashes. Move and copy detection silently failed for non-binary files: a renamed `.txt` file appeared as an unrelated add + remove.

A secondary symptom: the `trivial-identical` test vector produced `"action": "modify"` for a directory whose contents were all identical — semantically wrong, since nothing was modified.

## Decision

### 1. The IR tree includes identical nodes during transformer execution

`process_pair` always returns a `DiffNode`, never `Option<DiffNode>`. When a comparator returns `Identical`, or when the controller detects matching content hashes, it produces a node with `action: "identical"`. The tree seen by transformers is the *full comparison result*, not a delta.

After all transformers run, `prune_identical` removes `identical` nodes and any containers that become empty as a result. The final changeset is a clean delta.

This is the key invariant: **transformers see the full tree; renderers see the pruned tree.**

### 2. Expanding comparators pre-compute content hashes on Items

`Item` gains an `Option<String>` field `content_hash`. Expanding comparators (directory, zip) compute BLAKE3 hashes for all child files and attach them to Items before returning child `ItemPair`s.

The controller uses these hashes at three points:

- **Short-circuit:** If `pair.matching_content_hash()` returns `Some`, the controller produces an `identical` node without dispatching to any comparator. A comparator can opt out via `handles_identical() -> true` if it needs to process identical items (e.g., to expand an identical archive for structural visibility).
- **Propagation:** After a comparator produces a `DiffNode`, `attach_content_hashes` fills in `hash_left` / `hash_right` (or `hash` when identical) from `Item.content_hash`, using `entry().or_insert()` so comparator-set values take precedence.
- **Reuse:** The binary comparator checks `Item.content_hash` before computing its own hash, avoiding double work.

Result: every node in the tree carries content hashes regardless of which comparator produced it. Move and copy detection work for all file types.

## Alternatives considered

### Side-channel hash map instead of identical nodes in the tree

Instead of keeping identical nodes, the controller could build a separate `HashMap<String, String>` (path → hash) of all unchanged files and pass it to transformers. This avoids tree bloat for snapshots where most files are unchanged.

Rejected because: (a) the data exists either way — a hash map and a tree of identical nodes hold the same information, (b) transformers would need a different API to access the side channel, (c) the uniform tree structure is simpler to reason about, and (d) pruning is cheap (single tree walk).

### Each comparator propagates its own hashes

Instead of the controller attaching hashes from `Item.content_hash`, each comparator (text, CSV, etc.) could compute and store hashes in its own `DiffNode::details`.

Rejected because: (a) duplicates hashing work (directory already read the file to hash it), (b) requires every comparator author to remember hash propagation, (c) inconsistent key naming across comparators, and (d) the controller is the natural place for cross-cutting concerns.

### Hash at the comparator trait level (return hash from `compare`)

`CompareResult` could carry an optional hash alongside the node, with the controller always storing it.

This would work but adds complexity to the trait for something only needed when an expanding comparator pre-computed hashes. The `Item.content_hash` approach is simpler: it's metadata on the input, not a new output channel.

## Trade-offs

- **Tree size during transformer execution.** For archival snapshots where 95% of files are unchanged, the IR tree is ~20x larger than the final delta. This is transient (pruned before serialization) and the nodes are lightweight (a few strings, no file contents). If it ever matters, a lazy/streaming pruning pass could drop identical subtrees earlier.
- **Hashing cost.** The directory comparator now reads every child file to hash it, even files that will be claimed by specialized comparators that don't need the hash. For the target use case (archival datasets), the read is amortized by OS page cache (the comparator reads the same data moments later). For very large files where this matters, a future optimization could defer hashing to a background thread or make it opt-in via config.
- **`handles_identical` complexity.** Adds a method to the Comparator trait that most implementations ignore. The default is `false`, so it's zero-cost for simple comparators. The escape hatch exists for the zip-expanding-identical-archives case, which is a real (if uncommon) need.
