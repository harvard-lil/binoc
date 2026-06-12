---
audience: Python plugin author
---

# Write a Python comparator

**Goal.** Build a Python comparator that teaches binoc one new file
format, then run it from a script or notebook.

**Prerequisites.**
- `pip install binoc`.
- Understanding of what a comparator *is* (see
  [Plugin model](../explanation/plugin-model.md)).

## The minimal shape

A comparator subclasses `binoc.Comparator`, declares the extensions or
media types it claims, and implements `compare(pair)`:

```python
import binoc

class FastaComparator(binoc.Comparator):
    name = "biobinoc.fasta"
    extensions = [".fasta", ".fa", ".fna"]

    def compare(self, pair):
        if pair.left_path and pair.right_path:
            left = open(pair.left_path).read()
            right = open(pair.right_path).read()
            if left == right:
                return binoc.Identical()

            node = binoc.DiffNode(
                action="modify",
                item_type="fasta",
                path=pair.logical_path,
                tags=["biobinoc.sequence-changed"],
                details={
                    "sequences_left": left.count(">"),
                    "sequences_right": right.count(">"),
                },
                summary=f"{right.count('>')} sequences in new version",
            )
            return binoc.Leaf(node)

        elif pair.right_path:
            return binoc.Leaf(binoc.DiffNode(
                action="add", item_type="fasta", path=pair.logical_path,
            ))

        else:
            return binoc.Leaf(binoc.DiffNode(
                action="remove", item_type="fasta", path=pair.logical_path,
            ))
```

Key points:

- `name` is a namespaced string (for example `biobinoc.fasta`, not
  `fasta`). Use your package name as the prefix. See
  [Plugin discovery](../reference/plugin-discovery.md) for
  conventions.
- `extensions` is a list of lowercase dotted suffixes. Dispatch is
  declarative: the first comparator to claim an item wins.
- `media_types` is the MIME-type equivalent of `extensions`.
  Python comparators can declare it, but they do not receive
  `media_type` on `ItemPair`; use it only as a dispatch filter.
- `can_handle(pair)` is the imperative fallback for comparators that
  declare neither `extensions` nor `media_types`.
- If your descriptor matches and the file turns out to be unsuitable,
  return `binoc.Skip()` from `compare()` and the controller will try
  the next candidate.
- `compare()` returns `binoc.Identical()`, `binoc.Skip()`,
  `binoc.Leaf(node)`, or `binoc.Expand(node, children)` (for
  container formats that expand into child item pairs).
- `pair.left_path` / `pair.right_path` are physical file paths, or
  `None` for adds / removes. `pair.logical_path` is the user-facing
  path used in rendered output.

## Use it without packaging

For scripts and notebooks, attach the comparator instance to a config
directly. Ad-hoc comparators run before the built-in binary fallback,
so a custom extension can claim its file before `binoc.binary` reports
a generic byte-level change.

```python
import binoc

config = binoc.Config.default()
config.add_comparator(FastaComparator())

changeset = binoc.diff("snapshot-a", "snapshot-b", config=config)
print(binoc.to_markdown([changeset]))
```

This is the intended path for one-off analysis. When you're ready to
distribute, see [Publish a plugin](publish-a-plugin.md).

## Classify your tags

Out of the box your custom tags (for example `biobinoc.sequence-changed`)
render in the default flat Markdown list. Teach the renderer your grouping via
[dataset config](../../users/reference/dataset-config.md):

```yaml
output:
  markdown:
    groups:
      - heading: "Substantive changes"
        tags:
          - biobinoc.sequence-changed
      - heading: "Clerical changes"
        tags:
          - biobinoc.header-change
```

```python
config = binoc.Config.from_file("dataset.yaml")
config.add_comparator(FastaComparator())
```

See
[Significance classification](../../users/explanation/significance-classification.md)
for why this is a renderer concern rather than an IR concern.

## DiffNode API

Nodes are immutable-ish — builder methods return new nodes:

```python
node = binoc.DiffNode(action="modify", item_type="fasta", path="seqs.fa")
node = node.with_tag("biobinoc.gap-change")
node = node.with_detail("gap_count", 42)
node = node.annotate_from("biobinoc", "note", "large gap shift")
node = node.with_source_path("old_seqs.fa")   # for moves/renames
node = node.with_children([child1, child2])

# Reading
node.action        # "modify"
node.item_type     # "fasta"
node.path          # "seqs.fa"
node.tags          # ["biobinoc.gap-change"]
node.details       # {"gap_count": 42}
node.children      # [child1, child2]
node.annotations   # [{"package": "binoc", "key": "note", "value": "..."}]
```

## Limits of Python comparators

Python comparators receive a simplified interface:

- **No `DataAccess`.** You get file paths, not the trait object.
  You cannot publish artifacts or allocate scratch workspaces through
  the SDK.
- **No `content_hash` or `media_type`** on `ItemPair`.

For any of those capabilities, write a Rust plugin — see
[Write a Rust comparator](write-a-rust-comparator.md). For most
domain formats, the Python interface is enough.

## Testing

Construct an `ItemPair` and call `compare()` directly:

```python
import binoc

comp = FastaComparator()
pair = binoc.ItemPair.both(
    "tests/old.fasta", "tests/new.fasta",
    "old.fasta", "new.fasta",
)
result = comp.compare(pair)
assert isinstance(result, binoc.Leaf)
assert result.node.action == "modify"
assert "biobinoc.sequence-changed" in result.node.tags
```

For end-to-end tests, see
[Test a plugin with vectors](test-a-plugin-with-vectors.md).

## Where to go next

- [Publish a plugin](publish-a-plugin.md) — package the comparator
  on PyPI so `pip install` makes it available automatically.
- [Write a Python transformer](write-a-python-transformer.md) — add
  a pattern-detection pass over the IR your comparator produces.
- [Write a Python renderer](write-a-python-renderer.md) — emit a
  custom output format.
