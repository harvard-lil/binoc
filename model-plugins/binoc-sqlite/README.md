# binoc-sqlite

SQLite correspondence rule pack for [Binoc](https://github.com/example/binoc).
Diffs `.sqlite` / `.sqlite3` / `.db` files by decomposing each database into its
tables. The database node becomes a plain container (like a zip), and every SQL
table is published as a **child node** — `data.sqlite/>customers` — carrying
normal Binoc `binoc.tabular.v1` data for row, column, and cell analysis. The SQL
table name is used verbatim as the child name, since it is the table's intrinsic
identity.

## Use

Separate PyPI publishing for this native rule pack is paused. The default
`binoc` wheel does not include SQLite because it is excluded from the
`binoc-cli` `bundled` feature set; enable the `sqlite` feature explicitly when
building from source.

```bash
cargo run -p binoc-cli --features sqlite -- diff snapshot-a snapshot-b
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

- **data.sqlite/>t**: 1 row added
  - Rows added
    - row 2: 2
```

Without the plugin, the same files are reported through the byte-level fallback.

## What it compares

The plugin registers a single parse rule that turns each database into a
container of `binoc.tabular.v1` table children. All diffing is then handled by
the stdlib pair rules and tabular writer operating on those child nodes:

- **Table set**: a table added, removed, or renamed renders as a child node
  added, removed, or moved under the database container.
- **Table content**: row, column, and cell changes render on the table child via
  the stdlib tabular writer (e.g. `binoc.row-addition`, `binoc.column-addition`,
  `binoc.schema-change`).

Configure renderer grouping in your dataset config; see the main docs'
[Plugin model](../../docs/plugin-developers/explanation/plugin-model.md).

## Development

This crate is part of the Binoc workspace. From the **workspace root**:

- Run plugin tests: `cargo test -p binoc-sqlite`
- Or from this directory: `just test` (justfile runs from parent)

Test vectors live in `test-vectors/`. They use `.sqlite.d` directories of `.sql` files; the test harness builds the `.sqlite` files at test time (see `tests/test_vectors.rs`). To regenerate expected-output snapshots:

```bash
just snapshot-update
```

(Run from `binoc-sqlite/`; the justfile runs the insta update from the workspace root.)

For writing your own Binoc plugins, see the main repo's
[Write a Rust rule pack](../../docs/plugin-developers/howto/write-a-rust-rule-pack.md)
and [Write a Python renderer](../../docs/plugin-developers/howto/write-a-python-renderer.md).
