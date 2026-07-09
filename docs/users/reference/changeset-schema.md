---
audience: pipeline integrator
---

# Changeset JSON schema

<!--
  GENERATED FILE — do not edit by hand.
  Source of truth: binoc-sdk IR types (binoc-sdk/src/ir.rs, src/types.rs).
  Regenerate with `just docs-schema`.
-->

A changeset JSON document is a tree of [`DiffNode`](#diffnode) values wrapped
in a [`Changeset`](#changeset) envelope. The shape is deliberately open:
`action`, `item_type`, and `tags` are unbounded strings that plugins extend.
Consumers should treat unknown values as opaque and fall through to generic
handling.

The machine-readable schema (JSON Schema draft 2020-12) lives alongside this
page at [`changeset-schema.json`](changeset-schema.json) and is generated
from the Rust IR types. The tables below are a rendering of that schema.

## What is *not* in the changeset

- **Significance classification.** Changelog grouping is a renderer
  concern, applied at render time from configured headings and tag lists.
  The IR is judgment-free. See
  [Significance classification](../explanation/significance-classification.md).
- **Transient session data.** `source_items` and `artifacts` are wire-visible
  because the plugin ABI carries them across (potentially process-isolated)
  boundaries, but they are stripped at the output boundary via
  `DiffNode::strip_transient` before changeset JSON is written for users.
  They appear in the schema below, but callers writing changeset files
  should not expect to see populated values. See the
  [Transient fields on wire ADR](../../adr/2026-04-16-transient_fields_on_wire.md).

## Stability

The IR is still evolving. Once a first stable version is cut, the schema
will be versioned and this page will document compatibility guarantees.
Until then, treat the shape as informative and pin your downstream pipeline
to known plugin versions.

## Where to go next

- [IR and changesets](../../plugin-developers/explanation/ir-and-changesets.md) — the conceptual
  model behind the shape documented here.
- [Save and render changesets](../howto/save-and-render-changesets.md) —
  producing and combining changeset JSON from the CLI.
- [Extract changed data](../howto/extract-changed-data.md) — using the
  provenance fields to pull actual changed content out of a changeset.


## Types

### `Changeset`

A structured description of how to get from one snapshot to the next.

| Field | Type | Required | Description |
|---|---|---|---|
| `claims` | array of [`GlobalClaim`](#globalclaim) | no | Run-scoped claims that do not belong to one tree node. Reserved for the CFM-41 global-claim prototype; empty in current engine output. |
| `diagnostics` | array of [`Diagnostic`](#diagnostic) | no |  |
| `from_snapshot` | string | yes |  |
| `metadata` | object (map of string → string) | no |  |
| `root` | [`DiffNode`](#diffnode) \| null | no |  |
| `to_snapshot` | string | yes |  |

### `DiffNode`

A node in the projected diff tree — the durable changeset structure consumed by renderers, serializers, and bindings.

| Field | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | Open enum: "add", "remove", "modify", "move", "reorder", "schema_change", etc. Plugins may define new actions. |
| `annotations` | array of [`Annotation`](#annotation) | no | Renderer-visible annotations supplied by rule packs. |
| `artifacts` | array of [`ArtifactDescriptor`](#artifactdescriptor) | no | Published artifacts for this node. Session-scoped working data: carried across the plugin ABI wire as descriptors (the bytes live in the shared `data_root` cache), but not meaningful outside a session. Callers writing changeset output must strip this via [`DiffNode::strip_transient`] before serializing. |
| `children` | array of [`DiffNode`](#diffnode) | no | Child diff nodes forming the tree structure. |
| `detail_blocks` | array of [`DetailBlock`](#detailblock) | no | Renderer-visible, structured evidence blocks. Rule packs populate these with bounded examples while they still have domain knowledge; renderers decide how much to display. |
| `details` | object (free-form) | no | Structured payload, schema determined by item_type/action convention. |
| `diagnostics` | array of [`Diagnostic`](#diagnostic) | no | Node-scoped diagnostics emitted during a run. Transient: the controller hoists them into [`Changeset::diagnostics`] at the end of the diff, then clears this field so the output shape stays as one durable top-level diagnostics list. |
| `item_type` | string | yes | Open string: "directory", "file", "tabular", "zip_archive", etc. No built-in types — conventions, not enforcement. |
| `path` | string | yes | Location within snapshot (logical path, including interior paths like "archive.zip/>data/file.csv"). `/>` marks a decompose boundary; a literal segment beginning with `>` is escaped as `\>`. |
| `source_items` | [`ItemPair`](#itempair) \| null | no | The original item pair associated with this projected node when one is available. Session-scoped working data: available during a live run for rules and extractors that need to re-read source data. Callers writing changeset output must strip this via [`DiffNode::strip_transient`] before serializing. |
| `sources` | array of [`Source`](#source) | no | Renderer-visible provenance for this projected node. |
| `summary` | [`Summary`](#summary) \| null | no | Optional structured one-liner describing the change. Set during projection; renderers format each [`Segment`] by its type. Build it with [`Summary`]'s builder, or pass a plain string — `impl Into<Summary>` wraps it as a single [`Segment::Text`]. |
| `tags` | array of string | no | Open bag of semantic tags, namespaced by convention. e.g. "binoc.column-reorder", "biobinoc.gap-change" |

### `ItemPair`

A pair of items to compare. Either side may be None (add/remove).

| Field | Type | Required | Description |
|---|---|---|---|
| `left` | [`ItemRef`](#itemref) \| null | no |  |
| `right` | [`ItemRef`](#itemref) \| null | no |  |

### `ItemRef`

Metadata-only view of one side of a comparison. Carries logical identity and content metadata but NOT a filesystem path — data access goes through `DataAccess`. # Metadata invariants `content_hash`, `size`, and `media_type` are **opportunistic hints**. Producers (expand rules like directory/zip, or data backends) populate them when doing so is cheap — typically as a byproduct of work they were already performing. Consumers **must not assume presence**, but **may trust presence**: when a field is set, the value accurately reflects the current bytes. Use [`ItemRef::resolve_hash`] / [`ItemRef::resolve_size`] to obtain a value with a transparent fall-back read. This keeps fast paths (directory-only listings, short-circuit identical detection) cheap while letting consumers that need a value — most notably the move detector, which correlates leaves across container boundaries — hydrate on demand.

| Field | Type | Required | Description |
|---|---|---|---|
| `content_hash` | string \| null | no |  |
| `handle` | string | no | Opaque identifier used by DataAccess implementations to locate data. Plugin authors should not create or interpret this value directly. |
| `is_dir` | boolean | yes |  |
| `logical_path` | string | yes | User-meaningful location within a snapshot. `/>` marks a decompose boundary; a literal segment beginning with `>` is escaped as `\>`. |
| `media_type` | string \| null | no |  |
| `projection_hint` | [`ProjectionHint`](#projectionhint) | no | Optional projection metadata supplied by rule packs while they still know the vocabulary. Core carries this through but does not interpret file names, media types, or plugin-specific tags. |
| `size` | integer \| null | no |  |
| `tabular_parse` | [`TabularParseConfig`](#tabularparseconfig) \| null | no | Optional stdlib-resolved tabular parse hints carried from dataset config to whichever tabular parser eventually claims this item. |

### `ArtifactDescriptor`

Descriptor for a published artifact attached to a node. Artifacts are the unified mechanism for both private reuse and cross-plugin composition. Parse rules publish artifacts; downstream rules consume them by format.

| Field | Type | Required | Description |
|---|---|---|---|
| `format` | [`ArtifactFormat`](#artifactformat) | yes |  |
| `handle` | string | yes | Opaque handle managed by the SDK's DataAccess implementation. Plugins should not create or interpret this value directly. |
| `producer` | string | yes |  |
| `subject` | [`ArtifactSubject`](#artifactsubject) | yes |  |

### `ArtifactFormat`

Identifies an artifact's data format as a structured tuple of (package, name, version). - **`package`** — the package that owns and defines this format, resolvable through the language's normal package system (e.g. `"binoc"`, `"binoc-csv"`, `"acme-parquet"`). - **`name`** — the format name within that package (e.g. `"tabular"`, `"relational-schema"`). - **`version`** — a single integer. Bump only for breaking schema changes. Adding optional fields to an existing version is fine and does not require a bump (JSON/serde naturally ignore unknown fields and default missing ones).

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `package` | string | yes |  |
| `version` | integer (uint32) | yes |  |

### `ArtifactSubject`

Which side of a comparison an artifact describes.

String enum. One of:

- `left`
- `right`
- `pair`

### `Annotation`

Renderer-visible metadata attached to a projected diff node by a rule pack. Annotations are intentionally progressively typed: producers can start with a string or simple JSON value, and renderers can either display the generic value shape or add package/key-specific handling later. The package namespace keeps independently-authored plugins from colliding on common keys.

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | string | yes |  |
| `package` | string | yes |  |
| `value` | any | yes |  |

### `CsvDialectConfig`

| Field | Type | Required | Description |
|---|---|---|---|
| `bom` | boolean \| null | no |  |
| `delimiter` | string \| null | no |  |
| `escape` | string \| null | no |  |
| `newline` | string \| null | no |  |
| `quote` | string \| null | no |  |

### `DetailBlock`

Renderer-visible, bounded evidence attached to a diff node.

| Field | Type | Required | Description |
|---|---|---|---|
| `examples` | array of [`DetailExample`](#detailexample) | no | Captured examples for inline rendering. |
| `extract` | array of [`ExtractHint`](#extracthint) | no | Named extract aspects for exhaustive retrieval. |
| `id` | string | yes | Stable within this node, for anchors and extract selection. |
| `kind` | string | yes | Open, namespaced kind such as `binoc.tabular.cell_changes.v1`. |
| `label` | string \| null | no | Short renderer-facing label. |
| `total_count` | integer \| null | no | Total matching items if known, including omitted examples. |
| `truncated` | boolean | no | Whether the producer truncated capture before exhausting candidates. |

### `DetailExample`

One bounded example inside a detail block.

| Field | Type | Required | Description |
|---|---|---|---|
| `after` | [`ValuePreview`](#valuepreview) \| null | no | Value after the change, if present. |
| `before` | [`ValuePreview`](#valuepreview) \| null | no | Value before the change, if present. |
| `fields` | object (free-form) | no | Domain-specific structured context. |
| `locator` | object (free-form) | no | Structured locator such as row/column, line range, or key path. |

### `Diagnostic`

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | string | yes |  |
| `extract` | [`ExtractHint`](#extracthint) \| null | no |  |
| `location` | string \| null | no |  |
| `message` | [`Summary`](#summary) | yes |  |
| `severity` | [`DiagnosticSeverity`](#diagnosticseverity) | yes |  |

### `DiagnosticSeverity`

String enum. One of:

- `error`
- `warning`
- `suggestion`

### `ExtractHint`

Pointer to an extract aspect that can return exhaustive content.

| Field | Type | Required | Description |
|---|---|---|---|
| `aspect` | string | yes | Aspect name accepted by `binoc extract`. |
| `changeset_path` | string \| null | no |  |
| `label` | string \| null | no |  |

### `GlobalClaim`

Reserved run-scoped claim slot. The shape is intentionally provisional pending the CFM-41 global-claim prototype. It gives renderers and serialized changesets a stable place for non-tree claims without committing the claim vocabulary yet.

| Field | Type | Required | Description |
|---|---|---|---|
| `params` | object (free-form) | no | Claim-specific structured parameters. |
| `summary` | [`Summary`](#summary) \| null | no | Optional renderer-facing summary for the claim. |
| `verb` | string | yes | Open claim verb for a renderer- or plugin-defined run-scoped claim. |

### `ProjectionHint`

Product-facing projection metadata supplied by rules, not inferred by core.

| Field | Type | Required | Description |
|---|---|---|---|
| `action` | string \| null | no |  |
| `annotations` | array of [`Annotation`](#annotation) | no |  |
| `item_type` | string \| null | no |  |
| `retract_tags` | array of string | no | Tags this hint *removes* from the accumulated projection. Tag overlay is union-only, so an annotator that supersedes an earlier framing (e.g. a CFM-71 container reshape replacing a pair-time `binoc.move`) needs a way to drop the now-stale tag — otherwise the IR carries contradictory tags (inert in rendering, but incoherent in JSON). A retraction is honored whenever tags are merged: the named tags are removed from the result and can never be re-introduced by the *same* hint. |
| `summary` | [`Summary`](#summary) \| null | no |  |
| `tags` | array of string | no |  |

### `Segment`

*(unrecognised schema shape)*

### `Side`

*(unrecognised schema shape)*

### `Source`

Renderer-visible provenance for a projected diff node. Most nodes have one source. Move and copy nodes use a `from` source whose path differs from the projected node path; many-to-one projections such as merges and deduplications carry multiple sources.

| Field | Type | Required | Description |
|---|---|---|---|
| `action` | string \| null | no | Open action associated with this source in the projection. |
| `evidence` | string \| null | no | Open evidence string from the rule/link that established provenance. |
| `path` | string | yes | Logical path of the source item. |
| `side` | [`Side`](#side) | yes | Snapshot side where `path` resolves. |

### `Summary`

*(unrecognised schema shape)*

### `TabularParseConfig`

| Field | Type | Required | Description |
|---|---|---|---|
| `delimiter` | string \| null | no |  |
| `dialect` | [`CsvDialectConfig`](#csvdialectconfig) \| null | no |  |
| `header` | boolean | no |  |
| `header_line` | integer \| null | no |  |
| `records_path` | string \| null | no | JSON record collection path for document formats that need to expose a nested array as the tabular record stream. |
| `skip_lines` | integer \| null | no |  |

### `ValuePreview`

A bounded preview of one value in a detail example.

| Field | Type | Required | Description |
|---|---|---|---|
| `media_type` | string \| null | no |  |
| `truncated` | boolean | no |  |
| `value` | any | yes |  |
