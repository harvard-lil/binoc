# Tabular Collection Artifact Model

**Date:** 2026-06-01
**Status:** Accepted

## Context

Binoc already has a standard `binoc.tabular.v1` artifact for one table. That is
enough for a single CSV, but several common dataset shapes contain multiple
logical tables:

- an Excel workbook with multiple sheets
- a SQLite database with multiple tables
- a directory of CSV files that together form one dataset
- one CSV containing multiple logical table regions

Today the SQLite plugin is "multi-table-ish" but does its own schema/row-count
diffing and emits `sqlite_table` child nodes directly. That does not give Excel,
SQLite, CSV regions, and future formats a shared model for table-level reporting
or keyed row analysis.

The cross-plugin artifact decision already established the right mechanism:
standard public artifacts are SDK-owned schema contracts, and the controller
only carries opaque descriptors and handles.

## Decision

Binoc will standardize a `binoc.tabular_collection.v1` artifact, serialized as
JSON, for "this source contains multiple logical tables." Individual tables
continue to publish `binoc.tabular.v1` artifacts.

The collection artifact is a manifest, not a second copy of table data. It
records table identities, source locations, and shape summaries so generic
transformers and renderers can reason about table sets without knowing whether
the source was Excel, SQLite, CSV, or something else.

### Type sketch

```rust
pub fn tabular_collection_v1() -> ArtifactFormat {
    ArtifactFormat::new("binoc", "tabular_collection", 1)
}

pub struct TabularCollectionData {
    pub tables: Vec<TableMember>,
}

pub struct TableMember {
    /// Stable identity used to match this table across snapshots.
    pub logical_name: String,

    /// Where renderers and extractors should find the table node in the IR.
    pub node_path: String,

    /// Where the table came from inside the source artifact.
    pub source: TableSourceLocation,

    /// Cheap shape summary. Full cell data lives in tabular_v1 artifacts on
    /// table nodes.
    pub shape: TableShape,

    /// Optional plugin/domain metadata. Consumers must ignore unknown fields.
    pub metadata: BTreeMap<String, serde_json::Value>,
}

pub struct TableSourceLocation {
    /// Logical path of the source item in the Binoc tree.
    pub item_path: String,

    /// Format-neutral source kind: "file", "sheet", "sqlite_table",
    /// "csv_region", etc. Open string, not an enum enforced by core.
    pub kind: String,

    /// Source-specific locator, such as {"sheet": "Products"} or
    /// {"table": "products"} or {"start_row": 42, "end_row": 99}.
    pub locator: BTreeMap<String, serde_json::Value>,
}

pub struct TableShape {
    pub columns: Vec<String>,
    pub row_count: Option<u64>,
}
```

`logical_name` and `source` are both required. `logical_name` is the stable table
identity used for matching. `source` is provenance: it lets users understand
where the table came from and lets extractors reopen the original source if
needed.

The schema uses open strings and metadata maps for source kinds because the SDK
must not bake in every table-bearing format. The stable contract is the
collection/table shape, not a closed list of container technologies.

### IR shape

A multi-table comparator returns a collection node with children:

```text
data.xlsx                         action: modify  item_type: tabular_collection
  data.xlsx::Applications          action: modify  item_type: tabular
  data.xlsx::Products              action: add     item_type: tabular
  data.xlsx::Submissions           action: modify  item_type: tabular
```

The collection node publishes left/right `tabular_collection_v1` artifacts. Each
table child publishes left/right `tabular_v1` artifacts as applicable.

Example artifact layout:

```json
{
  "tables": [
    {
      "logical_name": "Products",
      "node_path": "data.xlsx::Products",
      "source": {
        "item_path": "data.xlsx",
        "kind": "sheet",
        "locator": {"sheet": "Products"}
      },
      "shape": {
        "columns": ["BLA Number", "Product Number", "Drug Name"],
        "row_count": 214
      },
      "metadata": {}
    }
  ]
}
```

A SQLite comparator uses the same model:

```json
{
  "logical_name": "products",
  "node_path": "data.sqlite::products",
  "source": {
    "item_path": "data.sqlite",
    "kind": "sqlite_table",
    "locator": {"table": "products"}
  },
  "shape": {"columns": ["id", "name"], "row_count": 52},
  "metadata": {}
}
```

One CSV containing multiple logical tables also uses the same model:

```json
{
  "logical_name": "adverse_events",
  "node_path": "report.csv::adverse_events",
  "source": {
    "item_path": "report.csv",
    "kind": "csv_region",
    "locator": {"start_row": 42, "end_row": 120}
  },
  "shape": {"columns": ["case_id", "event_seq"], "row_count": 78},
  "metadata": {}
}
```

### Table matching

Table identity is `logical_name`. Source location is not part of the key because
sheet names, SQL table names, or CSV regions can move while representing the
same logical table. Source location is retained as provenance and for diagnostic
messages.

Comparators derive `logical_name` using this precedence:

1. explicit `dataset.tables.entries.<name>.match` config
2. native logical name, such as workbook sheet name or SQLite table name
3. source-derived fallback, such as a CSV stem or generated region name

If two tables in the same side resolve to the same `logical_name`, the
collection has an ambiguous table identity. The comparator should emit a
diagnostic tagged `binoc.table-identity-ambiguous` and avoid pretending the
tables can be matched one-to-one.

### Transformer composition

The existing thin-comparator pattern still applies.

Multi-table comparators:

1. parse the source format
2. publish `tabular_collection_v1` on the collection node
3. publish `tabular_v1` on each table child
4. compare table set identity by `logical_name`
5. emit bare collection and table nodes with artifacts

Generic transformers then do the analysis:

- `binoc.table_collection_analyzer` compares collection manifests and annotates
  table additions, removals, table renames if later supported, and collection
  summaries.
- `binoc.tabular_analyzer` continues to analyze individual table children using
  `tabular_v1`.
- keyed row diffing consumes table-local row identity from dataset config and
  `tabular_v1` data from the child node.
- later statistical transformers can tag high-churn tables without needing to
  know Excel, SQLite, or CSV parsing.

This keeps source parsing in comparators and semantic analysis in transformers.
The controller remains unaware of table collections.

### Renderer shape

Renderers should treat a collection node as a table set and report table-level
changes before row/cell details. The Markdown shape should be:

```markdown
## data.xlsx

- Applications changed: 2 rows added; 1 cell changed.
- Products added: 214 rows, 3 columns.
- Submissions churned: 83% of rows changed; review as a replacement candidate.
```

The collection-level summary should use table names and table actions:

- `table A changed`
- `table B added`
- `table C removed`
- `table D churned`

Detailed row, column, and cell changes remain on table child nodes. The
collection summary is a navigation layer, not a replacement for child detail.

Standard collection tags:

- `binoc.table-addition`
- `binoc.table-removal`
- `binoc.table-change`
- `binoc.table-churn`
- `binoc.table-identity-ambiguous`
- `binoc.tabular-collection-change`

As elsewhere, these are factual tags. Markdown or future renderers map them to
significance categories through renderer config.

## Consequences

- Excel, SQLite, CSV-region, and directory-of-CSV plugins can share generic
  collection/table transformers.
- SQLite can migrate away from plugin-private table child analysis toward
  standard `tabular_collection_v1` plus `tabular_v1`.
- `tabular_v1` remains the unit for actual row/column/cell analysis.
- The collection artifact avoids copying full table data while still making
  table identity and source provenance available to downstream plugins.
- The SDK gains another standard public artifact schema, so compatibility rules
  from the published-artifacts ADR apply.

## Alternatives Considered

**Put all tables into one large `tabular_v1` artifact with a table-name
column.** Rejected because it loses native table boundaries, makes per-table
keys awkward, and cannot represent different schemas cleanly.

**Make SQLite/Excel comparators emit only child `tabular_v1` nodes and skip a
collection artifact.** This gives the renderer a tree but no standard manifest
for table identity, source locations, or table-set analysis. Generic collection
transformers would have to infer too much from paths.

**Make table source location part of identity.** Rejected because a sheet can be
renamed or a table can move inside a CSV while remaining the same logical table.
Source location is provenance; `logical_name` is identity.

**Standardize a relational-schema artifact instead.** Useful for SQL-specific
schema work, but too narrow for Excel sheets and CSV regions. Relational-schema
artifacts can still exist as plugin-owned or future SDK artifacts alongside the
format-neutral collection manifest.

**Put collection semantics in core IR types.** Rejected because it violates the
type-ignorant controller rule. A collection is a convention expressed through
open `item_type`, tags, child nodes, and standard artifacts.
