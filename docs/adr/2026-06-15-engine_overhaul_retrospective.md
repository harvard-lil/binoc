---
audience: core contributor
---

# The Engine Overhaul, Told Whole: Single-Tree to Correspondence-First

**Date:** 2026-06-15
**Status:** Retrospective

!!! note "What this document is"
    Between June 11 and 15, 2026, binoc's engine was rebuilt from the ground up
    and the plugin-boundary policy was reset. That work generated a dozen-plus
    ADRs, a migration tracker, prototype crates, and research notes, written in
    the order ideas arrived — including several that were proposed and then
    unwound. This ADR tells the same story *in the order it makes sense*, so a
    future developer can follow what we built, why each layer holds, and what we
    tried and abandoned. It is the narrative home for the arc; the per-decision
    ADRs it cites remain the detailed record. Where this and the code disagree,
    the code wins.

## The problem

The old engine represented a diff as a **single mutable tree**. Format-specific
*comparators* produced nodes carrying an `action` (`add`/`remove`/`move`/
`modify`); *transformers* rewrote that tree in ordered passes to compact it
(roll leaf moves into a folder move, collapse a column reorder, pair a renamed
file). The full design is preserved in
[Binoc: the architecture, told as a story](2026-06-12-historical_single_tree_architecture_story.md).

That model worked for single files but every hard problem on the roadmap turned
out to be a symptom of one choice — *baking the edit script into the structure*.
When a later pass learned something new about **correspondence** (which left item
is which right item), the tree had to be surgically rewritten:

- **Recompare machinery.** A pairing discovered during the transformer phase had
  to re-enter the comparator phase and have its result spliced back in
  (`pending_recompare`, `inflate_pending_recompares`, bespoke merge semantics).
- **Cross-subtree double-counting.** A file moved *out* of a renamed archive
  produced a move node outside the archive's subtree; re-diffing the archive
  re-manufactured a `remove` for a change already claimed elsewhere.
- **Rollup surgery.** Inferring a container move from leaf evidence meant
  rewriting the tree on path strings, accreting guard after guard because
  nothing structurally prevented an incoherent rollup.
- **Leaf-only pairing.** All three pairing mechanisms only paired childless
  nodes; containers were second-class, and user-declared container
  correspondences were silently ignored.
- **Refinements couldn't compose.** A pass that recognized a *total* pattern
  ("pure column reorder") fell through the moment the change was a conjunction
  ("reorder *and* rows added").

Each had been given its own local patch design (a claims ledger, a scoped
delta-loop, a replace-then-re-derive merge). Laid side by side, all four patches
were hand-built approximations of **the same structure** — the standard
representation in the tree-diff literature: *two trees plus a mapping, with the
edit script derived from the mapping rather than stored as it.*

A second, deeper problem sat underneath (the historical story's "Claim 7"): the
architecture had proven it could compose *parsers* (open a zip, parse a CSV) but
not *inferences*. The vision's flagship — "someone ran a find-replace from
sneakers to shoes" — is cross-dataset process inference, and nothing in the old
model could express or compose that.

## Success criteria

The bar was never "does it diff CSVs." It was:

1. **The tricks compose.** A new format, a new pattern detector, or a new output
   opinion is *one* component that automatically works with all the others
   (the `m * n * o -> m + n + o` bet).
2. **The core stays type-ignorant.** Zero format knowledge in `binoc-core`; the
   stdlib is a plugin pack with no special status.
3. **Open vocabularies, judgments in config.** `action`, `item_type`, `tags`,
   evidence kinds, and edit verbs are open namespaced strings; significance is a
   renderer concern, not an IR level.
4. **Honest partial output beats false precision.** One corrupt member must not
   abort the changelog; a claim is made only when it is exactly true.
5. **The changeset JSON stays a stable contract**, deterministic and
   snapshot-testable, with bounds everywhere against hostile input.
6. **Greenfield.** Pre-1.0, no back-compat effort (AGENTS.md rule 9).

## The architecture, in dependency order

This is the order to *learn* the engine, which is not the order it was built.

### 1. The IR: two side trees plus links

The foundational move. Left and right snapshots each expand into an
**immutable side tree** of `ItemRef`-backed nodes. Because snapshots don't
change, side trees are append-only and every node-keyed computation (hash,
parse, artifact) is pure and memoizable with no invalidation problem.

Correspondence is a separate **many-to-many relation of links** between left and
right nodes. Each link carries the *evidence kind* that produced it (hash,
declared, name-under-paired-parent, fuzzy, child-evidence, copy) and a priority.
**Links are the claims ledger, promoted into the IR.**

`add`/`remove`/`move`/`modify` stop being stored state and become **queries**:
an unlinked left node is a remove; a link whose endpoints have different paths is
a move; a many-to-one copy-evidence link is a copy. The entire bug class "derived
state maintained by tree surgery drifted out of sync" becomes *unrepresentable*.
(ADR: [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md).)

### 2. Pass 1 — saturation (expand, parse, pair)

A worklist driver runs rules to quiescence, dispatching them **solely on their
declared input/output types** — it contains no knowledge of any specific rule.
Three rule families:

- **expand**: a node -> child nodes (zip, tar, gzip, directory).
- **parse**: a node -> artifacts (typed data); fires at most once per node.
- **pair**: evidence -> link.

Two scheduling rules are part of the semantics. The **frontier rule**: pairing
gets first claim on every new node, so a hash-link can settle a node before any
rule works beneath it — this generalizes the old hash short-circuit into one
ordinary pair rule. **First-claim-wins expand**: the first matching expand rule
claims a node even if it yields zero children.

The driver's only built-in policy is **settledness**: a pair rule may flag a
link as content-identical, and no rule fires beneath a settled link unless its
descriptor opts in. The driver does not know what hashing *is*. Pairing priority
is configuration order, not engine code. Evidence declarations are
**fail-closed**: a rule that emits an undeclared evidence kind fails the run,
because undeclared evidence would silently corrupt the claims ledger.

Termination is structural, not budgeted: expansion is bounded by input, parse is
once-per-node, links/annotations are add-only, and the one non-monotone operation
(link revision) only moves to strictly higher priority. **Recompare, inflation,
and merge cease to exist** — there is no derived tree to splice into. A "late"
fuzzy or declared pairing simply adds a link, and rules waiting on a link fire
then.

One subtlety worth its own ADR: whether a parse rule must wait for a link is *not
a local property of the rule* — it depends on which pair rules are configured to
read its output. So the flag was deleted and the fact is **derived** once per run
(`preconsumed_formats`). (ADR:
[Derive Parse-Rule Link Gating from Pair Reads](2026-06-13-derived_requires_link.md).)

### 3. Typed artifacts — the data plane

Rules carry bulk parsed data in versioned **artifacts**, the side channel that
decouples parsers from analyzers.

- **`tabular.v1`** was rewritten greenfield around a typed `Value` enum
  (`Null/Bool/Number/String/Nested`) with a `Schema` (`fields`, `has_header`,
  `key`); `Nested` is equality-only in v1. Shape facts (`is_rectangular`,
  `has_named_columns`) are *derived facts rules self-gate on*, not subtypes.
- **`structured_document.v1`** is a format-neutral value tree (a `format` tag
  distinguishes JSON/YAML/TOML/INI), so consistently-typed JSON/JSONL lists
  project as `tabular.v1` while arbitrary documents still diff.

  (ADR: [Typed Records](2026-06-14-typed_record_tabular_and_structured_document.md).)
- **Parsed children and decompose boundaries.** Multi-table sources (SQLite,
  Excel, SAS `.xpt`) emit child `tabular.v1` *nodes* rather than a manifest. Two
  path separators carry distinct meaning: `/` is membership (an
  already-navigable tree) and `/>` is a decompose boundary (a format binoc had
  to crack open). All path logic is centralized in `binoc_sdk::path`; nothing
  parses paths for behavior. (ADR:
  [Parsed Children and Decompose Boundaries](2026-06-14-parsed_children_and_decompose_boundaries.md).)
- **Tiered metadata.** Rich source-format metadata (variable labels, value-label
  dictionaries, encoding, provenance) is carried in three tiers by key:
  per-column and per-table on `tabular.v1`, and file-level in a separate
  **`parser_metadata.v1`** artifact. (ADR:
  [Tiered Artifact Metadata](2026-06-15-tiered_artifact_metadata.md).)

### 4. Pass 2 — edit lists and compaction

Every link gets an **edit list**: what changed across that correspondence, in
open-vocabulary verbs.

- **Naive writers ship with each artifact format** and emit the dumbest true
  edit list (per-cell edits, child add/remove, "contents differ"). All
  compactness is *earned* by later rules.
- **Composable per-artifact writers.** The artifact — not the link — is the
  rendering unit. A node legitimately carries several artifacts (`tabular.v1`
  *and* `parser_metadata.v1`), so dispatch **composes then selects**: it runs
  one writer per *present* format and concatenates, with structural writers
  (container/text/fallback) marked by a `fallback` flag. Each `Edit` is tagged
  with its producing format so compaction stays format-scoped. (ADR:
  [Composable Per-Artifact Writers](2026-06-15-composable_per_artifact_writers.md).)
- **Compaction rules rewrite edit lists to shorter equivalents** — N consistent
  cell edits -> "columns reordered by σ"; per-link lists across the dataset ->
  one find-replace claim. Acceptance requires **strictly decreasing description
  cost** (verbs + parameter sizes + residual edits), which is both the
  termination measure and the product's minimum-description-length objective made
  operational. Pass 2 runs as an explicit ordered pipeline, not a fixpoint (the
  LLVM lesson: keep judgment ordering curated).

### 5. Correspondence semantics on top of links

With links first-class, the hard correspondence patterns became ordinary rules
instead of tree surgery:

- **Multi-input file-set fusion.** A parse claim can span a *correlated set* of
  sibling nodes (a shapefile's `.shp`/`.shx`/`.dbf`/`.prj`). The rule's own
  parse-or-**decline** is the sole authority on validity; precedence is
  arity-descending. A new store primitive, **`subsume`** (a flag, not a
  deletion — the N->1 dual of `add_child`), folds members in while retaining them
  as provenance. (ADR:
  [Multi-Input Claims](2026-06-15-multi_input_file_sets_and_shapefile_fusion.md).)
- **Split / merge via partition identities.** Row-partition splits
  (`observations.csv` -> per-year files) are now representable. Partition identity
  is a **format-owned capability**, computed just-in-time and never stored: a
  registered `IdentityExtractor` yields opaque, globally-comparable
  `IdentityToken`s, and a generic `disjoint_cover` coverage query lets one
  conservative pair rule claim a split/merge **iff** the cover is complete,
  disjoint, and unambiguous — otherwise it declines with `binoc.possible_split`.
  This is the **first concrete producer** of the reserved `Changeset.claims`
  slot, and it stops the fuzzy pairer from lying about a split as a "move + add."
  (ADR: [Partition Identities](2026-06-15-partition_identities_jit_format_capability.md).)
- **Container reshape + reconciliation.** Linked containers of *differing kind*
  (a directory of CSVs <-> a SQLite file) render as one representation change, not
  move-plus-add/remove, via a single reconciliation pass and a
  `container_reshape_hint` annotator.

### 6. Projection — the contract survives at the boundary

The public changeset tree is **derived at the output boundary** from (side
trees, links, edit lists, annotations), exactly as the old `prune_identical`
projected. So the existing gold files and external consumers are the regression
harness for the migration rather than casualties of it. Projection owns one
product rule: never state the same child change twice. Projection metadata comes
from rule packs as **projection hints** (item types, tags, move/copy judgments,
edit visibility); core overlays them and otherwise treats every string as opaque.
A late addition, `ProjectionHint::retract_tags`, lets a reshape drop a
superseded `binoc.move` tag so JSON tag sets stay coherent. The
[default Markdown policy](2026-06-14-default_markdown_changelog_policy.md) reads
the projection as a *changelog* (no `Claims: none`, no zero-edit bookkeeping
containers), not an IR dump.

### 7. Diagnostics and cross-cutting discipline

- **Structured `Summary` segments.** `summary` and diagnostic messages are typed
  `Vec<Segment>` (`Text`/`Path`/`Uint`/`Float`), so renderers never reverse-parse
  prose to recover counts vs. years or re-match their own output. (ADR:
  [Structured Summary Segments](2026-06-03-structured-summary-segments.md).)
- **Declared write-sets.** A rule descriptor declares what it *emits*, checked in
  the test harness — never used for scheduling (declarations that drive a solver
  rot; verifier-checked ones stay honest). (ADR:
  [Declared Write-Sets](2026-06-11-declared_write_sets_on_transformer_descriptor.md).)
- **Invariant and lint tiers.** Three tiers: harness invariants (hard-fail every
  vector), mechanical lints in `binoc_sdk::lints` (including the tripwire that
  `binoc-core` never reads write-sets), and agent-level judgment checks. (ADR:
  [Invariant and Lint Tiers](2026-06-12-invariant_and_lint_tiers.md).)
- **Measured performance.** Only guarded *parallel parse* was landed (~23% on a
  CPU-heavy fixture); pair scheduling and parallel expand were parked as
  unmeasured. (ADR: [CFM-44](2026-06-13-cfm_44_measured_correspondence_performance.md).)

## Layers tried and abandoned

Placed where they would have fit in the story above — several were live ideas
that the new structure dissolved:

- **Cross-phase data cache** (at layer 3). An ephemeral cache to avoid
  re-parsing was built against the *mutating* old tree, where it fought
  invalidation. Abandoned, then revived soundly as side-tree memoization — the
  same goal, made trivial by immutability.
- **Full comparison tree + `prune_identical`** (at layer 6). The old engine kept
  `identical` nodes through the transformer phase purely so move detection could
  see unchanged files. Subsumed: side trees naturally contain identical nodes,
  and pruning is just a projection step.
- **Transformer scope system** (at layer 2). A general per-rule scope mechanism
  was tried and removed for being silently data-destructive; the old engine fell
  back to a single `Root` special case. The worklist driver makes the whole
  question moot — tree-wide work is just a rule that reads the relation.
- **`pending_recompare` / the recompare bounce** (at layer 2). The
  transformer->comparator->transformer loop-back, with hand-rolled merge
  semantics, was the single most complex piece of controller code. **Deleted
  outright** — late links replace it. (Supersedes
  [Transformer-Initiated Recompare](2026-06-03-transformer_initiated_recompare.md).)
- **The single-tree patch designs** (at layer 1). The claims-ledger-by-hand and
  scoped-delta-loop patches were fully designed and reviewed, and kept as the
  *fallback* if the prototype failed the structural-projection gate. It passed;
  they were never needed.
- **`tabular_collection_v1` + collection writers** (at layer 3). A manifest
  artifact listing logical tables, with bespoke writers. Deleted in favor of
  child table nodes — which removed a latent double-count for free.
- **The stacked-table writer stopgap** (at layer 4). A temporary heuristic that
  detected stacked-table regions *inside a writer*, used to preserve behavior
  during the migration. Retired; section detection belongs in parse rules
  (`binoc.parse.csv_stacked_tables`).
- **`ColumnReorderDetector`'s tag-handoff** (at layer 5). A second pass re-read a
  tag and re-scanned the cells to confirm a fact the analyzer already had — "a
  function call drawn slowly," plus a `tags.clear()` bug. Folded into the
  analyzer; the tag survives as a *fact* renderers dispatch on. (ADR:
  [Inline Pure-Reorder Judgment](2026-06-11-inline_pure_reorder_judgment.md).)
- **The `humanize_numbers` identifier-guard** (at layer 7). A rider that tried to
  stop a prose-scanning renderer from mangling year-bearing filenames. Removed
  once diagnostics became typed `Summary` segments and the prose scan was gone.
- **One-writer-per-link dispatch** (at layer 4). Generalized — not deleted — into
  composable per-artifact writers, of which it is the single-artifact degenerate
  case.
- **e-graph full equality saturation, and one scheduler for everything**
  (at design time). Rejected up front: nodes are fat and do real I/O, so
  saturation is prohibitive, and a declared-dependency scheduler over
  judgment-laden passes is the LLVM legacy-pass-manager mistake. The adopted
  hybrid keeps a fixpoint driver only for Pass 1's monotone dataflow.

## The parallel thread: a new plugin-boundary stance

The migration dissolved the comparator/transformer taxonomy the old plugin ABI
was shaped around ([Plugin SDK and ABI](2026-03-12-plugin_sdk_and_abi.md)), whose
core assumption was **freeze one C ABI for all plugin families up front**. With
that broken anyway, the stance changed to **"start in process with a clear
boundary, standardize slowly"**:

- **The seam is mandatory everywhere.** Every plugin-facing type stays
  serializable data with no host internals, so *any* extension point can later
  move behind a process boundary. This is nearly free and preserves the
  extraction option.
- **Transit machinery exists only at the stable tier.** C ABI entry points and
  wire payloads are built per family, *only after* that family's trait shape and
  vocabulary have stopped moving — the graduation signal being two release
  windows with no change. Everything else is the **in-process proposed tier**:
  trait objects, free to churn.
- **Skew is lockstep during 0.x.** No tolerance ranges, no grandfather
  exemptions; breaking changes fix all in-tree plugins atomically. In-tree
  plugins are *first-party SDK consumers* for now, not proof of the arm's-length
  boundary — a deliberate, time-boxed loosening of the old "build the boundary
  everywhere" rule. (ADR:
  [Tiered Plugin Surface](2026-06-12-tiered_plugin_surface_pre_1_0.md).)

Applying that honestly, **only renderers graduated** to the stable C-ABI tier
(they read the public projected IR and write only strings). Expand/parse reset
their clock when parsed-children landed; pair rules are blocked on a transit
shape for the chatty `EngineView`. (ADR:
[Stable ABI Tier for CFM-27b](2026-06-13-stable_abi_tier_cfm_27b.md).) Heavy or
domain-specific readers (Stata/SAS) live as
[optional first-party plugins](2026-06-01-optional_first_party_plugins.md) using
the same surface as third-party packs.

## Outcome

- The correspondence-first engine is the **only** live diff architecture. The
  legacy comparator/transformer traits, ABI entry points, Python bridge classes,
  stdlib comparison/rewrite packs, prototype crates, and legacy-named changeset
  fields were all removed. (ADR:
  [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md).)
- The arity matrix — the full space of structural and correspondence changes — is
  populated and proven by test vectors:

  | | within-snapshot (structure) | across-snapshot (correspondence) |
  |---|---|---|
  | **1 -> N** | expand / parsed children | split |
  | **N -> 1** | file-set fusion / `subsume` | merge · reshape reconciliation |
  | **1 -> 1** | parse leaf | move / modify |

- The engine crates carry **zero** `TODO`/`FIXME`/`unimplemented!` markers, and
  the pre-merge gaps are closed.
- **Performance** regressed at the cutover (debug baseline ~2.6× the old engine),
  accepted in exchange for correctness and simplicity; parallel parse clawed back
  a measured slice. The remaining levers (dirty-set scheduling, parallel expand)
  are parked until a profile justifies them.
- On the historical story's "claims to attack": composition of parsers,
  first-class container pairing, the disappearance of recompare and cross-subtree
  double-counting, and composable refinements are all **resolved**. The deepest
  one — composing *inferences* — is **partially** resolved: the
  split/merge claim is the first real inference producer and the cost-ratchet
  gives later ones a principled home, but dataset-wide find/replace is still
  ahead.

## Future work and open questions

**Known rough edges (acceptable to defer):**

- **Reshape child-orphan.** A parse/pair round-ordering race: fuzzy pairing can
  link a still-unparsed container before parse decomposes it, orphaning a child.
  The honest fix (force parse-before-fuzzy, or add backtracking to pair dispatch)
  is non-local and explicitly rejected as over-engineering for now (see the Known
  Limitation in [Derived Requires-Link](2026-06-13-derived_requires_link.md)).
- Multi-input dispatch tidies (invalid-anchor decline channel; per-rule
  boilerplate); identity-token per-run caching (not a measured bottleneck);
  composed-edit raw ordering (renderer owns reader-facing order).

**Feature arcs (the rewrite-rule iteration phase):**

- **New claim producers** — find/replace, numeric unit conversion,
  precision-rounding, near-duplicate row hints — built on the strict-cost ratchet,
  generalizing the `binoc.tabular_split`/`_merge` claim shape.
- **More formats** — FASTQ, VCF, TIFF/image equality, text-section split. The
  partition machinery is format-generic: a new format registers an
  `IdentityExtractor` and gets split/merge for free.
- **ABI graduations** — pair-rule ABI (blocked on the `EngineView` transit shape),
  expand/parse once their shapes settle; Arrow-backed `tabular.v2` if
  interchange justifies it.
- **Beyond pairwise** — N-snapshot / k-tree lineages, a true graph renderer
  (a renderer option, not an engine change), dirty-set frontier scheduling.

**Still-open questions from the historical story:** whether the several
downstream message channels can be collapsed (Claim 5), and whether pairwise diff
is enough for real provenance questions about lineages (Claim 8).

## Consolidation guidance

This ADR is intended to be the readable entry point to the arc. With it in place:

- The migration **tracker** (`CORRESPONDENCE_ENGINE_TRACKER.md`) and the
  prototype crates have served their purpose; their durable content is here, in
  the cited detail ADRs, and in git history.
- The [historical single-tree story](2026-06-12-historical_single_tree_architecture_story.md)
  remains valuable as the full "before" picture and the source of the claims this
  arc answers; keep it, referenced from here.
- The detail ADRs cited above are the normative record for each layer and should
  be kept; this document is the index and narrative over them, not a replacement.

## Alternatives Considered

The major alternatives — keeping the single tree with the patch designs,
top-down container pairing as another rewrite pass, full e-graph saturation, and
one scheduler for judgment too — are recorded in the
[Correspondence-First Engine ADR](2026-06-12-correspondence_first_engine.md). The
plugin-policy alternatives (rebuild full ABI equivalence up front; sequence all
deletion after all ABI work; blessed in-tree plugins only) are in the
[Tiered Plugin Surface ADR](2026-06-12-tiered_plugin_surface_pre_1_0.md).
