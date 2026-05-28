---
audience: new user
---

# Tutorial

Binoc is most useful when a plain filesystem diff would be noisy. In
this tutorial you will use the sample snapshots in this repository,
run a few increasingly useful diffs, and finish by teaching binoc one
new format.

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

No changes detected.

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

## Substantive Changes

- **story.txt**: 2 lines added, 1 removed

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

## Clerical Changes

- **data.csv**: Columns reordered (content unchanged)

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

## Substantive Changes

- **data.csv**: Column added: 'email'; columns reordered; 1 row added

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

## Substantive Changes

- **archive.zip/data.txt**: 1 line added, 1 removed
- **archive.zip/extra.txt**: New file (1 line)

```

Binoc expands container formats like zip before dispatching their contents.
Paths such as `archive.zip/data.txt` point inside the archive.

## Teaching binoc a new format

Binoc can learn domain formats through plugins. For example, the standard
library does not understand FASTA:

```bash
binoc diff ./docs/examples/fasta-demo/snapshot-a/sequences.fasta ./docs/examples/fasta-demo/snapshot-b/sequences.fasta
```

```output
# Changelog: ./docs/examples/fasta-demo/snapshot-a/sequences.fasta → ./docs/examples/fasta-demo/snapshot-b/sequences.fasta

## Substantive Changes

- **sequences.fasta**: Content changed (92 bytes → 102 bytes)

```

All the default tool can say is that the two FASTA files are different.

With a short plugin, we can report the useful fact instead of the generic byte
change. This example defines the comparator in one Python script; the same
class could later be packaged as a reusable plugin.

```bash
python - <<'PY'
from pathlib import Path
import binoc

class FastaComparator(binoc.Comparator):
    name = 'bio.fasta'
    extensions = ['.fasta', '.fa']

    def compare(self, pair):
        left = self._parse(Path(pair.left_path).read_text()) if pair.left_path else {}
        right = self._parse(Path(pair.right_path).read_text()) if pair.right_path else {}

        ids = sorted(set(left) | set(right))
        sequences_changed = sum(
            1
            for record_id in ids
            if left.get(record_id, {}).get('seq') != right.get(record_id, {}).get('seq')
        )
        headers_changed = sum(
            1
            for record_id in ids
            if left.get(record_id, {}).get('hdr') != right.get(record_id, {}).get('hdr')
        )

        if not sequences_changed and not headers_changed:
            return binoc.Identical()

        summary = (
            f'{sequences_changed} sequence(s) changed'
            if sequences_changed
            else f'Headers updated ({headers_changed} records); sequences unchanged'
        )
        tags = ['bio.header-change'] if headers_changed else []
        if sequences_changed:
            tags.append('bio.sequence-change')

        return binoc.Leaf(
            binoc.DiffNode(
                action='modify',
                item_type='fasta',
                path=pair.logical_path,
                summary=summary,
                tags=tags,
            )
        )

    @staticmethod
    def _parse(text):
        records = {}
        current = None
        for line in text.strip().split('\n'):
            if line.startswith('>'):
                current = line.split()[0][1:]
                records[current] = {'hdr': line, 'seq': ''}
            elif current:
                records[current]['seq'] += line.strip()
        return records

config = binoc.Config(comparators=['binoc.text'])
config.add_comparator(FastaComparator())
changeset = binoc.diff(
    './docs/examples/fasta-demo/snapshot-a/sequences.fasta',
    './docs/examples/fasta-demo/snapshot-b/sequences.fasta',
    config=config,
)
print(binoc.to_markdown([changeset]))
PY
```

```output
# Changelog: ./docs/examples/fasta-demo/snapshot-a/sequences.fasta → ./docs/examples/fasta-demo/snapshot-b/sequences.fasta

## Other Changes

- **sequences.fasta**: Headers updated (2 records); sequences unchanged


```

Now binoc reports that only the headers changed, and these two FASTA files do
not substantively differ.

## Where next

If you want to keep working from the command line, continue with
[Diff two snapshots](howto/diff-two-snapshots.md),
[Save and render changesets](howto/save-and-render-changesets.md), and
[Extract changed data](howto/extract-changed-data.md).

If you want to use packaged extensions, go to
[Install and use plugins](howto/install-and-use-plugins.md). If you
want to turn the FASTA script into a real plugin, continue with
[Write a Python comparator](howto/write-a-python-comparator.md).

If you want to learn more about the design of binoc, start with the
[Architecture overview](explanation/architecture.md).
