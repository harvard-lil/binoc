---
audience: data steward, pipeline integrator
---

# Save and render changesets

**Goal.** Produce JSON and Markdown artifacts from a diff, route
outputs to specific files, and combine several saved changesets into
one changelog.

**Prerequisites.** You can already
[diff two snapshots](diff-two-snapshots.md).

## Mental model

Binoc has two distinct output concepts:

- **The changeset JSON** is the raw "IR" (intermediate representation) — the structured diff tree.
  It's what `extract`, `changelog`, and downstream pipelines consume.
- **Rendered outputs** (Markdown today; HTML or others via plugins)
  are *views* of the changeset. They apply significance classification
  and format the tree for humans.

`json` is a reserved format name handled by the CLI directly. Every
other format is a registered renderer — including `markdown`, which
is a renderer just like third-party renderers.

See
[Output routing and CLI UX ADR](../adr/2026-03-09-output_routing_and_cli_ux.md)
for the reasoning.

## Save one format to a file

Pass `-o PATH`. The format is inferred from the extension:

```bash
binoc diff A B -o changeset.json      # raw changeset IR
binoc diff A B -o CHANGES.md          # Markdown changelog
```

Stdout still shows the default (Markdown). To suppress stdout:

```bash
binoc diff A B -o changeset.json -q
```

## Save several formats in one run

`-o` is repeatable:

```bash
binoc diff A B \
    -o changeset.json \
    -o CHANGES.md \
    -q
```

One run, two files, no stdout.

## Use a non-standard extension

If an extension doesn't map cleanly to a renderer, prefix with
`format:`:

```bash
binoc diff A B -o json:output.dat
binoc diff A B -o markdown:CHANGES.txt
```

The split happens on the first colon, but only when the prefix
contains no path separators — so `/tmp/file.json` is still parsed as
a plain path, not "format `/tmp/file`, path `json`".

## Combine many changesets into one changelog

Running `binoc diff` produces a changeset for **one** snapshot pair.
When you are tracking a dataset across many releases, you end up with
a sequence of changesets:

```
changesets/
    2024-q1-to-q2.json
    2024-q2-to-q3.json
    2024-q3-to-q4.json
```

`binoc changelog` accepts one or more changeset files and produces a
combined changelog:

```bash
binoc changelog changesets/*.json -o CHANGELOG.md
```

It shares the same `-o` / `--format` / `-q` flags as `diff`.

## Programmatic access (Python)

```python
import binoc

changeset = binoc.diff("path/to/snapshot-a", "path/to/snapshot-b")
print(binoc.to_markdown([changeset]))
print(binoc.to_json([changeset]))
```

`to_markdown` / `to_json` accept a **list** of changesets so the same
combining logic used by `binoc changelog` is available from Python.

## Common issues

### "Multiple renderers claim extension X"

If two renderer plugins register the same file extension, binoc
errors and lists the conflicting names. Disambiguate with the
`format:path` prefix.

### "No renderer found for extension X"

The CLI errors when it can't infer a format from the file extension
and suggests the `format:path` syntax.

## Where to go next

- [Extract changed data](extract-changed-data.md) — given a saved
  changeset, pull out the actual changed content.
- [Changeset JSON schema](../reference/changeset-schema.md) — the
  contract for anyone consuming the JSON.
- [Dataset config](../reference/dataset-config.md) — per-renderer
  config, including significance classification.
