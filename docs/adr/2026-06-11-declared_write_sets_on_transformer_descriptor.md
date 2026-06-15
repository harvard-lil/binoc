# Declared Write-Sets on TransformerDescriptor

**Date:** 2026-06-11
**Status:** Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) — `TransformerDescriptor` was removed in the migration; the write-set discipline carried over to rule descriptors and is mechanized in [Invariant and Lint Tiers](2026-06-12-invariant_and_lint_tiers.md)

## Context

`TransformerDescriptor` declared only what a transformer READS — the
`match_types`/`match_tags`/`match_actions`/`match_artifacts`/`node_shape`
dispatch filters. What a transformer WRITES (the tags it adds, the action
values it sets, the item types and artifact formats it introduces) was
undeclared, discoverable only by reading the implementation.

That gap had concrete costs. Audits like the one behind the
[pure-reorder collapse](2026-06-11-inline_pure_reorder_judgment.md) had to
grep every transformer body to learn who emits which tag; nothing checked
that a transformer's emissions stayed inside what its author believed it
emitted (the `ColumnReorderDetector` `tags.clear()` bug lived undetected
in exactly that blind spot); and there was no machine-readable way to ask
"which tags are single-producer/single-consumer dispatch channels?"

MLIR's open-vocabulary experience says the cheap, high-value move is a
`dependentDialects` analogue: transformers declare the vocabularies they
may *emit*, checked by a verifier.

## Decision

Add four declared write-sets to `TransformerDescriptor`, with builders in
the existing style: `emits_tags`, `emits_actions`, `emits_item_types`,
and `publishes_artifacts` (artifact formats). `emits_item_types` exists
because transformers do write item types — `TableSplitter` rewrites a
node to `tabular_collection` and creates `tabular` children.

**Declared vs. legacy is distinguishable.** Each field is
`Option<Vec<…>>` with `#[serde(default)]`: `None` means "legacy plugin,
nothing declared" and is exempt from enforcement; `Some(vec![])` means
"writes nothing" and is enforced. This deliberately inverts the
`match_*` convention — in READ fields, empty means *unconstrained*; in
WRITE fields, empty means *writes nothing*. The asymmetry is documented
on the struct. The `Option` is the escape hatch that lets third-party
plugins compiled against older SDKs keep loading and running unchecked.

**Never for scheduling or dispatch.** Write-sets are for verification,
lint, and future capability negotiation only. No ordering logic may be
built on them. The rationale is LLVM's fifteen-year arc: the legacy pass
manager had declared dependencies plus a scheduler, and the new pass
manager — like MLIR after it — abandoned that for explicit, user-ordered
pipelines. Declared effects fed to a solver rot into untruthful
declarations precisely because they are load-bearing; declared effects
checked by a verifier stay honest because lying fails the build. Binoc's
pipeline order remains an explicit config list ("config order is
semantics, no solver").

**Wire visibility.** Descriptors already cross the C ABI inside
`PluginDescription` (the `_binoc_plugin_describe` registration payload),
so the new `#[serde(default)]` fields are wire-visible with no request
struct changes; `TransformRequest` carries nodes, not descriptors, and is
untouched. Per the [SDK compatibility policy](2026-03-12-plugin_sdk_and_abi.md),
additive `#[serde(default)]` fields do not bump the compatibility floor
(`MIN_COMPATIBLE_MINOR` stays 1); the SDK minor version bumps 0.1 → 0.2
so a plugin built against the write-set SDK is identifiable and is not
loaded by older hosts that would silently ignore its declarations.

**Harness enforcement, not runtime.** The test-vector harness's
`AbiTransformer` wrapper snapshots the facts of each transform call's
input subtree (tags, actions, item types, artifact formats anywhere in
the tree) and asserts that everything *new* in the output subtree(s) is
inside the transformer's declared write-set. A violation is a test
failure naming the transformer and the undeclared emission. Because
every stdlib vector runs through the ABI-wrapped registry, every
transformer pass on every vector is checked; production runs pay
nothing. The set-difference semantics means moving an existing tag or
action between nodes is not an "emission" — only introducing one the
input tree didn't have.

**Lint for single-producer/single-consumer tags.**
`single_producer_single_consumer_tags()` walks registered descriptors
and flags any tag declared in exactly one `emits_tags` and matched by
exactly one other transformer's `match_tags` — the "function call drawn
slowly" shape that the pure-reorder collapse retired. Callers pass an
allowlist for tags that are legitimately consumed outside transformer
dispatch: `binoc.cell-change` → `binoc-row-reorder` is the documented
example, allowlisted because renderer group configs also consume the tag
and the consumer genuinely needs its own scan.

**Stdlib declares fully.** Every stdlib transformer and
`binoc-row-reorder` declares all four write-sets (audited against the
implementations); a test asserts stdlib never regresses to `None`. The
declarations are facts about the code, not aspirations — the harness
catches drift in either direction for emissions the vectors exercise.

Out of scope, recorded deliberately: inherent-vs-discardable tag
classification (MLIR's other half), cost functions, any change to
transformer ordering or recompare, and the comparator descriptor refactor
— though the schema is shaped to unify later into a reads/writes pair
shared with `ComparatorDescriptor` (comparators publish artifacts and
emit actions too); a doc comment on `TransformerDescriptor` sketches it.

## Alternatives Considered

**Enforce at runtime in the controller.** Rejected: production diffs
would pay a full tree walk per transformer pass to catch what is a
plugin-author bug, and a runtime failure would turn a harmless undeclared
annotation into a user-facing crash. The harness sees every stdlib
transformer on every vector; third-party authors get the same check by
running their vectors through the shared harness.

**Use the write-sets to order or skip transformers.** Rejected
permanently, not deferred — see the LLVM rationale above. The moment
declarations drive scheduling, authors are incentivized to game them and
the verifier's ground truth is gone.

**A single `declared: bool` flag plus plain `Vec` fields.** Same
expressiveness, but it allows the incoherent state `declared: true` with
the field's meaning depending on a sibling flag, and serde's `None`
omission keeps legacy descriptors byte-identical on the wire.
`Option<Vec>` makes "undeclared" unrepresentable as a value collision.

**Per-node (positional) diff instead of tree-set difference.** Stricter —
it would catch a transformer copying an existing tag onto a new node —
but transformers legitimately restructure trees (fold children, split
tables, relocate remainders), and node identity across a rewrite is not
well-defined. Set semantics over the subtree is the invariant that
survives restructuring.
