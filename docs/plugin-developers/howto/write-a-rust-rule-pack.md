---
audience: Rust plugin author
---

# Write a Rust rule pack

**Goal.** Add domain-specific comparison behavior by registering
correspondence rules in Rust.

**Current status.** Rule packs are an in-process Rust surface during pre-1.0.
Python rule authoring and native rule-pack loading are deferred until the stable
ABI tier is ready. Python plugins can author renderers today.

## Choose the rule family

Most plugins contribute one or two focused rules, not a whole engine:

| Need | Rule family |
|---|---|
| Open a container and list children | `ExpandRule` |
| Parse bytes into a typed artifact | `ParseRule` |
| Propose that two side-tree nodes correspond | `PairRule` |
| Explain one link as edits | `EditListWriter` |
| Replace noisy edits with a shorter equivalent | `CompactionRule` |
| Add final tags, item types, actions, or summaries | `ProjectionAnnotator` |

Prefer the narrowest family that owns the domain knowledge. A parser should not
also decide renderer grouping; a writer should emit factual edits and tags, then
let renderers decide presentation.

## Minimal parse rule

A parse rule declares cheap selectors, reads source bytes through `DataAccess`,
and publishes a typed artifact.

```rust
use binoc_sdk::{
    tabular_v1, BinocResult, DataAccess, ItemRef, NodeMatch, ParseDescriptor,
    ParseOutput, ParseRule,
};

pub struct FastaParseRule;

impl ParseRule for FastaParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "biobinoc.parse.fasta".into(),
            input: NodeMatch {
                extensions: vec!["fasta".into(), "fa".into()],
                ..NodeMatch::default()
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let artifact_bytes = parse_fasta_as_tabular_json(&bytes)?;
        Ok(artifact_bytes.into())
    }
}

# fn parse_fasta_as_tabular_json(_: &[u8]) -> BinocResult<Vec<u8>> {
#     Ok(Vec::new())
# }
```

If you publish a public artifact, document its schema and version it. If you can
reuse an existing public artifact such as `binoc.tabular.v1`, do that instead of
inventing a private format.

## Minimal writer

An edit-list writer claims links whose artifacts it understands and returns
open-vocabulary edits.

```rust
use binoc_sdk::{
    tabular_v1, BinocResult, DataAccess, Edit, EditListWriter, LinkCtx,
    NodeMatch, ShapeFilter, WriteOutput, WriterDescriptor,
};
use serde_json::json;

pub struct FastaWriter;

impl EditListWriter for FastaWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "biobinoc.write.fasta".into(),
            formats: vec![tabular_v1()],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let Some((left, right)) = load_fasta_artifacts(ctx, data)? else {
            return Ok(None);
        };
        if left.sequence == right.sequence {
            return Ok(Some(Vec::new().into()));
        }
        Ok(Some(
            vec![Edit::new(
                "biobinoc.sequence_changed",
                json!({ "from_len": left.sequence.len(), "to_len": right.sequence.len() }),
            )
            .with_item_type("biobinoc.fasta")
            .with_tag("biobinoc.sequence-changed")]
            .into(),
        ))
    }
}

# struct FastaData { sequence: String }
# fn load_fasta_artifacts(
#     _: &LinkCtx<'_>,
#     _: &dyn DataAccess,
# ) -> BinocResult<Option<(FastaData, FastaData)>> {
#     Ok(None)
# }
```

Return `Ok(None)` when the link is not yours. This is the producer-kind
self-filter: shared artifacts can come from multiple packs, so a specialized
writer must decline foreign payloads and let generic writers run.

## Register the pack

In-process Rust consumers register rules by mutating
`CorrespondenceEngineConfig`:

```rust
use std::sync::Arc;
use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config.rules.insert(0, CoreRule::Parse(Arc::new(FastaParseRule)));
    config.writers.insert(0, Arc::new(FastaWriter));
}
# struct FastaParseRule;
# struct FastaWriter;
```

The in-tree model plugins use this pattern. See
`model-plugins/binoc-sqlite` for a parser plus collection writer and
`model-plugins/binoc-row-reorder` for a specialized writer.

## Test with vectors

Use the shared vector harness from Rust. Register your rule pack into the engine
config, materialize any source-tree fixtures, and run `run_vector` or
`run_vector_with_correspondence_engine_config`.

Vectors should assert the user-visible behavior: actions, tags, child counts,
diagnostics, and selected gold output. Name vectors for what they prove, such as
`fasta-sequence-change`, not for implementation details.

## Where to go next

- [Dispatch model](../explanation/dispatch-model.md)
- [Artifacts and composition](../explanation/artifacts-and-composition.md)
- [Test a plugin with vectors](test-a-plugin-with-vectors.md)
- [Rust SDK reference](../reference/sdk.md)
