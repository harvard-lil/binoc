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

Binoc currently ships **43 shared examples** in this gallery.

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
| [`binary-fallback-diagnostic`](#binary-fallback-diagnostic) | Unknown file type compared by the binary fallback emits a suggestion | data.parquet: 1 edit | Default pipeline |
| [`csv-cell-changes`](#csv-cell-changes) | Individual cell values changed | data.csv: 2 edits | Default pipeline |
| [`csv-column-addition`](#csv-column-addition) | New column added | data.csv: 2 edits | Default pipeline |
| [`csv-column-removal`](#csv-column-removal) | Column removed | data.csv: 2 edits | Default pipeline |
| [`csv-column-reorder`](#csv-column-reorder) | Columns shuffled, content identical | data.csv: 1 edit | Default pipeline |
| [`csv-distribution-shift`](#csv-distribution-shift) | Numeric column distribution shifts with keyed row matching | data.csv: 5 edits | Custom config |
| [`csv-keyed-null-duplicate`](#csv-keyed-null-duplicate) | Configured CSV row keys surface null and duplicate key diagnostics | data.csv: 14 edits | Custom config |
| [`csv-keyed-row-diff`](#csv-keyed-row-diff) | Configured CSV row keys match reordered rows and report keyed row/cell changes | data.csv: 3 edits | Custom config |
| [`csv-mid-row-insertion`](#csv-mid-row-insertion) | A mid-table row insertion compacts while column reorder/addition rules remain independent | data.csv: 3 edits | Default pipeline |
| [`csv-mixed-changes`](#csv-mixed-changes) | Multiple change types | data.csv: 3 edits | Default pipeline |
| [`csv-rename-modify`](#csv-rename-modify) | CSV renamed and modified: detected as a single move by fuzzy correlation | data_v2.csv: Moved from data.csv (modified) | Default pipeline |
| [`csv-row-addition`](#csv-row-addition) | New rows appended | data.csv: 1 edit | Default pipeline |
| [`csv-row-removal`](#csv-row-removal) | Rows removed from CSV | data.csv: 2 edits | Default pipeline |
| [`csv-stacked-tables`](#csv-stacked-tables) | Detects two logical tables stacked in one messy CSV | data.csv: 1 edit | Default pipeline |
| [`csv-verbosity-full`](#csv-verbosity-full) | Markdown full verbosity renders every captured changed-cell example. | data.csv: 5 edits | Custom config |
| [`directory-file-copy`](#directory-file-copy) | New file with same content as an existing unchanged file detected as a copy | duplicate.txt: Copied from original.txt | Default pipeline |
| [`directory-nested`](#directory-nested) | Subdirectories with mixed changes | data: 0 edits | Default pipeline |
| [`directory-nested-with-tar`](#directory-nested-with-tar) | Shows binoc diffing a tar archive and a plain directory that contain overlapping internal paths. | data.tar.gz/records.csv: 1 edit | Default pipeline |
| [`file-correspondence-container`](#file-correspondence-container) | Config declares a correspondence between zip containers, which is unsupported; the rule is ignored with a warning diagn… | archive.zip: Moved from data.zip | Custom config |
| [`file-correspondence-scheme`](#file-correspondence-scheme) | Config declares that a state CSV moved into a new directory scheme is the same logical file | (root): 0 edits | Custom config |
| [`file-correspondence-token`](#file-correspondence-token) | Config declares that year-stamped CSV filenames are the same logical file | running_list_as_of_2023.csv: Moved from running_list_as_of_2022.csv (modified) | Custom config |
| [`folder-move-nested`](#folder-move-nested) | Detects a whole-folder rename and rolls many file moves up into one folder-move entry. | documentation: Moved from docs | Default pipeline |
| [`folder-move-partial`](#folder-move-partial) | Detects a mostly-moved folder rename and preserves only the added/removed/modified remainder entries beneath it. | (root): 0 edits | Default pipeline |
| [`gzip-inner-dispatch`](#gzip-inner-dispatch) | Gzipped CSV and text are decompressed and redispatched under their inner names | census.txt.gz/census.txt: 1 edit | Default pipeline |
| [`kitchen-sink`](#kitchen-sink) | Runs text, CSV, archive, move, and copy detection together in one end-to-end example. | archive.tar.gz/inventory.csv: 1 edit | Default pipeline |
| [`single-file-add`](#single-file-add) | File present in B but not A | (root): 0 edits | Default pipeline |
| [`single-file-modify-binary`](#single-file-modify-binary) | Binary file, different hash | data.bin: 1 edit | Default pipeline |
| [`single-file-modify-csv`](#single-file-modify-csv) | CSV file compared directly (file-to-file, not via directory) | data.csv: 1 edit | Default pipeline |
| [`single-file-modify-text`](#single-file-modify-text) | Text file with line-level changes | story.txt: 1 edit | Default pipeline |
| [`single-file-modify-text-root`](#single-file-modify-text-root) | Text file compared directly (file-to-file, not via directory) | story.txt: 1 edit | Default pipeline |
| [`single-file-remove`](#single-file-remove) | File present in A but not B | (root): 0 edits | Default pipeline |
| [`tar-nested`](#tar-nested) | Nested tar.gz containing CSV | outer.tar.gz/inner.tar.gz/data.csv: 1 edit | Default pipeline |
| [`tar-simple`](#tar-simple) | Tar.gz archive with changes inside | archive.tar.gz/data.csv: 1 edit | Default pipeline |
| [`text-rename-modify`](#text-rename-modify) | Text file renamed and modified: detected as a single move by fuzzy correlation | meeting-notes-v2.txt: Moved from notes.txt (modified) | Default pipeline |
| [`tree-wide-correlation`](#tree-wide-correlation) | Shows tree-wide move and copy detection across nested zip boundaries, including one-to-many copies and many-to-one moves. | gamma-renamed.txt: Moved from outer.zip/inner.zip/gamma.txt | Default pipeline |
| [`trivial-identical`](#trivial-identical) | Two identical directories → empty changeset | # Changelog: snapshot-a → snapshot-b | Default pipeline |
| [`trivial-identical-csv`](#trivial-identical-csv) | Two identical CSV files → no changes reported | # Changelog: snapshot-a → snapshot-b | Default pipeline |
| [`tsv-cell-changes`](#tsv-cell-changes) | Tab-delimited file parses into real columns and reports cell changes | data.tsv: 2 edits | Default pipeline |
| [`zip-nested`](#zip-nested) | Nested zip containing CSV | outer.zip/inner.zip/data.csv: 1 edit | Default pipeline |
| [`zip-rename-contents-rewritten`](#zip-rename-contents-rewritten) | Documents a known gap — a renamed zip whose children were all renamed AND rewritten (no content similarity) yields unpa… | (root): 0 edits | Default pipeline |
| [`zip-rename-identical`](#zip-rename-identical) | Zip archive renamed with identical contents; bottom-up roll-up of the inner clean file moves compacts the pair into a s… | archive.zip: Moved from data.zip | Default pipeline |
| [`zip-rename-inner-rename-edit`](#zip-rename-inner-rename-edit) | Zip archive renamed while its only child was renamed and had one cell edited; the modified move counts as roll-up evide… | archive.zip: Moved from data.zip | Default pipeline |
| [`zip-simple`](#zip-simple) | Zipped files with changes inside | archive.zip: 0 edits | Default pipeline |

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

- **data.parquet**: 1 edit
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

- **data.csv**: 2 edits
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

- **data.csv**: 2 edits
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

- **data.csv**: 2 edits
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

- **data.csv**: 1 edit
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

- **data.csv**: 5 edits
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

- **data.csv**: 14 edits

## Warnings

- configured row keys had null values; fell back to positional row comparison (`binoc.write.tabular`) [binoc.keyed_row_identity_degraded]
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

- **data.csv**: 3 edits
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

- **data.csv**: 3 edits
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

- **data.csv**: 3 edits
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

- **data_v2.csv**: Moved from data.csv (modified)
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

- **data.csv**: 1 edit
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

- **data.csv**: 2 edits
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

- **data.csv**: 1 edit
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

- **data.csv**: 5 edits
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

- **data**: 0 edits
- **data/records.csv**: 1 edit
- **data/extra.csv**: Added
- **docs/readme.txt**: 1 edit
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

- **data.tar.gz/records.csv**: 1 edit
- **data/records.csv**: 1 edit
```

## file-correspondence-container

Config declares a correspondence between zip containers, which is unsupported; the rule is ignored with a warning diagn…

- **Browse source:** [file-correspondence-container](https://github.com/harvard-lil/binoc/tree/main/test-vectors/file-correspondence-container)
- **Tags:** `zip`, `file-correspondence`, `diagnostics`
- **Snapshots:** `snapshot-a` has 1 file — `data.zip.d/file.csv`; `snapshot-b` has 1 file — `archive.zip.d/file.csv`
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

- **(root)**: 0 edits
- **by-state**: Added
- **by-state/AL**: Moved from data
- **by-state/AL/records.csv**: Moved from data/state_AL.csv (modified)
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

- **running_list_as_of_2023.csv**: Moved from running_list_as_of_2022.csv (modified)
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

- **(root)**: 0 edits
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

- **census.txt.gz/census.txt**: 1 edit
- **data.csv.gz/data.csv**: 2 edits
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

- **archive.tar.gz/inventory.csv**: 1 edit
- **bundle.zip/notes.txt**: 1 edit
- **data.csv**: 2 edits
- **docs**: 0 edits
- **docs/readme.txt**: 1 edit
- **docs/old-notes.txt**: Removed
- **docs/new-file.txt**: Added
- **icon.bin**: 1 edit
- **license-copy.txt**: Copied from license.txt
- **metrics.csv**: 1 edit
- **summary.txt**: Moved from report.txt
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

- **(root)**: 0 edits
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

- **data.csv**: 1 edit
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

- **story.txt**: 1 edit
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

- **story.txt**: 1 edit
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

- **(root)**: 0 edits
- **removed_file.txt**: Removed
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

- **outer.tar.gz/inner.tar.gz/data.csv**: 1 edit
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

- **archive.tar.gz/data.csv**: 1 edit
- **archive.tar.gz/hello.txt**: 1 edit
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

- **meeting-notes-v2.txt**: Moved from notes.txt (modified)
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
- **kept-copy.txt**: Copied from kept.txt
- **merged.bin**: Moved from dup.bin
- **outer.zip**: 0 edits
- **outer.zip/alpha-renamed.txt**: Moved from alpha.txt
- **outer.zip/inner.zip/beta-renamed.txt**: Moved from outer.zip/beta.txt
- **outer.zip/kept-copy.txt**: Copied from kept.txt
- **outer.zip/dup-b.bin**: Removed
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

- **data.tsv**: 2 edits
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

- **outer.zip/inner.zip/data.csv**: 1 edit
```

## zip-rename-contents-rewritten

Documents a known gap — a renamed zip whose children were all renamed AND rewritten (no content similarity) yields unpa…

- **Browse source:** [zip-rename-contents-rewritten](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-rename-contents-rewritten)
- **Tags:** `zip`, `archive`, `known-gap`
- **Snapshots:** `snapshot-a` has 3 files — `data.zip.d/x.csv`, `data.zip.d/y.csv`, `data.zip.d/z.csv`; `snapshot-b` has 3 files — `archive.zip.d/p.csv`, `archive.zip.d/q.csv`, `archive.zip.d/r.csv`

Run it:
```bash
binoc diff \
  ./test-vectors-materialized/zip-rename-contents-rewritten/snapshot-a \
  ./test-vectors-materialized/zip-rename-contents-rewritten/snapshot-b
```
Result:
```markdown
# Changelog: snapshot-a → snapshot-b

- **(root)**: 0 edits
- **data.zip**: Removed
- **data.zip/x.csv**: Removed
- **data.zip/y.csv**: Removed
- **data.zip/z.csv**: Removed
- **archive.zip**: Added
- **archive.zip/p.csv**: Added
- **archive.zip/q.csv**: Added
- **archive.zip/r.csv**: Added
```

## zip-rename-identical

Zip archive renamed with identical contents; bottom-up roll-up of the inner clean file moves compacts the pair into a s…

- **Browse source:** [zip-rename-identical](https://github.com/harvard-lil/binoc/tree/main/test-vectors/zip-rename-identical)
- **Tags:** `zip`, `archive`, `folder-move`
- **Snapshots:** `snapshot-a` has 3 files — `data.zip.d/x.csv`, `data.zip.d/y.csv`, `data.zip.d/z.csv`; `snapshot-b` has 3 files — `archive.zip.d/x.csv`, `archive.zip.d/y.csv`, `archive.zip.d/z.csv`

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
- **Snapshots:** `snapshot-a` has 1 file — `data.zip.d/old.csv`; `snapshot-b` has 1 file — `archive.zip.d/new.csv`

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
- **archive.zip/new.csv**: Moved from data.zip/old.csv (modified)
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

- **archive.zip**: 0 edits
- **archive.zip/data.txt**: 1 edit
- **archive.zip/extra.txt**: Added
```
