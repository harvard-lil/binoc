# Transformer dispatch: bottom-up by default, Root for tree-wide walkers

**Date:** 2026-04-16
**Status:** Implemented

## Context

`TransformerDescriptor` previously had a `scope` field selecting between two
traversal strategies:

- **`TransformScope::Node`** — bottom-up walk; the transformer is called on
  every matching node after its children have already been processed.
- **`TransformScope::Subtree`** — top-down walk; at the first matching
  ancestor the transformer is handed the whole subtree, and the framework
  does not recurse further.

Both `MoveDetector` and `CopyDetector` in `binoc-stdlib` used `Subtree` with
`NodeShapeFilter::Container`. This turned out to be wrong in two ways.

### Bug 1: nested matches were invisible

In `Subtree` mode, if the transformer matched a node and returned
`Unchanged`, the controller treated that as "this subtree is done" and did
not recurse. Concretely: `MoveDetector` would match the root directory, find
no add/remove pairs at root level, return `Unchanged`, and the `docs/`
subdirectory that *did* have a matching pair was never visited. `Unchanged`
was documented as "zero cost, no-op," but in `Subtree` mode it was silently
data-destructive.

### Bug 2: the detectors wanted `Node` all along

Even with Bug 1 fixed, `MoveDetector`/`CopyDetector` didn't want `Subtree`
semantics: they wanted "run me on every container, bottom-up, so inner
containers resolve their add/remove pairs first and the parent sees a clean
residual." That's exactly what `Node` provides.

### The underlying conceptual split

The two strategies expressed two different things a tree-walking transformer
might want:

1. **A visitor.** "Run me on every node that matches my descriptor." Order:
   bottom-up, so child transformations are the input to the enclosing one.
   Covers correlation, summarization, rewriting.
2. **An owner.** "When you find a subtree I match, hand it to me and stop.
   I will walk it myself." Matters when a transformer needs the *original*
   (untransformed-by-itself) shape, or wants to suppress recursive
   self-application.

The visitor covers almost every plugin. The owner shape only matters for a
narrow set of cases — most prominently hierarchical dedup (e.g., a whole
directory moved, suppress inner move detection on its contents).

## Decision

### Part 1: one default traversal strategy (bottom-up, visit every match)

`TransformScope` is removed. The controller has one default traversal:
bottom-up, visit every matching node. Concretely:

- `TransformScope` enum deleted from `binoc-sdk`.
- `scope` field and `with_scope` builder dropped from
  `TransformerDescriptor`.
- The controller's `apply_transformer` is a single bottom-up walk.

For every realistic plugin in the codebase this is a clearing operation, not
a semantic change.

### Part 2: `NodeShapeFilter::Root` for tree-wide walkers

When we came to generalize move and copy detection to work across container
boundaries (files that move out of a zip into a surrounding directory, or
whole folder renames that span nested subfolders), we needed something the
visitor semantics doesn't provide: **run exactly once, at the tree root,
and let the transformer walk the tree itself**.

Rather than reintroduce a full "owner" scope as a separate axis (which was
what this ADR originally predicted), we added a new variant to the existing
`NodeShapeFilter`:

```rust
pub enum NodeShapeFilter {
    Any,       // default
    Container, // nodes with children
    Leaf,      // nodes without children
    Root,      // the tree root — called exactly once per diff
}
```

The controller threads an `is_root` flag through its bottom-up recursion
(true only at the outermost call); `Root` matches iff that flag is set. A
transformer declaring `NodeShapeFilter::Root` sees the whole tree, exactly
once, and is responsible for its own traversal. When dispatched with `Root`,
the controller also *skips* the automatic recursion into children — the
transformer is given the tree as it is and decides what to visit.

This is strictly simpler than a separate owner/subtree axis: it reuses
machinery we already have (the shape filter), is a single variant to add,
and covers every use case we currently need. The two stdlib detectors that
use it —`CorrelationDetector` and `FolderMoveDetector`— both run once at
the root. If a future use case needs "owner at an arbitrary matching
subtree," we can add that axis then; Root handles the concrete cases we
have today.

### Part 3: per-transformer config

Related but separable: `Transformer::transform` gained a third argument:

```rust
fn transform(
    &self,
    node: DiffNode,
    data: &dyn DataAccess,
    config: &serde_json::Value,
) -> TransformResult;
```

This mirrors `Renderer::render`. The controller reads per-transformer
configuration from `DatasetConfig.transformer_config` (a `BTreeMap<String,
serde_json::Value>` keyed by transformer name) and passes the appropriate
slice into each `transform` call. Transformers that don't need
configuration simply ignore the argument; the default is
`serde_json::Value::Null`.

`FolderMoveDetector.threshold` is the first knob that uses this channel;
`CorrelationDetector` uses it for the `aggregate_many_to_many` flag. The
wire format (`plugin_abi::TransformRequest`) carries `config` across the
ABI boundary.

## Alternatives Considered

- **Keep `TransformScope` and make `Subtree` mean "`Unchanged` recurses".**
  Arguably principled, but the semantic distinction is still subtle enough
  that plugin authors pick the wrong one — the stdlib itself did.

- **Reintroduce `TransformScope::Subtree` with owner semantics for the
  correlation detectors.** The original reintroduction criteria in an
  earlier draft of this ADR. Rejected: owner-scope "pick a matching
  subtree, stop recursing" is meaningfully more general than what we need.
  For the two concrete plugins (`CorrelationDetector`, `FolderMoveDetector`)
  the matching subtree is always the root. `NodeShapeFilter::Root` is the
  right-sized primitive; a separate axis would cost a new enum and a new
  piece of conceptual surface area for no gain.

- **Top-down vs. bottom-up traversal as a separate axis** decoupled from
  `Node`/`Root`. Plausible future design — a transformer says "I want to
  see this node before/after my children are processed." Overshoots current
  needs. Bottom-up is the right default; add top-down only when a concrete
  transformer wants it.

- **Put config in `DataAccess` instead of a fourth parameter.** Rejected:
  mixes two concerns (I/O and configuration), and `DataAccess` is a
  per-session binding while config is per-transformer.

## Consequences

- Stdlib plugins that need tree-wide correlation declare
  `NodeShapeFilter::Root` and walk the tree themselves — see
  `binoc-stdlib/src/transformers/correlation_detector.rs` and
  `folder_move_detector.rs`.
- Existing plugins keep working: the new `config` parameter is the only
  breaking trait change, and the greenfield rule (AGENTS.md #9) allows it.
- Tree-wide walkers pay a full-tree walk per invocation, but they run once
  per diff — the visitor shape's O(n) amortized cost across many
  invocations doesn't apply.
- The `Root` dispatch shape is a narrow, well-typed admission that
  correlation passes are special. If a third use case appears it's already
  named.

## Reintroduction criteria for "owner" scope (if ever needed)

We'll add a full owner/subtree mode only when a concrete plugin needs
"run once at the topmost matching ancestor; `Replace`/`ReplaceMany`/`Remove`
are final; `Unchanged` means 'decline' and the controller keeps descending".
No such plugin exists today. Candidates to watch:

1. **Hierarchical dedup at arbitrary nesting.** E.g., the SQLite plugin
   wants to collapse all row-level edits into a single schema-aware
   summary — but only at the `.sqlite` node, not at arbitrary containers.
   `Root` doesn't cover this because the target isn't the root.
2. **Subtree-wide statistical rewriting that needs pre-transformation
   shape.** E.g., "if this subtree has >200 leaf changes, collapse to a
   one-line summary." Works under the default bottom-up visitor if
   children propagate counts via `details`; breaks if we need information
   destroyed by bottom-up summarization.

When either arrives with a concrete implementation in hand, add the scope
and document its interaction with `NodeShapeFilter::Root`.
