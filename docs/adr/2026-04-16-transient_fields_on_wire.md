# Transient Fields Are Wire-Visible; Output Stripping Is a Boundary Concern

**Date:** 2026-04-16
**Status:** Implemented

## Context

`DiffNode` carries a mix of durable and session-scoped fields. Durable fields —
`action`, `item_type`, `path`, `tags`, `details`, `children`, `comparator`,
`transformed_by` — belong in a changeset JSON file that a user keeps. Transient
fields — `source_items` (handles to the original `ItemRef` pair) and
`artifacts` (descriptors for derived data stored under `data_root/.cache/`) —
are meaningful only during a live diff session. Handles reference temp paths
and the artifact cache, both of which dissolve when the session ends.

The original implementation marked both transient fields `#[serde(skip)]`,
motivated by "don't put them in changeset JSON." That same annotation governs
every serialization path, including the plugin ABI wire where
`TransformRequest`/`ExtractRequest` carry a `DiffNode` as JSON between the
controller and a separately-compiled plugin. The ABI request types compensated
by carrying top-level `source_items` and `artifacts` sidecar fields, which
`export_plugin!` (and the corresponding test harness wrapper) spliced back
onto the root of the received node.

This worked as long as the plugin only needed the root-level transient fields.
Any container-matching transformer that reasons about its children broke it.
Children are regular `DiffNode`s whose `source_items`/`artifacts` were stripped
by `#[serde(skip)]` in transit and were never restored, because the ABI
protocol only carried root-level sidecars. The concrete symptom: `MoveDetector`
running over a CSV-bearing directory would return `Replace(container)` with
children whose `tabular_v1` artifacts had been deleted, and the subsequent
`TabularAnalyzer` would find empty artifact lists and produce bare "Tabular
modified" nodes instead of the usual `binoc.cell-change` /
`binoc.column-reorder` analysis. The ABI test harness
(`AbiTransformer` in `binoc-sdk::test_support`) faithfully reproduced the
production protocol and caught the regression, but only by eyeballing the
snapshot — there was no automatic parity check against direct dispatch.

## Decision

The confusion came from treating one serde attribute as answering two
different questions. Separate them.

### 1. `#[serde(skip)]` → `skip_serializing_if` on transient fields

`DiffNode.source_items` and `DiffNode.artifacts` are serialized whenever they
are populated. They use `skip_serializing_if = "Option::is_none"` /
`skip_serializing_if = "Vec::is_empty"` so empty nodes stay compact, but they
are present on the wire whenever they carry information. Standard serde
behavior handles children automatically — transient fields round-trip for
every node in a subtree, not just the root.

### 2. Stripping is the output writer's job

`DiffNode::strip_transient()` and `Changeset::strip_transient()` recursively
clear the transient fields on a node/tree. The controller calls
`Changeset::strip_transient()` at the end of `Controller::diff()` before
returning, so every consumer (JSON output, renderers, insta snapshots, Python
bindings, CLI) sees a stripped changeset. Extract does not rely on stripped
output: it rebuilds transient state on demand by replaying the comparator
chain (see [provenance and extract](2026-03-05-provenance_and_extract.md)).

### 3. ABI request types stop carrying transient sidecars

`TransformRequest` and `ExtractRequest` no longer have top-level `source_items`
or `artifacts` fields. Everything transient lives inside the serialized `node`.
The `export_plugin!` macro, the Python native-plugin loader
(`binoc-python/src/lib.rs`), and the ABI test harness
(`binoc-sdk::test_support`) all lose their per-call "save top-level transient
fields, splice them back on" logic. Three copies of the same workaround are
gone.

### 4. Test harness enforces direct/ABI parity

`run_vector_with_abi_log` in the test vector harness now takes two registry
builders — one direct, one ABI-wrapped — runs the same vector through both,
and asserts the resulting changesets are byte-identical JSON. Any future
divergence between in-process dispatch and the ABI protocol (whether
caused by a protocol bug, a plugin "cheating" with in-process state, or a
nondeterminism) fails the test rather than requiring a human to notice a
snapshot drift. This converts the harness from "faithful reproduction" to
"enforced equivalence."

## Consequences

- The cost of the abstraction leak is paid once at the controller exit
  boundary, not N times at every protocol site.
- Session-transient fields become a legitimate, documented wire concern. Any
  new transient field added to `DiffNode` (or similar wire IR) needs exactly
  two pieces of work: add it to `strip_transient`, and inherit
  `skip_serializing_if` for efficiency. No changes to the ABI request types,
  no new restoration code in loaders.
- Plugins authored in any language get identical behavior to Rust stdlib
  plugins. Previously, anyone reimplementing `export_plugin!` in another
  language would have had to rediscover the sidecar dance.
- Direct/ABI parity is a first-class test invariant, not a reviewer obligation.

## Alternatives Considered

**Per-path transient envelope on every request/response.** Keep
`#[serde(skip)]` on `DiffNode`; add
`transient_fields: Vec<{ path, source_items, artifacts }>` to wire types.
Walk subtrees to collect before send, walk again to restore after. More
protocol surface, two separate places that define what "transient" means,
and the envelope has to stay in sync with plugin authors' `path` assignments.
Rejected.

**Declare container-matching transformers don't see child transients.**
Simplest for the protocol: make child transients an official non-feature of
any transformer acting on a container. Such transformers that need child
artifacts would have to `data.local_path()` source items and re-parse each
child. Rejected because it contradicts the thin-comparator /
composable-transformer model established by the [transformer composition and
artifact flow](2026-03-20-transformer_composition_and_artifact_flow.md) ADR: a
container-scope transformer that wants to reclassify based on child-level
semantic content is a normal thing to build, and forcing each such transformer
to re-parse every child defeats the purpose of publishing artifacts at all.

**A separate out-of-process test harness.** Build a subprocess-based runner
that exercises the real `export_plugin!`-generated entry points in actual
address-space isolation, modeling what the `AbiTransformer` wrapper only
approximates. Deferred: the current C ABI is same-process dlopen, so the
in-process harness accurately models the data plane. Build a subprocess runner
when there is a concrete driver (panic/crash isolation testing, a WASM or
remote plugin runner, a non-Rust plugin language) rather than pre-emptively.

## Scope Rule

Whenever a field on a wire type is "don't show in user-facing output but
needed during a session," the right treatment is:

1. Serialize it normally (with `skip_serializing_if` on empty).
2. Add it to a `strip_transient` on the containing type.
3. Call `strip_transient` at the output writer boundary.

Do not conflate "hide from user output" with "hide from the ABI wire."
