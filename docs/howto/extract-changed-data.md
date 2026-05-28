---
audience: data steward, pipeline integrator
---

# Extract changed data

**Goal.** Given a saved changeset, pull out the actual changed
content — the added rows, removed lines, or reordered columns — not
just the summary.

**Prerequisites.**
- A changeset JSON produced by `binoc diff A B -o changeset.json`.
- Both original snapshots still present on disk (extract reopens
  them).
- The same plugin set that produced the changeset (extract looks up
  plugins by name to reopen and format the data).

## The one-liner

```bash
binoc extract changeset.json PATH ASPECT
```

Where `PATH` is the logical path of a node in the changeset (for
example `data.csv` or `archive.zip/records.csv`) and `ASPECT` is the
kind of data to extract.

Example — pull the rows that were added to a CSV:

```bash
binoc diff before/ after/ -o changeset.json -q
binoc extract changeset.json data.csv rows_added
```

```text
name,age
Bob,25
Charlie,35
```

The output is valid CSV. Pipe it into another tool or inspect it
directly.

## Available aspects

Aspects depend on the node type. The common ones for the standard
library:

| Node type | Aspects |
|---|---|
| Tabular (CSV today; any comparator publishing `tabular_v1`) | `rows_added`, `rows_removed`, `cells_changed`, `columns_added`, `columns_removed`, `content` |
| Text | `diff`, `content_left`, `content_right`, `content` |
| Column reorder | `column_order` |

A plugin-authored comparator or transformer can define its own
aspects. Unknown aspects produce an error listing what's supported
for that node.

## Why both snapshots have to exist

The changeset JSON captures *what* changed — it does not carry the
changed bytes themselves (that would balloon the file). Extract walks
the **provenance chain** recorded on each node (`comparator` and
`transformed_by` fields) and reopens the snapshots through the same
comparator sequence that produced the node: directory → zip →
directory → csv, for example. At the leaf, it re-derives the data
and hands it to whichever plugin last touched the node to format.

The upshot: extract needs the snapshots and the plugins, but it does
not need to re-diff. See
[Provenance and extract ADR](../adr/2026-03-05-provenance_and_extract.md) and
[Extract and provenance](../explanation/extract-and-provenance.md).

## Common issues

### "Comparator X cannot extract aspect Y from node Z"

The node's responsible plugin doesn't know that aspect. Either pick
a supported aspect for the node type, or — if you're a plugin author
— implement `extract` for your plugin.

### "Plugin X not found"

Extract uses plugin names, so the environment running `extract` must
have the same plugins installed as the environment that produced the
changeset. If you produced a changeset with a custom plugin and try
to extract in a plain `pip install binoc` environment, you'll hit
this. Install the plugin and retry.

### Changeset describes a snapshot path that no longer exists

Extract reopens the original snapshots. Moving or deleting them
after the diff breaks extract. Keep the snapshots alongside the
changeset if you plan to extract later.

## Where to go next

- [Extract and provenance](../explanation/extract-and-provenance.md)
  — the design of the reopen chain.
- [Changeset JSON schema](../reference/changeset-schema.md) — the
  `comparator` and `transformed_by` fields extract relies on.
- [Write a Python comparator](write-a-python-comparator.md) /
  [Write a Rust comparator](write-a-rust-comparator.md) — the
  hooks to implement for a custom plugin to support extract.
