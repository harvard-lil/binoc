# Markdown Renderer Groups Replace Significance-Map Grouping

**Date:** 2026-06-02
**Status:** Implemented

## Context

The Markdown renderer originally grouped changes with a map-like
`output.markdown.significance` config keyed by category name. That design had
three practical problems:

- It did not preserve section order naturally. A map shape suggests that keys
  are unordered, while changelog sections need predictable top-to-bottom
  priority.
- It encouraged derived headings from category names, which made the output
  feel more schema-driven than author-driven.
- It described multi-tag behavior as "significance" priority even though the
  real rule was simply "first configured match wins."

Separately, Binoc had already removed the shipped clerical/substantive default
mapping. The default Markdown output is now a flat list with no group
headings unless the user configures them.

## Decision

The Markdown renderer uses `output.markdown.groups`, an ordered list of group
objects:

```yaml
output:
  markdown:
    groups:
      - heading: "Critical review"
        tags: [bio.cross-contamination]
      - heading: "Substantive changes"
        tags: [binoc.column-addition, binoc.content-changed]
```

The semantics are:

- `groups` is optional. If omitted or empty, Markdown renders a flat list with
  no headings.
- Each group has a literal `heading` string and a `tags` list.
- Sections render in declared order.
- If a node matches multiple groups, it goes to the first matching group.
- If any groups are configured, unmatched nodes render under a trailing
  `## Other Changes` section.

This supersedes the Markdown-specific naming and semantics described in
[2026-03-09-renderer_config.md](2026-03-09-renderer_config.md), while keeping
that ADR's broader decision intact: renderer-side grouping remains a renderer
concern, not an IR concern.

## Alternatives Considered

Keep `significance` as a map and document an ordering convention.

Rejected because it leaves the structure at odds with the renderer's actual
behavior. Ordered groups are simpler to read, serialize, and reason about than
an implicitly ordered map.

Keep `significance` but change only the value type.

Rejected because the old name suggests built-in levels or severity classes.
`groups` better describes what the Markdown renderer actually does: place
reportable nodes into ordered, author-defined sections.
