# Per-Renderer Config and Significance as a Renderer Concern

**Date:** 2026-03-09
**Status:** Decided (implemented)

## Problem

The original design placed significance classification config in a flat `output.significance` section of the dataset config:

```yaml
output:
  significance:
    clerical:
      - binoc.column-reorder
    substantive:
      - binoc.column-addition
```

This had two problems:

1. **It "knew too much" for the config layer.** The significance mapping is inherently format-specific — it only matters to renderers that render human-readable changelogs (Markdown, potentially HTML). JSON migration output doesn't use it. A CI-check renderer might want a completely different mapping. Putting it at the top level of `output` implied it was a universal output concern.

2. **No mechanism for renderer-specific config.** The `Renderer` trait's `render` method received a monolithic `OutputConfig` struct. Every new renderer-specific knob would require modifying this shared struct — the opposite of a plugin architecture. A future HTML renderer wanting a `theme` setting, or a CI renderer wanting a `fail_on` list, would all crowd into the same type.

## Options Considered

### A: Significance as a transformer (rejected)

Make significance classification a transformer that annotates IR nodes with a `significance` field. Renderers read annotations.

Rejected because it pushes a renderer concern into the IR phase, violating the design principle that the IR carries factual tags and the renderer interprets them. It would bake a significance judgment into the migration JSON, which is supposed to be format-agnostic and judgment-free. Different renderers couldn't apply different significance mappings to the same migration.

### B: Per-renderer config sections (chosen)

The `output` section becomes a map from renderer names to renderer-specific config objects. Each renderer defines its own config schema and defaults.

```yaml
output:
  markdown:
    significance:
      clerical:
        - binoc.column-reorder
      substantive:
        - binoc.column-addition
```

The `Renderer::render` method receives a `serde_json::Value` (the renderer's own config section) instead of a monolithic `OutputConfig`. The renderer deserializes it into its own config type with `#[serde(default)]` for missing fields.

## Decision

Option B. The implementation:

- **`OutputConfig`** is now a struct wrapping `BTreeMap<String, serde_json::Value>` with `#[serde(flatten)]`. `get_for_renderer(name)` resolves both short names (`"markdown"`) and qualified names (`"binoc.markdown"`).

- **`Renderer::render`** receives `&serde_json::Value` — the renderer's own config section, or an empty object if absent. Renderers deserialize this into their own config types and apply their own defaults.

- **`MarkdownRendererConfig`** is a new public type in `binoc-core::output` with a `significance: BTreeMap<String, Vec<String>>` field. Default significance (the standard clerical/substantive mapping) lives here, not in the global config.

- **The migration IR is unchanged.** Tags remain factual; significance classification remains a renderer concern. Different renderers can apply different significance mappings to the same migration.

## Consequences

- The same pattern naturally extends to future renderer-specific config (HTML themes, CI failure rules, etc.) without modifying shared types.
- Third-party renderers define their own config schemas — no coordination with core needed.
- The existing config format changes: `output.significance.*` becomes `output.markdown.significance.*`. This is a breaking change, acceptable since the project is pre-release.
- Per-plugin config for comparators and transformers is not yet implemented but could follow the same pattern (`comparator_config`, `transformer_config` sections) if needed.
