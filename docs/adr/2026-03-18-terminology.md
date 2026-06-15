# Terminology

**Date:** 2026-03-18
**Status:** Accepted; updated by the 2026-06-12 correspondence-first migration

## Context

Binoc introduces a number of domain-specific terms. This ADR catalogs the deliberate choices and documents the rejected alternatives so that contributors and plugin authors understand the vocabulary.

## Decisions

### Core Objects

| Chosen term | Meaning |
|---|---|
| **Snapshot** | A set of files representing a dataset's state at a point in time. |
| **Changeset** | The stored finalized IR: a structured description of how one snapshot differs from the next. A tree of diff nodes. Chosen over *migration* (strongly implies an executable transformation — database migrations are scripts you run — but binoc changesets are descriptive records, not replayable operations), *diff* (already overloaded: verb, command name, `DiffNode`), *delta* (precise but abstract for user-facing contexts), *patch* (implies applicability, same problem as migration). |
| **Changelog** | A human-level summary rendered from changesets. |

### Program Components

Superseded in part by
[2026-06-12-correspondence_first_engine.md](2026-06-12-correspondence_first_engine.md).
`Comparator` and `Transformer` remain useful compatibility and author-facing
terms, but the current engine's primary phases are correspondence rule families:
expand, parse, pair, write edits, compact, annotate projection, and render.

| Chosen term | Meaning |
|---|---|
| **Controller** | The type-ignorant host that creates a run, drives the correspondence engine, and renders or extracts results. |
| **Comparator** | Compatibility/author-facing role for rules that parse or expand source data. In the current engine this usually maps to expand rules, parse rules, pair rules, and writers. |
| **Transformer** | Compatibility/author-facing role for rules that optimize or annotate the result. In the current engine this usually maps to compaction rules, projection annotators, and renderers. |
| **Correspondence rule** | One rule registered with the correspondence engine: expand, parse, pair, writer, compaction, or projection annotation. |
| **Renderer** | A plugin that renders changesets into a presentation format. |
| **Porcelain** (for the CLI) | Borrowed from git's terminology: the CLI is a user-facing layer over the library. |

### IR Fields
| Chosen term | Meaning |
|---|---|
| **Intermediate Representation (IR)** | The tree of diff nodes that represents the changes between two snapshots. |
| **action** | Open enum describing what happened: `"add"`, `"remove"`, `"modify"`, `"move"`, `"reorder"`, etc. Chosen over *kind* (ambiguous alongside `item_type` — both read as "type of something"). |
| **item_type** | Human-readable label describing what the item *is*: `"directory"`, `"file"`, `"tabular"`, `"zip_archive"`, etc. Used by renderers for fallback descriptions (e.g. "File modified", "New tabular"). It is a projected fact, not a core scheduling key. |
| **tags** | An open bag of semantic strings attached to projected diff nodes by rule packs. |
| **details** | Structured payload on a projected diff node. |
| **annotations** | Renderer/plugin metadata on a projected diff node. |
| **summary** | Optional human-readable one-liner describing a change, set during projection. |

### Comparison Mechanics

Superseded in part by the correspondence-first engine ADR. The terms below are
historical or compatibility vocabulary unless otherwise noted.

| Chosen term | Meaning |
|---|---|
| **Item pair** | Compatibility vocabulary for two source items: left item (old) and right item (new), either side potentially absent. The correspondence engine stores side items separately and links them. |
| **left / right** | The two sides of a comparison or correspondence link. |
| **Link** | A correspondence between one left item and one right item. |
| **Evidence** | An open-vocabulary string explaining why a pair rule proposed a link. |
| **Edit list** | The open-vocabulary edits emitted by a writer for one link before projection. |
| **Claim** (verb) | Historical comparator-dispatch vocabulary; in the current engine, rules propose links or emit artifacts/edits instead of claiming the whole comparison. |
| **Expand** / **Leaf** | Historical comparator-output vocabulary. Expand remains a rule family; leaf output is now represented by parsed artifacts, writer edits, and projection. |
| **Logical path** | The user-meaningful path within a snapshot, including interior paths like `"archive.zip/data/file.csv"`. |

### Significance Classification

Superseded in part by [2026-06-02-renderer_groups.md](2026-06-02-renderer_groups.md) for the current Markdown grouping model and the removal of shipped default headings.

| Chosen term | Meaning |
|---|---|
| **Clerical** | Changes that are mechanically necessary but semantically unimportant: column reordering, whitespace normalization, encoding changes. Chosen over *ministerial* (precise in records management but unfamiliar to most developers), *minor*/*trivial* (judgmental), *cosmetic* (implies visual concerns). |
| **Substantive** | Changes that alter the information content: added columns, removed rows, schema changes. |

Note: these were the original category names used in the Markdown renderer design. They are not baked into the IR. Current renderer config uses explicit ordered groups with literal headings. 

### Testing

| Chosen term | Meaning |
|---|---|
| **Test vector** | A self-contained directory with two snapshots, a manifest, and optional expected output, exercising one capability. |
| **Gold file** | An optional expected-output file in a test vector, checked by exact comparison. |

### Other

| Chosen term | Meaning |
|---|---|
| **Open enum** / **open bag** | The extensibility model for `action`, `item_type`, and `tags` — plugins can define new values without modifying core types. |
| **Plugin pack** | A distribution unit of comparators, transformers, and renderer configs (e.g. `biobinoc`). |
| **Standard library** / **stdlib** | The built-in plugin pack (`binoc-stdlib`). |
