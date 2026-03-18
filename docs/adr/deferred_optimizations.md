# Deferred Performance Optimizations

**Date:** 2026-03-09
**Status:** Decided (not pursuing)

## Problem

The original DESIGN.md committed to three performance optimizations that assumed a different bottleneck profile than what materialized:

1. **Arena-allocated IR nodes.** "Building a tree of 100k nodes does not mean 100k individual heap allocations."
2. **Arrow as internal tabular format.** `arrow-rs` as a dependency of the CSV comparator, with zero-copy Arrow RecordBatch transfer to Python.
3. **Lazy Python tree traversal with opaque handles.** Python holds a reference to the Rust-owned tree; attribute access crosses the FFI boundary without copying.

All three were designed to minimize allocation and copying overhead in the IR layer. After building the system, the IR layer is not where the time goes.

## Analysis

### Where time actually goes

The dominant costs in a binoc diff are I/O (reading files, extracting zips), hashing (BLAKE3), content comparison (CSV cell diffing, text diffing), and move/copy detection across the item set. IR node allocation is a rounding error. A realistic large diff (thousands of changed files with CSV column changes) produces low tens of thousands of nodes — each a struct with a few strings, a small tag set, and a small details map. The tree is built once, traversed a few times (transformers, pruning, output), serialized, and dropped.

### Arena allocation

Would require `DiffNode<'arena>` lifetime annotations rippling through every type, trait, and implementation in the system: `CompareResult`, `TransformResult`, `Comparator`, `Transformer`, the controller, all stdlib comparators/transformers, the Python bindings, and serde. The rayon parallelism in `process_children` adds friction — standard arenas aren't `Send + Sync`. The builder pattern (`.with_tag()`, `.with_children()`) would need a fundamentally different API. PyO3 wrappers can't hold arena references (Python GC has its own lifetime), so you'd deep-copy at the FFI boundary anyway, nullifying the benefit for the Python use case. Estimated touch: ~20 files across all four crates.

### Arrow internals

The CSV comparator needs row-oriented access (iterate rows, compare cells at column indices). Arrow's columnar format adds complexity for no algorithmic benefit. The `arrow-rs` dependency is heavy. The claimed benefit — zero-copy transfer to Python via the `extract` verb — depends on `extract` being implemented for tabular data, which it isn't yet. If `extract` is built out and users need to pull large tabular slices into pandas, Arrow transport at that boundary would be a localized, moderate change. But using Arrow as the *internal* comparison format is the wrong tool.

### Lazy Python traversal

Would require `PyDiffNode` to hold an `Arc<Changeset>` plus a path/index into the tree, with every getter navigating from the root (O(depth) per access). The actual clone cost is negligible — a `DiffNode` is a handful of strings and small maps. The Python GC overhead of wrapper objects dwarfs the Rust clone cost. The implementation would be a full rewrite of the PyDiffNode section for marginal benefit, while making the code substantially harder to reason about.

## Decision

**Do not pursue any of these three optimizations.** The DESIGN.md has been updated to describe the actual implementations:

- IR nodes are owned structs with `Vec<DiffNode>` children. Simple, serde-compatible, rayon-compatible, PyO3-compatible.
- The CSV comparator uses the `csv` crate with string-based comparison. Arrow is noted as a potential future transport optimization at the `extract` boundary if needed.
- Python wrappers own a clone of the Rust node. Traversal uses standard Python iteration and indexing.

If node allocation ever shows up in a profile, simpler interventions (pre-sized `Vec` capacities, `SmallVec` for children) would get most of the benefit at a fraction of the cost.

## Principle

Performance claims in design docs should describe measured bottlenecks or at least the expected bottleneck profile, not prescribe implementation techniques in advance. The original doc assumed the IR layer would be hot; it isn't. The optimizations it prescribed would have made the plugin API harder to use (arena lifetimes), increased dependency weight (Arrow), and complicated the FFI layer (opaque handles) — all for parts of the system that are not on the critical path.
