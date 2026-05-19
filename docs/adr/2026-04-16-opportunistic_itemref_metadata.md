# Opportunistic ItemRef Metadata, Transformer-Hydrated for Correlation

**Date:** 2026-04-16
**Status:** Implemented

## Context

`ItemRef` carries optional metadata fields — `content_hash`, `size`, and
`media_type` — alongside the logical path and backend handle. Before this ADR
two questions about these fields were underspecified:

1. **Are they required or optional?** The [full-tree content-hash ADR](2026-03-05-full_comparison_tree_and_content_hashes.md)
   established that expanding comparators (directory, zip) populate
   `content_hash` "as an optimization." In practice the move detector, move
   propagation to `DiffNode.details`, and the binary comparator's short-circuit
   *all* depend on hashes being present, which made "optimization" a polite
   fiction: every new expanding comparator in the workspace had to remember to
   pre-hash, and a third-party plugin that forgot would silently break move
   detection for its children.

2. **What about derived facts like size?** The binary comparator renders
   "Content changed (N bytes → M bytes)" summaries. It had a fast path that
   reused a cached `content_hash` from an upstream comparator but didn't have
   a cached size, so it set `size = 0` whenever the hash was pre-computed.
   The visible symptom was `Content changed (0 bytes → 0 bytes)` in the
   tutorial's FASTA example and anywhere else the directory comparator pre-
   hashed a file that later turned out to differ.

The underlying question was whether to make these fields *required* (every
`ItemRef` the dispatcher sees is fully hydrated, no `Option` at all) or
*truly optional, with a documented fallback*.

## Decision

Treat metadata as **opportunistic hints**, and lift hydration responsibility
to consumers via a small SDK accessor.

### 1. `ItemRef` metadata fields are optional hints

`content_hash: Option<String>`, `size: Option<u64>`, `media_type:
Option<String>`. The invariant is:

> Producers populate these fields when doing so is cheap — typically as a
> byproduct of work they were already performing (reading bytes to hash,
> sniffing MIME). Consumers must not assume presence, but may trust presence:
> when a field is set, the value accurately reflects the current bytes.

This makes the contract explicit without mandating eager I/O for backends
that can't afford it (remote, WASM, synthesized-from-`data.provide`).

### 2. `resolve_hash` / `resolve_size` on `ItemRef`

Two accessors live on `ItemRef` itself:

```rust
impl ItemRef {
    pub fn resolve_hash(&self, data: &dyn DataAccess) -> BinocResult<String>;
    pub fn resolve_size(&self, data: &dyn DataAccess) -> BinocResult<u64>;
}
```

Each returns the cached field if set, otherwise reads bytes through
`DataAccess` and computes. These replace the ad-hoc
`if item.content_hash.is_some() { ... } else { read_bytes + hash }` dance
that `BinaryComparator` was reinventing.

Consumers that need a guaranteed value call `resolve_*`. Consumers that want
only the cached value read the field directly.

### 3. `MoveDetector` hydrates from `source_items`, propagates via `details`

Previously the move detector read hashes exclusively from
`DiffNode.details["hash_left"]` / `["hash_right"]`, which the controller
attached from `ItemRef.content_hash` only when an upstream comparator had
populated it. Children whose ItemRefs lacked a hash silently fell out of
correlation.

Now the move detector walks children needing correlation (`action == "add"`
or `"remove"`), reads the appropriate side's hash from `details`, and on a
miss calls `source_items.{left,right}.resolve_hash(data)`. The result is
written back into `details` so downstream transformers read it cheaply —
hydrated hashes propagate through the same channel as comparator-set ones.
There is no second cache layer.

This mirrors the general pass-back pattern for derived node facts: mutations
happen on an owned `DiffNode` inside a transformer and survive into the next
transformer's input. `ItemRef` itself is read-only in this model; we do not
introduce interior mutability or `&mut` plumbing through
`ItemPair`/`source_items`.

### 4. `DirectoryComparator` populates `size` alongside the hash

`make_item_ref` already called `read_bytes` for hashing + MIME sniffing. It
now also caches `bytes.len()` into `ItemRef.size`, making the common-case
modify summary in the binary comparator accurate with no additional I/O.

## Alternatives Considered

**Eager hydration at registration time.** Force every `ItemRef` produced by
`data.register_local` (and peers) to hash and stat before returning.
Rejected: imposes I/O on consumers that don't care (e.g., a hypothetical
name-only directory diff), can't be met by backends that don't locally
materialize bytes, and has no clean answer for directories (no content hash)
or synthesized items from `data.provide()`.

**Leaf-emitter comparators required to attach hashes.** Every comparator
returning a leaf must ensure `details["hash_*"]` is populated. Rejected: in
practice this already happens via the controller's `attach_content_hashes`
when the upstream expand-comparator pre-hashed its children, so the
requirement really falls on expand-comparators. Formalizing it as a trait
rule (or a post-check on every `CompareResult::Leaf`) would either duplicate
work or produce silent wrong results when a plugin author forgets. Lazy
transformer-side hydration is strictly more forgiving with no real downside.

**Interior mutability on `ItemRef` fields.** Put `content_hash` behind
`OnceCell` (or similar) so any reader can cache on miss. Rejected: noisy in
serde (the fields are on-the-wire), awkward through `Clone`, and redundant
with the details-based pass-back we already use for transformers.

**Add `mtime`.** Considered and deferred. Unlike size, mtime is not a
byproduct of any operation we already perform; it requires a separate
`fs::metadata` call (or archive header parse) and has no value for backends
that synthesize content. It is also a famously unreliable proxy for "did
the content change" and would tempt comparators into shortcuts that silently
miss real changes. Will revisit when a concrete consumer appears.

## Consequences

- The move detector no longer depends on every expanding comparator
  remembering to pre-hash. A zip or tar or custom-archive comparator that
  chooses not to hash its entries is still correctly handled; the cost is
  paid at correlation time and only when move detection actually runs.
- The binary comparator's summary reports accurate byte counts for modified
  files, including those whose hashes were pre-computed by the directory
  comparator.
- The optional-hints contract is documented on `ItemRef` itself, not
  scattered across ADR prose. Plugin authors encounter the rule when they
  look at the struct.
- Hash and size computation follow the "touched once" principle: once any
  consumer (comparator, transformer, controller) resolves a value it lives
  in `DiffNode.details` (or, for upstream `ItemRef` caches, in the
  `content_hash`/`size` fields) and downstream readers reuse it.

## Scope rule

When adding a new byproduct metadata field to `ItemRef` (e.g. a cheap
checksum flavor, a compression ratio, a detected text encoding), follow the
same pattern:

1. `Option<T>` with `skip_serializing_if = "Option::is_none"`.
2. An `ItemRef::resolve_*` accessor with a clear fallback computation.
3. Populate at the natural producer site (expanding comparator, data
   backend) if and only if it is genuinely free there.
4. Consumers that need the value always call `resolve_*`, never
   `unwrap`-on-`Option` for the field directly.
