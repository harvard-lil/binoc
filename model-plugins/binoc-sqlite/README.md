# binoc-sqlite

SQLite comparator plugin for [Binoc](https://github.com/example/binoc). Diffs `.sqlite` / `.sqlite3` / `.db` files by **schema** (tables, columns, types) and **row counts** — not row-by-row content — so you get summaries like “1 table modified”, “1 row added (1 → 2 rows)”, or “Table added (2 columns, 5 rows)” instead of a raw binary change.

## Install

From PyPI (requires Binoc and Python 3.10+):

```bash
pip install binoc binoc-sqlite
```

From the repo (when developing Binoc or this plugin):

```bash
uv run --with ./binoc-python --with ./binoc-sqlite binoc diff snapshot-a snapshot-b
```

## Example

Build two SQLite DBs and diff them (requires `sqlite3` on PATH):

```bash
mkdir -p /tmp/demo/snapshot-a /tmp/demo/snapshot-b
echo "CREATE TABLE t (id INT); INSERT INTO t VALUES (1);" | sqlite3 /tmp/demo/snapshot-a/data.sqlite
echo "CREATE TABLE t (id INT); INSERT INTO t VALUES (1); INSERT INTO t VALUES (2);" | sqlite3 /tmp/demo/snapshot-b/data.sqlite
binoc diff /tmp/demo/snapshot-a /tmp/demo/snapshot-b
```

Example output:

```markdown
# Changelog: /tmp/demo/snapshot-a → /tmp/demo/snapshot-b

## Other Changes

- **data.sqlite**: 1 table modified
- **data.sqlite/t**: 1 row added (1 → 2 rows)
```

Without the plugin, the same files would be reported as “Content changed” by the binary comparator.

## What it compares

- **Schema**: tables added/removed, columns added/removed, column type changes.
- **Row counts** per table (not cell-level diffs).

Tags emitted include `binoc-sqlite.row-addition`, `binoc-sqlite.table-addition`, `binoc-sqlite.schema-change`, etc. Configure significance (e.g. ministerial vs substantive) in your dataset config; see [Writing Binoc Plugins](../docs/writing_plugins.md).

## Development

This crate is part of the Binoc workspace. From the **workspace root**:

- Run plugin tests: `cargo test -p binoc-sqlite`
- Or from this directory: `just test` (justfile runs from parent)

Test vectors live in `test-vectors/`. They use `.sqlite.d` directories of `.sql` files; the test harness builds the `.sqlite` files at test time (see `tests/test_vectors.rs`). To regenerate expected-output snapshots:

```bash
just snapshot-update
```

(Run from `binoc-sqlite/`; the justfile runs the insta update from the workspace root.)

For writing your own Binoc plugins (Rust or Python), see the main repo’s [Writing Binoc Plugins](../docs/writing_plugins.md) and [design doc](../docs/design.md).
