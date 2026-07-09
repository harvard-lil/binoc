---
audience: data steward, plugin consumer
---

# Plugin catalog

Binoc ships a capable [standard library](../../plugin-developers/explanation/plugin-model.md) (`binoc-stdlib`) plus first-party format packs. Most format packs are compiled into the fat `binoc` wheel; SQLite remains an explicit opt-in pack and is not published as a separate PyPI wheel.

To find a match, compare your filenames (suffixes) and, when available, detected media types to the tables under each plugin.

For package ids that may appear in changelog output, see the [plugin registry](plugin-registry.md).

!!! tip "Publishing or listing a plugin"

    If you maintain a plugin and want it listed here, see [Publish a plugin](../../plugin-developers/howto/publish-a-plugin.md).

!!! note "Generated page"

    Entries are maintained in `plugin_registry.json` at the repository root. Maintainers regenerate this Markdown with `scripts/build_third_party_plugins_page.py` (`just docs-plugin-catalog`).

## binoc-avro

Parses Avro snapshots as structured data so changes in records and fields surface as normal Binoc diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Avro files. |
| Produces | Parsed Avro records and field-level changes. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-avro](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-avro)
- **Source path:** `model-plugins/binoc-avro`
- **Rust crate:** `binoc-avro`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.avro` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-binformats](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-binformats)
- **Source path:** `model-plugins/binoc-binformats`
- **Rust crate:** `binoc-binformats`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | - |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-dbf](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-dbf)
- **Source path:** `model-plugins/binoc-dbf`
- **Rust crate:** `binoc-dbf`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.dbf` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-excel](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-excel)
- **Source path:** `model-plugins/binoc-excel`
- **Rust crate:** `binoc-excel`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.xlsx` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html)
- **Source path:** `model-plugins/binoc-html`
- **PyPI:** `binoc-html`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.html`, `.htm` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-parquet](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-parquet)
- **Source path:** `model-plugins/binoc-parquet`
- **Rust crate:** `binoc-parquet`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.parquet` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder)
- **Source path:** `model-plugins/binoc-row-reorder`
- **Rust crate:** `binoc-row-reorder`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.csv`, `.tsv` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-shapefile](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-shapefile)
- **Source path:** `model-plugins/binoc-shapefile`
- **Rust crate:** `binoc-shapefile`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.shp`, `.dbf`, `.shx` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite)
- **Source path:** `model-plugins/binoc-sqlite`
- **Rust crate:** `binoc-sqlite`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-stat-binary](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-stat-binary)
- **Source path:** `model-plugins/binoc-stat-binary`
- **Rust crate:** `binoc-stat-binary`

### When it handles your files

#### `binoc-stat-binary.stata`

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.dta` |
| `media_types` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-stat-binary.sas7bdat`

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.sas7bdat` |
| `media_types` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-stat-binary.xpt`

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.xpt` |
| `media_types` | - |
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
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-xml](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-xml)
- **Source path:** `model-plugins/binoc-xml`
- **Rust crate:** `binoc-xml`

### When it handles your files

This rule pack is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.xml` |
| `media_types` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

## Catalog file for tools

The canonical data lives in `plugin_registry.json` (JSON). Hosts that suggest plugins for unrecognized formats should read that file; dispatch fields describe the rule pack's advertised file selectors.
