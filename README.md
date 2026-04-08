# Binoc: The Missing Changelog for Datasets

Binoc generates changelogs for datasets that don't have them. Given a series of snapshots of a dataset downloaded at different times, Binoc detects what changed, expresses those changes as a minimal structured diff, and produces human-readable summaries that distinguish substantive policy changes from clerical housekeeping.

The core workflow: an archivist, data scientist, or steward has five copies of a government dataset containing CSVs, downloaded over two years. Some are identical. Some have reordered columns. One has a new category relevant to their research. Binoc tells them exactly what changed, when, and whether (by their definition) it matters.

To submit feedback on Binoc, please fill out [our form](https://forms.gle/MDZTZ1DvhuAanM8P9) or send us an email at publicdata@law.harvard.edu.

## Example

A dataset ships as a zip of CSVs alongside a SQLite database. Between quarterly releases, the CSV columns were reordered and the database grew:

```bash
binoc diff release-q3/ release-q4/
```

```
# Changelog: release-q3/ → release-q4/

## Clerical Changes

- **data.zip/agencies.csv**: Columns reordered (content unchanged)

## Substantive Changes

- **summary.sqlite**: Content changed (12.0 KB → 12.0 KB)
```

Binoc looked inside the zip and compared the CSV column-by-column — the reorder is flagged as clerical housekeeping, not a real data change. But `.sqlite` is opaque to the standard library, so you only learn that the bytes differ.

```bash
pip install binoc-sqlite
binoc diff release-q3/ release-q4/
```

```
# Changelog: release-q3/ → release-q4/

## Clerical Changes

- **data.zip/agencies.csv**: Columns reordered (content unchanged)

## Substantive Changes

- **summary.sqlite/allocations**: 3 rows added (84 → 87 rows)
```

Same command, richer output. The plugin parsed the database and found the actual change: three new rows in the `allocations` table. Plugins install via pip and work immediately — no configuration required.

## Why It Exists

Datasets published by governments, research institutions, and public bodies are living artifacts, and can change without warning or documentation (or without consistent documentation). The archival and data science communities need tooling to:

- Detect whether a new snapshot of a dataset actually differs from the previous one.
- Describe changes precisely — not just "the file changed," but "three columns were reordered (clerical) and one column was split into two (substantive)."
- Produce changelogs that are machine-readable for automated pipelines and human-readable for policy analysis.
- Handle real-world messiness: datasets inside zip archives, nested containers, mixed formats, renamed files.

Generic diff tools don't understand data formats, while version control systems track lines, not columns or schemas. Binoc bridges this gap.

## Current Capabilities

- Compare directory snapshots recursively
- Diff zip archives, including nested zip contents
- Diff tar/tar.gz/tgz archives, including nested tar contents
- Compare CSV files with row, column, and cell awareness
- Compare text files at line level
- Compare binary files by content hash
- Detect moves and copies from content hashes
- Extract actual changed data from changeset nodes (added rows, text diffs, etc.)
- Render changesets as JSON or Markdown changelogs
- Extend comparison and transformation pipelines via Rust native plugins (C ABI), Python plugins, or in-workspace stdlib plugins

## Documentation

- [tutorial.md](docs/tutorial.md): end-to-end contributor walkthrough.
- [test-vectors/](test-vectors/): fixtures demonstrating major capabilities.
- [docs/adr/](docs/adr/): records of architectural decisions.

## Quick Start

### Install via pip (recommended)

```bash
pip install binoc
```

Or run without installing:

```bash
uvx binoc diff path/to/snapshot-a path/to/snapshot-b
```

### Usage

Diff two snapshots (prints a Markdown changelog to stdout by default):

```bash
binoc diff path/to/snapshot-a path/to/snapshot-b
```

Get raw changeset JSON instead:

```bash
binoc diff path/to/snapshot-a path/to/snapshot-b --format json
```

Save outputs to files (format inferred from extension, or use `format:path` syntax):

```bash
binoc diff path/to/snapshot-a path/to/snapshot-b \
  -o changeset.json -o CHANGELOG.md -q
```

Combine saved changesets into a changelog:

```bash
binoc changelog changesets/*.json
```

Extract the actual changed data from a changeset node (requires original snapshots):

```bash
binoc extract changeset.json data.csv rows_added
```

### Plugins

Third-party plugin packs extend binoc with domain-specific comparators and transformers. Install a plugin and its formats are available automatically:

```bash
pip install binoc-sqlite                # SQLite schema + row count diffing
binoc diff snapshots/v1 snapshots/v2    # .sqlite/.db files now get semantic diffs
```

Or with `uvx`, no install needed:

```bash
uvx binoc --with binoc-sqlite diff snapshots/v1 snapshots/v2
```

Plugins can be Rust crates (compiled as native shared libraries via the `export_plugin!` macro) or pure Python. See [docs/adr/](docs/adr/) for architecture and [model-plugins/](model-plugins/) for reference implementations.

### Rust-only CLI

A standalone Rust binary with standard library plugins (no Python, no plugin discovery) is also available:

```bash
cargo install binoc-cli
binoc-cli diff path/to/snapshot-a path/to/snapshot-b
```

### Development

Prerequisites: [Rust](https://rustup.rs/), [just](https://github.com/casey/just) (`brew install just`), and [uv](https://docs.astral.sh/uv/).

```bash
just build   # Rust workspace + Python bindings
just test    # full suite: Rust + Python
just docs    # regenerate tutorial after code changes
```

To test the full Python CLI with local plugin crates (no PyPI needed):

```bash
uv run --with ./binoc-python --with ./model-plugins/binoc-sqlite \
  binoc diff path/to/snapshot-a path/to/snapshot-b
```

This builds both packages from source and wires up entry-point discovery automatically. The same pattern works for any local plugin crate that has a `pyproject.toml` with a `[project.entry-points."binoc.plugins"]` section. For a self-contained plugin example (install, run, test vectors), see [model-plugins/binoc-sqlite/](model-plugins/binoc-sqlite/).

## Workspace Layout

| Path | Role |
|---|---|
| `binoc-sdk/` | Plugin SDK: traits, IR types, `DataAccess`, `export_plugin!` macro, C ABI wire types |
| `binoc-core/` | Controller loop, config, plugin registry, output functions |
| `binoc-stdlib/` | Standard comparators and transformers (architecturally identical to third-party plugins) |
| `binoc-cli/` | CLI library + standalone Rust binary |
| `binoc-python/` | PyO3 bindings, native plugin loader (`libloading`), Python plugin bridges, `binoc` CLI entry point |
| `model-plugins/` | Reference plugin implementations: `binoc-sqlite` (Rust comparator), `binoc-row-reorder` (Rust transformer), `binoc-html` (Python renderer) |
| `test-vectors/` | Shared test fixtures for standard library plugins |
| `docs/` | Documentation, design notes, and ADRs |


## Future Work

- Additional plugins such as Excel, Parquet, PDF
- `binoc plugin install` / `binoc plugin list` CLI subcommands
- Richer Python notebook ergonomics
- LLM-summarized output formatter
- WASM/IPC plugin transport (ABI designed for it; not yet implemented)
- Memory-bounded processing for very large trees
- Similarity-based rename detection for modified-and-moved files
- Fixed-point transformer iteration (transformers currently run in a single pass, may miss optimizations)
