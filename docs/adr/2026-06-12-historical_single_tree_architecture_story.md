---
audience: core contributor
---

# Binoc: the architecture, told as a story

**Date:** 2026-06-12
**Status:** Historical

!!! note "Research note — a presentation, not a spec"
    This is a narrative walkthrough of binoc's architecture for skilled
    technologists: the vision, the strategy, the assumed constraints, and then
    every named concept introduced in the order it became necessary, so the
    necessity (or non-necessity) of each can be challenged. It is analysis, not
    normative documentation — where it and the code disagree, the code wins.
    Sections that describe the old single-tree comparator/transformer engine
    are historical context only; the current implementation is the
    correspondence-first engine.
    The constraint labels (`C1`…), moves (`Move 0`…), and claims (`Claim 1`…)
    are defined inline below. Inline `(ADR: name)` tags point to entries in the
    [ADR index](README.md); for the maintained prose see the
    [architecture overview](../plugin-developers/explanation/architecture.md),
    [correspondence-first engine ADR](2026-06-12-correspondence_first_engine.md),
    and the [engine-overhaul retrospective](2026-06-15-engine_overhaul_retrospective.md).
    Sources: the codebase and the ADR decision record, as of June 2026.

---

## Part I — The vision

**Binoc produces semantically compact descriptions of the difference between
two datasets.**

"Semantically compact" carries two meanings at once:

1. **Minimum description length.** "Two columns were swapped in the zipped CSV
   inside the tar" is shorter than 10,000 diff hunks, and it is *lossless* in
   the sense a human cares about: you can reconstruct what happened.
2. **Description in human process terms.** The shortest description is often a
   hypothesis about the *process* that produced the change — "someone did a
   find-replace from sneakers to shoes" compresses 10,000 line edits into one
   sentence. Compactness ends up meaning *inference*.

This is not deterministically solvable in general — it's the same shape as
compiler optimization. You can't write one algorithm that finds the minimal
description of an arbitrary diff. But you *can* do well with a collection of
composable rewrite passes, each of which knows one trick: how to open a zip,
how to detect a column reorder, how to recognize a rename-plus-edit. The
description shrinks one semantic step at a time.

**The success criterion for the architecture is therefore not "does it diff
CSVs" but "do the tricks compose."** A new input format, a new pattern
detector, a new output style — each should be one new component that
automatically works with all existing components, so the ecosystem improves
datasets-in-general rather than one dataset at a time.

## Part II — The strategy

The strategy is the classic compiler move, stated in the docs as the
**m × n × o → m + n + o** claim:

- A dataset world has *m* formats → write *m* **comparators** (front-ends:
  parse bytes into a common representation).
- There are *n* cross-cutting patterns worth detecting → write *n*
  **transformers** (optimization passes: rewrite the representation to a
  shorter equivalent).
- There are *o* opinions about what matters → write *o* **renderer configs**
  (back-ends: map facts to judgments and prose).

The load-bearing bet is the thing in the middle: a single **intermediate
representation** — a tree of `DiffNode`s with open vocabularies, plus a typed
side-channel (**artifacts**) for bulk data — that is:

- rich enough to carry every format's changes,
- neutral enough that passes written for one format help every format,
- stable enough to serialize, version, and pass across a plugin ABI.

Everything else in the architecture — and there is a lot of "everything
else" — exists to try to win that bet under real-world constraints.

## Part III — Assumed constraints, arbitrary decisions, goals

These are the key assumptions or decisions, each traceable to an ADR. If any
of these constraints can be weakened or any decision improved, the
architecture may simplify.

**Audience & ecosystem**

- **C1. Users live in Python; the engine must be fast.**
  - Assumption: Archivists and data scientists will be most comfortable with python's package ecosystem; per-file dispatch must run at native speed.
  - Decision: Bilingual split: Python owns *discovery* (entry points), Rust owns *execution* (harness, sdk, native plugin calling).
  - Benefits: Python's namespacing and auto-discovery along with uvx turns out very ergonomic for multiple namespace problems.
  - ADR: plugin_discovery
- **C2. Rust has no stable ABI.** Separately compiled native plugins cannot
  share Rust trait objects. → C ABI entry points + JSON wire types + SDK
  version checking. (ADR: plugin_sdk_and_abi)
- **C3. Plugins are trusted code.** They run in-process with host privileges.
  Sandboxing untrusted plugins is explicitly out of scope; the threat model is
  pathological *inputs*, not malicious plugins.
  (ADR: security_posture_and_auditing)
- **C4. Minimal default install.** Stdlib is bundled with every user; every
  dependency is compile-time and supply-chain cost. → stdlib boundary policy
  (containers, fallbacks, CSV in; SQLite, Stata out), optional first-party
  plugins, `binoc[all]`. (ADRs: stdlib_boundary, optional_first_party_plugins)
- **C5. Forward compatibility for an ecosystem that doesn't exist yet.**
  Third-party plugins must keep working as the SDK grows. →
  `#[non_exhaustive]` enums, versioned artifact formats, independent release
  tags, SDK-keyed compatibility. (ADRs: plugin_sdk_and_abi,
  independent_release_tags)

**Operational model**

- **C6. Composable, local batch tool, two snapshots on disk.** Not a service, not
  incremental, not n-way. Session-scoped state is acceptable; nothing persists
  between runs except the changeset JSON.
- **C7. The changeset JSON is a stable contract.** Pipeline integrators
  consume it programmatically. → Hard line between *durable* fields and
  *transient* session fields stripped at the output boundary.
  (ADR: transient_fields_on_wire)
- **C8. Extract must work from a saved changeset, without re-diffing.**
  "Give me the rows that were added" days later, from JSON + the two
  snapshots. → provenance fields + the reopen chain.
  (ADR: provenance_and_extract)
- **C9. Output must be deterministic and snapshot-testable.** Gold-file
  testing (insta) of full changeset + changelog per test vector.
  (ADR: snapshot_testing_for_test_vectors)

**Scale & safety**

- **C10. Inputs can be large and hostile.** Decompression bombs, million-row
  tables, pathological archives. → bounds everywhere: gzip decompression cap,
  rename-detection cap (400), diagnostics cap (16), detail-byte render budget
  (200 KB), bounded examples with `truncated` flags.
- **C11. Measured bottleneck is I/O + hashing + content comparison, not IR
  allocation.** Arena allocation, Arrow internals, and lazy traversal were
  considered and explicitly rejected as premature.
  (ADR: deferred_optimizations)

**Design discipline**

- **C12. The controller is type-ignorant.** Zero format knowledge in core; the
  stdlib is just a plugin pack with no special status. This is the rule that
  makes "plugin-first" true rather than aspirational.
- **C13. Open-world vocabularies.** `action`, `item_type`, `tags`, detail-block
  `kind`, annotation keys are open strings, namespaced by package. A genomics
  plugin emits `action: "gap-shift"` without touching core.
- **C14. Facts in the IR, judgments in renderer config.** Whether a column
  reorder "matters" depends on who's asking; the IR records the reorder, the
  renderer config maps it to a heading. (ADRs: renderer_config,
  renderer_groups)

## Part IV — The story: each concept, in order of necessity

The format of each move: **problem → invention → what's now possible → what it
cost (the width it added).**

### Move 0: name the things being compared

Two snapshots are two trees of bytes. The first invention is *not* "path":

- **`ItemRef`** — `{ logical_path, is_dir, handle, content_hash?, size?,
  media_type? }`. The `handle` is opaque and only meaningful to a
  `DataAccess`; the `logical_path` is identity *within the comparison*
  ("archive.zip/data/records.csv"). They differ because items may be
  synthesized (decompressed gzip output in a temp workspace) rather than real
  files. Hash/size/media-type are **opportunistic hints** — populated when
  cheap, resolvable on demand (ADR: opportunistic_itemref_metadata).
- **`ItemPair`** — `{ left: Option<ItemRef>, right: Option<ItemRef> }`. The
  unit of all comparison work. `None` on one side means add/remove.
- **`DataAccess`** — the only door to bytes: `read_bytes`, `open_read`
  (streaming), `local_path` (for tools like SQLite that need a real file),
  `provide`/`register_local` (stage synthesized children), `workspace`
  (scratch dir), plus artifact storage (Move 4). Path-confined to the two
  snapshot trees + session workspace for safety (C10), and it's the
  abstraction that lets plugin I/O cross the C ABI (C2).

*Now possible:* anything that can present `ItemRef`s can be diffed; bytes can
come from disk, from inside an archive, or from a decompression stream,
uniformly. *Cost:* a three-field metadata-hint contract everyone must honor
("may trust presence, must not assume presence").

### Move 1: invent the output language (the IR)

- **`Changeset`** — `{ from_snapshot, to_snapshot, root: Option<DiffNode>,
  metadata, diagnostics }`.
- **`DiffNode`** — the IR node. Core durable fields:
  `action` (open string: add/remove/modify/move/reorder/identical/…),
  `item_type` (open display label), `path`, `source_path` (for moves),
  `summary`, `tags` (open set), `children`, `details` (plugin-private JSON),
  `detail_blocks`, `annotations`, provenance (`comparator`,
  `transformed_by`).

The tree mirrors the recursive structure of the snapshots (dataset →
directory → archive → file → table). Open strings everywhere are the C13
choice: composition by convention, not by central enum.

*Now possible:* one serializable answer format for every plugin and every
renderer. *Cost:* nothing is type-checked across plugins; "compose" means
"agree on strings."

### Move 2: who produces IR? Comparators, and the recursion engine

- **`Comparator`** — the only plugin kind with raw data access. One method
  matters: `compare(pair, data) -> CompareResult`.
- **`CompareResult`** — the four-way result that *is* the engine:
  - `Identical` — no node.
  - `Leaf(DiffNode)` — terminal judgment ("these CSVs differ thus").
  - `Expand(DiffNode, Vec<ItemPair>)` — "this is a container; here are the
    child pairs." The controller recurses (in parallel, rayon) on the
    children, dispatching each pair afresh.
  - `Skip` — "not mine after all"; controller tries the next comparator.
    Exists so dispatch can be cheap-optimistic without a pre-check round-trip
    over an ABI (C2).
- **Dispatch** (three stages, first claim wins, config order = priority):
  item scope (file/container) → extension match → magic-byte/MIME
  `media_type` match (because extensions lie — ADR: media_type_detection) →
  call it, accept `Skip`.
- **Hash short-circuit:** expanding comparators pre-hash children (BLAKE3);
  if a pair's hashes match and no matching comparator sets
  `handles_identical`, the controller emits `identical` without calling
  anyone. The single biggest performance lever in the system.

*Now possible:* **container composition for free.** `Expand` + re-dispatch
means zip-inside-tar-inside-directory works without anyone writing
"zip-inside-tar." Gzip is just an expanding comparator with one child whose
name has the `.gz` stripped, and `.csv.gz` flows on to the CSV comparator
(ADR: single_stream_gzip). This is the first composition claim that
demonstrably cashes out. *Cost:* archives are fully extracted to temp dirs;
`Skip` makes dispatch ordering semantics part of the config surface.

### Move 3: compaction needs cross-node patterns → Transformers

A renamed file is an `add` + a `remove` in distant subtrees. No comparator can
see both. So:

- **`Transformer`** — `transform(node, data, config) -> TransformResult
  { Unchanged | Replace(node) | ReplaceMany(nodes) | Remove }`. Rewrite
  passes over the IR, run in config order, each pass once, bottom-up
  (children before parents).
- **Forced consequence — the full tree:** move detection structurally
  requires seeing *unchanged* files (a renamed unchanged file is otherwise an
  unrelated add+remove). So the tree keeps `identical` nodes through the
  transformer phase and a final **`prune_identical`** pass removes them
  before output (ADR: full_comparison_tree). Content hashes propagate to all
  nodes for this reason.
- **Forced consequence — tree-wide passes:** correlation detectors need the
  whole tree at once, not a node at a time. Rather than a scope system (tried,
  removed — it was silently data-destructive), there is exactly one special
  case: `NodeShapeFilter::Root` matches the root once and the transformer
  walks the tree itself (ADR: transformer_scope_yagni).

Stdlib examples: `CorrelationDetector` (exact-hash move/copy regrouping),
`FuzzyCorrelationDetector` (Jaccard token similarity for rename+modify,
threshold 0.5, cap 400), `FolderMoveDetector` (roll N file-moves up into one
folder-move when ≥80% of a directory moved), `ColumnReorderDetector`
(`modify` → `reorder`, "content unchanged"), `TableSplitter` (one CSV with
stacked tables → children).

*Now possible:* the actual *compaction*: N nodes → 1 node with a better
story. Each detector is one trick, and tricks stack. *Cost:* pass-ordering is
now semantics (declared correspondence must run before heuristic correlation;
fuzzy after exact); transformers run once each, no fixpoint.

### Move 4: transformers need data without re-parsing → Artifacts

A column-reorder detector must verify content equivalence — it needs the
*parsed table*, not the node summary. Re-parsing means every analyzer embeds
every parser, which destroys m + n. First attempt: an ephemeral cross-phase
cache (ADR: cross_phase_data_cache — superseded). The general mechanism:

- **`ArtifactFormat`** — `(package, name, version)`, e.g.
  `binoc.tabular.v1`. Versioned, documented, stable if public.
- **`ArtifactDescriptor`** — `{ format, subject: Left|Right|Pair, producer,
  handle }`, attached to nodes; bytes live in the session `data_root`,
  published/fetched through `DataAccess` (so artifacts also cross the ABI).
- **Standard formats so far:** `tabular_v1` (`{ headers, rows }` — fully
  materialized strings) and `tabular_collection_v1` (manifest of logical
  tables with identities, source locators, shapes — so Excel sheets, SQLite
  tables, and stacked CSV regions all present as "a set of named tables";
  ADR: tabular_collection_artifact_model).
- **The thin-comparator pattern** (ADR: transformer_composition): the CSV
  comparator *doesn't analyze anything*. It parses, checks identity, publishes
  `tabular_v1`, and emits a bare node. `TabularAnalyzer` — a transformer —
  does all row/column/cell semantics by reading artifacts. Refinement
  transformers pattern-match on the tags the baseline pass set.

*Now possible:* **the central composition claim.** The SQLite plugin (~one
comparator) publishes `tabular_collection_v1` + per-table `tabular_v1` and
inherits the entire tabular analysis stack — keyed row diffs, column
detection, reorder collapsing, stats annotation — for free. Same for Stata/SAS.
This is the m + n machine working as designed. *Cost:* artifacts are eagerly
materialized whole (headers + `Vec<Vec<String>>` serialized to JSON bytes);
a performance ceiling hiding in plain sight, accepted under C11 until
measured otherwise.

### Move 5: transformer dispatch must be open too

Hardcoding `match_types: ["directory", "zip_archive"]` misses every future
container. So dispatch keys moved from display labels to structure
(ADR: transformer_dispatch_refinement):

- **`TransformerDescriptor`** — `match_artifacts` (formats), `match_tags`,
  `match_actions`, `match_types`, `node_shape` (Any/Container/Leaf/Root).
  Semantics: **AND across fields, OR within a field, empty = unconstrained.**

*Now possible:* "I analyze anything carrying a `tabular_v1` artifact" — a
transformer that automatically applies to formats invented after it shipped.
*Cost:* a small declarative matching language that every contributor must
learn, and tags-as-dispatch-keys means tags are now API.

### Move 6: transformers find pairs but mustn't parse → recompare

Fuzzy rename detection pairs an added file with a removed file. Now what's
*inside* the pair? The transformer must not parse formats (that's the
comparator layer). So:

- **`pending_recompare: Option<ItemPair>`** on a node — the transformer says
  "I assert these correspond; someone diff their contents." The controller's
  `inflate_pending_recompares` phase re-dispatches the pair through the full
  comparator pipeline and **merges** the result into the wrapper node (union
  tags, carry artifacts, stash the comparator's summary as a
  `content_summary` annotation; an identical recompare downgrades a plain
  modify to pruneable). (ADRs: transformer_initiated_recompare,
  rename_modify_detection)

*Now possible:* "renamed *and* edited" — reported as one move node containing
a real content diff; declared file correspondences from user config get full
semantic diffs. The pipeline is now a (single-bounce) loop:
comparators → transformers → comparators. *Cost:* re-parse of every
recompared pair; bespoke merge semantics that exist only in controller code.

### Move 7: humans need prose → Renderers, and the facts/judgments wall

- **`Renderer`** — `render(&[Changeset], config) -> String`. Markdown in
  stdlib; HTML as a Python reference plugin; JSON is the changeset itself.
- **Significance is renderer config, not IR** (C14): the IR says
  `tags: ["binoc.column-reorder"]`; the user's `output.markdown.groups` —
  an ordered list of `{ heading, tags }` — decides whether that lands under
  "Housekeeping" or "Schema changes." First match wins; unmatched →
  "Other Changes." Zero code per new opinion: that's the *o* in m + n + o.
  (ADRs: renderer_config, renderer_groups)
- Then three inventions to stop renderers from parsing prose, each closing a
  real hole:
  - **`Summary` = `Vec<Segment>`** where `Segment ∈ { Text, Path{side},
    Uint, Float }` — because renderers were regex-ing "Moved from X" out of
    free text and guessing which digits were counts vs years
    (ADR: structured_summary_segments).
  - **`DetailBlock` / `DetailExample` / `ValuePreview`** — structured
    evidence: namespaced `kind`, `total_count`, bounded examples with
    locators and before/after previews, `truncated` flag, `extract` hints
    pointing at exhaustive retrieval. Renderer **verbosity**
    (summary/examples/full) and byte budgets decide how much to show;
    capture caps and render caps are separate (ADR: example_verbosity).
  - **`Annotation`** — `{ package, key, value: JSON }` — namespaced,
    progressively-typed renderer hints from transformers
    (ADR: progressive_renderer_annotations).

*Now possible:* one IR → terse summary, examples-rich changelog, or full dump;
per-team significance with no code. *Cost:* a `DiffNode` now has **five**
sibling channels for "saying something downstream" (summary, tags, details,
detail_blocks, annotations) — see Part VI.

### Move 8: "show me the actual data" → Extract and provenance

A changelog that says "1,204 rows added" must be able to produce the rows —
from the saved JSON, days later, without re-diffing (C8).

- **Provenance** on every node: `comparator` (who made it),
  `transformed_by` (who touched it, in order).
- **`reopen(pair, child_path, data)`** on comparators: reconstruct physical
  access one container level down without diffing. The controller walks the
  saved node's ancestor chain — directory → zip → directory → csv — calling
  `reopen` at each step, then asks the *last plugin that touched the node* to
  `extract(node, aspect, data)` (aspects like `rows_added`, `column_order`).

*Now possible:* `binoc extract changeset.json path/to/node rows_added` →
the data, pipeline-ready. *Cost:* `reopen` is a second, parallel contract
comparators must implement correctly, exercised only on the extract path.

### Move 9: things go wrong non-fatally → Diagnostics

Missing keys, duplicate table names, binary fallbacks: users must see these,
but a local identity problem must not hide every other change in the dataset.

- **`Diagnostic`** — `{ severity: Error|Warning|Suggestion, code (namespaced),
  message, location }`. Emitted on nodes, hoisted to the changeset, deduped
  by (code, location), capped at 16. **Even error severity does not abort** —
  errors are reportable findings; CI decides what's fatal from the changeset
  (ADRs: diagnostics_channel, error_diagnostics_are_reportable_findings).

### Move 10: the ecosystem machinery (making C1–C5 true)

- **`binoc-sdk`** — the one published Rust crate: traits, IR, `DataAccess`,
  wire types, `export_plugin!`. Engine internals (`binoc-core`) are not API.
- **Loading, two modes:** same-build plugins = direct trait objects;
  separately-compiled = `cdylib` + C ABI entry points + JSON requests
  (`CompareRequest`/`TransformRequest`/…), SDK version checked at
  registration. Test harness forces stdlib plugins through the JSON wire and
  asserts byte-identical changesets vs direct dispatch (ABI parity as a test
  invariant).
- **Discovery:** Python scans `entry_points(group="binoc.plugins")`; an entry
  point is either a Python `register(registry)` callable or a native library
  path to dlopen. After registration, all dispatch is native.
- **Transient vs wire** (C7): `source_items`, `artifacts`, node
  `diagnostics`, `pending_recompare` serialize *across the ABI* but are
  stripped by `strip_transient()` at the output boundary.
- **Dataset config** (ADR: unified_dataset_config): user-declared semantics
  the data can't reveal — file correspondence rules (regex pairing with
  identity-failure policies), table row keys, parse options — flowing to all
  plugins as `DatasetSemanticsV1`, with the controller still type-ignorant.
- **Test vectors** — ~40 paired snapshot fixtures with manifests and insta
  gold files; `.zip.d`/`.sqlite.d` staging dirs materialized by a shared
  `VectorMaterializer` harness that plugins reuse for their own vectors.
  The vector set is the de-facto spec of what binoc can currently say.

## Part V — The Feynman diagram

### The whole engine in pseudocode

```text
diff(snapshot_a, snapshot_b, config):
  pair = ItemPair(root_a, root_b)

  # PHASE 1: COMPARE — recursive, parallel
  node(pair):
    if hashes_match(pair) and no matching comparator handles_identical:
        return Identical(pair)
    for comp in config.comparators where matches(comp.descriptor, pair):
        #            scope ∧ (extensions ∨) ∧ (media_types ∨), first claim wins
        match comp.compare(pair, data_access):
            Skip               -> continue
            Identical          -> return Identical(pair)
            Leaf(n)            -> return n             # may publish artifacts
            Expand(n, pairs)   -> return n + parallel_map(node, pairs)

  tree = node(pair)                  # FULL tree: identical nodes included

  # PHASE 2: TRANSFORM — each pass once, config order, bottom-up
  for t in config.transformers:
      tree = bottom_up(tree, n =>
          if matches(t.descriptor, n):   # (artifacts ∨) ∧ (tags ∨) ∧ (actions ∨)
              t.transform(n, data_access, config[t]))     # ∧ (types ∨) ∧ shape
      inflate_pending_recompares(tree)   # re-dispatch pairs via PHASE 1, merge

  # PHASE 3: FINALIZE
  prune_identical(tree)
  hoist_diagnostics(tree); dedupe_and_cap(16)
  strip_transient(tree)        # source_items, artifacts, node diags, recompare
  changeset = Changeset(a, b, tree)

  # PHASE 4: RENDER — o opinions, zero code each
  for r in config.renderers: emit r.render([changeset], config[r])
```

### The data plane

```text
            bytes                    IR                       prose
  Snapshots ──────> Comparators ──────────> DiffNode tree ──> Renderers
      ^   DataAccess     │ publish               │  ▲             │
      │                  ▼                       ▼  │ rewrite     ▼
      │              Artifacts ──read──> Transformers        markdown/html
      │            (package.name.vN)        │                changeset.json
      │                                     │ pending_recompare
      └──────────── reopen chain ◄──────────┘        │
                (extract, days later)         back to Comparators (once)
```

### One node, all channels (the width inventory)

| Channel on `DiffNode` | Written by | Read by | Durable? | Exists because |
|---|---|---|---|---|
| `action`, `item_type`, `path`, `source_path` | comparators, transformers | everyone | yes | the core statement |
| `tags` | comparators, transformers | transformer dispatch, renderer groups | yes | open semantic facts; dispatch keys |
| `summary` (Segments) | comparators, transformers | renderers | yes | typed one-liner; no prose re-parsing |
| `details` (free JSON) | the producing plugin | same plugin (extract), humans | yes | plugin-private payload |
| `detail_blocks` | comparators, transformers | renderers (verbosity) | yes | bounded structured evidence + extract hints |
| `annotations` | transformers | renderers | yes | namespaced renderer hints |
| `comparator`, `transformed_by` | controller/plugins | extract chain | yes | provenance (C8) |
| `children` | comparators (Expand), transformers | everyone | yes | the tree |
| `diagnostics` | plugins | changeset (hoisted) | hoisted | non-fatal findings |
| `source_items` | controller | transformers | no | re-access without re-walk |
| `artifacts` | comparators | transformer dispatch + reads | no | parsed-data side channel |
| `pending_recompare` | transformers | controller | no | the loop-back |

Twelve channels. Each has an ADR. Whether each *needs* to be distinct is
exactly the question to put in front of reviewers — see Part VI.

## Part VI — Claims to attack (where to poke)

These are the generalization bets, restated as falsifiable claims. Evidence so
far: ~40 synthetic test vectors, ~6 real datasets (`../binoc-showcase`), 4
model plugins.

**Claim 1: One IR shape fits all formats.**
Stress: the tree's nodes stop at file/table granularity; *within* a table,
changes live in tags + detail_blocks + details, not nodes. So there are really
two change representations: structural (nodes) and intra-leaf (side
channels). A moved file is a node; a moved column is a tag. Is that a
principled line (nodes = things with paths) or an accident of how CSV support
grew? What happens with formats whose interior structure is deep (JSON
documents, XML, nested Parquet)?

**Claim 2: Artifacts decouple parsers from analyzers at acceptable cost.**
Stress: `tabular_v1` is eager, whole-table, stringly
(`Vec<Vec<String>>` → JSON bytes on disk per side). It is also the *only*
proven public artifact format (plus its collection manifest). The
m + n machine has been demonstrated for exactly one value of the
abstraction. Does the model survive a 10 GB table, or a format whose natural
artifact is columnar/lazy? (C11 says don't optimize early — but eager
materialization is an *interface* choice, not an implementation detail, and
interfaces are the things this project promises to keep stable.)

**Claim 3: Open string vocabularies + config ordering compose safely.**
Stress: tags are simultaneously facts, dispatch keys, and renderer group
keys — stringly-typed API with no registry and no collision detection beyond
naming convention. Transformer order is user config, and correctness depends
on it (declared-before-heuristic, exact-before-fuzzy, baseline-before-
refinement). Each pass runs once; there is no fixpoint. The compiler analogy
predicts the phase-ordering problem arrives the moment two independent
plugins' passes interact. What's the plan — priority numbers? dependency
declarations? iterate-to-quiescence?

**Claim 4: One recompare bounce is enough.**
Stress: transformer-initiated recompare is a single dispatch back through
comparators with hand-rolled merge semantics (union tags, annotation-stash the
summary). If a recompared pair's content diff *itself* warrants
transformation (the renamed file is a CSV with a column reorder), do the
tabular transformers see it? Ordering says only transformers later in the
config will. The loop is real but shallow — is that a principle or a TODO?

**Claim 5: The five downstream-message channels are all necessary.**
`summary` vs `tags` vs `details` vs `detail_blocks` vs `annotations`. Each
ADR is locally convincing; jointly, every plugin author faces a five-way
"where do I put this fact?" decision, and renderers must honor all five.
Could `details` (private) + one structured public channel cover it? This is
the strongest "elaborate where simple would do" candidate.

**Claim 6: The performance ceiling is acceptable and not architectural.**
Archives fully extracted to temp dirs; `read_bytes` whole-file reads
dominant in practice; eager artifacts; recompare re-parses; the only
asymptotic win is the hash short-circuit. All fine for snapshot pairs of
a few GB. The ADRs defer optimization (correctly per C11) — but several
ceilings are baked into *contracts* (`CompareResult::Expand` implies
materialized children; `tabular_v1` implies materialized tables), and
contracts are what can't change later. Which ceilings are interface-load-
bearing?

**Claim 7: The architecture is shaped for the actual vision.**
The vision's find-replace example — "someone did a find-replace from sneakers
to shoes" — is a stretch goal, not a committed deliverable: the user need is
unestablished, and it is *process inference across the whole dataset*. Today
nothing performs cross-file or cross-cell pattern induction: detail blocks are
bounded and truncated (the evidence for the pattern is discarded at capture
time), and tree-wide transformers see nodes, not cell-level edits. A
find-replace detector would need either unbounded evidence capture or a new
dataset-scope phase with artifact-level access to *both* sides of every changed
leaf at root scope. The pieces (Root transformers, artifacts, annotations)
plausibly suffice — but no existing pass is of this kind, so the claim is
untested. See
[Searching and verifying edit-list rewrites](../core-developers/research/edit-list-search-and-verification.md)
§6 for the structural constraint. This is the
deepest hole: the architecture has proven it can compose *parsers*; it has
not yet proven it can compose *inferences*.

**Claim 8: Pairwise is enough.**
`diff a b c` runs pairwise A→B, B→C. Real provenance questions ("when did
this column first appear?") are about lineages, and some compactions only
exist across longer windows (a column removed then re-added). Session-scoped
everything (C6) makes multi-snapshot reasoning a future architecture, not a
future feature.

## Part VII — Precedents

The companion prior-art survey — which existing tools are the strongest
argument against building binoc from scratch, and which systems have the most
to teach its architecture — lives in its own research note:
**[Prior art and architecture precedents](../core-developers/research/precedents.md)**.

---

*Companion material: the
[architecture overview](../plugin-developers/explanation/architecture.md)
and the rest of the explanation set (the project's own telling), the
[ADR index](README.md) (the decision record), and `test-vectors/`
(the de-facto spec). This is generated analysis, not project documentation —
poke holes in it too.*
