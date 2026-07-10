# Binoc Test Vectors

This directory contains test vectors for the binoc diff engine. Each vector defines a pair of snapshots (A and B) and expected structural assertions on the resulting changeset.

## Directory Layout

Each vector lives in its own subdirectory. A root `manifest.toml` (optional) provides default `[config]` and `[expected]`; per-vector manifests override.

```
test-vectors/
├── README.md           # This file
├── manifest.toml       # Optional defaults for all vectors here
├── trivial-identical/
│   ├── manifest.toml   # Vector metadata and expected assertions
│   ├── snapshot-a/     # "Before" snapshot
│   └── snapshot-b/     # "After" snapshot
├── single-file-add/
│   ├── manifest.toml
│   ├── snapshot-a/
│   └── snapshot-b/
└── ...
```

Plugin crates (e.g. `binoc-sqlite`) may ship their own test vectors under `binoc-sqlite/test-vectors/` with the same manifest format and root-defaults merge. Those are run by `cargo test -p binoc-sqlite`.

## Manifest Format

Each vector has a `manifest.toml` with this schema:

```toml
[vector]
name = "vector-name"
description = "What this tests"
tags = ["tag1", "tag2"]

[docs]
# Optional: user-facing gallery metadata
# summary = "Short description for the examples gallery"
# setup = "Optional setup note shown above the run command"

[config]
# Optional: override default dataset/rendering config
# dataset = { paths = [{ match = "**/*.csv", row_identity = { columns = ["id"] } }] }
# output = { markdown = { verbosity = "summary" } }

[expected]
# Structural assertions
# root_action = "modify"
# child_count = 1
# has_tags = ["binoc.column-reorder"]
# significance = "clerical"
```

### Sections

- **`[vector]`** — Metadata: `name`, `description`, `tags`
- **`[docs]`** — Optional user-facing gallery metadata:
  - `summary` — short, user-facing description used in the examples gallery
  - `setup` — optional setup note that overrides the generator's default text
- **`[config]`** — Optional dataset config overrides from the normal config schema:
  `dataset`, `renderers`, and renderer config under `output` such as
  `output.markdown.verbosity`. Legacy `comparators`, `transformers`, and
  `transformer_config` keys are rejected.
- **`[expected]`** — Assertions on the changeset output:
  - `root_action` — Kind of the root diff node (e.g. `modify`, `add`, `remove`)
  - `child_count` — Number of children at root
  - `has_tags` — Tags that must appear (in root or descendants)
  - `significance` — e.g. `clerical`, `substantive`

## Snapshot Layout

- **`snapshot-a/`** — The "from" snapshot (baseline)
- **`snapshot-b/`** — The "to" snapshot (target)

Snapshots are plain directory trees. The test harness compares `snapshot-a` to `snapshot-b` and runs assertions from `manifest.toml`.

## Zip Vectors

For zip-based vectors, use `.zip.d` directories. The test harness builds these into `.zip` files before comparison:

- `archive.zip.d/data.txt` → `archive.zip` containing `data.txt`
- `outer.zip.d/inner.zip.d/data.csv` → nested zips

## Tar Vectors

For tar-based vectors, use `.tar.d`, `.tar.gz.d`, or `.tgz.d` directories. The test harness builds these into tar archives before comparison:

- `archive.tar.gz.d/data.txt` → `archive.tar.gz` containing `data.txt`
- `outer.tar.gz.d/inner.tar.gz.d/data.csv` → nested tar.gz archives

## Gzip Vectors

For single-stream gzip vectors, use `.gz.d` directories with the uncompressed
inner file inside. The test harness builds the compressed file before
comparison:

- `data.csv.gz.d/data.csv` → `data.csv.gz` containing `data.csv` bytes
- `census.txt.gz.d/census.txt` → `census.txt.gz` containing `census.txt` bytes

## Opaque Binary Vectors

For deterministic opaque payloads, use a `.bin.d` staging directory containing
`recipe.json`. The repeated hex pattern fills `size` bytes; optional patches
replace bounded regions with their own repeated patterns. The harness builds
the recipe into the filename with the final `.d` removed.

```json
{
  "size": 65536,
  "repeat_hex": "00010203",
  "patches": [
    { "offset": 32768, "length": 64, "repeat_hex": "ff" }
  ]
}
```

For example, `payload.bin.d/recipe.json` becomes `payload.bin`. The materializer
also supports `.binless.d` for the extensionless-content-sniffing fixture.

## SQLite Vectors (plugin)

In plugin test vectors (e.g. `binoc-sqlite/test-vectors/`), use `.sqlite.d` or `.db.d` directories. Building the `.sqlite`/`.db` file from those sources is the **plugin’s** responsibility (via the harness’s optional `prepare` callback), not the shared harness; see `binoc-sqlite/tests/test_vectors.rs`. Example layout: `data.sqlite.d/01_schema.sql` and `data.sqlite.d/02_data.sql` → `data.sqlite`.

## Naming Conventions

- **Vector names**: `kebab-case`, descriptive (e.g. `csv-column-reorder`, `single-file-add`)
- **Tags**: Lowercase, hyphenated (e.g. `binoc.column-reorder`, `binoc.content-changed`)

## Adding New Vectors

1. Create a new directory: `test-vectors/<vector-name>/`
2. Add `manifest.toml` with `[vector]`, optional `[config]`, and `[expected]`
3. Create `snapshot-a/` and `snapshot-b/` with the required files
4. For binary artifacts, use source staging directories such as `.zip.d`,
   `.tar.gz.d`, `.gz.d`, or `.bin.d`; the harness builds them deterministically

## Available Vectors

| Vector | Description |
|--------|-------------|
| trivial-identical | Two identical directories → empty changeset |
| single-file-add | File present in B but not A |
| single-file-remove | File present in A but not B |
| single-file-modify-text | Text file with line-level changes |
| single-file-modify-binary | Binary file, different hash |
| csv-column-reorder | Columns shuffled, content identical |
| csv-row-addition | New rows appended |
| csv-column-addition | New column added |
| csv-column-removal | Column removed |
| csv-cell-changes | Individual cell values changed |
| csv-mixed-changes | Multiple change types |
| directory-file-move | File moved (same content, different location) |
| directory-nested | Subdirectories with mixed changes |
| zip-simple | Zipped files with changes inside |
| zip-nested | Nested zip containing CSV |
| tar-simple | Tar.gz archive with changes inside |
| tar-nested | Nested tar.gz containing CSV |
