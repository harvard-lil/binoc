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

Binoc looked inside the zip and compared the CSV column-by-column. The default
published wheel intentionally leaves SQLite out of the bundled rule set, so a
SQLite database is still reported as a binary content change unless you build
with the opt-in SQLite pack.

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

First-party format packs extend binoc with domain-specific rule packs and
renderers. Most are already compiled into the fat `binoc` wheel; the
[plugin catalog](users/reference/third-party-plugins.md) records which packs are
bundled, opt-in, or distributed separately.

See [install and use plugins](users/howto/install-and-use-plugins.md) to manage
plugins and [Plugin model](plugin-developers/explanation/plugin-model.md) to
understand the current extension surfaces.

## Project status

Binoc is in active development. The CLI is ready to use; internals are unstable
and expected to change. We welcome feedback, plugin authors, and contributors.

- File issues or suggestions: [github.com/harvard-lil/binoc/issues](https://github.com/harvard-lil/binoc/issues)
- Email the team: [publicdata@law.harvard.edu](mailto:publicdata@law.harvard.edu)
