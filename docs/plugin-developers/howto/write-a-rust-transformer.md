---
audience: Rust plugin author
---

# Write a Rust transformer

**Goal.** Build a transformer in Rust that rewrites nodes in the
completed IR tree, ending with a working plugin packaged through
maturin.

**Prerequisites.**
- Rust toolchain and maturin.
- Familiarity with
  [Write a Rust comparator](write-a-rust-comparator.md) (the project
  layout, `Cargo.toml`, and `export_plugin!` macro are identical).
- Conceptual background in [Plugin model](../explanation/plugin-model.md)
  and [Artifacts and composition](../explanation/artifacts-and-composition.md).

For a reference implementation, read
[`model-plugins/binoc-row-reorder`](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-row-reorder).

## The minimal shape

```rust
use binoc_sdk::*;

#[derive(Default)]
pub struct SequenceNormalizer;

impl Transformer for SequenceNormalizer {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("biobinoc.sequence_normalizer")
            .with_match_types(vec!["fasta".into()])
    }

    fn transform(
        &self,
        node: DiffNode,
        _data: &dyn DataAccess,
    ) -> BinocResult<TransformResult> {
        let left = node.details.get("sequences_left");
        let right = node.details.get("sequences_right");
        if node.action == "modify" && left == right {
            return Ok(TransformResult::Replace(
                node.with_tag("biobinoc.whitespace-only"),
            ));
        }
        Ok(TransformResult::Unchanged)
    }
}
```

Key points:

- **Dispatch is declarative.** `TransformerDescriptor` declares which
  nodes you match on — any combination of match_types, match_tags,
  match_actions, match_artifacts, and node_shape. All non-empty fields
  must match (AND-of-ORs). See
  [Dispatch model](../explanation/dispatch-model.md).
- **Return types.** `TransformResult::Unchanged`, `Replace(node)`,
  `ReplaceMany(nodes)`, or `Remove`. `Unchanged` is the zero-cost
  path — return it whenever you don't need to rewrite.
- **Ordering matters.** Transformers run in the order declared in
  the dataset config. Later transformers see the output of earlier
  ones. The tree walk is bottom-up, so when your transformer sees a
  container, its children are already in their final form.

Register the transformer via the same `export_plugin!` macro used
for comparators:

```rust
binoc_sdk::export_plugin! {
    module: biobinoc,
    transformers: [SequenceNormalizer],
}
```

Or register it alongside your comparators:

```rust
binoc_sdk::export_plugin! {
    module: biobinoc,
    comparators: [FastaComparator],
    transformers: [SequenceNormalizer],
}
```

## Consume a comparator's artifact

The cleanest way for a transformer to get structured data is to
**consume an artifact** published by a comparator. This decouples the
transformer from any specific comparator — any comparator publishing
`tabular_v1` can feed your transformer:

```rust
impl Transformer for TabularEnricher {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("biobinoc.tabular_enricher")
            .with_match_artifacts(vec![tabular_v1()])
    }

    fn transform(
        &self,
        node: DiffNode,
        data: &dyn DataAccess,
    ) -> BinocResult<TransformResult> {
        let fmt = tabular_v1();
        let Some(desc) = node.artifacts.iter().find(|a| {
            a.format == fmt && a.subject == ArtifactSubject::Left
        }) else {
            return Ok(TransformResult::Unchanged);
        };
        let Some(bytes) = data.get_artifact(desc)? else {
            return Ok(TransformResult::Unchanged);
        };
        let tabular: TabularData = serde_json::from_slice(&bytes).unwrap();
        let new_node = enrich(node, &tabular);
        Ok(TransformResult::Replace(new_node))
    }
}
```

Artifacts are the public composition contract. See
[Artifacts and composition](../explanation/artifacts-and-composition.md)
for the full story and the thin-comparator pattern.

## When to reach for `source_items`

The controller sets `DiffNode::source_items` on every node during the
diff. If no artifact carries what you need, transformers can re-parse
source data:

```rust
fn transform(
    &self,
    node: DiffNode,
    data: &dyn DataAccess,
) -> BinocResult<TransformResult> {
    let Some(pair) = &node.source_items else {
        return Ok(TransformResult::Unchanged);
    };
    // re-parse pair.left / pair.right via data.read_bytes(...) etc.
    ...
}
```

**Prefer artifacts.** They avoid redundant re-parsing across
transformers and enable cross-plugin composition. Use `source_items`
only when you genuinely need raw byte access (for example, hashing
for move detection) or the comparator you want to cooperate with does
not publish a suitable artifact.

## Testing

See [Test a plugin with vectors](test-a-plugin-with-vectors.md) for
the shared harness. For unit tests, construct `DiffNode` values and
call `transform()` directly.

## Where to go next

- [Publish a plugin](publish-a-plugin.md) — packaging, entry points,
  versioning.
- [Write a Rust comparator](write-a-rust-comparator.md) — emit the
  nodes (or artifacts) your transformer consumes.
- [Artifacts and composition](../explanation/artifacts-and-composition.md)
  — the deeper story on publishing and consuming typed data across
  plugin boundaries.
