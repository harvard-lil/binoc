# Binoc: The Missing Changelog for Datasets

Binoc generates changelogs for datasets that don't have them. Given a
series of snapshots of a dataset downloaded at different times, it
detects what changed, expresses those changes as a minimal structured
diff, and produces human-readable summaries that distinguish
substantive policy changes from clerical housekeeping. The primary
audience is archivists, data scientists, and stewards tracking
undocumented changes to published datasets.

**Documentation:** <https://harvard-lil.github.io/binoc/>

## Install

```bash
pip install binoc
```

Or run without installing:

```bash
uvx binoc diff path/to/snapshot-a path/to/snapshot-b
```

Plugins extend binoc with domain-specific format support and install
the same way:

```bash
pip install binoc-sqlite          # semantic SQLite schema + row-count diffing
```

The [documentation site](https://harvard-lil.github.io/binoc/) has
tutorials, how-to recipes, reference for the CLI / Python API /
Rust SDK / changeset schema, and the architectural explanation set.
Start at the [Tutorial](https://harvard-lil.github.io/binoc/tutorial/)
if you're new, [Start here](https://harvard-lil.github.io/binoc/start-here/)
for a role-based map of the site, or the
[Architecture overview](https://harvard-lil.github.io/binoc/explanation/architecture/)
if you're evaluating or extending binoc.

## Project status

Binoc is in a collaborative design phase. The CLI is ready to use;
internals are unstable and expected to change. We welcome feedback,
plugin authors, and contributors.

- File issues or suggestions:
  <https://github.com/harvard-lil/binoc/issues>
- Email: <lil@law.harvard.edu>
- Feedback form: <https://forms.gle/MDZTZ1DvhuAanM8P9>

## Architectural ground rules

The contract for human and AI contributors lives in
[`AGENTS.md`](./AGENTS.md). The long-form record of every major
design decision lives in
[`docs/adr/`](https://harvard-lil.github.io/binoc/adr/).
