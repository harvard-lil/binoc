---
audience: data steward, plugin consumer
---

# Plugin registry

This registry covers the plugins readers are most likely to encounter in changelog output: the built-in `binoc-stdlib` pack plus the in-tree `model-plugins/` packs.

Use the package ids as stable anchors when linking from rendered changesets. The generated headings below match those ids one-for-one.

!!! note "Generated page"

    Entries are maintained in `plugin_registry.json` at the repository root. Maintainers regenerate this Markdown with `scripts/build_plugin_registry_page.py`.

## binoc-stdlib

Binoc's built-in rule pack. It handles the baseline formats and structural cases most readers encounter first: directories, archives, compressed streams, CSV/TSV, common text formats, JSON/YAML/TOML/INI, generic binary fallback, move/copy detection, and output renderers.

| Field | Value |
|---|---|
| Tier | Built in |
| Distribution | Included with every binoc build. |
| Handles | Directory and archive structure, tabular data, text and structured documents, compressed snapshots, binary fallback, move/copy correspondence, and Markdown output. |
| Produces | Container expansions, tabular and text edits, move/copy and reshape claims, binary fallback diagnostics, and Markdown changelogs. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/binoc-stdlib](https://github.com/harvard-lil/binoc/tree/main/binoc-stdlib)
- **Source path:** `binoc-stdlib`
- **Rust crate:** `binoc-stdlib`

### Rule packs

#### `binoc-stdlib`

| Field | Value |
|---|---|
| `extensions` | — |
| `media_types` | — |
| `scope` | `all` |

*Rule families supplied:* `expand`, `parse`, `pair`, `writer`, `compaction`, `annotator`, `render`

## binoc-avro

Parses Avro snapshots as structured data so changes in records and fields surface as normal Binoc diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Avro files. |
| Produces | Parsed Avro records and field-level changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-avro](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-avro)
- **Source path:** `model-plugins/binoc-avro`
- **Rust crate:** `binoc-avro`

### Rule packs

#### `binoc-avro.avro`

| Field | Value |
|---|---|
| `extensions` | `.avro` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-binformats

Parses common binary interchange formats into JSON-like values so content changes become visible as structured diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | CBOR, BSON, Ion, MessagePack, and plist snapshots. |
| Produces | Parsed structured values and value-level changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-binformats](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-binformats)
- **Source path:** `model-plugins/binoc-binformats`
- **Rust crate:** `binoc-binformats`

### Rule packs

#### `binoc-binformats.binformats`

| Field | Value |
|---|---|
| `extensions` | — |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-dbf

Parses DBF snapshots so table and row changes in legacy dBase-style datasets show up as structured tabular diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | DBF files. |
| Produces | Tabular records and table/row edits. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-dbf](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-dbf)
- **Source path:** `model-plugins/binoc-dbf`
- **Rust crate:** `binoc-dbf`

### Rule packs

#### `binoc-dbf.dbf`

| Field | Value |
|---|---|
| `extensions` | `.dbf` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-excel

Parses Excel workbooks so sheet, row, column, and cell changes can be compared as Binoc tabular changes.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Excel workbook snapshots. |
| Produces | Workbook structure and sheet/table content changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-excel](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-excel)
- **Source path:** `model-plugins/binoc-excel`
- **Rust crate:** `binoc-excel`

### Rule packs

#### `binoc-excel.excel`

| Field | Value |
|---|---|
| `extensions` | `.xlsx` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `expand`, `parse`

## binoc-html

Supports HTML-related parsing and rewriting for datasets that publish or embed structured content in HTML snapshots.

| Field | Value |
|---|---|
| Tier | First-party add-on |
| Distribution | Maintained in this repository but distributed outside the default fat `binoc` wheel. |
| Handles | HTML files and HTML-embedded structured data. |
| Produces | Parsed HTML-derived artifacts and content changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html)
- **Source path:** `model-plugins/binoc-html`
- **PyPI:** `binoc-html`

### Rule packs

#### `binoc-html.html`

| Field | Value |
|---|---|
| `extensions` | `.html`, `.htm` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-parquet

Parses Parquet snapshots so column, row, and cell changes appear as ordinary tabular diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Parquet files. |
| Produces | Tabular records and tabular diffs. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-parquet](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-parquet)
- **Source path:** `model-plugins/binoc-parquet`
- **Rust crate:** `binoc-parquet`

### Rule packs

#### `binoc-parquet.parquet`

| Field | Value |
|---|---|
| `extensions` | `.parquet` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-row-reorder

Detects row reordering in tabular data so shuffles are represented as ordering changes rather than as noisy row removals and additions.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Tabular collections with reordered rows. |
| Produces | Row-reorder correspondence and compact move-like row changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder)
- **Source path:** `model-plugins/binoc-row-reorder`
- **Rust crate:** `binoc-row-reorder`

### Rule packs

#### `binoc-row-reorder.csv`

| Field | Value |
|---|---|
| `extensions` | `.csv`, `.tsv` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `pair`, `writer`

## binoc-shapefile

Handles shapefile bundles and their sidecar files so feature, geometry, and attribute changes stay aligned across the dataset package.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Shapefile bundles and related sidecars. |
| Produces | Feature-level and geometry-level diffs. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-shapefile](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-shapefile)
- **Source path:** `model-plugins/binoc-shapefile`
- **Rust crate:** `binoc-shapefile`

### Rule packs

#### `binoc-shapefile.shapefile`

| Field | Value |
|---|---|
| `extensions` | `.shp`, `.dbf`, `.shx` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `expand`, `parse`

## binoc-sqlite

Compares SQLite databases: schema, columns, keys, and row counts. Useful when a dataset ships as `.db` / `.sqlite` snapshots instead of flat files.

| Field | Value |
|---|---|
| Tier | First-party opt-in |
| Distribution | Not published as a separate PyPI wheel. The pack remains in-tree and can be enabled explicitly; SQLite is excluded from the default `bundled` feature set. |
| Handles | SQLite database files. |
| Produces | Table/column layout changes, type changes, key changes, and row-count diffs. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite)
- **Source path:** `model-plugins/binoc-sqlite`
- **Rust crate:** `binoc-sqlite`

### Rule packs

#### `binoc-sqlite.sqlite`

| Field | Value |
|---|---|
| `extensions` | `.sqlite`, `.sqlite3`, `.db` |
| `media_types` | `application/vnd.sqlite3`, `application/x-sqlite3` |
| `scope` | `files` |

*Rule families supplied:* `parse`, `writer`, `materializer`

## binoc-stat-binary

Reads Stata, SAS, and SAS transport files as standard Binoc tabular data so normal column, row, and cell diffing applies.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Stata `.dta`, SAS `.sas7bdat`, and SAS transport `.xpt` files. |
| Produces | Tabular records and tabular diffs for statistical binary formats. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-stat-binary](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-stat-binary)
- **Source path:** `model-plugins/binoc-stat-binary`
- **Rust crate:** `binoc-stat-binary`

### Rule packs

#### `binoc-stat-binary.stata`

| Field | Value |
|---|---|
| `extensions` | `.dta` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-stat-binary.sas7bdat`

| Field | Value |
|---|---|
| `extensions` | `.sas7bdat` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-stat-binary.xpt`

| Field | Value |
|---|---|
| `extensions` | `.xpt` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-xml

Parses XML snapshots into structured data so hierarchical document and metadata changes can be tracked as Binoc diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | XML files and XML-based metadata documents. |
| Produces | Structured XML artifacts and element/attribute-level changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **Documentation:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-xml](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-xml)
- **Source path:** `model-plugins/binoc-xml`
- **Rust crate:** `binoc-xml`

### Rule packs

#### `binoc-xml.xml`

| Field | Value |
|---|---|
| `extensions` | `.xml` |
| `media_types` | — |
| `scope` | `files` |

*Rule families supplied:* `parse`
