# Typed Records: a Greenfield `tabular` Artifact and a Generic `structured_document`

**Date:** 2026-06-14
**Status:** Accepted

## Context

The `tabular` artifact (`binoc.tabular.v1`) is the spine of binoc's most valuable
output: row/column/cell diffs with rename, reorder, and alignment detection. Its
codec today is dead simple:

```rust
pub struct TabularData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
```

Every cell is a `String`. That was the right shape for CSV, which carries no
types. But the formats we want to diff next are not stringly-typed:

- **JSON arrays of like-shaped objects** and **JSONL** — the canonical "list of
  records" shape. Values are typed (`number`, `bool`, `null`, `string`) and may
  nest (`object`/`array`).
- **DB tables, Excel sheets, columnar/data-science formats** (already produced by
  the `binoc-sqlite` and `binoc-stat-binary` plugins) — typed columns, sometimes
  with declared schemas.

Stringly-typed cells throw away the signal that makes record diffing worth doing:
a number rounding (`1.23456` → `1.23`), a type change (`int` → `text`), a
boolean flip. They also can't represent a nested object as a cell at all.

Separately, JSON today parses to a `json_document` artifact
(`binoc-stdlib.json_document.v1`) that is only ever compared by whole-value
canonical equality (plus key-order facts). That artifact is not JSON-specific in
any essential way — it is a generic value tree. YAML, TOML, and other
tree-structured config/data formats want exactly the same treatment.

This ADR resolves the long-parked CFM-68 question — *"should JSON records project
as `tabular` when rectangular enough, or as a sibling `record_collection` with
its own writer?"* — and the value-model question underneath it.

This is **greenfield**: there is no back-compatibility constraint. We redefine the
`tabular` codec in place rather than versioning around the old one.

## Decision

Two logical artifacts, split by the data's actual structure.

### 1. `binoc.tabular.v1` — the consistently-typed record cluster (rewritten)

One artifact for the whole cluster: CSV/TSV, JSON arrays-of-objects, JSONL, DB
tables, Excel sheets, columnar/data-science formats. We keep the artifact id
`binoc.tabular.v1` (greenfield — there is no old consumer to disambiguate from)
and replace its codec.

```rust
/// A cell value. Scalars diff by content; Nested diffs by equality only (v1).
pub enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    /// A nested object/array. Stored canonicalized (object keys sorted) so
    /// `PartialEq` is a content-equality check. Not recursed into in v1.
    Nested(Box<serde_json::Value>),
}

pub struct TabularData {
    pub schema: Schema,
    pub rows: Vec<Vec<Value>>,
}

pub struct Schema {
    pub fields: Vec<Field>,
    /// Did the source supply column names (CSV header, object keys)? When false,
    /// columns are positional and `Field::name` is `None`.
    pub has_header: bool,
    /// Declared identity field names, in order. Empty when the source declares no
    /// key. Drives keyed row alignment when present.
    pub key: Vec<String>,
}

pub struct Field {
    /// `None` for headerless/positional columns.
    pub name: Option<String>,
    /// Optional source-declared type ("integer", "real", "text", ...), when the
    /// source format carries one (DB, Parquet, Stata). Absent for CSV/JSON.
    pub declared_type: Option<String>,
}
```

**The value model has a deliberate "stop".** Scalars (`Null`/`Bool`/`Number`/
`String`) get the full diff treatment — cell edits, type changes, numeric
precision. `Nested` is **equality-only**: it is stored canonicalized so
`PartialEq` is a content comparison, a changed nested cell emits an ordinary cell
edit, and we never recurse into it. This keeps every existing rewrite rule working
unchanged (they all reduce a cell to a comparison or a signature, and `Nested`
participates like any other value), and it is forward-compatible: making `Nested`
recurse later is purely additive (new edit verbs), not a schema break.

**Type is a cell property, not a table requirement.** A column may hold mixed
variants across rows (legal in JSON). `declared_type` is *optional column
metadata* recorded when the source hands us one; the diff engine never needs it
and works cell-by-cell off the variants. "Typed vs untyped" is therefore not a
flag anyone sets — it is observable from which `Value` variants are present
(all-`String` ⇒ effectively untyped, the CSV case).

**Numbers** compare via `serde_json::Number` semantics: `1` (integer) ≠ `1.0`
(float) — a meaningful type signal — while `1.0` and `1.00` collapse to the same
float (we do not preserve trailing-zero precision in v1; rounding like
`1.23456`→`1.23` is still caught because the values differ).

#### Shape facts and rule preconditions

The spectrum from "ragged list of strings" to "typed keyed DB table" is expressed
as **derived facts that rule preconditions read**, not as artifact subtypes.
Derived facts can't lie; a plugin-asserted boolean can. `TabularData` exposes:

```rust
impl TabularData {
    fn is_rectangular(&self) -> bool;     // every row arity == fields.len()
    fn has_named_columns(&self) -> bool;  // schema.has_header && all fields named
    fn stable_columns(&self) -> bool;     // has_named_columns || is_rectangular
    fn column_index(&self, name: &str) -> Option<usize>;
    fn column_values(&self, name: &str) -> Option<Vec<&Value>>;
}
```

Each rule family gates on the precondition it needs, via the narrow rule-level
self-filters the engine already sanctions (AGENTS rule 5 — declarative dispatch
plus content self-filters). The pivotal precondition is **stable column
identity**:

| Rule family | Precondition | Ragged / unnamed input |
|---|---|---|
| Pair (Jaccard over tokens) | none | works |
| Row add/remove, row alignment (LCS over signatures) | a notion of "row" | works |
| **Cell** edit | **stable column identity** | degrades to row-grain |
| Column add/remove/reorder | columns as entities | skipped |
| Column **rename** | column *names* | skipped |
| Numeric-precision / type-change | typed cells (non-`String`) | n/a |

Ragged or headerless input is **not an error**. The writer emits row-grain edits
only (a row changed wholesale) when it can't identify columns — the most truthful
statement available. There are **no `tabular_rectangular` vs `tabular_ragged`
artifacts**; one artifact, rules self-gate on the facts.

#### Edit vocabulary — unchanged verbs, typed params

The `tabular.*` verb vocabulary is retained verbatim so the writer, all four
compaction rules, the projection annotator, and the markdown renderer keep
working: `tabular.set_headers`, `tabular.add_column`, `tabular.remove_column`,
`tabular.reorder_columns`, `tabular.rename_column`, `tabular.edit_cell`,
`tabular.add_row`, `tabular.remove_row`, `tabular.append_rows`,
`tabular.row_alignment_basis`, `tabular.row_identity_degraded`,
`tabular.row_identity_degraded`.

The only change is that captured cell values in params (`from`/`to`/`values`)
become typed JSON instead of always-strings — `Value` serializes naturally to
JSON, so `Value::String("85")` → `"85"`, `Value::Number(85)` → `85`,
`Value::Bool(true)` → `true`. The renderer already renders `from`/`to` as
`serde_json::Value` (strings quoted, others bare), so this is the intended
behavior, not new rendering work. For all-string sources (CSV) the serialized
params are byte-identical to today, which keeps the CSV snapshot churn near zero.

### 2. `binoc.structured_document.v1` — the generic value tree (renamed)

`json_document` is renamed and promoted to the SDK package as a **format-neutral
value tree**: the home for JSON, JSONL-of-mixed-shape, YAML, TOML, and future
tree formats. This is the fallback when the record detector declines, and the
place we "rack up parsers" cheaply.

```rust
pub struct StructuredDocument {
    /// The universal tree. All source formats transcode into serde_json::Value.
    pub value: serde_json::Value,
    /// Source format tag: "json" | "yaml" | "toml" | ...
    pub format: String,
    /// Open, format-specific serialization facts (key order, indentation, BOM,
    /// trailing newline). Consumers ignore unknown fields.
    pub source: serde_json::Value,
}
```

It diffs by whole-value canonical equality with a generic value-path change list
(`document.value_change`) and a reserialization-only signal
(`document.reserialized`, covering JSON key reorder / indentation where `value` is
equal but `source` differs). The existing JSON serialization/value-change facts
move under this generic vocabulary.

### Detection boundary

Record-collection detection runs **at the parse root**:

- A JSON document whose top level is an array of like-shaped objects (or an object
  map of like-shaped objects with a detectable key) → `tabular`.
- JSONL where lines share a shape → `tabular`; mixed-shape JSONL →
  `structured_document`.
- Otherwise → `structured_document`.

A record array *nested inside* a document is just an equality `Nested` cell in v1;
it becomes node-level only via the future SQLite-table-style expand path (CFM-69),
which is explicitly a non-goal of records-as-rows. Detection is conservative: when
shape/key stability isn't clear, fall back to `structured_document` rather than
forcing a ragged table.

### Parsers shipped with this change

To prove the artifact works across formats (not just JSON), stdlib ships:

- **→ `tabular`:** CSV/TSV (rewritten onto `Value`, all-`String` cells), JSON
  array-of-objects, JSONL (like-shaped).
- **→ `structured_document`:** JSON (existing, retargeted), YAML, TOML.

Plugins continue to produce `tabular` from their formats unchanged in spirit
(`binoc-sqlite` tables, `binoc-stat-binary` SAS/Stata/XPT) — they construct
`TabularData` via the new typed constructor and can supply `declared_type`.

## Consequences

- One artifact, one vocabulary, one renderer path for the entire record cluster;
  the nesting and type spectrum lives in *data-derived facts*, not in the
  plugin-facing surface.
- CSV behavior and snapshots are essentially preserved (all-string cells);
  the new signal (types, precision, nested equality) appears only for sources
  that actually carry it.
- `structured_document` gives every tree format a working diff for free and a
  clear place to add parsers without touching the engine.
- Blast radius (all updated in this change): `binoc-sdk` (`TabularData`,
  `Value`, `Schema`, `tabular_extract`, `TabularDataPair`, new
  `StructuredDocument`/`structured_document_v1`); `binoc-stdlib`
  `correspondence/{parse,pair,compact,writers,mod}.rs` and
  `renderers/markdown.rs`; the three model plugins (`binoc-sqlite`,
  `binoc-stat-binary`, `binoc-row-reorder`); stdlib tests. ~114 snapshot files
  regenerate via `just snapshot-update`. No Python codec changes (the bindings
  move the IR whole).
- Physical-encoding fast paths (columnar/Arrow-backed storage for huge tables)
  are explicitly *not* in this change but are *not precluded*: `Value`-typed
  rows are the logical contract; a future internal encoding discriminant can
  back the same `TabularData` accessor API for the flat/columnar case.

## Alternatives Considered

- **Keep `tabular` all-string; do record diffs with a separate
  `record_collection` artifact.** Two artifacts to keep in lockstep, two writer
  paths, and CSV/DB-tables — which want identical treatment — split across them.
  Rejected: the value model, not the artifact identity, is the real axis. One
  artifact with a typed cell model and a shared "records" contract gets the reuse
  without the drift.
- **Make `tabular` recurse into nested values now.** Requires a tree-diff edit
  vocabulary (`record.set_path`, typed path edits) that the existing compaction
  rules can't consume, and a much larger change. Rejected for v1 in favor of the
  equality-only `Nested` stop, which is forward-compatible.
- **Stay stringly-typed and stringify JSON numbers/bools.** Loses precisely the
  signal (type change, precision) that motivates record diffing. Rejected.
- **One `Value` = `serde_json::Value` directly.** `serde_json::Value`'s `Null`
  and object/array variants don't model "canonicalized nested for equality"
  cleanly, and we want control over number/nested equality semantics. A purpose
  built enum that wraps `serde_json::Number`/`Value` for scalars and nested is
  clearer.
- **Subtype the artifact by nesting (`tabular_flat` vs `tabular_nested`).** Pushes
  a physical/performance concern into the plugin-facing vocabulary. Rejected:
  encoding is an internal concern under one logical artifact; consumers see
  records regardless.
- **Keep `json_document` JSON-only and add `yaml_document`, `toml_document`.**
  Needless proliferation of identical value-tree artifacts. Rejected in favor of
  one `structured_document` with a `format` tag.
