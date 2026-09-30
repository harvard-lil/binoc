# Parsed Children and Decompose Boundaries (CFM-69)

**Date:** 2026-06-14
**Status:** Accepted

## Context

Binoc decomposes a node into sub-nodes two ways:

- **Expand** rules (zip, tar, gzip, directory) return `Vec<ItemRef>` children that
  carry physical handles and are re-dispatched through the whole pipeline.
- **Parse** rules (CSV, stacked CSV, JSON, SQLite, Excel, …) return an artifact
  for the node plus, in one case (stacked CSV), `Vec<ParsedChild>` children that
  carry already-computed artifacts.

The result was three inconsistent path conventions and a half-built table model:

| Producer | Child nodes? | Separator | Parent artifact |
|---|---|---|---|
| zip/tar/gzip/dir expand | yes (`ItemRef`) | `/` | none |
| stacked CSV parse | yes (`ParsedChild`) | `#` (`data.csv#table_1`) | `tabular_collection_v1` |
| Excel multi-sheet | yes (`ParsedChild`) | `::` (`book.xlsx::Sheet1`) | `tabular_collection_v1` |
| SQLite | **no** (fake `::` paths in a manifest) | `::` | `tabular_collection_v1` carrying the diffs |

`tabular_collection_v1` existed because table-level substructure was not a
linkable endpoint: identity had to live *somewhere*, so it lived in a manifest
on the parent and a collection writer diffed it. That writer also re-emitted the
per-table row changes that the child nodes (where they exist) already emit —
a latent double-count.

CFM-69 settles one convention for parsed children, so that split/merge (CFM-72),
container reshape (CFM-71), and SQLite-as-tables (CFM-70) all build on linkable
table/sheet/section endpoints instead of a manifest.

## Decision

### 1. Two path separators with distinct meaning

- **`/` — membership.** Structure that already existed as a navigable tree:
  directory entries, and paths *inside* an extracted archive. No format had to be
  decoded to reveal it.
- **`/>` — decompose boundary.** A node binoc had to **open a format** to reveal:
  the immediate members of an archive expansion, and every parsed table / sheet /
  section. Read it as a URI-fragment-like "we cracked this open."

So `dir1/data.zip/>reports/q1.csv/>table_2` reads as: real directory `dir1` ->
opened the zip -> real internal directory `reports` -> opened the CSV -> its second
table. A **directory is `/`, not `/>`**: its tree is already navigable, and (like
paths inside a zip) it conceptually parses in one go. Only format-decoding
(zip/tar/gzip, CSV-stack, SQLite, Excel) earns `/>`.

A real path segment literally beginning with `>` is escaped with a leading
backslash when it follows a member separator. For example, a file named
`>q1.csv` inside `dir` is written `dir/\>q1.csv`; `dir/>q1.csv` is always a
decompose boundary followed by `q1.csv`. A literal leading backslash is escaped
too, so `\>q1.csv` is written `\\>q1.csv`. The SDK path helpers implement this
rule for callers that build logical paths through `member_child` or
`decompose_child`.

### 2. The separator is cosmetic; structure lives in fields and the tree

Nothing parses a path string to make decisions. Parent/child relationships come
from the IR tree (`add_child`), and child kind rides on the existing
`ItemRef.projection_hint.item_type` (`"tabular"`, `"text"`, …) — no new field.
All separator handling is centralized in one SDK module (`binoc_sdk::path`):
`member_child`, `decompose_child`, `file_name` (splits on either separator), and
`segments` (yields each `(cumulative_path, name)` for projection nesting). The
old ad-hoc `child_logical`, `table_node_path`/`sheet_node_path`, and
`project.rs`'s `split('/')` are all replaced by these.

### 3. One child-node concept; two producers kept only as an optimization

Expand and parse stay as two producer traits because they differ in **content
delivery**, not in concept:

- expand delivers children **by reference** (a physical handle, re-dispatched and
  re-typed) — right for large/opaque members;
- parse delivers children **by value** (an artifact the rule already computed) —
  right for structure the rule understood in one pass.

Downstream they are the *same node*: the core already adds both via `add_child`,
both get a `content_hash`, both are pair-rule endpoints. A "container parse" (a
parser that only decomposes, emitting children and no parent artifact) is now
shaped exactly like an expand. To allow that, **`ParseOutput`'s parent artifact
becomes optional** (`ParseDescriptor.output: Option<ArtifactFormat>`, parent
bytes optional): a leaf parser emits an artifact and no children; a container
parser emits children and no artifact.

### 4. Drop the table-collection manifest entirely

Remove `tabular_collection_v1`, `TabularCollectionData`/`TableMember`/`TableShape`/
`TableSourceLocation`, `tabular_collection_name_edits`,
`TabularCollectionDiffConfig`, `TabularCollectionWriter` (stdlib) and
`SqliteCollectionWriter` (plugin).

Every multi-table source emits **child table nodes** carrying `tabular_v1`; the
parent file becomes a plain container node, exactly like a zip. Then:

- **Membership changes** (table added/removed/renamed) render as child
  add/remove/move via the existing pair rules — `NameUnderPairedParent` for
  same-name tables under a linked parent, `HashPair`/`CopyPair` for a verbatim
  table moved between containers.
- **Content changes** render on the child via `TabularWriter`.

This deletes the double-count and means CFM-71 reshape correlates child nodes
directly rather than through a manifest. A parent manifest is reintroduced only
if a concrete reshape need proves one necessary.

**Invariant:** a parent's residual edits never duplicate its children's edits.

### 5. Deterministic child identity

Logical name = **intrinsic identity** where the format provides it (SQL table
name, Excel sheet name, detected stacked-table title). Positional fallback
(`table_1`, `table_2`) only when nothing intrinsic exists. Intrinsic-named
children pair by name (robust across reorder); positional children pair by
position/content.

### 6. Single-table sources stay leaf nodes

A single-table source is its own table: plain CSV, a one-sheet workbook,
Parquet, Arrow IPC, and Avro emit `tabular_v1` directly on the file node — no
child, no `/>`. Children appear only when a source genuinely holds several
table/sheet/section endpoints. This mirrors `CsvParse` (leaf) vs
`CsvStackedTablesParse` (container) and is principled, not residual variation.

### 7. Childness bar (enforced by the plugin lint)

A parse rule emits a child node only for substructure that *could plausibly have
shipped as a separate file* — tables, sheets, named sections, top-level archive
members. Rows, cells, and array elements stay as artifact-internal edits, never
nodes. Added to `.agents/skills/lint-plugin/SKILL.md`.

### 8. Reconciliation pass — direction set, build deferred to CFM-71

Container reshape (directory-of-CSVs <-> SQLite) and the existing same-path
"Merged from" collision are the same operation at different inputs: reconcile
several linked endpoints into one coherent projected container and re-parent the
linked children under it. We will **generalize `merge_projected_collision` into a
single parent-reconciliation pass**, with same-path collision as its degenerate
case, rather than adding a second code path. Implemented under CFM-71; recorded
here so it is not re-litigated.

## Alternatives Considered

- **Slash everywhere (no decompose glyph).** Rejected: `data.csv/table_1` is
  indistinguishable from a directory named `data.csv`. The thing that makes
  `archive.zip/...` honest is the *suffix* convention, which does not transfer to
  parsed substructure that never had a filename.
- **JAR-style `!/`.** Rejected despite useful precedent: Java JAR URLs use it
  to split a container URL from an entry path, but the marker still needs an
  escaping story for portable Binoc logical paths and reads less like ordinary
  path descent than `/` plus a directional marker.
- **GDAL-style paired braces for nested virtual filesystems.** Rejected as the
  canonical spelling: braces make ambiguous archive paths explicit and compose
  well for virtual filesystems, but they turn Binoc's projected node paths into
  nested expressions rather than left-to-right paths. Worth retaining as prior
  art if logical paths ever need type-qualified reopen chains.
- **A distinct fragment glyph only for parse (`#`), `/` for archive expansion.**
  This was the first proposal. Rejected in favor of `/>` for both
  format-decode boundaries, because the meaningful line is
  *already-navigable* vs *binoc-had-to-decode* — and a zip is decoded just like a
  CSV. The retrievable-vs-synthesized distinction it tried to encode lives in the
  content-delivery field (handle vs artifact), not in the path.
- **Demote the manifest to a thin reconciliation summary instead of deleting it.**
  Rejected: once children are first-class linkable nodes, membership and reshape
  both fall out of ordinary node pairing/projection, so the manifest carries no
  information the tree does not. Keeping a half-used artifact is exactly the
  variation this work removes.
- **Merge `ExpandRule` and `ParseRule` into one `Decompose` trait now.** Deferred:
  the by-reference/by-value split is a real laziness optimization; unifying the
  *node* (this ADR) captures the benefit without a larger trait refactor. Named
  as a possible later cleanup.
- **Promote Parquet/Avro/single-sheet Excel to trivial one-child collections for
  uniformity.** Rejected: a single table is a leaf; wrapping it in a one-member
  container is ceremony, and the childness bar says a leaf is a leaf.
