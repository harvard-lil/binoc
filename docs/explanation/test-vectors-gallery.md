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

Binoc currently ships **28 shared examples** in this gallery.

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
| [`csv-cell-changes`](#csv-cell-changes) | Individual cell values changed | data.csv: 2 cells changed | Default pipeline |
| [`csv-column-addition`](#csv-column-addition) | New column added | data.csv: Column added: 'email' | Default pipeline |
| [`csv-column-removal`](#csv-column-removal) | Column removed | data.csv: Column removed: 'city' | Default pipeline |
| [`csv-column-reorder`](#csv-column-reorder) | Columns shuffled, content identical | data.csv: Columns reordered (content unchanged) | Custom config |
| [`csv-mixed-changes`](#csv-mixed-changes) | Multiple change types | data.csv: Column added: 'email'; columns reordered; 1 row added | Default pipeline |
| [`csv-rename-modify`](#csv-rename-modify) | CSV renamed and a column added: detected as a single move with content diff via fuzzy correlation | data_v2.csv: Moved from data.csv (modified) | Default pipeline |
| [`csv-row-addition`](#csv-row-addition) | New rows appended | data.csv: 2 rows added | Default pipeline |
| [`csv-row-removal`](#csv-row-removal) | Rows removed from CSV | data.csv: 2 rows removed | Default pipeline |
| [`directory-file-copy`](#directory-file-copy) | New file with same content as an existing unchanged file detected as a copy | duplicate.txt: Copied from original.txt | Default pipeline |
| [`directory-nested`](#directory-nested) | Subdirectories with mixed changes | data/extra.csv: New table (2 columns, 1 rows) | Default pipeline |
| [`directory-nested-with-tar`](#directory-nested-with-tar) | Shows binoc diffing a tar archive and a plain directory that contain overlapping internal paths. | data/records.csv: 1 row added | Default pipeline |
| [`folder-move-nested`](#folder-move-nested) | Detects a whole-folder rename and rolls many file moves up into one folder-move entry. | documentation: Folder moved from docs | Default pipeline |
| [`kitchen-sink`](#kitchen-sink) | Runs text, CSV, archive, move, and copy detection together in one end-to-end example. | metrics.csv: Columns reordered (content unchanged) | Default pipeline |
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

## Other Changes

- **data.csv**: 2 cells changed
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

## Substantive Changes

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

## Substantive Changes

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

## Clerical Changes

- **data.csv**: Columns reordered (content unchanged)
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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

- **data.csv**: 2 rows removed
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

## Other Changes

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

## Substantive Changes

- **data/extra.csv**: New table (2 columns, 1 rows)
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

## Substantive Changes

- **data/records.csv**: 1 row added

## Other Changes

- **data.tar.gz/records.csv**: 1 cell changed
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

## Other Changes

- **documentation**: Folder moved from docs
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

## Clerical Changes

- **metrics.csv**: Columns reordered (content unchanged)

## Substantive Changes

- **archive.tar.gz/inventory.csv**: 1 row added
- **bundle.zip/notes.txt**: 2 lines added, 1 removed
- **docs/new-file.txt**: New file (1 line)
- **docs/old-notes.txt**: File removed (1 line)
- **docs/readme.txt**: 2 lines added, 2 removed
- **icon.bin**: Content changed (19 bytes → 19 bytes)

## Other Changes

- **data.csv**: 2 cells changed
- **license-copy.txt**: Copied from license.txt
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

## Substantive Changes

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

## Substantive Changes

- **data.bin**: Content changed (4 bytes → 4 bytes)
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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

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

## Substantive Changes

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

## Other Changes

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

## Other Changes

- **data.tsv**: 2 cells changed
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

## Substantive Changes

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

## Substantive Changes

- **archive.zip/data.txt**: 1 line added, 1 removed
- **archive.zip/extra.txt**: New file (1 line)
```
