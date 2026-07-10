---
audience: data steward, plugin consumer
---

# Plugin catalog

Binoc ships a capable [standard library](../../plugin-developers/explanation/plugin-model.md) (`binoc-stdlib`) plus first-party format packs. Most format packs are compiled into the fat `binoc` wheel; SQLite remains an explicit opt-in pack and is not published as a separate PyPI wheel.

For format parsers, compare your filenames (suffixes) and, when available, detected media types to the tables under each plugin. Other scopes identify group parsers, artifact writers, and changeset renderers explicitly.

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

### Dispatch

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.avro` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
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

### Dispatch

#### `binoc-binformats.parse.cbor`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.cbor` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-binformats.parse.msgpack`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.msgpack`, `.mp` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-binformats.parse.bson`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.bson` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-binformats.parse.plist`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.plist` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-binformats.parse.ion`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.ion` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
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

### Dispatch

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.dbf` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
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

### Dispatch

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-html

Renders Binoc changesets as self-contained HTML changelogs.

| Field | Value |
|---|---|
| Tier | First-party add-on |
| Distribution | Maintained in this repository but distributed outside the default fat `binoc` wheel. |
| Handles | Completed Binoc changesets. |
| Produces | Self-contained HTML changelogs. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html)
- **Source path:** `model-plugins/binoc-html`
- **PyPI:** `binoc-html`

### Dispatch

This renderer consumes completed Binoc changesets.

| Field | Value |
|---|---|
| `extensions` | - |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `changesets` |

*Rule families supplied:* `render`

## binoc-parquet

Parses Parquet and Arrow IPC snapshots so column, row, and cell changes appear as ordinary tabular diffs.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Parquet, Arrow IPC, and Feather files. |
| Produces | Tabular records and tabular diffs. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-parquet](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-parquet)
- **Source path:** `model-plugins/binoc-parquet`
- **Rust crate:** `binoc-parquet`

### Dispatch

#### `binoc-parquet.parse.parquet`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.parquet` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-parquet.parse.arrow-ipc`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.arrow`, `.feather`, `.ipc` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

## binoc-row-reorder

Detects row reordering in tabular data so shuffles are represented as ordering changes rather than as noisy row removals and additions.

| Field | Value |
|---|---|
| Tier | First-party bundled |
| Distribution | Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature. |
| Handles | Paired `binoc.tabular.v1` artifacts with the same rows in a different order. |
| Produces | A `tabular.reorder_rows` edit when row multisets match. |

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)
- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder)
- **Source path:** `model-plugins/binoc-row-reorder`
- **Rust crate:** `binoc-row-reorder`

### Dispatch

This writer consumes paired artifacts in the listed **artifact formats**, independent of the source filename.

| Field | Value |
|---|---|
| `extensions` | - |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | `binoc.tabular.v1` |
| `scope` | `artifacts` |

*Rule families supplied:* `writer`

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

### Dispatch

#### `binoc-shapefile.fuse`

This group parser uses the listed **extensions** as anchors and correlates sibling files with the listed **member extensions**.

| Field | Value |
|---|---|
| `extensions` | `.shp` |
| `media_types` | - |
| `member_extensions` | `.shx`, `.dbf`, `.prj`, `.cpg` |
| `artifact_formats` | - |
| `scope` | `file-groups` |

*Rule families supplied:* `group-parse`

#### `binoc-shapefile.parse.shp`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.shp` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

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

### Dispatch

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.sqlite`, `.sqlite3`, `.db` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

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

### Dispatch

#### `binoc-stat-binary.stata.parse`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.dta` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-stat-binary.sas7bdat.parse`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.sas7bdat` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-stat-binary.xpt.parse`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.xpt` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
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

### Dispatch

#### `binoc-xml.parse.xml`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | `.xml`, `.rdf`, `.kml`, `.gml`, `.atom`, `.rss` |
| `media_types` | - |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

#### `binoc-xml.parse.xml_media`

This rule is relevant when a file path matches one of the **extensions** or its detected **media type** matches.

| Field | Value |
|---|---|
| `extensions` | - |
| `media_types` | `text/xml`, `application/xml`, `application/rdf+xml`, `application/atom+xml` |
| `member_extensions` | - |
| `artifact_formats` | - |
| `scope` | `files` |

*Rule families supplied:* `parse`

## Catalog file for tools

The canonical data lives in `plugin_registry.json` (JSON). Hosts that suggest plugins for unrecognized formats should read that file; dispatch fields describe the rule's advertised selectors and processing scope.
