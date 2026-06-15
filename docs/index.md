---
audience: anyone
---

# Binoc

**The missing changelog for datasets.**

Binoc generates changelogs for datasets that don't have them. Given a series of
snapshots of a dataset downloaded at different times, binoc detects what
changed, expresses those changes as a minimal structured diff, and produces
human-readable summaries from the resulting changeset.

The core workflow: an archivist, data scientist, or steward has five copies of
a government dataset containing CSVs, downloaded over two years. Some are
identical. Some have reordered columns. One has a new category relevant to
their research. Binoc tells them exactly what changed, when, and whether (by
their definition) it matters.

## Example

A dataset ships as a zip of CSVs alongside a SQLite database. Between quarterly
releases, the CSV columns were reordered and the database grew:

```bash
binoc diff release-q3/ release-q4/
```

```
# Changelog: release-q3/ → release-q4/

- **data.zip/agencies.csv**: Columns reordered (content unchanged)
- **summary.sqlite**: Content changed (12.0 KB → 12.0 KB)
```

Binoc looked inside the zip and compared the CSV column-by-column. But
`.sqlite` is opaque to the standard library, so you only learn that the bytes
differ.

```bash
pip install binoc-sqlite
binoc diff release-q3/ release-q4/
```

```
# Changelog: release-q3/ → release-q4/

- **data.zip/agencies.csv**: Columns reordered (content unchanged)
- **summary.sqlite/allocations**: 3 rows added (84 → 87 rows)
```

Same command, richer output. The plugin parsed the database and found the
actual change: three new rows in the `allocations` table. Plugins install via
pip and work immediately — no configuration required.

## Getting started

New to binoc? Start with the **[Tutorial](tutorial.md)** for a guided
walkthrough, or see **[Start here](start-here.md)** for pages helpful to different audiences.

## Install

```bash
pip install binoc
```

Or run without installing:

```bash
uvx binoc diff path/to/snapshot-a path/to/snapshot-b
```

See [diff two snapshots](users/howto/diff-two-snapshots.md) for the full first-run
walkthrough.

## Plugins

Third-party plugins extend binoc with domain-specific rule packs and renderers.
Install a plugin and its formats are available automatically:

```bash
pip install binoc-sqlite
binoc diff snapshots/v1 snapshots/v2    # .sqlite/.db files now get semantic diffs
```

See [install and use plugins](users/howto/install-and-use-plugins.md) to manage
plugins and [Plugin model](plugin-developers/explanation/plugin-model.md) to
understand the current extension surfaces.

## Project status

Binoc is in active development. The CLI is ready to use; internals are unstable
and expected to change. We welcome feedback, plugin authors, and contributors.

- File issues or suggestions: [github.com/harvard-lil/binoc/issues](https://github.com/harvard-lil/binoc/issues)
- Email the team: [publicdata@law.harvard.edu](mailto:publicdata@law.harvard.edu)
