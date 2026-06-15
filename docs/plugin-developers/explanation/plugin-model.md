---
audience: plugin author, core contributor
---

# Plugin model

A binoc plugin is a package that contributes one or more rule families and/or
renderers. The standard library is just the built-in plugin pack; third-party
packs use the same SDK-owned contracts for the surfaces that are ready for
external use.

## The current extension model

```mermaid
flowchart LR
    Bytes[Snapshot bytes] --> E[Expand rules]
    E --> P[Parse rules]
    P --> L[Pair rules]
    L --> W[Edit-list writers]
    W --> C[Compaction rules]
    C --> X[Projection annotators]
    X --> IR[(Changeset tree)]
    IR --> R[Renderers]
```

| Extension | What it is for | Typical owner |
|---|---|---|
| **Expand rule** | Open a container and emit child items. | Archive, directory, workbook, database plugins |
| **Parse rule** | Parse raw bytes into typed artifacts. | CSV, statistical data, domain format plugins |
| **Pair rule** | Propose left/right item correspondences. | Move/copy, declared-correspondence, domain identity plugins |
| **Edit-list writer** | Explain one correspondence as edits. | Format or artifact owners |
| **Compaction rule** | Replace noisy edit lists with shorter claims. | Analysis plugins |
| **Projection annotator** | Add factual hints for final IR projection. | Rule packs that own vocabularies |
| **Renderer** | Render changesets into a presentation format. | Output plugins |

The old two-phase taxonomy is retired from live plugin APIs. It remains only in
historical ADRs that explain why the correspondence-first rule families
replaced it.

## How plugins compose

Plugins compose through artifacts and open vocabularies:

- A parser publishes a typed artifact such as `("binoc", "tabular", 1)`.
- A pair rule links items using evidence such as `binoc.hash` or a declared
  dataset key.
- A writer emits edit verbs such as `row_added`, `cell_changed`, or a
  plugin-defined verb.
- A compaction rule can rewrite those edits if it strictly reduces cost.
- A renderer maps tags and actions to whatever output grouping the user wants.

This preserves the m + n + o promise: new formats, new analyses, and new
renderers do not have to duplicate each other.

## Stable tier vs. proposed tier

During pre-1.0, binoc intentionally has two plugin tiers:

| Tier | Current posture |
|---|---|
| **Stable ABI tier** | Renderers, plus rule families only after their trait signatures and vocabularies have stayed stable. |
| **In-process proposed tier** | Expand, parse, pair, writer, compaction, and annotator traits while their shapes are still evolving. |

The in-tree model plugins use the in-process tier as first-party consumers. They
are not proof that every rule family is ready for an arm's-length ABI. See the
[tiered plugin surface ADR](../../adr/2026-06-12-tiered_plugin_surface_pre_1_0.md).

## Python or Rust?

Both are useful, but their current strengths differ:

| If you... | ...write Python | ...write Rust |
|---|---|---|
| ...want to build a renderer or script around generated changesets | Yes | Yes |
| ...want to prototype quickly in notebooks | Yes |  |
| ...need in-process correspondence rule traits today |  | Yes |
| ...need `DataAccess`, artifact publishing, or vector materializers |  | Yes |
| ...need low overhead on thousands of items |  | Yes |

Python rule authoring for the correspondence engine belongs to the stable-tier
ABI work after the rule shapes settle.

## Discovery

`pip install some-binoc-plugin` is the natural distribution gesture for binoc's
audience. Discovery uses Python entry points:

1. Plugin packages declare a `binoc.plugins` entry point in `pyproject.toml`.
2. At startup, `importlib.metadata.entry_points(group="binoc.plugins")` finds
   installed packages.
3. Each package's `register(registry)` function is called.
4. The registry contributes renderers and any stable-tier plugin surfaces it
   exposes.

Rust embedders can also assemble a correspondence engine config directly from
SDK rule traits.

## What belongs in `binoc-stdlib`

The standard library is the built-in rule pack. It should contain structural
necessities and broadly expected formats with modest dependency cost:

- containers and wrappers: directory, zip, tar, gzip
- universal fallbacks: text and binary projection
- common tabular support: CSV/TSV parsing and tabular edit writing
- generic pairing and compaction rules: hash pairing, declared pairs, fuzzy
  pairing, column reorder, row-addition summaries

Domain formats such as SQLite, statistical binaries, FASTA, Parquet, and Excel
belong in separate plugin packs unless they become unavoidable baseline support.

## Naming and namespacing

Namespace plugin-defined identifiers:

| Thing | Convention | Examples |
|---|---|---|
| Plugin/rule names | `package.name` | `biobinoc.fasta`, `climate.netcdf` |
| Evidence kinds | `package.evidence-name` | `binoc.hash`, `biobinoc.sequence-id` |
| Tags | `package.tag-name` | `binoc.column-reorder`, `biobinoc.sequence-changed` |
| Item types | `package.type-name` | `binoc.tabular`, `biobinoc.fasta-alignment` |
| Actions/edit verbs | Standard values unnamespaced; custom values namespaced | `add`, `remove`, `modify`; `biobinoc.gap-shift` |

Standard `binoc.*` names are reserved for the standard library.

## Where to go next

- For the engine overview: [Architecture overview](architecture.md).
- For rule dispatch: [Dispatch model](dispatch-model.md).
- For typed payloads: [Artifacts and composition](artifacts-and-composition.md).
- For writing Rust rules: [Write a Rust rule pack](../howto/write-a-rust-rule-pack.md).
- For packaging: [Publish a plugin](../howto/publish-a-plugin.md).
