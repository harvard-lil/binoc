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

Binoc currently ships **38 shared examples** in this gallery.

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
| [`binary-fallback-diagnostic`](#binary-fallback-diagnostic) | Unknown file type compared by the binary fallback emits a suggestion | data.parquet: Content changed (7 bytes → 7 bytes) | Default pipeline |
| [`csv-cell-changes`](#csv-cell-changes) | Individual cell values changed | data.csv: 2 cells changed | Default pipeline |
| [`csv-column-addition`](#csv-column-addition) | New column added | data.csv: Column added: 'email' | Default pipeline |
| [`csv-column-removal`](#csv-column-removal) | Column removed | data.csv: Column removed: 'city' | Default pipeline |
| [`csv-column-reorder`](#csv-column-reorder) | Columns shuffled, content identical | data.csv: Columns reordered (content unchanged) | Custom config |
| [`csv-distribution-shift`](#csv-distribution-shift) | Numeric column distribution shifts with keyed row matching | data.csv: 4 rows modified by key | Custom config |
| [`csv-keyed-null-duplicate`](#csv-keyed-null-duplicate) | Configured CSV row keys surface null and duplicate key diagnostics | data.csv: 4 rows added by key; 4 rows removed by key; 1 row modified by key | Custom config |
| [`csv-keyed-row-diff`](#csv-keyed-row-diff) | Configured CSV row keys match reordered rows and report keyed row/cell changes | data.csv: 1 row added by key; 1 row removed by key; 1 row modified by key | Custom config |
| [`csv-mixed-changes`](#csv-mixed-changes) | Multiple change types | data.csv: Column added: 'email'; columns reordered; 1 row added | Default pipeline |
| [`csv-rename-modify`](#csv-rename-modify) | CSV renamed and a column added: detected as a single move with content diff via fuzzy correlation | data_v2.csv: Moved from data.csv (modified) | Default pipeline |
| [`csv-row-addition`](#csv-row-addition) | New rows appended | data.csv: 2 rows added | Default pipeline |
| [`csv-row-removal`](#csv-row-removal) | Rows removed from CSV | data.csv: 2 rows removed | Default pipeline |
| [`csv-stacked-tables`](#csv-stacked-tables) | Detects two logical tables stacked in one messy CSV | data.csv: Table table_2 changed: 1 row added | Default pipeline |
| [`csv-verbosity-full`](#csv-verbosity-full) | Markdown full verbosity renders every captured changed-cell example. | data.csv: 5 cells changed | Custom config |
| [`directory-file-copy`](#directory-file-copy) | New file with same content as an existing unchanged file detected as a copy | duplicate.txt: Copied from original.txt | Default pipeline |
| [`directory-nested`](#directory-nested) | Subdirectories with mixed changes | data/extra.csv: New table (2 columns, 1 row) | Default pipeline |
| [`directory-nested-with-tar`](#directory-nested-with-tar) | Shows binoc diffing a tar archive and a plain directory that contain overlapping internal paths. | data/records.csv: 1 row added | Default pipeline |
| [`file-correspondence-scheme`](#file-correspondence-scheme) | Config declares that a state CSV moved into a new directory scheme is the same logical file | states/AL.csv: 1 row added | Custom config |
| [`file-correspondence-token`](#file-correspondence-token) | Config declares that year-stamped CSV filenames are the same logical file | running_list.csv: 1 row added | Custom config |
| [`folder-move-nested`](#folder-move-nested) | Detects a whole-folder rename and rolls many file moves up into one folder-move entry. | documentation: Folder moved from docs | Default pipeline |
| [`folder-move-partial`](#folder-move-partial) | Detects a mostly-moved folder rename and preserves only the added/removed/modified remainder entries beneath it. | FoodData_Central_csv_2026-04-30: Folder moved from FoodData_Central_csv_2025-12-18 | Custom config |
| [`gzip-inner-dispatch`](#gzip-inner-dispatch) | Gzipped CSV and text are decompressed and redispatched under their inner names | census.txt: 1 line added, 1 removed | Default pipeline |
| [`kitchen-sink`](#kitchen-sink) | Runs text, CSV, archive, move, and copy detection together in one end-to-end example. | archive.tar.gz/inventory.csv: 1 row added | Default pipeline |
| [`single-file-add`](#single-file-add) | File present in B but not A | new_file.txt: New file (1 line) | Default pipeline |
| [`single-file-modify-binary`](#single-file-modify-binary) | Binary file, different hash | data.bin: Content changed (4 bytes → 4 bytes) | Default pipeline |
| [`single-file-modify-csv`](#single-file-modify-csv) | CSV file compared directly (file-to-file, not via directory) | data.csv: 1 row added | Default pipeline |
| [`single-file-modify-text`](#single-file-modify-text) | Text file with line-level changes | story.txt: 2 lines added, 1 removed | Default pipeline |
| [`single-file-modify-text-root`](#single-file-modify-text-root) | Text file compared directly (file-to-file, not via directory) | story.txt: 2 lines added, 1 removed | Default pipeline |
| [`single-file-remove`](#single-file-remove) | File present in A but not B | removed_file.txt: File removed (1 line) | Default pipeline |
| [`tar-nested`](#tar-nested) | Nested tar.gz containing CSV | outer.tar.gz/inner.tar.gz/data.csv: 1 row added | Default pipeline |
| [`tar-simple`](#tar-simple) | Tar.gz archive with changes inside | archive.tar.gz/data.csv: 1 row added | Default pipeline |
| [`text-rename-modify`](#text-rename-modify) | Text file renamed and lines added: detected as a single move with content diff via fuzzy correlation | meeting-notes-v2.txt: Moved from notes.txt (modified) | Default pipeline |
| [`tree-wide-correlation`](#tree-wide-correlation) | Shows tree-wide move and copy detection across nested zip boundaries, including one-to-many copies and many-to-one moves. | gamma-renamed.txt: Moved from outer.zip/inner.zip/gamma.txt | Default pipeline |
| [`trivial-identical`](#trivial-identical) | Two identical directories → empty changeset | No changes detected. | Default pipeline |
| [`trivial-identical-csv`](#trivial-identical-csv) | Two identical CSV files → no changes reported | No changes detected. | Default pipeline |
| [`tsv-cell-changes`](#tsv-cell-changes) | Tab-delimited file parses into real columns and reports cell changes | data.tsv: 2 cells changed | Default pipeline |
| [`zip-nested`](#zip-nested) | Nested zip containing CSV | outer.zip/inner.zip/data.csv: 1 row added | Default pipeline |
| [`zip-simple`](#zip-simple) | Zipped files with changes inside | archive.zip/data.txt: 1 line added, 1 removed | Default pipeline |

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

- **data.parquet**: Content changed (7 bytes → 7 bytes)

## Suggestions

- Compared as binary; a plugin may provide a more semantic diff. (`data.parquet`) [binoc.binary-fallback]
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
  - Changed cells; use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
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
```

## csv-column-reorder

Columns shuffled, content identical

- **Browse source:** [csv-column-reorder](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-column-reorder)
- **Tags:** `csv`, `column-reorder`, `clerical`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
Save this dataset config as `/tmp/csv-column-reorder.yaml`:

```yaml
comparators:
  - binoc.directory
  - binoc.csv
transformers:
  - binoc.tabular_analyzer
  - binoc.column_reorder_detector
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/csv-column-reorder/snapshot-a \
  ./test-vectors-materialized/csv-column-reorder/snapshot-b \
  --config /tmp/csv-column-reorder.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data.csv**: Columns reordered (content unchanged)
```

## csv-distribution-shift

Numeric column distribution shifts with keyed row matching

- **Browse source:** [csv-distribution-shift](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-distribution-shift)
- **Tags:** `csv`, `statistics`, `row-identity`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
Save this dataset config as `/tmp/csv-distribution-shift.yaml`:

```yaml
dataset:
  tables:
    defaults:
      row_identity:
        columns:
          - id
transformer_config:
  binoc.tabular_stats_annotator:
    enabled: true
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
  - Distribution shifts
    - column 'score': mean 20 -> 35.5, median 20 -> 40, range 10-30 -> 12-50, nulls 1 -> 0, mean abs delta 10.667 across 3 paired rows
  - Changed cells (showing 3 of 5); use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
    - key id '1', column 'score': '10' -> '12'
    - key id '2', column 'label': 'beta' -> 'beta2'
    - key id '2', column 'score': '20' -> '35'
```

## csv-keyed-null-duplicate

Configured CSV row keys surface null and duplicate key diagnostics

- **Browse source:** [csv-keyed-null-duplicate](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-keyed-null-duplicate)
- **Tags:** `csv`, `keyed`, `null-key`, `duplicate-key`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
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

- **data.csv**: 4 rows added by key; 4 rows removed by key; 1 row modified by key
  - Changed cells; use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
    - key id 'b', column 'score': '20' -> '21'

## Warnings

- 2 rows had null configured key values (`data.csv`) [binoc.null-key]
- 1 configured row key appeared more than once (`data.csv`) [binoc.duplicate-key]
```

## csv-keyed-row-diff

Configured CSV row keys match reordered rows and report keyed row/cell changes

- **Browse source:** [csv-keyed-row-diff](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-keyed-row-diff)
- **Tags:** `csv`, `keyed`, `row-addition`, `row-removal`, `cell-change`
- **Snapshots:** `snapshot-a` has 1 file — `data.csv`; `snapshot-b` has 1 file — `data.csv`
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
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

- **data.csv**: 1 row added by key; 1 row removed by key; 1 row modified by key
  - Changed cells; use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
    - key id 'p2', column 'price': '20' -> '25'
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

- **data.csv**: Column added: 'email'; columns reordered; 1 row added
```

## csv-rename-modify

CSV renamed and a column added: detected as a single move with content diff via fuzzy correlation

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

- **data_v2.csv**: Moved from data.csv (modified)
- **data_v2.csv**: Column added: 'email'
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
```

## csv-stacked-tables

Detects two logical tables stacked in one messy CSV

- **Browse source:** [csv-stacked-tables](https://github.com/harvard-lil/binoc/tree/main/test-vectors/csv-stacked-tables)
- **Tags:** `csv`, `stacked-tables`, `tabular-collection`, `row-addition`
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

- **data.csv**: Table table_2 changed: 1 row added
- **data.csv#table_2**: 1 row added
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
  - Changed cells; use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
    - row 1, column 'score': '10' -> '11'
    - row 2, column 'score': '20' -> '21'
    - row 3, column 'score': '30' -> '31'
    - row 4, column 'score': '40' -> '41'
    - row 5, column 'score': '50' -> '51'
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

- **data/extra.csv**: New table (2 columns, 1 row)
- **data/records.csv**: 1 row added
- **docs/readme.txt**: 2 lines added, 1 removed
```

## directory-nested-with-tar

Shows binoc diffing a tar archive and a plain directory that contain overlapping internal paths.

- **Browse source:** [directory-nested-with-tar](https://github.com/harvard-lil/binoc/tree/main/test-vectors/directory-nested-with-tar)
- **Tags:** `directory`, `tar`, `overlap`, `artifact-collision`
- **Snapshots:** `snapshot-a` has 2 files — `data.tar.gz.d/records.csv`, `data/records.csv`; `snapshot-b` has 2 files — `data.tar.gz.d/records.csv`, `data/records.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/directory-nested-with-tar/snapshot-a \
  ./test-vectors-materialized/directory-nested-with-tar/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **data/records.csv**: 1 row added
- **data.tar.gz/records.csv**: 1 cell changed
  - Changed cells; use `binoc extract CHANGESET "data.tar.gz/records.csv" cells_changed` for all changed cells
    - row 2, column 'count': '20' -> '25'
```

## file-correspondence-scheme

Config declares that a state CSV moved into a new directory scheme is the same logical file

- **Browse source:** [file-correspondence-scheme](https://github.com/harvard-lil/binoc/tree/main/test-vectors/file-correspondence-scheme)
- **Tags:** `csv`, `file-correspondence`, `scheme-change`
- **Snapshots:** `snapshot-a` has 1 file — `data/state_AL.csv`; `snapshot-b` has 1 file — `by-state/AL/records.csv`
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
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

- **states/AL.csv**: 1 row added
```

## file-correspondence-token

Config declares that year-stamped CSV filenames are the same logical file

- **Browse source:** [file-correspondence-token](https://github.com/harvard-lil/binoc/tree/main/test-vectors/file-correspondence-token)
- **Tags:** `csv`, `file-correspondence`, `declared-correspondence`
- **Snapshots:** `snapshot-a` has 1 file — `running_list_as_of_2022.csv`; `snapshot-b` has 1 file — `running_list_as_of_2023.csv`
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
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

- **running_list.csv**: 1 row added
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

- **documentation**: Folder moved from docs
```

## folder-move-partial

Detects a mostly-moved folder rename and preserves only the added/removed/modified remainder entries beneath it.

- **Browse source:** [folder-move-partial](https://github.com/harvard-lil/binoc/tree/main/test-vectors/folder-move-partial)
- **Tags:** `folder-move`, `partial`, `rollup`, `directory`
- **Snapshots:** `snapshot-a` has 10 files — `FoodData_Central_csv_2025-12-18/README.txt`, `FoodData_Central_csv_2025-12-18/data/categories.csv`, `FoodData_Central_csv_2025-12-18/data/food.csv`, `FoodData_Central_csv_2025-12-18/data/nutrients.csv`, +6 more; `snapshot-b` has 10 files — `FoodData_Central_csv_2026-04-30/README.txt`, `FoodData_Central_csv_2026-04-30/data/categories.csv`, `FoodData_Central_csv_2026-04-30/data/food.csv`, `FoodData_Central_csv_2026-04-30/data/new-table.csv`, +6 more
- **Setup:** This example uses a custom dataset config to narrow the pipeline to the comparators and transformers that make the behavior obvious.
Save this dataset config as `/tmp/folder-move-partial.yaml`:

```yaml
transformer_config:
  binoc.folder_move_detector:
    threshold: 0.8
```


Run it:
```bash
binoc diff \
  ./test-vectors-materialized/folder-move-partial/snapshot-a \
  ./test-vectors-materialized/folder-move-partial/snapshot-b \
  --config /tmp/folder-move-partial.yaml
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **FoodData_Central_csv_2026-04-30**: Folder moved from FoodData_Central_csv_2025-12-18
- **FoodData_Central_csv_2026-04-30/data/new-table.csv**: New table (2 columns, 1 row)
- **FoodData_Central_csv_2026-04-30/docs/modified.txt**: Text modified
- **FoodData_Central_csv_2026-04-30/docs/old-table.txt**: File removed (1 line)
```

## gzip-inner-dispatch

Gzipped CSV and text are decompressed and redispatched under their inner names

- **Browse source:** [gzip-inner-dispatch](https://github.com/harvard-lil/binoc/tree/main/test-vectors/gzip-inner-dispatch)
- **Tags:** `gzip`, `csv`, `text`, `cell-change`, `row-addition`, `line-change`
- **Snapshots:** `snapshot-a` has 2 files — `census.txt.gz.d/census.txt`, `data.csv.gz.d/data.csv`; `snapshot-b` has 2 files — `census.txt.gz.d/census.txt`, `data.csv.gz.d/data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/gzip-inner-dispatch/snapshot-a \
  ./test-vectors-materialized/gzip-inner-dispatch/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **census.txt**: 1 line added, 1 removed
- **data.csv**: 1 row added; 1 cell changed
  - Changed cells; use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
    - row 2, column 'name': 'Bob' -> 'Robert'
```

## kitchen-sink

Runs text, CSV, archive, move, and copy detection together in one end-to-end example.

- **Browse source:** [kitchen-sink](https://github.com/harvard-lil/binoc/tree/main/test-vectors/kitchen-sink)
- **Tags:** `csv`, `text`, `binary`, `tar`, `zip`, `directory`, `move`, `copy`, `column-reorder`, `integration`
- **Snapshots:** `snapshot-a` has 9 files — `archive.tar.gz.d/inventory.csv`, `bundle.zip.d/notes.txt`, `data.csv`, `docs/old-notes.txt`, +5 more; `snapshot-b` has 10 files — `archive.tar.gz.d/inventory.csv`, `bundle.zip.d/notes.txt`, `data.csv`, `docs/new-file.txt`, +6 more

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/kitchen-sink/snapshot-a \
  ./test-vectors-materialized/kitchen-sink/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.tar.gz/inventory.csv**: 1 row added
- **bundle.zip/notes.txt**: 2 lines added, 1 removed
- **data.csv**: 2 cells changed
  - Changed cells; use `binoc extract CHANGESET "data.csv" cells_changed` for all changed cells
    - row 1, column 'age': '30' -> '31'
    - row 3, column 'city': 'Seattle' -> 'Portland'
- **docs/new-file.txt**: New file (1 line)
- **docs/old-notes.txt**: File removed (1 line)
- **docs/readme.txt**: 2 lines added, 2 removed
- **icon.bin**: Content changed (19 bytes → 19 bytes)
- **license-copy.txt**: Copied from license.txt
- **metrics.csv**: Columns reordered (content unchanged)
- **summary.txt**: Moved from report.txt

## Suggestions

- Compared as binary; a plugin may provide a more semantic diff. (`icon.bin`) [binoc.binary-fallback]
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

- **new_file.txt**: New file (1 line)
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

- **data.bin**: Content changed (4 bytes → 4 bytes)

## Suggestions

- Compared as binary; a plugin may provide a more semantic diff. (`data.bin`) [binoc.binary-fallback]
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

- **story.txt**: 2 lines added, 1 removed
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

- **story.txt**: 2 lines added, 1 removed
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

- **removed_file.txt**: File removed (1 line)
```

## tar-nested

Nested tar.gz containing CSV

- **Browse source:** [tar-nested](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tar-nested)
- **Tags:** `tar`, `nested`, `csv`
- **Snapshots:** `snapshot-a` has 1 file — `outer.tar.gz.d/inner.tar.gz.d/data.csv`; `snapshot-b` has 1 file — `outer.tar.gz.d/inner.tar.gz.d/data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tar-nested/snapshot-a \
  ./test-vectors-materialized/tar-nested/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **outer.tar.gz/inner.tar.gz/data.csv**: 1 row added
```

## tar-simple

Tar.gz archive with changes inside

- **Browse source:** [tar-simple](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tar-simple)
- **Tags:** `tar`, `archive`
- **Snapshots:** `snapshot-a` has 2 files — `archive.tar.gz.d/data.csv`, `archive.tar.gz.d/hello.txt`; `snapshot-b` has 2 files — `archive.tar.gz.d/data.csv`, `archive.tar.gz.d/hello.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tar-simple/snapshot-a \
  ./test-vectors-materialized/tar-simple/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.tar.gz/data.csv**: 1 row added
- **archive.tar.gz/hello.txt**: 1 line added
```

## text-rename-modify

Text file renamed and lines added: detected as a single move with content diff via fuzzy correlation

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

- **meeting-notes-v2.txt**: Moved from notes.txt (modified)
- **meeting-notes-v2.txt**: 2 lines added
```

## tree-wide-correlation

Shows tree-wide move and copy detection across nested zip boundaries, including one-to-many copies and many-to-one moves.

- **Browse source:** [tree-wide-correlation](https://github.com/harvard-lil/binoc/tree/main/test-vectors/tree-wide-correlation)
- **Tags:** `move`, `copy`, `aggregation`, `zip`, `nested`, `archive`, `tree-wide`
- **Snapshots:** `snapshot-a` has 6 files — `alpha.txt`, `dup.bin`, `kept.txt`, `outer.zip.d/beta.txt`, +2 more; `snapshot-b` has 7 files — `gamma-renamed.txt`, `kept-copy.txt`, `kept.txt`, `merged.bin`, +3 more

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/tree-wide-correlation/snapshot-a \
  ./test-vectors-materialized/tree-wide-correlation/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **gamma-renamed.txt**: Moved from outer.zip/inner.zip/gamma.txt
- **kept-copy.txt**: Copied from kept.txt to kept-copy.txt and outer.zip/kept-copy.txt
- **merged.bin**: Moved from dup.bin and dup-b.bin
- **outer.zip/alpha-renamed.txt**: Moved from alpha.txt
- **outer.zip/inner.zip/beta-renamed.txt**: Moved from outer.zip/beta.txt
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

No changes detected.
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

No changes detected.
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
  - Changed cells; use `binoc extract CHANGESET "data.tsv" cells_changed` for all changed cells
    - row 1, column 'age': '30' -> '31'
    - row 2, column 'city': 'Boston' -> 'Cambridge'
```

## zip-nested

Nested zip containing CSV

- **Browse source:** [zip-nested](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-nested)
- **Tags:** `zip`, `nested`, `csv`
- **Snapshots:** `snapshot-a` has 1 file — `outer.zip.d/inner.zip.d/data.csv`; `snapshot-b` has 1 file — `outer.zip.d/inner.zip.d/data.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-nested/snapshot-a \
  ./test-vectors-materialized/zip-nested/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **outer.zip/inner.zip/data.csv**: 1 row added
```

## zip-simple

Zipped files with changes inside

- **Browse source:** [zip-simple](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-simple)
- **Tags:** `zip`, `archive`
- **Snapshots:** `snapshot-a` has 1 file — `archive.zip.d/data.txt`; `snapshot-b` has 2 files — `archive.zip.d/data.txt`, `archive.zip.d/extra.txt`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-simple/snapshot-a \
  ./test-vectors-materialized/zip-simple/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **archive.zip/data.txt**: 1 line added, 1 removed
- **archive.zip/extra.txt**: New file (1 line)
```
