# Transformer Dispatch Refinement

**Date:** 2026-03-20
**Status:** Implemented

## Context

Transformer dispatch originally relied on `match_types` — a list of `item_type` strings like `["directory", "zip_archive"]` or `["tabular"]`. This had two problems:

1. **Hardcoded vocabulary.** The move/copy detectors listed `["directory", "zip_archive"]` but missed `"tar_archive"`. Any new container comparator (from a third-party plugin or the stdlib) would be silently ignored unless someone remembered to update the list.

2. **Conflation of display label with dispatch key.** `item_type` was serving double duty: a human-readable noun for renderer fallback text ("File modified", "New tabular") and a programmatic dispatch key for transformer matching. These have different requirements — display labels should be descriptive and stable; dispatch keys should be precise and extensible.

The introduction of versioned artifact formats (`ArtifactFormat`) provided a structured, cross-plugin mechanism for data-consuming transformers. But structural transformers (move/copy detection) don't consume artifacts — they inspect child metadata.

## Decision

### `item_type` is a display label, not a dispatch key

`item_type` remains on `DiffNode` as a human-readable string used by renderers for fallback descriptions. It is not the primary mechanism for transformer dispatch.

### New dispatch dimensions

`TransformerDescriptor` gains two new fields alongside the existing `match_types`, `match_tags`, and `match_actions`:

- **`match_artifacts: Vec<ArtifactFormat>`** — the transformer matches nodes that have all listed artifact formats. This replaces `match_types` for data-consuming transformers (column reorder, row reorder). Renamed from the initial `require_artifacts` to align with the `match_*` naming convention.

- **`node_shape: NodeShapeFilter`** — `Any` (default), `Container` (only nodes with children), or `Leaf` (only childless nodes). This replaces `match_types: ["directory", "zip_archive"]` for structural transformers (move/copy detection).

### Matching semantics: fields are AND, values within fields are OR

All descriptor fields are combined with AND — every non-empty field must pass for the transformer to match. Within each list field, values are combined with OR — any single value satisfying the field is enough. Empty/default fields are unconstrained (always pass). A descriptor with all fields empty/default matches nothing.

Concretely, a transformer matches a node when:

```
(node_shape == Any, OR node shape matches)
AND (match_artifacts is empty, OR node has any listed artifact format)
AND (match_types is empty, OR node.item_type matches any listed type)
AND (match_tags is empty, OR node has any listed tag)
AND (match_actions is empty, OR node.action matches any listed action)
AND (at least one field is non-default)
```

This means the column reorder detector can declare `match_artifacts: [tabular_v1()], match_tags: ["binoc.column-reorder"]` — "tabular nodes that also have the column-reorder tag" — and the controller will skip nodes that lack either, avoiding unnecessary ABI round-trips.

### `match_types` is retained but de-emphasized

`match_types` still works for backwards compatibility and for cases where a transformer genuinely wants to match a specific `item_type` label. The stdlib transformers no longer use it — they use `match_artifacts` or `node_shape` instead.

## Alternatives Considered

- **All fields as OR (any match dispatches).** The initial implementation treated all criteria as a flat OR. This was overbroad — a `Container` transformer with no other constraints matched all containers *or* all nodes matching any other criterion, making it impossible to express "containers with a specific tag."

- **A `binoc.container` tag** on all container nodes, matched via `match_tags`. Extensible but adds a convention every container comparator must remember. `node_shape` is structural (derived from the tree) and requires no cooperation from comparators.

- **Removing `match_types` entirely.** Too aggressive — it's still useful for simple cases and Python plugins that don't use artifacts.

- **A boolean `match_children` field.** Doesn't express "any" (the default) cleanly. The three-valued enum is clearer.

- **A full boolean predicate DSL.** Considered but deferred — the AND-of-ORs model handles all current use cases. If a transformer needs `(artifact_A AND artifact_B)` or `(tag_X OR artifact_Y)`, we can revisit.
