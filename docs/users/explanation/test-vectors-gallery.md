---
audience: new user, data steward, archivist
---

# Examples gallery

<!--
  GENERATED FILE — do not edit by hand.
  Source of truth: test-vectors/*/manifest.toml, committed snapshot trees,
  and expected-output/changelog.snap files.
  Regenerate with `just docs-vectors`.
-->

These are runnable examples from binoc's test suite. Each example links to its source folder on GitHub, tells you whether it needs any extra setup, gives you the exact command to run, and shows the Markdown changelog binoc is expected to print.

Binoc currently ships **82 shared examples** in this gallery.

## One-time setup

Clone the repository and materialize the archive-based fixtures once:

```bash
git clone https://github.com/harvard-lil/binoc
cd binoc
just materialize
```

## At a glance

| Example | What it shows | Example output | Setup |
|---|---|---|---|
| [`binary-byte-range-localized`](#binary-byte-range-localized) | An opaque binary changes in one localized byte region; CDC byte-range localization reports the changed range and unchan… | payload.bin: 1 changed byte range; 66.667% unchanged; first range left [65,536, 131,072) to right [… | Default pipeline |
| [`binary-fallback-diagnostic`](#binary-fallback-diagnostic) | Unknown file type compared by the binary fallback emits a suggestion | data.parquet: Binary content changed; 1 extracted string added, 1 extracted string removed | Default pipeline |
| [`binary-strings-fallback`](#binary-strings-fallback) | Two opaque binary blobs with differing hashes. The change is hash-driven (binoc.content-changed), and an additive extra… | firmware.bin: Binary content changed; 2 extracted strings added, 2 extracted strings removed | Default pipeline |
| [`content-sniff-extensionless-tz`](#content-sniff-extensionless-tz) | Extensionless IANA tz source files are sniffed as text and disclose that inference; a binary sibling in the same bundle… | asia: 1 line added | Default pipeline |
| [`csv-auto-detected-pipe-dialect`](#csv-auto-detected-pipe-dialect) | Extensionless tabular input auto-detects a pipe dialect, discloses the inference, and still reports keyed cell changes | records: 1 row modified by key | Custom config |
| [`csv-auto-key-resort`](#csv-auto-key-resort) | Auto-detected CSV row keys turn a high positional-churn re-sort into a keyed cell change | data.csv: 1 row modified by key | Default pipeline |
| [`csv-cell-changes`](#csv-cell-changes) | Individual cell values changed | data.csv: 2 cells changed | Default pipeline |
| [`csv-column-addition`](#csv-column-addition) | New column added | data.csv: Column added: 'email' | Default pipeline |
| [`csv-column-removal`](#csv-column-removal) | Column removed | data.csv: Column removed: 'city' | Default pipeline |
| [`csv-column-rename-near-miss`](#csv-column-rename-near-miss) | A weak content overlap should remain a column add plus remove | data.csv: Column added: 'status'; Column removed: 'legacy' | Default pipeline |
| [`csv-column-rename-reorder`](#csv-column-rename-reorder) | A renamed CSV column also moves position | data.csv: Column renamed: 'status' -> 'state'; Columns reordered | Default pipeline |
| [`csv-column-reorder`](#csv-column-reorder) | Columns shuffled, content identical | data.csv: Columns reordered | Default pipeline |
| [`csv-declared-pipe-dialect`](#csv-declared-pipe-dialect) | Per-path declared pipe dialect parses a .txt table silently and reports keyed cell changes | data.txt: 1 row modified by key | Custom config |
| [`csv-disjoint-cohort-guardrail`](#csv-disjoint-cohort-guardrail) | Disjoint CSV cohorts trip the high-churn guardrail instead of publishing a fictional per-cell diff | data.csv: row correspondence uncertain; 1 changed-cell fraction | Default pipeline |
| [`csv-distribution-shift`](#csv-distribution-shift) | Numeric column distribution shifts with keyed row matching | data.csv: 4 rows modified by key | Custom config |
| [`csv-keyed-null-duplicate`](#csv-keyed-null-duplicate) | Configured CSV row keys surface null and duplicate key diagnostics | data.csv: 14 cells changed | Custom config |
| [`csv-keyed-row-diff`](#csv-keyed-row-diff) | Configured CSV row keys match reordered rows and report keyed row/cell changes | data.csv: 1 row added; 1 row removed; 1 row modified by key | Custom config |
| [`csv-mid-row-insertion`](#csv-mid-row-insertion) | A mid-table row insertion compacts while column reorder/addition rules remain independent | data.csv: Column added: 'email'; Columns reordered; 1 row added | Default pipeline |
| [`csv-mixed-changes`](#csv-mixed-changes) | Multiple change types | data.csv: Column added: 'email'; Columns reordered; 1 row added | Default pipeline |
| [`csv-numeric-rounding`](#csv-numeric-rounding) | CSV numeric cells rounded to a common modulus and representation-only numeric differences ignored | data.csv: Rounded 3 cells in 'population' to nearest 1000 | Default pipeline |
| [`csv-rename-modify`](#csv-rename-modify) | CSV renamed and modified: detected as a single move by fuzzy correlation | data_v2.csv: | Default pipeline |
| [`csv-row-addition`](#csv-row-addition) | New rows appended | data.csv: 2 rows added | Default pipeline |
| [`csv-row-insertion-suppression-sentinels`](#csv-row-insertion-suppression-sentinels) | Inserted rows with disclosure sentinels in count cells are reported as row additions, not value suppression | data.csv: 2 rows added | Default pipeline |
| [`csv-row-removal`](#csv-row-removal) | Rows removed from CSV | data.csv: 2 rows removed | Default pipeline |
| [`csv-sorted-row-fallback`](#csv-sorted-row-fallback) | CSV rows with no unique key use sorted row-content alignment so a pure re-sort is not reported as cell churn | data.csv: rows reordered; cell values unchanged under inferred row alignment | Default pipeline |
| [`csv-stacked-tables`](#csv-stacked-tables) | Detects two logical tables stacked in one messy CSV | data.csv/>table_2: 1 row added | Default pipeline |
| [`csv-to-tsv-reformat`](#csv-to-tsv-reformat) | Table reformatted from CSV to TSV with row edits: detected as one reformatted-and-modified table, not remove + add | data.tsv: | Default pipeline |
| [`csv-value-suppression`](#csv-value-suppression) | CSV cells replaced with disclosure suppression sentinels | data.csv: Suppressed 3 cells in 'count' | Default pipeline |
| [`csv-value-suppression-custom-sentinel`](#csv-value-suppression-custom-sentinel) | CSV cells replaced with a dataset-configured disclosure suppression sentinel | data.csv: 1 cell changed; Suppressed 2 cells in 'count' | Custom config |
| [`csv-verbosity-full`](#csv-verbosity-full) | Markdown full verbosity renders every captured changed-cell example. | data.csv: 5 cells changed | Custom config |
| [`csv-vintage-benchmark`](#csv-vintage-benchmark) | A 'vintage' reader compares two editions of the same published dataset and wants the structural story (a column appeared, a category vocabulary shifted) surfaced above the bulk data churn they intend to ignore. | facilities.csv: Column added: 'region'; 1 cell changed | Custom config |
| [`directory-file-copy`](#directory-file-copy) | New file with same content as an existing unchanged file detected as a copy | duplicate.txt: Copied from original.txt | Default pipeline |
| [`directory-nested`](#directory-nested) | Subdirectories with mixed changes | data/records.csv: 1 row added | Default pipeline |
| [`directory-nested-with-tar`](#directory-nested-with-tar) | Shows binoc diffing a tar archive and a plain directory that contain overlapping internal paths. | data.tar.gz/>records.csv: 1 cell changed | Default pipeline |
| [`enforcement-actions-merge-years`](#enforcement-actions-merge-years) | Per-year CSVs merged row-wise into one file; detected as a clean partition merge (CFM-72) | actions_2023.csv, actions_2024.csv merged into actions.csv | Default pipeline |
| [`file-correspondence-container`](#file-correspondence-container) | Config declares a correspondence between renamed zip containers | archive.zip: Moved from data.zip | Custom config |
| [`file-correspondence-scheme`](#file-correspondence-scheme) | Config declares that a state CSV moved into a new directory scheme is the same logical file | by-state: Added | Custom config |
| [`file-correspondence-token`](#file-correspondence-token) | Config declares that year-stamped CSV filenames are the same logical file | running_list_as_of_2023.csv: | Custom config |
| [`folder-move-nested`](#folder-move-nested) | Detects a whole-folder rename and rolls many file moves up into one folder-move entry. | documentation: Moved from docs | Default pipeline |
| [`folder-move-partial`](#folder-move-partial) | Detects a mostly-moved folder rename and preserves only the added/removed/modified remainder entries beneath it. | FoodData_Central_csv_2026-04-30: Added | Default pipeline |
| [`geojson-feature-cell-change`](#geojson-feature-cell-change) | A GeoJSON FeatureCollection where one feature's property changes; transcoded to a tabular artifact with the geometry as… | places.geojson: 1 cell changed | Default pipeline |
| [`gzip-inner-dispatch`](#gzip-inner-dispatch) | Gzipped CSV and text are decompressed and redispatched under their inner names | census.txt.gz/>census.txt: 1 line added; 1 line removed | Default pipeline |
| [`ini-value-change`](#ini-value-change) | An INI value changes; transcoded to a structured_document and reported as a value change | config.ini: $.replicas: "3" -> "5" | Default pipeline |
| [`json-array-order-significant`](#json-array-order-significant) | JSON array order changes are semantic content changes in stage 1 | metadata.json: $.ids[1]: 2 -> 3; $.ids[2]: 3 -> 2 | Default pipeline |
| [`json-key-order-reexport`](#json-key-order-reexport) | JSON object key order and pretty-printing changed without semantic value changes | metadata.json: Document serialization changed | Default pipeline |
| [`json-keyed-row-diff`](#json-keyed-row-diff) | Configured JSON record keys match reordered rows and report keyed row/cell changes | data.json: 1 row added; 1 row removed; 1 row modified by key | Custom config |
| [`json-records-cell-change`](#json-records-cell-change) | JSON array of like-shaped objects parsed as a typed table; numeric cell values change | data.json: 2 cells changed | Default pipeline |
| [`json-records-nested-value`](#json-records-nested-value) | JSON records with a nested object cell; the nested value changes and is reported as a single equality-based cell edit (… | people.json: 1 cell changed | Default pipeline |
| [`json-records-path-stix-objects`](#json-records-path-stix-objects) | STIX-shaped JSON bundle records under $.objects are parsed as a keyed table | enterprise.stix.json: 1 row modified by key | Custom config |
| [`json-records-type-only-column`](#json-records-type-only-column) | Typed JSON records re-serialized with one numeric column as strings; canonical values match | data.json: Column type changed: 'year' number -> string | Default pipeline |
| [`jsonl-row-addition`](#jsonl-row-addition) | JSONL stream of like-shaped objects parsed as a table; a record is appended | events.jsonl: 1 row added | Default pipeline |
| [`jsonld-value-change`](#jsonld-value-change) | A .jsonld file with no declared media type parses as a structured document tagged format=jsonld; a value change is repo… | person.jsonld: $.jobTitle: "Mathematician" -> "Computer Scientist" | Default pipeline |
| [`kitchen-sink`](#kitchen-sink) | Runs text, CSV, archive, move, and copy detection together in one end-to-end example. | archive.tar.gz/>inventory.csv: 1 row added | Default pipeline |
| [`nasa-gistemp-header-line`](#nasa-gistemp-header-line) | NASA GISTEMP-style table with a title line before the real header | GLB.Ts+dSST.csv: 1 row modified by key | Custom config |
| [`observations-repartition-equal-arity`](#observations-repartition-equal-arity) | Equal-arity N→M repartition: 2 tables grouped by region become 2 tables grouped by year, every row preserved exactly bu… | observations_2024.csv: | Default pipeline |
| [`observations-split-by-year`](#observations-split-by-year) | One CSV split row-wise into per-year files; detected as a clean partition split (CFM-72) | observations.csv split into observations_2024.csv, observations_2025.csv | Default pipeline |
| [`observations-split-residual`](#observations-split-residual) | A would-be split missing one row: partition declines (not complete), emits binoc.possible_split, and degrades to honest… | observations_2024.csv: | Default pipeline |
| [`ofac-sdn-headerless-position-key`](#ofac-sdn-headerless-position-key) | OFAC SDN-style 12-column headerless CSV keyed by ent_num in column 1 | SDN.CSV: 1 row modified by key | Custom config |
| [`per-path-dispatch-override`](#per-path-dispatch-override) | Per-path dispatch overrides promote extensionless CSV content before tabular row keying | forced: 1 row modified by key | Custom config |
| [`single-file-add`](#single-file-add) | File present in B but not A | new_file.txt: Added | Default pipeline |
| [`single-file-modify-binary`](#single-file-modify-binary) | Binary file, different hash | data.bin: 1 edit | Default pipeline |
| [`single-file-modify-csv`](#single-file-modify-csv) | CSV file compared directly (file-to-file, not via directory) | data.csv: 1 row added | Default pipeline |
| [`single-file-modify-text`](#single-file-modify-text) | Text file with line-level changes | story.txt: 2 lines added; 1 line removed | Default pipeline |
| [`single-file-modify-text-root`](#single-file-modify-text-root) | Text file compared directly (file-to-file, not via directory) | story.txt: 2 lines added; 1 line removed | Default pipeline |
| [`single-file-remove`](#single-file-remove) | File present in A but not B | removed_file.txt: Removed | Default pipeline |
| [`stacked-csv-broken-out`](#stacked-csv-broken-out) | Stacked-CSV tables broken out into one file per table; whole-table rehoming (reshape + 1:1), NOT a partition split (CFM… | changes.csv: Moved from report.csv/>table_1 | Default pipeline |
| [`tar-nested`](#tar-nested) | Nested tar.gz containing CSV | outer.tar.gz/>inner.tar.gz/>data.csv: 1 row added | Default pipeline |
| [`tar-simple`](#tar-simple) | Tar.gz archive with changes inside | archive.tar.gz/>data.csv: 1 row added | Default pipeline |
| [`text-rename-modify`](#text-rename-modify) | Text file renamed and modified: detected as a single move by fuzzy correlation | meeting-notes-v2.txt: | Default pipeline |
| [`toml-value-change`](#toml-value-change) | A TOML value changes; transcoded to a structured_document and reported as a value change | config.toml: $.replicas: 3 -> 5 | Default pipeline |
| [`tree-wide-correlation`](#tree-wide-correlation) | Shows tree-wide move and copy detection across nested zip boundaries, including one-to-many copies and many-to-one moves. | gamma-renamed.txt: Moved from outer.zip/>inner.zip/>gamma.txt | Default pipeline |
| [`trivial-identical`](#trivial-identical) | Two identical directories → empty changeset | # Changelog: snapshot-a → snapshot-b | Default pipeline |
| [`trivial-identical-csv`](#trivial-identical-csv) | Two identical CSV files → no changes reported | # Changelog: snapshot-a → snapshot-b | Default pipeline |
| [`tsv-cell-changes`](#tsv-cell-changes) | Tab-delimited file parses into real columns and reports cell changes | data.tsv: 2 cells changed | Default pipeline |
| [`yaml-value-change`](#yaml-value-change) | A YAML scalar value changes; transcoded to a structured_document and reported as a value change | config.yaml: $.replicas: 3 -> 5 | Default pipeline |
| [`zip-declared-container`](#zip-declared-container) | Config declares a correspondence between nested zip containers and preserves inner CSV content detail | outer.zip/>records.zip: Moved from outer.zip/>records-old.zip | Custom config |
| [`zip-json-key-order-reexport`](#zip-json-key-order-reexport) | JSON files inside zip expansion get parsed and rendered as serialization-only changes | archive.zip/>metadata.json: Document serialization changed | Default pipeline |
| [`zip-nested`](#zip-nested) | Nested zip containing CSV | outer.zip/>inner.zip/>data.csv: 1 row added | Default pipeline |
| [`zip-rename-contents-rewritten`](#zip-rename-contents-rewritten) | Documents a known gap — a renamed zip whose children were all renamed AND rewritten (no content similarity) yields unpa… | data.zip: Removed | Default pipeline |
| [`zip-rename-identical`](#zip-rename-identical) | Zip archive renamed with identical contents; bottom-up roll-up of the inner clean file moves compacts the pair into a s… | archive.zip: Moved from data.zip | Default pipeline |
| [`zip-rename-inner-rename-edit`](#zip-rename-inner-rename-edit) | Zip archive renamed while its only child was renamed and had one cell edited; the modified move counts as roll-up evide… | archive.zip: Moved from data.zip | Default pipeline |
| [`zip-simple`](#zip-simple) | Zipped files with changes inside | archive.zip/>data.txt: 1 line added; 1 line removed | Default pipeline |

## binary-byte-range-localized

An opaque binary changes in one localized byte region; CDC byte-range localization reports the changed range and unchan…

- **Browse source:** [binary-byte-range-localized](https://github.com/harvard-lil/binoc/tree/main/test-vectors/binary-byte-range-localized)
- **Tags:** `modify`, `binary`, `byte-range`
- **Snapshots:** `snapshot-a` has 1 file — `payload.bin`; `snapshot-b` has 1 file — `payload.bin`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/binary-byte-range-localized/snapshot-a \
  ./test-vectors-materialized/binary-byte-range-localized/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **payload.bin**: 1 changed byte range; 66.667% unchanged; first range left [65,536, 131,072) to right [65,536, 131,072)
```

## binary-fallback-diagnostic

Unknown file type compared by the binary fallback emits a suggestion

- **Browse source:** [binary-fallback-diagnostic](https://github.com/harvard-lil/binoc/tree/main/test-vectors/binary-fallback-diagnostic)
- **Tags:** `modify`, `binary`, `diagnostics`
- **Snapshots:** `snapshot-a` has 1 file — `data.parquet`; `snapshot-b` has 1 file — `data.parquet`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/binary-fallback-diagnostic/snapshot-a \
  ./test-vectors-materialized/binary-fallback-diagnostic/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.parquet**: Binary content changed; 1 extracted string added, 1 extracted string removed
  - Extracted strings added
    - 'after!\n'
  - Extracted strings removed
    - 'before\n'
```

## binary-strings-fallback

Two opaque binary blobs with differing hashes. The change is hash-driven (binoc.content-changed), and an additive extra…

- **Browse source:** [binary-strings-fallback](https://github.com/harvard-lil/binoc/tree/main/test-vectors/binary-strings-fallback)
- **Tags:** `modify`, `binary`, `strings`
- **Snapshots:** `snapshot-a` has 1 file — `firmware.bin`; `snapshot-b` has 1 file — `firmware.bin`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/binary-strings-fallback/snapshot-a \
  ./test-vectors-materialized/binary-strings-fallback/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **firmware.bin**: Binary content changed; 2 extracted strings added, 2 extracted strings removed
  - Extracted strings added
    - 'build-beta'
    - 'version=2.0.0'
  - Extracted strings removed
    - 'build-alpha'
    - 'version=1.0.0'
```

## content-sniff-extensionless-tz

Extensionless IANA tz source files are sniffed as text and disclose that inference; a binary sibling in the same bundle…

- **Browse source:** [content-sniff-extensionless-tz](https://github.com/harvard-lil/binoc/tree/main/test-vectors/content-sniff-extensionless-tz)
- **Tags:** `text`, `binary`, `inference`, `content-sniff`
- **Snapshots:** `snapshot-a` has 4 files — `asia`, `europe`, `northamerica`, `tzdata.binless`; `snapshot-b` has 4 files — `asia`, `europe`, `northamerica`, `tzdata.binless`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/content-sniff-extensionless-tz/snapshot-a \
  ./test-vectors-materialized/content-sniff-extensionless-tz/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **asia**: 1 line added
  - Content type inference: treated asia as text (content sniff, no extension)
  - Line changes
    - line 2: 'Zone    Asia/Tokyo  9:18:59 -   LMT 1887 Dec 31 15:00u' -> 'Rule    Japan   1949    only    -   Apr Sat>=1  24:00   1:00    D'
- **europe**: 1 line added
  - Content type inference: treated europe as text (content sniff, no extension)
  - Line changes
    - line 2: 'Zone    Europe/Paris    0:09:21 -   LMT 1891 Mar 16' -> 'Rule    EU  1996    max -   Oct lastSun   1:00u   0   -'
- **northamerica**: 1 line added
  - Content type inference: treated northamerica as text (content sniff, no extension)
  - Line changes
    - line 2: 'Zone    America/New_York -4:56:02 -   LMT 1883 Nov 18 17:00u' -> 'Rule    US  2007    max -   Mar Sun>=8    2:00    1:00    D'
- **tzdata.binless**: 1 edit
```

## csv-auto-detected-pipe-dialect

Extensionless tabular input auto-detects a pipe dialect, discloses the inference, and still reports keyed cell changes

- **Browse source:** [csv-auto-detected-pipe-dialect](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-auto-detected-pipe-dialect)
- **Tags:** `csv`, `dialect`, `pipe`, `inference`, `extensionless`
- **Snapshots:** `snapshot-a` has 1 file — `records`; `snapshot-b` has 1 file — `records`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/csv-auto-detected-pipe-dialect.yaml`:

```yaml
dataset:
  paths:
    - match: records
      content_type: text/csv
      row_identity:
        columns:
          - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-auto-detected-pipe-dialect/snapshot-a \
  ./test-vectors-materialized/csv-auto-detected-pipe-dialect/snapshot-b \
  --config /tmp/csv-auto-detected-pipe-dialect.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **records**: 1 row modified by key
  - Dialect provenance: detected `|`-delimited, no quoting, newline LF
  - Changed cells
    - key id '1', column 'value': 'old' -> 'new'
```

## csv-auto-key-resort

Auto-detected CSV row keys turn a high positional-churn re-sort into a keyed cell change

- **Browse source:** [csv-auto-key-resort](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-auto-key-resort)
- **Tags:** `csv`, `auto-key`, `row-reorder`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-auto-key-resort/snapshot-a \
  ./test-vectors-materialized/csv-auto-key-resort/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 1 row modified by key
  - Changed cells
    - key id 'brfss-003', column 'rate': '14.8' -> '15.0'

## Suggestions

- inferred row identity column 'id' from unique values with 100% overlap (`binoc.write.tabular`) [binoc.tabular_auto_key]
```

## csv-cell-changes

Individual cell values changed

- **Browse source:** [csv-cell-changes](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-cell-changes)
- **Tags:** `csv`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-cell-changes/snapshot-a \
  ./test-vectors-materialized/csv-cell-changes/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 2 cells changed
  - Changed cells
    - row 1, column 'score': '85' -> '92'
    - row 2, column 'score': '90' -> '88'
```

## csv-column-addition

New column added

- **Browse source:** [csv-column-addition](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-column-addition)
- **Tags:** `csv`, `column-addition`, `schema`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-column-addition/snapshot-a \
  ./test-vectors-materialized/csv-column-addition/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Column added: 'email'
  - Set Headers: from: ["name","age"]; to: ["name","age","email"]
  - Add Column: name: 'email'; values: {"total_values":2,"truncated":false,"values":["alice@test.com","bob@test.com"]}
```

## csv-column-removal

Column removed

- **Browse source:** [csv-column-removal](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-column-removal)
- **Tags:** `csv`, `column-removal`, `schema`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-column-removal/snapshot-a \
  ./test-vectors-materialized/csv-column-removal/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Column removed: 'city'
  - Set Headers: from: ["name","age","city"]; to: ["name","age"]
  - Remove Column: name: 'city'; values: {"total_values":2,"truncated":false,"values":["NYC","LA"]}
```

## csv-column-rename-near-miss

A weak content overlap should remain a column add plus remove

- **Browse source:** [csv-column-rename-near-miss](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-column-rename-near-miss)
- **Tags:** `csv`, `column-addition`, `column-removal`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-column-rename-near-miss/snapshot-a \
  ./test-vectors-materialized/csv-column-rename-near-miss/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Column added: 'status'; Column removed: 'legacy'
  - Set Headers: from: ["id","legacy"]; to: ["id","status"]
  - Add Column: name: 'status'; values: {"total_values":4,"truncated":false,"values":["alpha","draft","hold","closed"]}
  - Remove Column: name: 'legacy'; values: {"total_values":4,"truncated":false,"values":["alpha","beta","gamma","delta"]}
```

## csv-column-rename-reorder

A renamed CSV column also moves position

- **Browse source:** [csv-column-rename-reorder](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-column-rename-reorder)
- **Tags:** `csv`, `column-rename`, `column-reorder`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-column-rename-reorder/snapshot-a \
  ./test-vectors-materialized/csv-column-rename-reorder/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Column renamed: 'status' -> 'state'; Columns reordered
```

## csv-column-reorder

Columns shuffled, content identical

- **Browse source:** [csv-column-reorder](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-column-reorder)
- **Tags:** `csv`, `column-reorder`, `clerical`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-column-reorder/snapshot-a \
  ./test-vectors-materialized/csv-column-reorder/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Columns reordered
```

## csv-declared-pipe-dialect

Per-path declared pipe dialect parses a .txt table silently and reports keyed cell changes

- **Browse source:** [csv-declared-pipe-dialect](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-declared-pipe-dialect)
- **Tags:** `csv`, `dialect`, `pipe`, `per-path`
- **Snapshots:** `snapshot-a` has 1 file — `data.txt`; `snapshot-b` has 1 file — `data.txt`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/csv-declared-pipe-dialect.yaml`:

```yaml
dataset:
  paths:
    - match: data.txt
      content_type: text/csv
      row_identity:
        columns:
          - id
      dialect:
        delimiter: |
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-declared-pipe-dialect/snapshot-a \
  ./test-vectors-materialized/csv-declared-pipe-dialect/snapshot-b \
  --config /tmp/csv-declared-pipe-dialect.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.txt**: 1 row modified by key
  - Changed cells
    - key id '1', column 'value': 'old' -> 'new'
```

## csv-disjoint-cohort-guardrail

Disjoint CSV cohorts trip the high-churn guardrail instead of publishing a fictional per-cell diff

- **Browse source:** [csv-disjoint-cohort-guardrail](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-disjoint-cohort-guardrail)
- **Tags:** `csv`, `guardrail`, `high-churn`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-disjoint-cohort-guardrail/snapshot-a \
  ./test-vectors-materialized/csv-disjoint-cohort-guardrail/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: row correspondence uncertain; 1 changed-cell fraction

## Suggestions

- these two tables don't appear to correspond row-for-row (100% changed cells; candidate key columns: id, rate, county) (`data.csv`) [binoc.tabular_high_churn]
  - use `binoc extract CHANGESET "data.csv" content`
```

## csv-distribution-shift

Numeric column distribution shifts with keyed row matching

- **Browse source:** [csv-distribution-shift](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-distribution-shift)
- **Tags:** `csv`, `statistics`, `row-identity`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/csv-distribution-shift.yaml`:

```yaml
dataset:
  tables:
    defaults:
      row_identity:
        columns:
          - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-distribution-shift/snapshot-a \
  ./test-vectors-materialized/csv-distribution-shift/snapshot-b \
  --config /tmp/csv-distribution-shift.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 4 rows modified by key
  - Changed cells (showing 3 of 5)
    - changed cells by column: score 4, label 1
    - key id '1', column 'score': '10' -> '12'
    - key id '2', column 'label': 'beta' -> 'beta2'
    - key id '2', column 'score': '20' -> '35'
```

## csv-keyed-null-duplicate

Configured CSV row keys surface null and duplicate key diagnostics

- **Browse source:** [csv-keyed-null-duplicate](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-keyed-null-duplicate)
- **Tags:** `csv`, `keyed`, `null-key`, `duplicate-key`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/csv-keyed-null-duplicate.yaml`:

```yaml
dataset:
  tables:
    defaults:
      row_identity:
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
    entries:
      - path_regex: ^data\.csv$
        columns:
          - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-keyed-null-duplicate/snapshot-a \
  ./test-vectors-materialized/csv-keyed-null-duplicate/snapshot-b \
  --config /tmp/csv-keyed-null-duplicate.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 14 cells changed
  - Changed cells (showing 3 of 14)
    - changed cells by column: id 4, name 5, score 5
    - row 1, column 'id': 'a' -> 'b'
    - row 1, column 'name': 'Alice' -> 'Bob'
    - row 1, column 'score': '10' -> '21'

## Warnings

- configured row keys had null values; fell back to positional row comparison (`data.csv`) [binoc.keyed_row_identity_degraded]
  - use `binoc extract CHANGESET "data.csv" content`
```

## csv-keyed-row-diff

Configured CSV row keys match reordered rows and report keyed row/cell changes

- **Browse source:** [csv-keyed-row-diff](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-keyed-row-diff)
- **Tags:** `csv`, `keyed`, `row-addition`, `row-removal`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/csv-keyed-row-diff.yaml`:

```yaml
dataset:
  tables:
    - path_regex: ^data\.csv$
      columns:
        - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-keyed-row-diff/snapshot-a \
  ./test-vectors-materialized/csv-keyed-row-diff/snapshot-b \
  --config /tmp/csv-keyed-row-diff.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 1 row added; 1 row removed; 1 row modified by key
  - Changed cells
    - key id 'p2', column 'price': '20' -> '25'
  - Rows added
    - key id 'p4': 'p4', 'Delta', '40'
  - Rows removed
    - key id 'p3': 'p3', 'Gamma', '30'
```

## csv-mid-row-insertion

A mid-table row insertion compacts while column reorder/addition rules remain independent

- **Browse source:** [csv-mid-row-insertion](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-mid-row-insertion)
- **Tags:** `csv`, `row-addition`, `column-reorder`, `column-addition`, `lcs`, `compaction`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-mid-row-insertion/snapshot-a \
  ./test-vectors-materialized/csv-mid-row-insertion/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Column added: 'email'; Columns reordered; 1 row added
  - Rows added
    - row 2: 'LA', 'Bob', '25'
  - Add Column: name: 'email'; values: {"total_values":3,"truncated":false,"values":["alice@example.test","bob@example.test","charlie@example.test"]}
```

## csv-mixed-changes

Multiple change types

- **Browse source:** [csv-mixed-changes](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-mixed-changes)
- **Tags:** `csv`, `column-reorder`, `column-addition`, `row-addition`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-mixed-changes/snapshot-a \
  ./test-vectors-materialized/csv-mixed-changes/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Column added: 'email'; Columns reordered; 1 row added
  - Rows added
    - row 3: 'SF', 'Charlie', '35'
  - Add Column: name: 'email'; values: {"total_values":3,"truncated":false,"values":["a@test.com","b@test.com","c@test.com"]}
```

## csv-numeric-rounding

CSV numeric cells rounded to a common modulus and representation-only numeric differences ignored

- **Browse source:** [csv-numeric-rounding](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-numeric-rounding)
- **Tags:** `csv`, `cell-change`, `value-rounding`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-numeric-rounding/snapshot-a \
  ./test-vectors-materialized/csv-numeric-rounding/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Rounded 3 cells in 'population' to nearest 1000
```

## csv-rename-modify

CSV renamed and modified: detected as a single move by fuzzy correlation

- **Browse source:** [csv-rename-modify](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-rename-modify)
- **Tags:** `csv`, `fuzzy-move`, `rename-modify`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data_v2.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-rename-modify/snapshot-a \
  ./test-vectors-materialized/csv-rename-modify/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data_v2.csv**:
  - Moved from data.csv
  - Column added: 'email'
  - Set Headers: from: ["name","age","city"]; to: ["name","age","city","email"]
  - Add Column: name: 'email'; values: {"total_values":3,"truncated":false,"values":["alice@test.com","bob@test.com","carol@test.com"]}
```

## csv-row-addition

New rows appended

- **Browse source:** [csv-row-addition](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-row-addition)
- **Tags:** `csv`, `row-addition`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-row-addition/snapshot-a \
  ./test-vectors-materialized/csv-row-addition/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 2 rows added
  - Rows added
    - row 2: 'Bob', '25'
    - row 3: 'Charlie', '35'
```

## csv-row-insertion-suppression-sentinels

Inserted rows with disclosure sentinels in count cells are reported as row additions, not value suppression

- **Browse source:** [csv-row-insertion-suppression-sentinels](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-row-insertion-suppression-sentinels)
- **Tags:** `csv`, `row-addition`, `value-suppression-regression`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-row-insertion-suppression-sentinels/snapshot-a \
  ./test-vectors-materialized/csv-row-insertion-suppression-sentinels/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 2 rows added
  - Rows added
    - row 2: 'Beta', '*'
    - row 3: 'Gamma', '(D)'
```

## csv-row-removal

Rows removed from CSV

- **Browse source:** [csv-row-removal](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-row-removal)
- **Tags:** `csv`, `row-removal`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-row-removal/snapshot-a \
  ./test-vectors-materialized/csv-row-removal/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 2 rows removed
  - Rows removed
    - row 2: 'Bob', '25'
    - row 3: 'Charlie', '35'
```

## csv-sorted-row-fallback

CSV rows with no unique key use sorted row-content alignment so a pure re-sort is not reported as cell churn

- **Browse source:** [csv-sorted-row-fallback](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-sorted-row-fallback)
- **Tags:** `csv`, `row-reorder`, `sorted-fallback`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-sorted-row-fallback/snapshot-a \
  ./test-vectors-materialized/csv-sorted-row-fallback/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: rows reordered; cell values unchanged under inferred row alignment
```

## csv-stacked-tables

Detects two logical tables stacked in one messy CSV

- **Browse source:** [csv-stacked-tables](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-stacked-tables)
- **Tags:** `csv`, `stacked-tables`, `row-addition`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-stacked-tables/snapshot-a \
  ./test-vectors-materialized/csv-stacked-tables/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv/>table_2**: 1 row added
  - Rows added
    - row 12: '761012', 'Mu', 'Mu Pharma'
```

## csv-to-tsv-reformat

Table reformatted from CSV to TSV with row edits: detected as one reformatted-and-modified table, not remove + add

- **Browse source:** [csv-to-tsv-reformat](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-to-tsv-reformat)
- **Tags:** `csv`, `tsv`, `reformat`, `serialization-change`, `tabular-pair`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.tsv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-to-tsv-reformat/snapshot-a \
  ./test-vectors-materialized/csv-to-tsv-reformat/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.tsv**:
  - Moved from data.csv
  - 1 row added; 1 cell changed
  - Dialect provenance: detected tab-delimited, no quoting, newline LF
  - Changed cells
    - row 2, column 'age': '25' -> '26'
  - Rows added
    - row 4: 'Dave', '41', 'Austin'
```

## csv-value-suppression

CSV cells replaced with disclosure suppression sentinels

- **Browse source:** [csv-value-suppression](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-value-suppression)
- **Tags:** `csv`, `cell-change`, `value-suppression`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-value-suppression/snapshot-a \
  ./test-vectors-materialized/csv-value-suppression/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Suppressed 3 cells in 'count'
```

## csv-value-suppression-custom-sentinel

CSV cells replaced with a dataset-configured disclosure suppression sentinel

- **Browse source:** [csv-value-suppression-custom-sentinel](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-value-suppression-custom-sentinel)
- **Tags:** `csv`, `cell-change`, `value-suppression`, `dataset-config`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/csv-value-suppression-custom-sentinel.yaml`:

```yaml
dataset:
  reduced_precision:
    suppression_sentinels:
      - N/A
      - ""
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-value-suppression-custom-sentinel/snapshot-a \
  ./test-vectors-materialized/csv-value-suppression-custom-sentinel/snapshot-b \
  --config /tmp/csv-value-suppression-custom-sentinel.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 1 cell changed; Suppressed 2 cells in 'count'
  - Changed cells
    - row 3, column 'rate': '8.9' -> '9.1'
```

## csv-verbosity-full

Markdown full verbosity renders every captured changed-cell example.

- **Browse source:** [csv-verbosity-full](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-verbosity-full)
- **Tags:** `csv`, `cell-change`, `verbosity`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example sets `output.markdown.verbosity: full` so the changelog prints every captured changed-cell example instead of the default capped sample.
Save this dataset config as `/tmp/csv-verbosity-full.yaml`:

```yaml
output:
  markdown:
    verbosity: full
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-verbosity-full/snapshot-a \
  ./test-vectors-materialized/csv-verbosity-full/snapshot-b \
  --config /tmp/csv-verbosity-full.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 5 cells changed
  - Sources
    - data.csv (from, modify, binoc.pair.name)
  - Changed cells
    - row 1, column 'score': '10' -> '11'
    - row 2, column 'score': '20' -> '21'
    - row 3, column 'score': '30' -> '31'
    - row 4, column 'score': '40' -> '41'
    - row 5, column 'score': '50' -> '51'
```

## csv-vintage-benchmark

A 'vintage' reader compares two editions of the same published dataset and wants the structural story (a column appeared, a category vocabulary shifted) surfaced above the bulk data churn they intend to ignore.

- **Browse source:** [csv-vintage-benchmark](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-vintage-benchmark)
- **Tags:** `csv`, `vintage`, `metadata`, `benchmark`
- **Snapshots:** `snapshot-a` has 2 files — `facilities.csv`, `inspections.csv`; `snapshot-b` has 2 files — `facilities.csv`, `inspections.csv`
- **Setup:** The dataset is a yearly facilities register published as a small directory of
CSVs. Between the two editions:

  * `facilities.csv` gains a `region` column (schema change) and one row's
    `status` moves to a brand-new category value, `decommissioned`
    (a *vocabulary* shift — the set of distinct values in a categorical column
    grew).
  * `inspections.csv` changes only in its data: several scores are edited and
    two rows are appended. This is exactly the churn a vintage reader does not
    want to read.

The markdown config models the vintage stance as significance: schema/structural
tags are the high-priority group, bulk cell/row tags the low-priority group.
Because `classify_tags` promotes a node to the highest-priority group among its
tags, `facilities.csv` (which carries both schema and cell tags) floats up to
"Schema & vocabulary changes" while the pure-data `inspections.csv` sinks to
"Bulk data updates". That file-granularity separation is the best vintage view
binoc offers today.

WHAT THIS BENCHMARK IS FOR — the gap between today's output (see
`expected-output/changelog.snap`) and the target (see `VINTAGE-IDEAL.md`):

  1. Within-node significance. `facilities.csv`'s `region` addition and its
     `status` cell edit live on one node, so they cannot be separated: the
     vintage reader still sees the cell bullet. There is no config-driven
     edit-level drop/keep (only `EditProjection.visible`, set by writers).
  2. Vocabulary as a first-class change. The `active -> decommissioned` shift is
     reported as an ordinary `binoc.cell-change`, not as "the `status` vocabulary
     gained a value". Columns are not first-class nodes and distinct-value-set
     diffing does not exist.
  3. Summary statistics. `inspections.csv` is rendered as full cell/row detail,
     not as a one-line vintage statistic ("142 -> 144 rows, 3 cells changed").
     The Summary/GlobalClaim seams exist to carry such a fact; no rule emits one.

This vector is a kept benchmark, not a feature. It is expected to PASS against
current output; as the vintage story improves, update the snapshot and watch it
converge on VINTAGE-IDEAL.md. See docs/adr for the design rationale.
Save this dataset config as `/tmp/csv-vintage-benchmark.yaml`:

```yaml
output:
  markdown:
    groups:
      - heading: Schema & vocabulary changes
        tags:
          - binoc.schema-change
          - binoc.column-addition
          - binoc.column-removal
          - binoc.column-rename
          - binoc.metadata.value-label-set
      - heading: Bulk data updates
        tags:
          - binoc.cell-change
          - binoc.row-addition
          - binoc.row-removal
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-vintage-benchmark/snapshot-a \
  ./test-vectors-materialized/csv-vintage-benchmark/snapshot-b \
  --config /tmp/csv-vintage-benchmark.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

## Schema & vocabulary changes

- **facilities.csv**: Column added: 'region'; 1 cell changed
  - Changed cells
    - row 2, column 'status': 'active' -> 'decommissioned'
  - Set Headers: from: ["facility_id","name","status"]; to: ["facility_id","name","status","region"]
  - Add Column: name: 'region'; values: {"total_values":4,"truncated":false,"values":["north","east","west","south"]}

## Bulk data updates

- **inspections.csv**: 2 rows added; 3 cells changed
  - Changed cells
    - row 1, column 'score': '82' -> '85'
    - row 3, column 'score': '90' -> '91'
    - row 4, column 'score': '68' -> '70'
  - Rows added
    - row 5: 'I104', 'F001', '88'
    - row 6: 'I105', 'F002', '73'
```

## directory-file-copy

New file with same content as an existing unchanged file detected as a copy

- **Browse source:** [directory-file-copy](https://github.com/harvard-lil/binoc/tree/main/test-vectors/directory-file-copy)
- **Tags:** `copy`, `directory`, `content-hash`
- **Snapshots:** `snapshot-a` has 1 file — `original.txt`; `snapshot-b` has 2 files — `duplicate.txt`, `original.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/directory-file-copy/snapshot-a \
  ./test-vectors-materialized/directory-file-copy/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **duplicate.txt**: Copied from original.txt
```

## directory-nested

Subdirectories with mixed changes

- **Browse source:** [directory-nested](https://github.com/harvard-lil/binoc/tree/main/test-vectors/directory-nested)
- **Tags:** `directory`, `nested`, `mixed`
- **Snapshots:** `snapshot-a` has 2 files — `data/records.csv`, `docs/readme.txt`; `snapshot-b` has 3 files — `data/extra.csv`, `data/records.csv`, `docs/readme.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/directory-nested/snapshot-a \
  ./test-vectors-materialized/directory-nested/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data/records.csv**: 1 row added
  - Rows added
    - row 3: '3', 'Charlie'
- **data/extra.csv**: Added
- **docs/readme.txt**: 2 lines added; 1 line removed
  - Line changes
    - line 1: 'Version 1 readme' -> 'Version 2 readme'
```

## directory-nested-with-tar

Shows binoc diffing a tar archive and a plain directory that contain overlapping internal paths.

- **Browse source:** [directory-nested-with-tar](https://github.com/harvard-lil/binoc/tree/main/test-vectors/directory-nested-with-tar)
- **Tags:** `directory`, `tar`, `overlap`, `artifact-collision`
- **Snapshots:** `snapshot-a` has 2 files — `data.tar.gz`, `data/records.csv`; `snapshot-b` has 2 files — `data.tar.gz`, `data/records.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/directory-nested-with-tar/snapshot-a \
  ./test-vectors-materialized/directory-nested-with-tar/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.tar.gz/>records.csv**: 1 cell changed
  - Changed cells
    - row 2, column 'count': '20' -> '25'
- **data/records.csv**: 1 row added
  - Rows added
    - row 3: '3', 'Charlie'
```

## enforcement-actions-merge-years

Per-year CSVs merged row-wise into one file; detected as a clean partition merge (CFM-72)

- **Browse source:** [enforcement-actions-merge-years](https://github.com/harvard-lil/binoc/tree/main/test-vectors/enforcement-actions-merge-years)
- **Tags:** `csv`, `partition`, `merge`
- **Snapshots:** `snapshot-a` has 2 files — `actions_2023.csv`, `actions_2024.csv`; `snapshot-b` has 1 file — `actions.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/enforcement-actions-merge-years/snapshot-a \
  ./test-vectors-materialized/enforcement-actions-merge-years/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

Claims

- actions_2023.csv, actions_2024.csv merged into actions.csv

- **actions.csv**: Merged from actions_2023.csv, actions_2024.csv
```

## file-correspondence-container

Config declares a correspondence between renamed zip containers

- **Browse source:** [file-correspondence-container](https://github.com/harvard-lil/binoc/tree/main/test-vectors/file-correspondence-container)
- **Tags:** `zip`, `file-correspondence`, `declared-correspondence`, `container`
- **Snapshots:** `snapshot-a` has 1 file — `data.zip`; `snapshot-b` has 1 file — `archive.zip`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/file-correspondence-container.yaml`:

```yaml
dataset:
  files:
    correspondences:
      - name: archive-pair
        key: archive
        left:
          path_regex: ^data\.zip$
        right:
          path_regex: ^archive\.zip$
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/file-correspondence-container/snapshot-a \
  ./test-vectors-materialized/file-correspondence-container/snapshot-b \
  --config /tmp/file-correspondence-container.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.zip**: Moved from data.zip
```

## file-correspondence-scheme

Config declares that a state CSV moved into a new directory scheme is the same logical file

- **Browse source:** [file-correspondence-scheme](https://github.com/harvard-lil/binoc/tree/main/test-vectors/file-correspondence-scheme)
- **Tags:** `csv`, `file-correspondence`, `scheme-change`
- **Snapshots:** `snapshot-a` has 1 file — `data/state_AL.csv`; `snapshot-b` has 1 file — `by-state/AL/records.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/file-correspondence-scheme.yaml`:

```yaml
dataset:
  files:
    correspondences:
      - name: state-records
        key: "${state}"
        logical_path: "states/${state}.csv"
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
        left:
          path_regex: "^data/state_(?P<state>[A-Z]{2})\\.csv$"
        right:
          path_regex: "^by-state/(?P<state>[A-Z]{2})/records\\.csv$"
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/file-correspondence-scheme/snapshot-a \
  ./test-vectors-materialized/file-correspondence-scheme/snapshot-b \
  --config /tmp/file-correspondence-scheme.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **by-state**: Added
- **by-state/AL**: Moved from data
- **by-state/AL/records.csv**:
  - Moved from data/state_AL.csv
  - 1 row added
  - Rows added
    - row 2: '2', 'Birmingham'
```

## file-correspondence-token

Config declares that year-stamped CSV filenames are the same logical file

- **Browse source:** [file-correspondence-token](https://github.com/harvard-lil/binoc/tree/main/test-vectors/file-correspondence-token)
- **Tags:** `csv`, `file-correspondence`, `declared-correspondence`
- **Snapshots:** `snapshot-a` has 1 file — `running_list_as_of_2022.csv`; `snapshot-b` has 1 file — `running_list_as_of_2023.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/file-correspondence-token.yaml`:

```yaml
dataset:
  files:
    correspondences:
      - name: running-list
        key: "${list}"
        logical_path: "${list}.csv"
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
        left:
          path_regex: "^(?P<list>running_list)_as_of_[0-9]{4}\\.csv$"
        right:
          path_regex: "^(?P<list>running_list)_as_of_[0-9]{4}\\.csv$"
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/file-correspondence-token/snapshot-a \
  ./test-vectors-materialized/file-correspondence-token/snapshot-b \
  --config /tmp/file-correspondence-token.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **running_list_as_of_2023.csv**:
  - Moved from running_list_as_of_2022.csv
  - 1 row added
  - Rows added
    - row 3: '3', 'Cy'
```

## folder-move-nested

Detects a whole-folder rename and rolls many file moves up into one folder-move entry.

- **Browse source:** [folder-move-nested](https://github.com/harvard-lil/binoc/tree/main/test-vectors/folder-move-nested)
- **Tags:** `folder-move`, `rollup`, `nested`, `directory`
- **Snapshots:** `snapshot-a` has 4 files — `docs/readme.txt`, `docs/reports/annual.txt`, `docs/reports/quarterly/q1.txt`, `docs/reports/quarterly/q2.txt`; `snapshot-b` has 4 files — `documentation/readme.txt`, `documentation/reports/annual.txt`, `documentation/reports/quarterly/q1.txt`, `documentation/reports/quarterly/q2.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/folder-move-nested/snapshot-a \
  ./test-vectors-materialized/folder-move-nested/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **documentation**: Moved from docs
```

## folder-move-partial

Detects a mostly-moved folder rename and preserves only the added/removed/modified remainder entries beneath it.

- **Browse source:** [folder-move-partial](https://github.com/harvard-lil/binoc/tree/main/test-vectors/folder-move-partial)
- **Tags:** `folder-move`, `partial`, `rollup`, `directory`
- **Snapshots:** `snapshot-a` has 10 files — `FoodData_Central_csv_2025-12-18/README.txt`, `FoodData_Central_csv_2025-12-18/data/categories.csv`, `FoodData_Central_csv_2025-12-18/data/food.csv`, `FoodData_Central_csv_2025-12-18/data/nutrients.csv`, +6 more; `snapshot-b` has 10 files — `FoodData_Central_csv_2026-04-30/README.txt`, `FoodData_Central_csv_2026-04-30/data/categories.csv`, `FoodData_Central_csv_2026-04-30/data/food.csv`, `FoodData_Central_csv_2026-04-30/data/new-table.csv`, +6 more

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/folder-move-partial/snapshot-a \
  ./test-vectors-materialized/folder-move-partial/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **FoodData_Central_csv_2026-04-30**: Added
- **FoodData_Central_csv_2026-04-30/README.txt**: Moved from FoodData_Central_csv_2025-12-18/README.txt
- **FoodData_Central_csv_2026-04-30/data**: Moved from FoodData_Central_csv_2025-12-18/data
- **FoodData_Central_csv_2026-04-30/data/new-table.csv**: Added
- **FoodData_Central_csv_2026-04-30/docs**: Added
- **FoodData_Central_csv_2026-04-30/docs/changelog-note.txt**: Moved from FoodData_Central_csv_2025-12-18/docs/changelog-note.txt
- **FoodData_Central_csv_2026-04-30/docs/license.txt**: Moved from FoodData_Central_csv_2025-12-18/docs/license.txt
- **FoodData_Central_csv_2026-04-30/docs/schema.txt**: Moved from FoodData_Central_csv_2025-12-18/docs/schema.txt
- **FoodData_Central_csv_2026-04-30/docs/modified.txt**: Added
- **FoodData_Central_csv_2025-12-18**: Removed
- **FoodData_Central_csv_2025-12-18/docs**: Removed
- **FoodData_Central_csv_2025-12-18/docs/modified.txt**: Removed
- **FoodData_Central_csv_2025-12-18/docs/old-table.txt**: Removed
```

## geojson-feature-cell-change

A GeoJSON FeatureCollection where one feature's property changes; transcoded to a tabular artifact with the geometry as…

- **Browse source:** [geojson-feature-cell-change](https://github.com/harvard-lil/binoc/tree/main/test-vectors/geojson-feature-cell-change)
- **Tags:** `geojson`, `tabular`, `nested`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `places.geojson`; `snapshot-b` has 1 file — `places.geojson`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/geojson-feature-cell-change/snapshot-a \
  ./test-vectors-materialized/geojson-feature-cell-change/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **places.geojson**: 1 cell changed
  - Changed cells
    - row 1, column 'properties': {"name":"Boston","population":650000} -> {"name":"Boston","population":675000}
```

## gzip-inner-dispatch

Gzipped CSV and text are decompressed and redispatched under their inner names

- **Browse source:** [gzip-inner-dispatch](https://github.com/harvard-lil/binoc/tree/main/test-vectors/gzip-inner-dispatch)
- **Tags:** `gzip`, `csv`, `text`, `cell-change`, `row-addition`, `line-change`
- **Snapshots:** `snapshot-a` has 2 files — `census.txt.gz`, `data.csv.gz`; `snapshot-b` has 2 files — `census.txt.gz`, `data.csv.gz`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/gzip-inner-dispatch/snapshot-a \
  ./test-vectors-materialized/gzip-inner-dispatch/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **census.txt.gz/>census.txt**: 1 line added; 1 line removed
  - Line changes
    - line 2: '1|Aroostook|120' -> '1|Aroostook|121'
- **data.csv.gz/>data.csv**: 1 row added; 1 cell changed
  - Changed cells
    - row 2, column 'name': 'Bob' -> 'Robert'
  - Rows added
    - row 3: '3', 'Carla'
```

## ini-value-change

An INI value changes; transcoded to a structured_document and reported as a value change

- **Browse source:** [ini-value-change](https://github.com/harvard-lil/binoc/tree/main/test-vectors/ini-value-change)
- **Tags:** `ini`, `structured-document`, `value-change`
- **Snapshots:** `snapshot-a` has 1 file — `config.ini`; `snapshot-b` has 1 file — `config.ini`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/ini-value-change/snapshot-a \
  ./test-vectors-materialized/ini-value-change/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **config.ini**: $.replicas: "3" -> "5"
```

## json-array-order-significant

JSON array order changes are semantic content changes in stage 1

- **Browse source:** [json-array-order-significant](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-array-order-significant)
- **Tags:** `json`, `array-order`, `content-change`
- **Snapshots:** `snapshot-a` has 1 file — `metadata.json`; `snapshot-b` has 1 file — `metadata.json`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-array-order-significant/snapshot-a \
  ./test-vectors-materialized/json-array-order-significant/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **metadata.json**: $.ids[1]: 2 -> 3; $.ids[2]: 3 -> 2
```

## json-key-order-reexport

JSON object key order and pretty-printing changed without semantic value changes

- **Browse source:** [json-key-order-reexport](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-key-order-reexport)
- **Tags:** `json`, `serialization`, `key-order`
- **Snapshots:** `snapshot-a` has 1 file — `metadata.json`; `snapshot-b` has 1 file — `metadata.json`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-key-order-reexport/snapshot-a \
  ./test-vectors-materialized/json-key-order-reexport/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **metadata.json**: Document serialization changed
```

## json-keyed-row-diff

Configured JSON record keys match reordered rows and report keyed row/cell changes

- **Browse source:** [json-keyed-row-diff](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-keyed-row-diff)
- **Tags:** `json`, `keyed`, `row-addition`, `row-removal`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.json`; `snapshot-b` has 1 file — `data.json`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/json-keyed-row-diff.yaml`:

```yaml
dataset:
  tables:
    - path_regex: ^data\.json$
      columns:
        - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-keyed-row-diff/snapshot-a \
  ./test-vectors-materialized/json-keyed-row-diff/snapshot-b \
  --config /tmp/json-keyed-row-diff.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.json**: 1 row added; 1 row removed; 1 row modified by key
  - Changed cells
    - key id 'p2', column 'price': 20 -> 25
  - Rows added
    - key id 'p4': 'p4', 'Delta', 40
  - Rows removed
    - key id 'p3': 'p3', 'Gamma', 30
```

## json-records-cell-change

JSON array of like-shaped objects parsed as a typed table; numeric cell values change

- **Browse source:** [json-records-cell-change](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-records-cell-change)
- **Tags:** `json`, `records`, `tabular`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.json`; `snapshot-b` has 1 file — `data.json`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-records-cell-change/snapshot-a \
  ./test-vectors-materialized/json-records-cell-change/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.json**: 2 cells changed
  - Changed cells
    - row 1, column 'score': 85 -> 92
    - row 2, column 'score': 90 -> 88
```

## json-records-nested-value

JSON records with a nested object cell; the nested value changes and is reported as a single equality-based cell edit (…

- **Browse source:** [json-records-nested-value](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-records-nested-value)
- **Tags:** `json`, `records`, `tabular`, `nested`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `people.json`; `snapshot-b` has 1 file — `people.json`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-records-nested-value/snapshot-a \
  ./test-vectors-materialized/json-records-nested-value/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **people.json**: 1 cell changed
  - Changed cells
    - row 2, column 'meta': {"role":"user","tags":["z"]} -> {"role":"editor","tags":["z"]}
```

## json-records-path-stix-objects

STIX-shaped JSON bundle records under $.objects are parsed as a keyed table

- **Browse source:** [json-records-path-stix-objects](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-records-path-stix-objects)
- **Tags:** `json`, `records`, `records-path`, `stix`, `keyed`
- **Snapshots:** `snapshot-a` has 1 file — `enterprise.stix.json`; `snapshot-b` has 1 file — `enterprise.stix.json`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/json-records-path-stix-objects.yaml`:

```yaml
dataset:
  paths:
    - match: **/*.stix.json
      records_path: $.objects
      row_identity:
        columns:
          - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-records-path-stix-objects/snapshot-a \
  ./test-vectors-materialized/json-records-path-stix-objects/snapshot-b \
  --config /tmp/json-records-path-stix-objects.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **enterprise.stix.json**: 1 row modified by key
  - Changed cells
    - key id 'attack-pattern--1', column 'name': 'Old Name' -> 'New Name'
```

## json-records-type-only-column

Typed JSON records re-serialized with one numeric column as strings; canonical values match

- **Browse source:** [json-records-type-only-column](https://github.com/harvard-lil/binoc/tree/main/test-vectors/json-records-type-only-column)
- **Tags:** `json`, `records`, `tabular`, `column-type-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.json`; `snapshot-b` has 1 file — `data.json`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/json-records-type-only-column/snapshot-a \
  ./test-vectors-materialized/json-records-type-only-column/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.json**: Column type changed: 'year' number -> string
```

## jsonl-row-addition

JSONL stream of like-shaped objects parsed as a table; a record is appended

- **Browse source:** [jsonl-row-addition](https://github.com/harvard-lil/binoc/tree/main/test-vectors/jsonl-row-addition)
- **Tags:** `jsonl`, `records`, `tabular`, `row-addition`
- **Snapshots:** `snapshot-a` has 1 file — `events.jsonl`; `snapshot-b` has 1 file — `events.jsonl`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/jsonl-row-addition/snapshot-a \
  ./test-vectors-materialized/jsonl-row-addition/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **events.jsonl**: 1 row added
  - Rows added
    - row 3: 'carol', 'delete', 3
```

## jsonld-value-change

A .jsonld file with no declared media type parses as a structured document tagged format=jsonld; a value change is repo…

- **Browse source:** [jsonld-value-change](https://github.com/harvard-lil/binoc/tree/main/test-vectors/jsonld-value-change)
- **Tags:** `json`, `jsonld`, `structured-document`
- **Snapshots:** `snapshot-a` has 1 file — `person.jsonld`; `snapshot-b` has 1 file — `person.jsonld`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/jsonld-value-change/snapshot-a \
  ./test-vectors-materialized/jsonld-value-change/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **person.jsonld**: $.jobTitle: "Mathematician" -> "Computer Scientist"
```

## kitchen-sink

Runs text, CSV, archive, move, and copy detection together in one end-to-end example.

- **Browse source:** [kitchen-sink](https://github.com/harvard-lil/binoc/tree/main/test-vectors/kitchen-sink)
- **Tags:** `csv`, `text`, `binary`, `tar`, `zip`, `directory`, `move`, `copy`, `column-reorder`, `integration`
- **Snapshots:** `snapshot-a` has 9 files — `archive.tar.gz`, `bundle.zip`, `data.csv`, `docs/old-notes.txt`, +5 more; `snapshot-b` has 10 files — `archive.tar.gz`, `bundle.zip`, `data.csv`, `docs/new-file.txt`, +6 more

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/kitchen-sink/snapshot-a \
  ./test-vectors-materialized/kitchen-sink/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.tar.gz/>inventory.csv**: 1 row added
  - Rows added
    - row 3: 'sprockets', '20'
- **bundle.zip/>notes.txt**: 2 lines added; 1 line removed
  - Line changes
    - line 1: 'Version 1 notes.' -> 'Version 2 notes.'
- **data.csv**: 2 cells changed
  - Changed cells
    - row 1, column 'age': '30' -> '31'
    - row 3, column 'city': 'Seattle' -> 'Portland'
- **docs/readme.txt**: 2 lines added; 2 lines removed
  - Line changes
    - line 2: 'This is the original readme.' -> 'This is the updated readme.'
    - line 4: 'Some will change.' -> 'New content added here.'
- **docs/old-notes.txt**: Removed
- **docs/new-file.txt**: Added
- **icon.bin**: Binary content changed; 1 extracted string added, 1 extracted string removed
  - Extracted strings added
    - '\nFAKEICONv2'
  - Extracted strings removed
    - '\nFAKEICONv1'
- **license-copy.txt**: Copied from license.txt
- **metrics.csv**: Columns reordered
- **summary.txt**: Moved from report.txt
```

## nasa-gistemp-header-line

NASA GISTEMP-style table with a title line before the real header

- **Browse source:** [nasa-gistemp-header-line](https://github.com/harvard-lil/binoc/tree/main/test-vectors/nasa-gistemp-header-line)
- **Tags:** `csv`, `config`, `shape`, `header-line`, `keyed`
- **Snapshots:** `snapshot-a` has 1 file — `GLB.Ts+dSST.csv`; `snapshot-b` has 1 file — `GLB.Ts+dSST.csv`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/nasa-gistemp-header-line.yaml`:

```yaml
dataset:
  paths:
    - match: **/GLB.Ts+dSST.csv
      content_type: text/csv
      shape:
        header_line: 2
      row_identity:
        columns:
          - Year
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/nasa-gistemp-header-line/snapshot-a \
  ./test-vectors-materialized/nasa-gistemp-header-line/snapshot-b \
  --config /tmp/nasa-gistemp-header-line.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **GLB.Ts+dSST.csv**: 1 row modified by key
  - Changed cells
    - key Year '2024', column 'Feb': '118' -> '119'
```

## observations-repartition-equal-arity

Equal-arity N→M repartition: 2 tables grouped by region become 2 tables grouped by year, every row preserved exactly bu…

- **Browse source:** [observations-repartition-equal-arity](https://github.com/harvard-lil/binoc/tree/main/test-vectors/observations-repartition-equal-arity)
- **Tags:** `csv`, `partition`, `possible-split`, `equal-arity`
- **Snapshots:** `snapshot-a` has 2 files — `observations_north.csv`, `observations_south.csv`; `snapshot-b` has 2 files — `observations_2024.csv`, `observations_2025.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/observations-repartition-equal-arity/snapshot-a \
  ./test-vectors-materialized/observations-repartition-equal-arity/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **observations_2024.csv**:
  - Moved from observations_north.csv
  - 3 cells changed
  - Changed cells
    - row 2, column 'year': '2025' -> '2024'
    - row 2, column 'region': 'north' -> 'south'
    - row 2, column 'count': '15' -> '12'
- **observations_2025.csv**:
  - Moved from observations_south.csv
  - 3 cells changed
  - Changed cells
    - row 1, column 'year': '2024' -> '2025'
    - row 1, column 'region': 'south' -> 'north'
    - row 1, column 'count': '12' -> '15'

## Suggestions

- 'observations_north.csv' shares rows with other unmatched tables but the relationship is not a clean partition (residual, shared, or extra rows); left as add/remove (`binoc.pair.partition`) [binoc.possible_split]
```

## observations-split-by-year

One CSV split row-wise into per-year files; detected as a clean partition split (CFM-72)

- **Browse source:** [observations-split-by-year](https://github.com/harvard-lil/binoc/tree/main/test-vectors/observations-split-by-year)
- **Tags:** `csv`, `partition`, `split`
- **Snapshots:** `snapshot-a` has 1 file — `observations.csv`; `snapshot-b` has 2 files — `observations_2024.csv`, `observations_2025.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/observations-split-by-year/snapshot-a \
  ./test-vectors-materialized/observations-split-by-year/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

Claims

- observations.csv split into observations_2024.csv, observations_2025.csv

- **observations_2024.csv**: Split from observations.csv
- **observations_2025.csv**: Split from observations.csv
```

## observations-split-residual

A would-be split missing one row: partition declines (not complete), emits binoc.possible_split, and degrades to honest…

- **Browse source:** [observations-split-residual](https://github.com/harvard-lil/binoc/tree/main/test-vectors/observations-split-residual)
- **Tags:** `csv`, `partition`, `possible-split`
- **Snapshots:** `snapshot-a` has 1 file — `observations.csv`; `snapshot-b` has 2 files — `observations_2024.csv`, `observations_2025.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/observations-split-residual/snapshot-a \
  ./test-vectors-materialized/observations-split-residual/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **observations_2024.csv**:
  - Moved from observations.csv
  - 2 rows removed
  - Rows removed
    - row 3: '2025', 'north', '15'
    - row 4: '2025', 'south', '9'
- **observations_2025.csv**: Added

## Suggestions

- 'observations.csv' shares rows with other unmatched tables but the relationship is not a clean partition (residual, shared, or extra rows); left as add/remove (`binoc.pair.partition`) [binoc.possible_split]
```

## ofac-sdn-headerless-position-key

OFAC SDN-style 12-column headerless CSV keyed by ent_num in column 1

- **Browse source:** [ofac-sdn-headerless-position-key](https://github.com/harvard-lil/binoc/tree/main/test-vectors/ofac-sdn-headerless-position-key)
- **Tags:** `csv`, `config`, `shape`, `headerless`, `keyed`
- **Snapshots:** `snapshot-a` has 1 file — `SDN.CSV`; `snapshot-b` has 1 file — `SDN.CSV`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/ofac-sdn-headerless-position-key.yaml`:

```yaml
dataset:
  paths:
    - match: **/SDN.CSV
      content_type: text/csv
      shape:
        has_header: false
      row_identity:
        by_position:
          - 1
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/ofac-sdn-headerless-position-key/snapshot-a \
  ./test-vectors-materialized/ofac-sdn-headerless-position-key/snapshot-b \
  --config /tmp/ofac-sdn-headerless-position-key.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **SDN.CSV**: 1 row modified by key
  - Changed cells
    - key column_1 '100', column 'column_11': 'remarks' -> 'remarks-updated'
```

## per-path-dispatch-override

Per-path dispatch overrides promote extensionless CSV content before tabular row keying

- **Browse source:** [per-path-dispatch-override](https://github.com/harvard-lil/binoc/tree/main/test-vectors/per-path-dispatch-override)
- **Tags:** `config`, `dispatch`, `csv`, `keyed`
- **Snapshots:** `snapshot-a` has 2 files — `forced`, `records`; `snapshot-b` has 2 files — `forced`, `records`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/per-path-dispatch-override.yaml`:

```yaml
dataset:
  defaults:
    row_identity:
      columns:
        - id
  paths:
    - match: **/records
      content_type: text/csv
    - match: forced
      rule: binoc.parse.csv
      row_identity:
        columns:
          - id
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/per-path-dispatch-override/snapshot-a \
  ./test-vectors-materialized/per-path-dispatch-override/snapshot-b \
  --config /tmp/per-path-dispatch-override.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **forced**: 1 row modified by key
  - Changed cells
    - key id '1', column 'value': 'old' -> 'new'
- **records**: 1 row modified by key
  - Changed cells
    - key id '1', column 'value': 'old' -> 'new'
```

## single-file-add

File present in B but not A

- **Browse source:** [single-file-add](https://github.com/harvard-lil/binoc/tree/main/test-vectors/single-file-add)
- **Tags:** `add`, `file`
- **Snapshots:** `snapshot-a` has 0 files (empty snapshot); `snapshot-b` has 1 file — `new_file.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/single-file-add/snapshot-a \
  ./test-vectors-materialized/single-file-add/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **new_file.txt**: Added
```

## single-file-modify-binary

Binary file, different hash

- **Browse source:** [single-file-modify-binary](https://github.com/harvard-lil/binoc/tree/main/test-vectors/single-file-modify-binary)
- **Tags:** `modify`, `binary`
- **Snapshots:** `snapshot-a` has 1 file — `data.bin`; `snapshot-b` has 1 file — `data.bin`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/single-file-modify-binary/snapshot-a \
  ./test-vectors-materialized/single-file-modify-binary/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.bin**: 1 edit
```

## single-file-modify-csv

CSV file compared directly (file-to-file, not via directory)

- **Browse source:** [single-file-modify-csv](https://github.com/harvard-lil/binoc/tree/main/test-vectors/single-file-modify-csv)
- **Tags:** `csv`, `single-file`, `modify`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/single-file-modify-csv/snapshot-a/data.csv \
  ./test-vectors-materialized/single-file-modify-csv/snapshot-b/data.csv
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: 1 row added
  - Rows added
    - row 3: 'Charlie', '35'
```

## single-file-modify-text

Text file with line-level changes

- **Browse source:** [single-file-modify-text](https://github.com/harvard-lil/binoc/tree/main/test-vectors/single-file-modify-text)
- **Tags:** `modify`, `text`, `lines`
- **Snapshots:** `snapshot-a` has 1 file — `story.txt`; `snapshot-b` has 1 file — `story.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/single-file-modify-text/snapshot-a \
  ./test-vectors-materialized/single-file-modify-text/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **story.txt**: 2 lines added; 1 line removed
  - Line changes
    - line 2: 'Line 2' -> 'Line 2 revised'
```

## single-file-modify-text-root

Text file compared directly (file-to-file, not via directory)

- **Browse source:** [single-file-modify-text-root](https://github.com/harvard-lil/binoc/tree/main/test-vectors/single-file-modify-text-root)
- **Tags:** `text`, `single-file`, `modify`
- **Snapshots:** `snapshot-a` has 1 file — `story.txt`; `snapshot-b` has 1 file — `story.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/single-file-modify-text-root/snapshot-a/story.txt \
  ./test-vectors-materialized/single-file-modify-text-root/snapshot-b/story.txt
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **story.txt**: 2 lines added; 1 line removed
  - Line changes
    - line 2: 'Line 2' -> 'Line 2 revised'
```

## single-file-remove

File present in A but not B

- **Browse source:** [single-file-remove](https://github.com/harvard-lil/binoc/tree/main/test-vectors/single-file-remove)
- **Tags:** `remove`, `file`
- **Snapshots:** `snapshot-a` has 1 file — `removed_file.txt`; `snapshot-b` has 0 files (empty snapshot)

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/single-file-remove/snapshot-a \
  ./test-vectors-materialized/single-file-remove/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **removed_file.txt**: Removed
```

## stacked-csv-broken-out

Stacked-CSV tables broken out into one file per table; whole-table rehoming (reshape + 1:1), NOT a partition split (CFM…

- **Browse source:** [stacked-csv-broken-out](https://github.com/harvard-lil/binoc/tree/main/test-vectors/stacked-csv-broken-out)
- **Tags:** `csv`, `stacked-tables`, `reshape`
- **Snapshots:** `snapshot-a` has 1 file — `report.csv`; `snapshot-b` has 2 files — `changes.csv`, `products.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/stacked-csv-broken-out/snapshot-a \
  ./test-vectors-materialized/stacked-csv-broken-out/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **changes.csv**: Moved from report.csv/>table_1
- **products.csv**: Reshaped from report.csv (stacked tables → tabular)
- **report.csv/>table_2**: Removed
```

## tar-nested

Nested tar.gz containing CSV

- **Browse source:** [tar-nested](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tar-nested)
- **Tags:** `tar`, `nested`, `csv`
- **Snapshots:** `snapshot-a` has 1 file — `outer.tar.gz`; `snapshot-b` has 1 file — `outer.tar.gz`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tar-nested/snapshot-a \
  ./test-vectors-materialized/tar-nested/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **outer.tar.gz/>inner.tar.gz/>data.csv**: 1 row added
  - Rows added
    - row 2: 'Bob', '25'
```

## tar-simple

Tar.gz archive with changes inside

- **Browse source:** [tar-simple](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tar-simple)
- **Tags:** `tar`, `archive`
- **Snapshots:** `snapshot-a` has 1 file — `archive.tar.gz`; `snapshot-b` has 1 file — `archive.tar.gz`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tar-simple/snapshot-a \
  ./test-vectors-materialized/tar-simple/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.tar.gz/>data.csv**: 1 row added
  - Rows added
    - row 3: 'gamma', '3'
- **archive.tar.gz/>hello.txt**: 1 line added
```

## text-rename-modify

Text file renamed and modified: detected as a single move by fuzzy correlation

- **Browse source:** [text-rename-modify](https://github.com/harvard-lil/binoc/tree/main/test-vectors/text-rename-modify)
- **Tags:** `text`, `fuzzy-move`, `rename-modify`
- **Snapshots:** `snapshot-a` has 1 file — `notes.txt`; `snapshot-b` has 1 file — `meeting-notes-v2.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/text-rename-modify/snapshot-a \
  ./test-vectors-materialized/text-rename-modify/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **meeting-notes-v2.txt**:
  - Moved from notes.txt
  - 2 lines added
  - Line changes (showing 3 of 4)
    - line 10: '' -> '- Marketing strategy update'
    - line 11: 'Action Items:' -> ''
    - line 12: '- Alice to finalize budget by Friday' -> 'Action Items:'
```

## toml-value-change

A TOML value changes; transcoded to a structured_document and reported as a value change

- **Browse source:** [toml-value-change](https://github.com/harvard-lil/binoc/tree/main/test-vectors/toml-value-change)
- **Tags:** `toml`, `structured-document`, `value-change`
- **Snapshots:** `snapshot-a` has 1 file — `config.toml`; `snapshot-b` has 1 file — `config.toml`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/toml-value-change/snapshot-a \
  ./test-vectors-materialized/toml-value-change/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **config.toml**: $.replicas: 3 -> 5
```

## tree-wide-correlation

Shows tree-wide move and copy detection across nested zip boundaries, including one-to-many copies and many-to-one moves.

- **Browse source:** [tree-wide-correlation](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tree-wide-correlation)
- **Tags:** `move`, `copy`, `aggregation`, `zip`, `nested`, `archive`, `tree-wide`
- **Snapshots:** `snapshot-a` has 4 files — `alpha.txt`, `dup.bin`, `kept.txt`, `outer.zip`; `snapshot-b` has 5 files — `gamma-renamed.txt`, `kept-copy.txt`, `kept.txt`, `merged.bin`, +1 more

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tree-wide-correlation/snapshot-a \
  ./test-vectors-materialized/tree-wide-correlation/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **gamma-renamed.txt**: Moved from outer.zip/>inner.zip/>gamma.txt
- **kept-copy.txt**: Copied from kept.txt
- **merged.bin**: Moved from dup.bin
- **outer.zip/>alpha-renamed.txt**: Moved from alpha.txt
- **outer.zip/>inner.zip/>beta-renamed.txt**: Moved from outer.zip/>beta.txt
- **outer.zip/>kept-copy.txt**: Copied from kept.txt
- **outer.zip/>dup-b.bin**: Removed
```

## trivial-identical

Two identical directories → empty changeset

- **Browse source:** [trivial-identical](https://github.com/harvard-lil/binoc/tree/main/test-vectors/trivial-identical)
- **Tags:** `identical`, `baseline`
- **Snapshots:** `snapshot-a` has 1 file — `data.txt`; `snapshot-b` has 1 file — `data.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/trivial-identical/snapshot-a \
  ./test-vectors-materialized/trivial-identical/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b
```

## trivial-identical-csv

Two identical CSV files → no changes reported

- **Browse source:** [trivial-identical-csv](https://github.com/harvard-lil/binoc/tree/main/test-vectors/trivial-identical-csv)
- **Tags:** `csv`, `identical`, `baseline`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/trivial-identical-csv/snapshot-a \
  ./test-vectors-materialized/trivial-identical-csv/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b
```

## tsv-cell-changes

Tab-delimited file parses into real columns and reports cell changes

- **Browse source:** [tsv-cell-changes](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tsv-cell-changes)
- **Tags:** `tsv`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.tsv`; `snapshot-b` has 1 file — `data.tsv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tsv-cell-changes/snapshot-a \
  ./test-vectors-materialized/tsv-cell-changes/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.tsv**: 2 cells changed
  - Dialect provenance: detected tab-delimited, no quoting, newline LF
  - Changed cells
    - row 1, column 'age': '30' -> '31'
    - row 2, column 'city': 'Boston' -> 'Cambridge'
```

## yaml-value-change

A YAML scalar value changes; transcoded to a structured_document and reported as a value change

- **Browse source:** [yaml-value-change](https://github.com/harvard-lil/binoc/tree/main/test-vectors/yaml-value-change)
- **Tags:** `yaml`, `structured-document`, `value-change`
- **Snapshots:** `snapshot-a` has 1 file — `config.yaml`; `snapshot-b` has 1 file — `config.yaml`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/yaml-value-change/snapshot-a \
  ./test-vectors-materialized/yaml-value-change/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **config.yaml**: $.replicas: 3 -> 5
```

## zip-declared-container

Config declares a correspondence between nested zip containers and preserves inner CSV content detail

- **Browse source:** [zip-declared-container](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-declared-container)
- **Tags:** `zip`, `file-correspondence`, `declared-correspondence`, `container`
- **Snapshots:** `snapshot-a` has 1 file — `outer.zip`; `snapshot-b` has 1 file — `outer.zip`
- **Setup:** This example uses a custom dataset config to make the relevant correspondence behavior obvious.
Save this dataset config as `/tmp/zip-declared-container.yaml`:

```yaml
dataset:
  files:
    correspondences:
      - name: inner-archive-pair
        key: records
        logical_path: outer.zip/>records.zip
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
        left:
          path_regex: ^outer\.zip/>records-old\.zip$
        right:
          path_regex: ^outer\.zip/>records\.zip$
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-declared-container/snapshot-a \
  ./test-vectors-materialized/zip-declared-container/snapshot-b \
  --config /tmp/zip-declared-container.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **outer.zip/>records.zip**: Moved from outer.zip/>records-old.zip
- **outer.zip/>records.zip/>data.csv**: 1 cell changed
  - Changed cells
    - row 2, column 'score': '24' -> '25'
```

## zip-json-key-order-reexport

JSON files inside zip expansion get parsed and rendered as serialization-only changes

- **Browse source:** [zip-json-key-order-reexport](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-json-key-order-reexport)
- **Tags:** `zip`, `json`, `serialization`, `key-order`
- **Snapshots:** `snapshot-a` has 1 file — `archive.zip`; `snapshot-b` has 1 file — `archive.zip`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-json-key-order-reexport/snapshot-a \
  ./test-vectors-materialized/zip-json-key-order-reexport/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.zip/>metadata.json**: Document serialization changed
```

## zip-nested

Nested zip containing CSV

- **Browse source:** [zip-nested](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-nested)
- **Tags:** `zip`, `nested`, `csv`
- **Snapshots:** `snapshot-a` has 1 file — `outer.zip`; `snapshot-b` has 1 file — `outer.zip`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-nested/snapshot-a \
  ./test-vectors-materialized/zip-nested/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **outer.zip/>inner.zip/>data.csv**: 1 row added
  - Rows added
    - row 2: 'Bob', '25'
```

## zip-rename-contents-rewritten

Documents a known gap — a renamed zip whose children were all renamed AND rewritten (no content similarity) yields unpa…

- **Browse source:** [zip-rename-contents-rewritten](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-rename-contents-rewritten)
- **Tags:** `zip`, `archive`, `known-gap`
- **Snapshots:** `snapshot-a` has 1 file — `data.zip`; `snapshot-b` has 1 file — `archive.zip`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-rename-contents-rewritten/snapshot-a \
  ./test-vectors-materialized/zip-rename-contents-rewritten/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.zip**: Removed
- **data.zip/>x.csv**: Removed
- **data.zip/>y.csv**: Removed
- **data.zip/>z.csv**: Removed
- **archive.zip**: Added
- **archive.zip/>p.csv**: Added
- **archive.zip/>q.csv**: Added
- **archive.zip/>r.csv**: Added
```

## zip-rename-identical

Zip archive renamed with identical contents; bottom-up roll-up of the inner clean file moves compacts the pair into a s…

- **Browse source:** [zip-rename-identical](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-rename-identical)
- **Tags:** `zip`, `archive`, `folder-move`
- **Snapshots:** `snapshot-a` has 1 file — `data.zip`; `snapshot-b` has 1 file — `archive.zip`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-rename-identical/snapshot-a \
  ./test-vectors-materialized/zip-rename-identical/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.zip**: Moved from data.zip
```

## zip-rename-inner-rename-edit

Zip archive renamed while its only child was renamed and had one cell edited; the modified move counts as roll-up evide…

- **Browse source:** [zip-rename-inner-rename-edit](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-rename-inner-rename-edit)
- **Tags:** `zip`, `archive`, `folder-move`, `fuzzy-correlation`
- **Snapshots:** `snapshot-a` has 1 file — `data.zip`; `snapshot-b` has 1 file — `archive.zip`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-rename-inner-rename-edit/snapshot-a \
  ./test-vectors-materialized/zip-rename-inner-rename-edit/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.zip**: Moved from data.zip
- **archive.zip/>new.csv**:
  - Moved from data.zip/>old.csv
  - 1 cell changed
  - Changed cells
    - row 5, column 'score': '60' -> '61'
```

## zip-simple

Zipped files with changes inside

- **Browse source:** [zip-simple](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-simple)
- **Tags:** `zip`, `archive`
- **Snapshots:** `snapshot-a` has 1 file — `archive.zip`; `snapshot-b` has 1 file — `archive.zip`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-simple/snapshot-a \
  ./test-vectors-materialized/zip-simple/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.zip/>data.txt**: 1 line added; 1 line removed
  - Line changes
    - line 1: 'hello from zip A' -> 'hello from zip B'
- **archive.zip/>extra.txt**: Added
```
