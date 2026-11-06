# Changelog

User-visible changes to the published packages: `binoc` on PyPI (CLI,
Python API, bundled format packs) and `binoc-sdk` on crates.io. The two are
versioned independently; an entry that affects only `binoc-sdk` says so.

Add an entry under **Unreleased** in the same PR as the change. When a
release is cut, the version-bump PR renames that section to the version and
date (see [Cut a release](docs/core-developers/howto/cut-a-release.md)).

## Unreleased

## binoc 0.3.0 and binoc-sdk 0.3.0 — 2026-11-06

### Breaking changes

- **Dataset config format.** `dataset.tables` is removed. Row keys, CSV
  dialects, JSON `records_path`, and dispatch overrides are all set per path
  under `dataset.paths`:

  ```yaml
  # 0.2.0
  dataset:
    tables:
      - path: data.csv
        columns: [id]
  # 0.3.0
  dataset:
    paths:
      - match: data.csv
        row_identity:
          columns: [id]
  ```

- **Unknown dataset config keys are errors.** Any top-level `dataset` key
  other than `defaults`, `paths`, `files`, `correspondence`,
  `reduced_precision`, and `exclude` is reported as
  `binoc.dataset_config.unknown_field`, including `tables` from 0.2.0
  configs. Unknown keys inside those sections make the block unreadable
  (`binoc.dataset_config.invalid`, now an error rather than a warning).
  `binoc diff` prints these to stderr and exits 2, after writing its outputs.
- **Errors exit 2.** Failures through the `binoc` console script exited 1 in
  0.2.0. Exit 1 is now reserved for `binoc diff --exit-code`.
- **Row-reorder pack removed.** The standard library already detects row
  reorders, with the same `binoc.row-reorder` tag and `tabular.reorder_rows`
  edit; the pack's own summary text ("Rows reordered (same data, different
  sort order)") no longer appears. The `binoc-cli` `row-reorder` feature is
  gone ([ADR](docs/adr/2026-11-06-retire_row_reorder_plugin.md)).
- **Rust hosts:** `binoc_cli::run` returns the process exit status
  (`Result<i32, _>`); `run_with_version` reports a host-supplied version.

### New

- `binoc diff --exit-code`: exit 1 if any compared pair changed, 0 if all are
  identical.
- `dataset.exclude` and `binoc diff --exclude GLOB` leave paths out of the
  comparison on both sides, including archive members. Use them for capture
  manifests and other files that change on every capture.
- `binoc --version`, and `metadata["binoc.version"]` in every changeset written
  by `binoc diff` or `binoc report`.
- `binoc report` writes a local bundle (snapshots, config, trace, outputs) for
  reporting an imperfect diff.
- SQLite files (`.sqlite`, `.sqlite3`, `.db`) are compared table by table
  (`data.sqlite/>table`) with column, row, and cell diffs; the SQLite pack is
  now in the default wheel. In 0.2.0 these files were compared as bytes. Rows
  are matched by position unless a row key is configured for the table path.
- Per-path dataset config (`dataset.paths`): dispatch overrides (`rule`,
  `content_type`), tabular `shape`, CSV `dialect`, `row_identity`, JSON
  `records_path` keying, and structured-document `node_identity`.
- Row keys apply to tables inside `.gz` files; match the inner member, e.g.
  `fatalities/*.csv.gz/*.csv`.
- Keyed JSON records and keyed structured documents.
- CSV dialects are sniffed regardless of extension, and can be declared.
- CSV column renames are detected alongside reorders and keyed rows.
- Reduced-precision suppression sentinels are configurable
  (`dataset.reduced_precision`).
- Binary files report byte-range localized edits.
- Extensionless text files are content-sniffed instead of compared as binary.
- The tabular memory threshold and decompression caps are configurable
  (`dataset.correspondence`).
- Linux aarch64 wheels, alongside macOS x86_64/arm64, Linux x86_64, and
  Windows x86_64.

### Fixed

- `extract` uses configured row keys for `rows_added`, `rows_removed`, and
  `cells_changed`, so it agrees with a keyed changeset. Before, it compared by
  row position and could return rows that had not changed. For keyed tables,
  `cells_changed` output has a `key` column instead of `row`.
- JSON record tables (GeoJSON FeatureCollections and `records_path`) report
  changes outside the records, such as a feed's `metadata.generated` or a STIX
  bundle `id`, as table metadata changes. Before, a change only there showed
  as "no visible edit explained the difference", and alongside record changes
  it was not shown at all.
- With a configured row key, a table whose rows were only re-sorted reports
  `tabular.reorder_rows` (`binoc.row-reorder`). Before, it showed as "no visible
  edit explained the difference" unless the row-reorder pack was installed.
- Identical comparisons render "No changes detected." in Markdown.
- `binoc diff` with a missing snapshot names the path.
- `extract` hints point at known changeset paths.
- CSV delimiter sniffing false positives, including decimal-comma files.
- UTF-8 boundary handling in content sniffing.
- Correspondence and plugin-registry hardening.

### binoc-sdk

- `DatasetSemanticsV1` gains `exclude` and `KNOWN_FIELDS`; its nested config
  structs reject unknown fields.
- `tabular_extract_keyed` for row-key-aware tabular extraction.

## binoc 0.2.0 — 2026-07-01

- Correspondence-first diff engine
  ([ADR](docs/adr/2026-06-12-correspondence_first_engine.md)).
- One `binoc` wheel with the first-party format packs compiled in: Excel,
  Parquet, Avro, DBF, XML, Shapefile, CBOR/BSON/MessagePack/Ion/plist,
  Stata/SAS, and row-reorder detection
  ([ADR](docs/adr/2026-06-30-fat_binoc_distribution_and_abi_canary.md)).
  Separate publishing of `binoc-sqlite` and `binoc-stat-binary` paused.

## binoc 0.1.1 — 2026-04-10

- First release on PyPI.
