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

A dataset ships as a zip of CSVs alongside a statistical data file. Between
quarterly releases, the CSV columns were reordered and the data file grew:

```bash
binoc diff release-q3/ release-q4/
```

```
# Changelog: release-q3/ → release-q4/

- **data.zip/agencies.csv**: Columns reordered (content unchanged)
- **summary.dta**: 3 rows added (84 → 87 rows)
```

Binoc looked inside the zip, compared the CSV column-by-column, and used the
first-party statistical binary pack bundled in the `binoc` 0.2.0 wheel to
parse the `.dta` file into normal tabular data.

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

The `binoc` 0.2.0 wheel bundles the first-party format packs for Excel,
Parquet, Avro, DBF, XML, shapefiles, statistical binary files, binary
interchange formats, and row reorder detection. Third-party renderer plugins
can still be installed separately and are discovered automatically through
Python entry points.

SQLite support remains a first-party opt-in pack but is not included in the
default fat wheel while separate PyPI publishing is paused. See the
[plugin registry](users/reference/plugin-registry.md) for the current
distribution status of each pack and [Plugin model](plugin-developers/explanation/plugin-model.md)
to understand the current extension surfaces.

## Project status

Binoc is in active development. The CLI is ready to use; internals are unstable
and expected to change. We welcome feedback, plugin authors, and contributors.

- File issues or suggestions: [github.com/harvard-lil/binoc/issues](https://github.com/harvard-lil/binoc/issues)
- Email the team: [publicdata@law.harvard.edu](mailto:publicdata@law.harvard.edu)
