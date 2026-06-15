# Default Markdown Is A Changelog, Not An IR Dump

**Date:** 2026-06-14
**Status:** Implemented

## Context

After the correspondence-first migration, Markdown rendered the projected IR too
literally. Default output included an empty `Claims: none` preamble, provenance
sources under every node, and modified container rows whose only visible fact
was `0 edits` while children carried the real changes. That information is
useful for audit/debug views and remains available in JSON, but it made the
default human changelog read like internal bookkeeping.

At the same time, the Markdown renderer already supports explicit user-defined
groups through `output.markdown.groups`; see
[2026-06-02-renderer_groups.md](2026-06-02-renderer_groups.md). The question for
no-config Markdown is whether Binoc should ship default significance sections
or keep the flat list.

## Decision

Default Markdown stays flat unless the user configures groups, but it is now
curated:

- Empty claim sections are omitted. Claims render only when the changeset has
  claims.
- CFM-60 node provenance (`sources`) is hidden at default and summary
  verbosity. It renders only at `full` verbosity, while JSON keeps carrying it.
- Pure bookkeeping containers are not projected as their own bullets when their
  visible edits are empty and their children already carry the reportable
  changes.
- Existing structured edit details may be surfaced as bounded examples so the
  default changelog explains common row, cell, schema, and text edits instead of
  falling back to a bare edit count.

The shipped no-config posture is therefore "flat but clean." Significance
grouping remains a renderer configuration concern, not a built-in IR level or a
stdlib default taxonomy.

## Alternatives Considered

Ship default significance groups.

Rejected for now. Binoc's tags are intentionally open vocabulary, and a generic
default grouping would either be too opinionated for domain datasets or too
weak to be meaningful. Users who need sections can configure ordered Markdown
groups explicitly.

Show provenance by default but compact it.

Rejected because provenance is machine-facing audit data. Compacting it still
puts rule names and source-side mechanics in front of the normal changelog
reader.

Hide cosmetic and structured details by default.

Rejected because cosmetic facts and bounded examples are user-facing facts. The
renderer should suppress plumbing, not suppress meaningful low-risk evidence.
