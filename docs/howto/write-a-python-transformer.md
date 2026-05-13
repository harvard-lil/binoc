---
audience: Python plugin author
---

# Write a Python transformer

**Goal.** Build a transformer in Python that rewrites nodes in the
IR tree produced by comparators, ending with a working plugin
attached to a `Config` you can use from a script or notebook.

**Prerequisites.**
- `pip install binoc`.
- You have either written a comparator that emits the nodes you want
  to rewrite, or you plan to consume nodes emitted by an existing
  comparator. See [Write a Python comparator](write-a-python-comparator.md)
  and [Plugin model](../explanation/plugin-model.md).

## The minimal shape

A transformer subclasses `binoc.Transformer`, declares which nodes
it matches, and implements `transform(node)`:

```python
import binoc

class SequenceNormalizer(binoc.Transformer):
    name = "biobinoc.sequence_normalizer"
    match_types = ["fasta"]

    def transform(self, node):
        if (node.action == "modify"
                and node.details.get("sequences_left")
                    == node.details.get("sequences_right")):
            return binoc.Replace(
                node.with_tag("biobinoc.whitespace-only")
            )
        return binoc.Unchanged()
```

Key points:

- **Dispatch is declarative.** Declare matching criteria on the
  class: one or more of `match_types`, `match_tags`,
  `match_actions`, or `node_shape` (`"any"`, `"container"`,
  `"leaf"`). The controller dispatches when **all** non-empty fields
  match (AND-of-ORs): within each field any value suffices, and every
  populated field must match. An empty list means "no restriction on
  this axis".
- **Return types.** `binoc.Unchanged()`, `binoc.Replace(node)`,
  `binoc.ReplaceMany(nodes)`, or `binoc.Remove()`.
- **Ordering matters.** Transformers run in the order declared in
  the dataset config. Later transformers see the output of earlier
  ones. See [Dispatch model](../explanation/dispatch-model.md).
- **The tree walk is bottom-up.** When your transformer sees a
  container node, its children have already been transformed.

## Use it without packaging

```python
import binoc

config = binoc.Config.default()
config.add_comparator(FastaComparator())
config.add_transformer(SequenceNormalizer())

changeset = binoc.diff("snapshot-a", "snapshot-b", config=config)
print(binoc.to_markdown([changeset]))
```

For distribution, see [Publish a plugin](publish-a-plugin.md).

## Limits of Python transformers

Python transformers operate on the `DiffNode` tree only. In
particular:

- **No `source_items`.** Python transformers cannot re-parse source
  data. If your pattern detection needs raw bytes, either make the
  comparator publish the data as details on the node, or move the
  transformer to Rust (see
  [Write a Rust transformer](write-a-rust-transformer.md)).
- **No `DataAccess`.** Python transformers cannot read or publish
  artifacts.
- **No artifact dispatch.** `match_artifacts` is available to Rust
  transformers, not Python transformers.

This is deliberate. The Python interface optimizes for
straightforward IR rewrites; anything that needs raw data belongs in
Rust where the full `DataAccess` trait is available.

## Testing

Construct nodes and call `transform()` directly:

```python
import binoc

node = binoc.DiffNode(
    action="modify",
    item_type="fasta",
    path="seq.fa",
    details={"sequences_left": 10, "sequences_right": 10},
)
tr = SequenceNormalizer()
result = tr.transform(node)
assert isinstance(result, binoc.Replace)
assert "biobinoc.whitespace-only" in result.node.tags
```

For end-to-end tests, see
[Test a plugin with vectors](test-a-plugin-with-vectors.md).

## Where to go next

- [Publish a plugin](publish-a-plugin.md) — package this transformer
  on PyPI.
- [Write a Python comparator](write-a-python-comparator.md) — emit
  the nodes your transformer rewrites.
- [Artifacts and composition](../explanation/artifacts-and-composition.md)
  — how transformers in the Rust world consume typed data from
  comparators.
