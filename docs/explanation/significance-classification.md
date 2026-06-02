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
- **The renderer applies judgments.** A renderer's config maps tags into
  changelog groups with ordered headings. The same IR can be rendered
  through different configs to produce different changelogs.

This split is documented in the
[per-renderer config ADR](../adr/2026-03-09-renderer_config.md) and the
[terminology ADR](../adr/2026-03-18-terminology.md). "Significance
classification" is the historical name for this renderer-side grouping
feature.

## The default behavior

If you omit `output.markdown.groups`, the Markdown renderer emits a flat list
with no group headings at all.

Grouping is opt-in. Once `groups` is configured, the renderer emits sections
in declared order and puts any unmatched nodes in a trailing
`## Other Changes` section.

## Configuring groups

A dataset config YAML can define ordered groups under
`output.markdown.groups`:

```yaml
output:
  markdown:
    groups:
      - heading: "Critical review"
        tags:
          - bio.cross-contamination
      - heading: "Substantive changes"
        tags:
          - binoc.column-addition
          - binoc.content-changed
          - bio.sequence-change
      - heading: "Clerical changes"
        tags:
          - binoc.column-reorder
          - binoc.whitespace-change
          - bio.header-change
```

A plugin pack can ship a recommended grouping config that users opt into.
A team can ship a stricter or looser version of the same config without
changing any plugin code.

## Group order is priority

Groups are an ordered list, not a map. The renderer preserves the declared
order when it writes sections, and the same order defines priority when a
node matches more than one group.

If a node has both `binoc.column-reorder` and `binoc.column-addition`, and
the config lists "Substantive changes" before "Clerical changes", the node
goes into "Substantive changes". First matching group wins.

Headings are literal strings. The renderer does not auto-capitalize them or
append `" Changes"`.

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
  considers it a critical review item, a routine content change, or
  something else. The user does.

## Classification is one renderer's job

Different renderers can apply different classification logic — or none.
The JSON renderer doesn't classify at all; it just serializes the tree.
The Markdown renderer can emit either a flat list or configured groups. A custom HTML
renderer (see the [`binoc-html` model plugin](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html))
applies its own grouping with hooks for review workflows.

This is why grouping config is *per-renderer* (`output.markdown.groups`,
`output.html.groups`) rather than a global setting on the changeset.

## Where to go next

- For the design rationale → [renderer config ADR](../adr/2026-03-09-renderer_config.md),
  [terminology ADR](../adr/2026-03-18-terminology.md).
- For the config YAML keys → [Dataset config reference](../reference/dataset-config.md).
- For the Markdown renderer source — the canonical example —
  [`binoc-stdlib/src/renderers/markdown.rs`](https://github.com/harvard-lil/binoc/blob/main/binoc-stdlib/src/renderers/markdown.rs).
