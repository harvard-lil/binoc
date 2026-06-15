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
- The same rule/plugin set that produced the changeset.

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
| Tabular (CSV today; any parser publishing `tabular_v1`) | `rows_added`, `rows_removed`, `cells_changed`, `columns_added`, `columns_removed`, `column_order`, `content` |
| Text | `diff`, `content_left`, `content_right`, `content` |

A plugin-authored writer can define its own aspects. Unknown aspects produce an
error listing what's supported for that node.

## Why both snapshots have to exist

The changeset JSON captures *what* changed — it does not carry the
changed bytes themselves (that would balloon the file). Extract reruns
the correspondence engine against the original snapshots, finds the
projected node's live left/right link, and asks the rule that owns that
projection to return the requested aspect. Archive paths such as
`archive.zip/records.csv` are reopened by the same expand rules used
during diff.

The upshot: extract needs the snapshots and the plugins, but it does
not serialize changed data into the changeset.

## Common issues

### "Rule X cannot extract aspect Y from node Z"

The node's responsible rule does not expose that aspect. Pick a
supported aspect for the node type, or add extract support to the rule
that owns the projection.

### "Plugin X not found"

Extract reruns the rule pipeline, so the environment running `extract`
must have the same rule/plugin pack installed as the environment that
produced the changeset. If you produced a changeset with a custom
plugin and try to extract in a plain `pip install binoc` environment,
you'll hit this. Install the plugin and retry.

### Changeset describes a snapshot path that no longer exists

Extract reopens the original snapshots. Moving or deleting them
after the diff breaks extract. Keep the snapshots alongside the
changeset if you plan to extract later.

## Where to go next

- [Changeset JSON schema](../reference/changeset-schema.md) — the
  durable node fields used by renderers and extract.
