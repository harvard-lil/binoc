# Terminology

**Date:** 2026-03-18
**Status:** Accepted

## Context

Binoc introduces a number of domain-specific terms. This ADR catalogs the deliberate choices and documents the rejected alternatives so that contributors and plugin authors understand the vocabulary.

## Decisions

### Core Objects

| Chosen term | Meaning |
|---|---|---|---|
| **Snapshot** | A set of files representing a dataset's state at a point in time. |
| **Changeset** | The stored finalized IR: a structured description of how one snapshot differs from the next. A tree of diff nodes. Chosen over *migration* (strongly implies an executable transformation — database migrations are scripts you run — but binoc changesets are descriptive records, not replayable operations), *diff* (already overloaded: verb, command name, `DiffNode`), *delta* (precise but abstract for user-facing contexts), *patch* (implies applicability, same problem as migration). |
| **Changelog** | A human-level summary rendered from changesets. |

### Program Components

| Chosen term | Meaning |
|---|---|---|---|
| **Controller** | The work loop that dispatches item pairs to comparators, assembles the diff tree, and runs transformers. |
| **Comparator** | A plugin that claims an item pair and either emits a leaf diff or expands into child items. |
| **Transformer** | A plugin that rewrites the completed diff tree (IR → IR). |
| **Renderer** | A plugin that renders changesets into a presentation format. |
| **Porcelain** (for the CLI) | Borrowed from git's terminology: the CLI is a user-facing layer over the library. |

### IR Fields
| Chosen term | Meaning |
|---|---|
| **Intermediate Representation (IR)** | The tree of diff nodes that represents the changes between two snapshots. |
| **action** | Open enum describing what happened: `"add"`, `"remove"`, `"modify"`, `"move"`, `"reorder"`, etc. Chosen over *kind* (ambiguous alongside `item_type` — both read as "type of something"). |
| **item_type** | Open string describing what the item *is*: `"directory"`, `"file"`, `"tabular"`, `"zip_archive"`, etc. |
| **tags** | An open bag of semantic strings attached to diff nodes by comparators and transformers. |
| **details** | Comparator-specific payload on a diff node. |
| **annotations** | Transformer-added metadata on a diff node. |
| **summary** | Optional human-readable one-liner describing a change, set by comparators/transformers. |

### Comparison Mechanics

| Chosen term | Meaning |
|---|---|---|---|
| **Item pair** | The two-sided input to a comparator: left item (old) and right item (new), either side potentially absent. |
| **left / right** | The two sides of an item pair or comparison. |
| **Claim** (verb) | How a comparator wins dispatch: it "claims" an item pair. |
| **Expand** / **Leaf** | The two modes of comparator output: expand into children, or produce a terminal diff. |
| **Logical path** | The user-meaningful path within a snapshot, including interior paths like `"archive.zip/data/file.csv"`. |

### Significance Classification

| Chosen term | Meaning |
|---|---|---|---|
| **Clerical** | Changes that are mechanically necessary but semantically unimportant: column reordering, whitespace normalization, encoding changes. Chosen over *ministerial* (precise in records management but unfamiliar to most developers), *minor*/*trivial* (judgmental), *cosmetic* (implies visual concerns). |
| **Substantive** | Changes that alter the information content: added columns, removed rows, schema changes. |

Note: these are the *default* category names in the Markdown renderer. They are not baked into the IR. Any renderer or dataset config can define its own categories (e.g. `critical`, `informational`, `review-required`). 

### Testing

| Chosen term | Meaning |
|---|---|---|---|
| **Test vector** | A self-contained directory with two snapshots, a manifest, and optional expected output, exercising one capability. |
| **Gold file** | An optional expected-output file in a test vector, checked by exact comparison. |

### Other

| Chosen term | Meaning |
|---|---|---|---|
| **Open enum** / **open bag** | The extensibility model for `action`, `item_type`, and `tags` — plugins can define new values without modifying core types. |
| **Plugin pack** | A distribution unit of comparators, transformers, and renderer configs (e.g. `biobinoc`). |
| **Standard library** / **stdlib** | The built-in plugin pack (`binoc-stdlib`). |
