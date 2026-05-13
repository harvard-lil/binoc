---
audience: pipeline integrator, plugin author
---

# Significance classification

A column reorder is a fact. Whether that fact is *important* — whether it
should appear under "Substantive Changes" in a changelog, trigger a
review-request, or be silently filtered out — depends on who is reading.
Binoc separates the two:

- **The IR records facts.** Tags like `binoc.column-reorder`,
  `binoc.row-addition`, and `binoc.content-changed` are factual
  observations attached to nodes by comparators and transformers.
- **The renderer applies judgments.** A renderer's config maps tags to
  significance categories (`clerical`, `substantive`, `critical`,
  `informational`, …). The same IR can be rendered through different
  configs to produce different changelogs.

This split is documented in the
[per-renderer config ADR](../adr/2026-03-09-renderer_config.md) and the
[terminology ADR](../adr/2026-03-18-terminology.md).

## The default mapping

The default Markdown renderer uses two categories:

| Category | Default tags |
|---|---|
| **Clerical** | `binoc.column-reorder`, `binoc.whitespace-change`, `binoc.folder-rename`, `binoc.encoding-change` |
| **Substantive** | `binoc.column-addition`, `binoc.column-removal`, `binoc.schema-change`, `binoc.row-addition`, `binoc.row-removal`, `binoc.content-changed` |

Anything that doesn't match is grouped under "Other Changes."

Why "clerical" and "substantive"? They are domain-neutral and capture the
basic split most data audiences care about. *Clerical* over alternatives
like *minor*, *cosmetic*, or *ministerial* — see the
[terminology ADR](../adr/2026-03-18-terminology.md) for the rejected alternatives.

## Overriding the mapping

A dataset config YAML can replace or extend the defaults under
`output.markdown.significance`:

```yaml
output:
  markdown:
    significance:
      clerical:
        - binoc.column-reorder
        - binoc.whitespace-change
        - bio.header-change
      substantive:
        - binoc.column-addition
        - binoc.content-changed
        - bio.sequence-change
      critical:
        - bio.cross-contamination
```

A plugin pack can ship a recommended significance config that users opt into.
A team can ship a stricter or looser version of the same config without
changing any plugin code.

## How a node is classified when it has multiple tags

When a node carries tags from more than one category, the renderer picks
the *highest-priority* match. The priority is the order in which categories
are declared in the renderer config. If a node has both
`binoc.column-reorder` (clerical) and `binoc.column-addition`
(substantive), and the config lists `substantive` before `clerical`, the
node is classified as substantive.

This is consistent with the principle that *the IR records facts* — both
tags genuinely apply — and *the renderer decides what to do about it*.

## Why classification doesn't live in the IR

A naive design would put `significance: "substantive"` directly in
`DiffNode`, computed by the comparator or a transformer. Rejected because:

- **The same change has different significance in different contexts.** A
  whitespace change is clerical for a CSV but substantive for a Python
  source file (it might be a syntax change). Encoding choice in the IR
  picks one and forecloses the other.
- **Re-classification would require re-diffing.** A pipeline that wanted
  to apply a different significance policy would have to regenerate the
  changeset. With the renderer-side mapping, the same JSON can be
  re-classified by re-rendering.
- **Plugin packs that don't know about a tag shouldn't have to.** A plugin
  emitting `bio.sequence-change` doesn't know whether a downstream user
  considers it clerical or substantive. The user does.

## Classification is one renderer's job

Different renderers can apply different classification logic — or none.
The JSON renderer doesn't classify at all; it just serializes the tree.
The Markdown renderer is where the default mapping lives. A custom HTML
renderer (see the [`binoc-html` model plugin](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html))
applies its own grouping with hooks for review workflows.

This is why significance config is *per-renderer* (`output.markdown.significance`,
`output.html.significance`) rather than a global setting on the changeset.

## Where to go next

- For the design rationale → [renderer config ADR](../adr/2026-03-09-renderer_config.md),
  [terminology ADR](../adr/2026-03-18-terminology.md).
- For the config YAML keys → [Dataset config reference](../reference/dataset-config.md).
- For the Markdown renderer source — the canonical example —
  [`binoc-stdlib/src/renderers/markdown.rs`](https://github.com/harvard-lil/binoc/blob/main/binoc-stdlib/src/renderers/markdown.rs).
