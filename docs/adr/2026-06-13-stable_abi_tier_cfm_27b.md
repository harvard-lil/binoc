# Stable ABI Tier Assessment for CFM-27b

**Date:** 2026-06-13
**Status:** Implemented

## Context

The [tiered plugin surface ADR](2026-06-12-tiered_plugin_surface_pre_1_0.md)
sets a graduation signal for process-boundary plugin families: two release
windows with no trait-shape or vocabulary changes. The correspondence-first
engine migration is now complete enough to apply that signal honestly.

The existing C ABI in `binoc-sdk::plugin_abi` is renderer-only. It exposes
`_binoc_plugin_describe`, `_binoc_free_string`, and `_binoc_renderer_render`.
The payloads are JSON:

- `PluginDescription` writes the SDK version and renderer descriptors.
- `RenderRequest` reads projected `Changeset` values plus renderer config.
- `RenderResponse` writes either a rendered string or an error message.

Renderer requests carry no `DataAccess` handle and perform no artifact reads or
tree mutation. Their read set is the public projected IR; their write set is
only formatted output.

In the same branch, CFM-45 changed the parse rule surface: `ParseOutput` can now
create parsed children with their own artifacts. That is a useful in-process
shape, but it resets the stabilization clock for expand/parse graduation.

## Decision

Renderers graduate as the current stable ABI tier. No broad ABI framework
rewrite is needed for CFM-27b because the renderer ABI already has the right
shape: descriptor discovery, JSON request/response, SDK version checking, and a
host wrapper in `binoc-python`.

The renderer wire contract is:

```text
_binoc_plugin_describe() -> JSON PluginDescription
_binoc_renderer_render(index, JSON RenderRequest) -> JSON RenderResponse
_binoc_free_string(ptr)
```

`RenderRequest.changesets` is the complete projected public IR. Its vocabularies
remain open: action strings, item types, tags, source evidence strings,
detail-block kinds, global claim verbs, and edit verbs flow through the ABI as
data. The host must not reject an otherwise valid renderer request because it
does not recognize one of those values.

Expand and parse packs do not graduate in CFM-27b. Their descriptor data
(`NodeMatch`, output `ArtifactFormat`, and future read/write declarations) is
serializable, but the callable trait shape changed in this branch. The two
release-window clock starts after the parsed-child surface settles.

Pair rules explicitly remain proposed-tier only. The current trait hands a rule
the whole `EngineView`; Phase 3 may narrow pairing around dirty sets/frontiers.
A C ABI for pair rules needs a snapshot or batch view protocol after that shape
settles. The chatty whole-view trait must not be frozen across a process
boundary.

Edit-list writers, compaction rules, projection annotators, and dataset
configurators also remain in-process. They can graduate later when their
descriptor and read/write surfaces are explicit enough to audit without host
internals.

## Consequences

- CFM-27b lands as a minimal renderer-stability step rather than a new ABI
  framework.
- Renderer ABI parity is now covered by a focused SDK unit test that exercises
  the generated `export_plugin!` entry points and proves unknown/open IR
  vocabulary reaches a renderer unchanged.
- Expand/parse ABI work is deferred deliberately, not omitted accidentally; the
  parsed-child change resets the clock.
- Pair ABI work remains blocked on the `EngineView` transit design and the
  Phase 3 pairing performance decision.

## Alternatives Considered

**Graduate expand/parse immediately.** Rejected. Their data structs are
serializable, but CFM-45 changed the parse output contract in this branch. A
stable ABI built now would freeze a shape that has not passed the graduation
signal.

**Design all future ABI payloads now.** Rejected. Renderer payloads are settled
because renderers only read public changesets and write strings. Rule families
need explicit read/write descriptors and, for pair rules, an `EngineView` transit
shape. Designing those before Phase 3 would be speculative.

**Remove the C ABI until more families graduate.** Rejected. Renderers satisfy
the stable-tier signal today, and the existing C ABI is small, tested, and
useful for language-agnostic output plugins.
