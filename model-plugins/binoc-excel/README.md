# binoc-excel

Excel correspondence rule pack for [Binoc](https://github.com/example/binoc).
Diffs spreadsheet workbooks (`.xlsx` / `.xls` / `.xlsm` / `.xlsb` / `.ods`) by
treating a workbook as a **namespace of named sheets**. The workbook file itself
is a plain container node (no artifact); every non-empty sheet becomes a **child
node** (`book.xlsx/>Sheet1`) carrying standard Binoc `binoc.tabular.v1` data for
row, column, and cell analysis.

This holds even for a single-sheet workbook: sheets have intrinsic names, so a
one-sheet `book.xlsx` still emits a `book.xlsx/>Sheet1` child rather than an
artifact on the workbook node.

## Install

From PyPI (requires Binoc and Python 3.10+):

```bash
pip install binoc binoc-excel
```

From the repo (when developing Binoc or this plugin):

```bash
uv run --with ./binoc-python --with ./binoc-excel binoc diff snapshot-a snapshot-b
```

## Example

Given two snapshots whose `data.xlsx` differs by one cell on the `Scores` sheet:

```bash
binoc diff snapshot-a snapshot-b
```

Example output:

```markdown
# Changelog: snapshot-a → snapshot-b

- **data.xlsx/>Scores**: 1 cell changed
  - Changed cells
    - row 1, column 'score': 85.0 -> 92.0
```

Without the plugin, the same files are reported through the byte-level fallback.

## What it compares

- **Sheet set**: sheets added/removed/renamed surface as child node
  add/remove/move via the ordinary pair rules (the workbook is a container, like
  a zip), not via a parent manifest.
- **Schema**: columns added/removed and column type changes per sheet.
- **Sheet content**: row count and cell changes via each sheet's
  `binoc.tabular.v1` artifact.

Each sheet child uses the sheet's name verbatim, joined to the workbook path
with the decompose-boundary separator `/>` (see the
[parsed-children ADR](../../docs/adr/2026-06-14-parsed_children_and_decompose_boundaries.md)).
Cell values keep their source type (numbers, booleans, dates) so the typed diff
reports them as such. For renderer grouping and tags, see the main docs'
[Plugin model](../../docs/plugin-developers/explanation/plugin-model.md).

## Development

This crate is part of the Binoc workspace. From the **workspace root**:

- Run plugin tests: `cargo test -p binoc-excel`
- Or from this directory: `just test` (justfile runs from parent)

Test vectors live in `test-vectors/`. They use `.xlsx.d` directories of `.csv`
files (one CSV per sheet, filename = sheet name); the test harness builds the
`.xlsx` files at test time (see `tests/test_vectors.rs` and
`src/test_support.rs`). To regenerate expected-output snapshots:

```bash
INSTA_UPDATE=always cargo test -p binoc-excel --test test_vectors
```

For writing your own Binoc plugins, see the main repo's
[Write a Rust rule pack](../../docs/plugin-developers/howto/write-a-rust-rule-pack.md)
and [Write a Python renderer](../../docs/plugin-developers/howto/write-a-python-renderer.md).
