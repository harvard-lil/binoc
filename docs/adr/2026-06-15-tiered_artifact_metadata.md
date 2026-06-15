# Tiered Artifact Metadata: Column, Table, and a `parser_metadata_v1` Artifact

**Date:** 2026-06-15
**Status:** Implemented (channels + producers; rendering deferred)

## Context

A parser cracks a source format (CSV, Stata, SAS, Excel, SQLite, …) into one
`tabular_v1` artifact, or — for multi-table containers — a set of `tabular_v1`
child nodes. But the container format carries facts beyond the rows and columns:
per-variable labels and display formats, value-label dictionaries, a dataset
label, the source-format version, file encoding, creator/tooling provenance.

Before this change, the stat-binary plugin computed a rich per-variable and
file-level metadata JSON during parsing and then **discarded it** — it had been
attached to a `tabular_collection_v1` parent manifest that no longer exists.
Deleting the dead extraction (the immediately preceding cleanup) raised the real
question: these facts are genuinely useful to downstream consumers — rewrite
rules and renderers — and a user may well consider a change to them (a relabeled
variable, a dropped value-label set, a new file encoding) a *relevant* change.
So a metadata channel **should** exist. The question was its shape.

Two observations narrowed the design:

1. **The facts come in three grains, keyed differently.** Per-column facts are
   keyed to a *column*; table facts to a *table*; file-level facts to the
   *parse/node as a whole*. Jamming all three into one blob is exactly what made
   the old metadata homeless when its single host (the collection manifest) went
   away.

2. **Artifacts are the parser's output channel.** In binoc, "a parser attaches
   something to a node" is spelled "a parse rule publishes an artifact." A node
   already carries multiple artifacts of different formats, each diffed
   independently by a format-matched writer. So metadata is not a new concept to
   bolt onto the node type — it is more of the existing artifact mechanism.

## Decision

Carry metadata in **three tiers**, by key:

| Tier | Grain | Home | Codec |
|---|---|---|---|
| 1 | per-column | `TabularData.column_metadata` (parallel to `headers`) | `tabular_v1` |
| 2 | per-table | `TabularData.table_metadata` (an open bag) | `tabular_v1` |
| 3 | per-parse / file-level | a second artifact on the parsed node | `parser_metadata_v1` |

- **Tiers 1 and 2 ride on `tabular_v1`.** `column_metadata: Vec<serde_json::Value>`
  is parallel to `headers` (mirroring `column_types`); each entry is an open
  object, or `Null` for a column with no metadata. `table_metadata:
  serde_json::Value` is a table-scoped bag. Both are optional and
  `skip_serializing_if` empty, so a table with no metadata serializes
  byte-identically to before and existing tabular producers are unaffected.

- **Tier 3 is a new `parser_metadata_v1` artifact**, codec `ParserMetadata {
  format, value }` — `format` names the source format (`"stata_dta"`,
  `"sas7bdat"`, `"sas_xport"`) so a consumer can interpret `value` without
  guessing; `value` is an open bag diffed generically. It rides as a **second
  artifact on the parsed node**, published via a new `ParseOutput.artifacts`
  field (the parent-node analogue of `ParsedChild.artifacts`). A single-table
  leaf carries `tabular_v1` + `parser_metadata_v1`; a multi-table container
  (which has no table of its own) carries `parser_metadata_v1` alongside its
  `tabular_v1` children.

**Metadata does not need a current consumer to be useful.** The deliverable here
is the *channel*, populated by a real producer (stat-binary, whose previously
discarded labels/formats/value-labels/version facts are restored into the right
tiers). Carrying the facts on artifacts already makes them available to any
future rewrite rule, renderer, extractor, or third-party plugin, and visible in
traces and debugging — exactly as `tabular_v1` is useful independent of which
writer consumes it. A changeset is the *projected* edit list, so carried-but-
unconsumed metadata produces **zero** output churn until a writer reads it; the
full test suite confirmed no snapshot changed.

**Volatile fields are excluded** from the populated metadata (file
created/modified timestamps). They are wall-clock noise that would make a
metadata diff fire on every re-export; if a use case wants them later they can be
added under a clearly low-significance key.

## Rendering is deliberately deferred

The engine runs **one writer per link** (first match wins), so a node's owning
writer — `TabularWriter` for a leaf, `ContainerWriter` for a container — would be
the single place to render metadata changes, by reading the relevant artifacts.
That is a real, in-grain extension, but it needs its own significance design (a
relabeled column vs. a dropped value-label set vs. a creator rename are not
equally interesting, and significance is a renderer/config concern, not an IR
one). Rather than bundle that judgment into the channel work, this ADR ships the
channel and populated producers; a later change adds the writer-side rendering
and significance mapping. The "no current consumer needed" principle is what
makes this staging legitimate rather than half-done.

## Alternatives Considered

- **A new attribute on the node (`ItemRef`) instead of an artifact.** Rejected.
  `ItemRef` is identity/locator (path, hash, size, media type) plus the one
  projection channel; it exists *before* any parser runs. A metadata bag there
  would be null for the vast majority of nodes, carry no diff machinery (core is
  type-ignorant and could not diff it), and leak a domain concept into the core
  node type. Artifacts already solve diffing and keep domain knowledge behind the
  opaque `format + bytes` seam.

- **Reusing `structured_document_v1` for tier 3.** Rejected. That format and its
  writer mean "this node *is* a document the user authored" (it emits
  `document.value_change`). Metadata is facts *about* a different artifact;
  reusing the document format would mislabel a creator-name change as a document
  edit and conflate significance routing. A distinct `parser_metadata_v1` keeps
  the vocabulary honest.

- **Folding tier 3 into tier 2 for single-table leaves** (no separate artifact on
  leaves, only on containers). Rejected for consistency: `parser_metadata_v1`
  should mean the same thing and render the same way wherever it rides, so leaves
  and containers both carry it. The cost is a second artifact on leaf nodes,
  which the multi-artifact-per-node model already supports.

- **A synthetic metadata child node.** Rejected — it pollutes the logical tree
  with a child the container does not actually contain and needs stable
  cross-snapshot naming. Metadata is a property of a node, not a member of it.

- **Artifact-format inheritance / `parser_metadata` as a subtype of a record
  artifact.** Considered and deferred. `parser_metadata_v1` is kept a flat,
  standalone format for now; if it grows typed structure, that is a `v2` of the
  codec rather than new inheritance machinery in the artifact-format system.

## Consequences

- `binoc-sdk`: `TabularData` gains `column_metadata` + `table_metadata` (with
  `with_column_metadata` / `with_table_metadata` builders); new
  `parser_metadata_v1()` format and `ParserMetadata` codec; `ParseOutput` gains
  `artifacts` for secondary parent-node artifacts.
- `binoc-core`: the parse driver publishes `ParseOutput.artifacts` on the parsed
  node, after the primary `bytes` artifact.
- `binoc-stat-binary`: restores the previously discarded metadata into the three
  tiers — Stata/SAS variable labels, formats, and value-label set names into
  `column_metadata`; dataset name/label into `table_metadata`; source-format
  identity, version/encoding, and value-label dictionaries into a
  `parser_metadata_v1` artifact (on each leaf, and on the `.xpt` container).
- Tabular producers in other plugins gain the new (empty) fields with no
  behavior change.
