# binoc-sqlite

SQLite comparator plugin for [Binoc](https://github.com/example/binoc). Diffs `.sqlite` / `.sqlite3` / `.db` files as standard tabular collections: a database is a table set, and each SQLite table publishes normal Binoc tabular data for row, column, and cell analysis.

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

- **data.sqlite**: Table t changed: 1 row added
- **data.sqlite::t**: 1 row added
```

Without the plugin, the same files would be reported as “Content changed” by the binary comparator.

## What it compares

- **Table set**: tables added/removed/changed via `binoc.tabular_collection.v1`.
- **Schema**: columns added/removed, SQLite column type changes.
- **Table content**: row count and cell changes via per-table `binoc.tabular.v1`.

Tags emitted include `binoc-sqlite.row-addition`, `binoc-sqlite.table-addition`, `binoc-sqlite.schema-change`, etc. Configure significance (e.g. clerical vs substantive) in your dataset config; see [Writing Binoc Plugins](../docs/writing_plugins.md).

## Development

This crate is part of the Binoc workspace. From the **workspace root**:

- Run plugin tests: `cargo test -p binoc-sqlite`
- Or from this directory: `just test` (justfile runs from parent)

Test vectors live in `test-vectors/`. They use `.sqlite.d` directories of `.sql` files; the test harness builds the `.sqlite` files at test time (see `tests/test_vectors.rs`). To regenerate expected-output snapshots:

```bash
just snapshot-update
```

(Run from `binoc-sqlite/`; the justfile runs the insta update from the workspace root.)

For writing your own Binoc plugins (Rust or Python), see the main repo’s [Writing Binoc Plugins](../docs/writing_plugins.md).
