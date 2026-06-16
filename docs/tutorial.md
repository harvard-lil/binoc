---
audience: new user
---

# Tutorial

Binoc is most useful when a plain filesystem diff would be noisy. In
this tutorial you will use the sample snapshots in this repository and
run a few increasingly useful diffs.

Clone the repository so you have the tutorial fixtures:

    git clone https://github.com/harvard-lil/binoc
    cd binoc

Install binoc so the plain `binoc` command is available:

    pip install binoc

Or run it without installing:

    uvx binoc diff path/to/snapshot-a path/to/snapshot-b

The examples below show the plain commands a user would run after
installing `binoc`.

<!--
`just docs-tutorial` runs Showboat in a uv environment that includes
`./binoc-python`, so the visible `binoc` commands below execute against
the local source tree while still reading like normal user commands.
-->

## Build the example snapshots

The repository keeps some fixtures as source trees and materializes the
real archives on demand. Run this once from the repository root:

```bash
just materialize
```

You now have a `test-vectors-materialized/` folder full of ready-to-diff
snapshots.

## Run your first diff

Start with the simplest possible case: two identical snapshots.

```bash
binoc diff ./test-vectors-materialized/trivial-identical/snapshot-a ./test-vectors-materialized/trivial-identical/snapshot-b
```

```output
# Changelog: ./test-vectors-materialized/trivial-identical/snapshot-a → ./test-vectors-materialized/trivial-identical/snapshot-b


```

## See a text-file change

Now compare two snapshots where one text file changed:

```bash
cat ./test-vectors-materialized/single-file-modify-text/snapshot-a/story.txt
printf '\n---\n'
cat ./test-vectors-materialized/single-file-modify-text/snapshot-b/story.txt
```

```output
Line 1
Line 2
Line 3
Line 4
Line 5

---
Line 1
Line 2 revised
Line 3
Line 4
Line 5
Line 6
```

```bash
binoc diff ./test-vectors-materialized/single-file-modify-text/snapshot-a ./test-vectors-materialized/single-file-modify-text/snapshot-b
```

```output
# Changelog: ./test-vectors-materialized/single-file-modify-text/snapshot-a → ./test-vectors-materialized/single-file-modify-text/snapshot-b

- **story.txt**: 2 lines added; 1 line removed
  - Line changes
    - line 2: 'Line 2' -> 'Line 2 revised'

```

So far this is just a changelog-style summary of what you would get
with a textual diff.

## CSV-aware diffing

Binoc gets more interesting when the format itself matters. Here the
same rows are present in both snapshots, but the CSV columns were
reordered:

```bash
cat ./test-vectors-materialized/csv-column-reorder/snapshot-a/data.csv
printf '\n---\n'
cat ./test-vectors-materialized/csv-column-reorder/snapshot-b/data.csv
```

```output
name,age,city
Alice,30,NYC
Bob,25,LA

---
city,name,age
NYC,Alice,30
LA,Bob,25
```

```bash
binoc diff ./test-vectors-materialized/csv-column-reorder/snapshot-a ./test-vectors-materialized/csv-column-reorder/snapshot-b
```

```output
# Changelog: ./test-vectors-materialized/csv-column-reorder/snapshot-a → ./test-vectors-materialized/csv-column-reorder/snapshot-b

- **data.csv**: Columns reordered
  - Reorder Columns: order: ["city","name","age"]

```

That is the first genuinely useful binoc result. A line-oriented diff
would treat this like a rewrite. Binoc understands the header row and
recognizes that the data itself did not change.

Now look at a more realistic update: one new column, a reorder, and a
new row in the same file.

```bash
binoc diff ./test-vectors-materialized/csv-mixed-changes/snapshot-a ./test-vectors-materialized/csv-mixed-changes/snapshot-b
```

```output
# Changelog: ./test-vectors-materialized/csv-mixed-changes/snapshot-a → ./test-vectors-materialized/csv-mixed-changes/snapshot-b

- **data.csv**: Column added: 'email'; Columns reordered; 1 row added
  - Rows added
    - row 3: 'SF', 'Charlie', '35'
  - Reorder Columns: order: ["city","name","age"]
  - Add Column: name: 'email'; values: {"total_values":3,"truncated":false,"values":["a@test.com","b@test.com","c@test.com"]}

```

The changelog expresses a complex change in terms a dataset maintainer can
act on.

## Look inside a zip

The same `diff` command works on nested content too:

```bash
binoc diff ./test-vectors-materialized/zip-simple/snapshot-a ./test-vectors-materialized/zip-simple/snapshot-b
```

```output
# Changelog: ./test-vectors-materialized/zip-simple/snapshot-a → ./test-vectors-materialized/zip-simple/snapshot-b

- **archive.zip/>data.txt**: 1 line added; 1 line removed
  - Line changes
    - line 1: 'hello from zip A' -> 'hello from zip B'
- **archive.zip/>extra.txt**: Added

```

Binoc expands container formats like zip before dispatching their contents.
Paths such as `archive.zip/data.txt` point inside the archive.

## Unsupported formats

When Binoc has no semantic rules for a domain format, it still reports a
truthful fallback. For example, the standard library does not understand
FASTA:

```bash
binoc diff ./docs/examples/fasta-demo/snapshot-a/sequences.fasta ./docs/examples/fasta-demo/snapshot-b/sequences.fasta
```

```output
# Changelog: ./docs/examples/fasta-demo/snapshot-a/sequences.fasta → ./docs/examples/fasta-demo/snapshot-b/sequences.fasta

- **sequences.fasta**: Binary content changed; 1 extracted string added, 1 extracted string removed
  - Extracted strings added
    - '>gene_1 source=NCBI retrieved=2024-06\nATCGATCGATCG\n>gene_2 source=NCBI retrieved=2024-06\nGCTAGCTAGCTA\n'
  - Extracted strings removed
    - '>gene_1 src=GenBank date=2024-01\nATCGATCGATCG\n>gene_2 src=GenBank date=2024-01\nGCTAGCTAGCTA\n'

```

The suggestion is intentional: it tells you the output is a safe byte-level
result, not a domain-specific FASTA interpretation.

## Where next

If you want to keep working from the command line, continue with
[Diff two snapshots](users/howto/diff-two-snapshots.md),
[Save and render changesets](users/howto/save-and-render-changesets.md), and
[Extract changed data](users/howto/extract-changed-data.md).

If you want to use packaged extensions, go to
[Install and use plugins](users/howto/install-and-use-plugins.md). If you want
to understand how current plugins fit into the engine, continue with
[Plugin model](plugin-developers/explanation/plugin-model.md).

If you want to learn more about the design of binoc, start with the
[Architecture overview](plugin-developers/explanation/architecture.md).
