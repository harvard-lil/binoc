---
audience: data steward, archivist, pipeline integrator
---

# Diff two snapshots

**Goal.** Produce a human-readable changelog that describes what
changed across an ordered sequence of dataset snapshots.

**Prerequisites.** `binoc` installed (`pip install binoc` or
`uvx binoc …`). Nothing else.

## The one-liner

```bash
binoc diff path/to/snapshot-a path/to/snapshot-b
```

By default this prints a Markdown changelog to stdout. That's usually
what you want at a terminal.

To diff a release sequence in one run, pass more snapshots:

```bash
binoc diff release-q1/ release-q2/ release-q3/
```

Binoc will emit two pairwise sections: `release-q1/ → release-q2/` and
`release-q2/ → release-q3/`.

## What the output looks like

For a dataset that ships as a zip of CSVs alongside a SQLite database:

```text
# Changelog: release-q3/ → release-q4/

- **data.zip/agencies.csv**: Columns reordered (content unchanged)
- **summary.sqlite**: Content changed (12.0 KB → 12.0 KB)
```

Binoc looked inside the zip and compared the CSV column by column. The
`.sqlite` file is opaque to the standard library, so you only learn the bytes
differ. To get semantic SQLite diffing, see
[Install and use plugins](install-and-use-plugins.md).

## Choose an output format

The `--format` flag switches the stdout renderer. The two built-ins
are `markdown` (the default) and `json` (raw changeset IR):

```bash
binoc diff A B                    # Markdown to stdout (default)
binoc diff A B --format json      # raw changeset JSON object to stdout
binoc diff A B C --format json    # JSON array of pairwise changesets
```

A third-party plugin may register additional renderers (for example
an HTML renderer); reference it by name.

## Save outputs to files

`-o`/`--output` is repeatable. Each value is either
`format:path` (explicit format) or a bare path (format inferred from
extension):

```bash
binoc diff A B -o changeset.json -o CHANGES.md
```

Suppress stdout with `-q` when every output is going to a file:

```bash
binoc diff A B -o changeset.json -q
```

For the full story on output routing, see
[Save and render changesets](save-and-render-changesets.md) and
[Output routing and CLI UX ADR](../adr/2026-03-09-output_routing_and_cli_ux.md).

## Common issues

### "Content changed" with no detail

If binoc reports only `Content changed (X bytes → Y bytes)` for a
file, it means no comparator claimed the file's extension and it fell
through to the binary catch-all. That's the signal to install a
plugin that understands the format (see
[Install and use plugins](install-and-use-plugins.md)) or to write
one (see [Write a Python comparator](write-a-python-comparator.md)).

## Where to go next

- [Save and render changesets](save-and-render-changesets.md) — write
  JSON and Markdown artifacts, combine several changesets into one
  changelog.
- [Extract changed data](extract-changed-data.md) — pull actual
  changed rows or lines out of a changeset.
- [Tutorial](../tutorial.md) — a longer guided walkthrough of
  directory, zip, CSV, and plugin diffs.
