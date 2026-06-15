# Correspondence Engine Tracker

Lineage: `CORRLINK_MIGRATION_TRACKER.md` → `CORRESPONDENCE_ENGINE_TRACKER.md`.
The single-tree → correspondence-first migration and its cleanup/fuzz arc are
complete. **As of this update the numbered engine-overhaul work is done** — see
"End-of-arc status" below. This is now a **compact status board**: what landed,
and the buckets of remaining work (pre-merge decisions, post-merge punchlist,
future feature arcs). The long-form arc planning this supersedes lives in git
history and the fuzz report `binoc-fuzz-vectors.KujG6V/REPORT.md`.

Principles (unchanged): core stays type-ignorant; format knowledge lives in
stdlib/plugin rules; honest partial output beats false precision; greenfield, no
back-compat effort (AGENTS.md rule 9). Every change keeps
`just fmt && just check && just test` green and names the vector/test that proves
it.

## End-of-arc status (working state — for a fresh session)

- **Branch `pre-refactor`, all green** (`just check` + `just test`). Last shepherd
  run landed **CFM-84** (diagnostics → structured `Summary`). The arity matrix is
  now fully populated and the "the node was a proxy" correction has landed across
  input/content/output:

  | | within-snapshot (structure) | across-snapshot (correspondence) |
  |---|---|---|
  | **1 → N** | expand / parsed children (CFM-69/70) | split (CFM-72) |
  | **N → 1** | multi-input claims / `subsume` (CFM-83) | merge (CFM-72) · reconciliation (CFM-71) |
  | **1 → 1** | parse leaf | move / modify |

  input = the *claim* is the parse unit (CFM-83); content = a node carries N
  artifacts (CFM-80); output = the *artifact* is the render unit (CFM-81).
- **Verdict:** the core engine is at the end of its overhaul, and **all
  pre-merge gaps are now closed** (`retract_tags` landed; equal-arity coverage
  added; the CFM-83(e) stem-collision bug fixed). The engine crates
  (`binoc-core`/`binoc-sdk`/`binoc-stdlib`) carry **zero** `TODO`/`FIXME`/
  `unimplemented!` markers. Everything still open is either (a) a known-and-
  acceptable punchlist, or (b) net-new feature arcs that belong to the
  rewrite-rule iteration phase — not engine plumbing. The buckets are below.
- The working tree has untracked files from a separate trace-visualizer effort
  (`docs/users/explanation/replays*`, `scripts/build_replays.py`) — leave them.
- **Shepherding workflow (full autopilot on git):** hand each item to a subagent
  in an isolated worktree, as parallel as feasible; merge the branch into
  `pre-refactor`; run `just fmt && just check && just test`; regenerate gold via
  `INSTA_UPDATE=always cargo test -p <crate> --test test_vectors`; update this
  tracker; delete the merged worktree + branch.
- **Worktree gotcha:** agent worktrees may be created from `main` (an outdated
  layout). Every worktree agent must `git reset --hard pre-refactor` *first* and
  confirm `git log` shows recent CFM commits before working.
- **Concurrent sessions** share this checkout — stage by explicit path
  (`git add <file>`), never `git add -A`.

## Landed

- **Migration foundation.** CFM-42 parsimony instrumentation · CFM-60 public IR
  provenance (`sources`/`claims`) · CFM-43 declared container correspondences ·
  CFM-61 per-node rule-failure containment · CFM-44 measured performance pass ·
  CFM-27b stable ABI tier (renderer graduated; expand/parse/pair not yet) ·
  CFM-62 parsed-content tabular pairing with derived `requires_link`.
- **Render revisit + cheap facts.** CFM-64 JSON-stdout truncation fix · CFM-65
  text encoding/BOM/line-ending/whitespace facts · CFM-66 Markdown surfaces
  row/cell/text detail (keyed `rows modified by key`) · CFM-79 default Markdown
  reads as a changelog, not an IR dump (provenance opt-in, no `Claims: none`, no
  zero-edit container rows).
- **Typed artifacts.** CFM-67/68 — `tabular_v1` plus a format-neutral
  `structured_document_v1`; JSON/YAML/TOML/INI parse rules; canonical-JSON
  serialization-change detection; JSON record arrays project as `tabular_v1` when
  rectangular. (ADR: typed records.)
- **Parsed children + collection removal.** CFM-69 one parsed-child contract,
  `/` membership vs `/>` decompose separators (centralized in `binoc_sdk::path`;
  nothing parses paths for behavior), `tabular_collection_v1` and its writers
  deleted · CFM-70 SQLite/Excel/SAS-`.xpt` emit `tabular_v1` child nodes,
  single-table sources stay leaves; parent/child no-duplication invariant.
  Container nodes carry a named `item_type` set via `ParseOutput.projection`;
  `item_type` stays render-facing only (writer/shape dispatch keys off actual
  children, not the string). (ADR: parsed children and decompose boundaries.)
- **CFM-80 Tiered artifact metadata — channels only.** Per-column + per-table
  metadata on `tabular_v1`; new `parser_metadata_v1` artifact and
  `ParseOutput.artifacts`; stat-binary restores its dropped labels/formats/
  value-labels/version facts into the three tiers. (ADR: tiered artifact metadata.)
- **CFM-81 composable per-artifact writer dispatch.** The artifact is the
  rendering unit: per-link dispatch runs one writer per *present* artifact format
  and concatenates, composing with structural writers (marked by a `fallback`
  descriptor flag). `Edit.provenance` tags each edit; compaction is format-scoped;
  `writer_used`/`extract`/trace/`LinkDescriptionCost` migrated to per-link writer
  **sets**. (ADR: composable per-artifact writers.)
- **CFM-78 bounded diagnostics.** `bounded_index_list` caps inline index/key
  lists; `binoc.table_splitter.ambiguous` no longer fires on flat single-table
  CSVs (gated on a detected banner/title).
- **CFM-82 metadata rendering + significance.** `ParserMetadataWriter` and
  tabular column/table metadata diffing emit `metadata.value_change`, composed
  per-artifact (CFM-81); significance mapped from scope/semantic tags via
  renderer-group config, not the IR. Proven by `stata-metadata-change` and
  `xpt-dataset-label-change`. *Punchlist:* composed-edit order is
  artifact-format-sorted then structural-last; correct reader order comes from the
  renderer choosing block order — revisit only if a consumer relies on raw
  composed order. (ADR: tiered artifact metadata.)
- **Path-boundary escaping.** A logical-path segment literally starting with `>`
  (or `\`) is escaped with a leading backslash by `binoc_sdk::path`
  (`escape_segment`). Resolves the previously-parked `/>` ambiguity; documented in
  the parsed-children ADR.
- **CFM-83 multi-input claims (file-set fusion; `subsume`).** A parse claim can
  span a correlated set of nodes (extra members + correlation on the `ParseRule`
  trait; defaults + `From<NodeMatch>` keep single-input ergonomic). Default
  correlation = same-parent first-dot lowercased stem; arity-descending precedence.
  New store primitive **`subsume`** (`subsumed_by` flag, not deletion): subsumed
  members are excluded from dispatch/projection but retained as provenance.
  Shapefile fusing parser fuses `{.shp,.shx,.dbf,.prj,.cpg}` into one "Shapefile
  layer" node; declines cleanly. Proven by `shapefile-fusion-roads` /
  `shapefile-fusion-decline`. (ADR: multi-input claims.)
  *Punchlist follow-ups:* (a) decline memoization is now rule-scoped
  (node+format+rule) rather than format-scoped — a core-dispatch invariant change
  to keep in mind; (b) an invalid anchor declines via the *error* channel, not the
  clean empty-output decline channel; (c) `parse`/`parse_group` boilerplate on
  every fusing rule; (d) **RESOLVED** — equal-arity vectors added (see the
  equal-arity / stem-collision entry below); (e) **RESOLVED** — `roads.v2.shp` vs
  `roads.shp` stem collision was a silent member-misattribution bug, now fixed by
  final-extension-only `stem_key` (no capture/template machinery needed).
- **CFM-71 container reshape + parent reconciliation.** `merge_projected_collision`
  generalized into one reconciliation pass in `project.rs`; linked containers of
  *differing kind* render as a representation change, not move+add/remove. A
  `container_reshape_hint` annotator reads both endpoints' `item_type` (core passes
  `source_item_type` but never inspects it) and emits
  `container_representation_change`. Proven by `csv-dir-to-sqlite-reshape`.
  *Punchlist:* the reconciled node still carries inert `binoc.move`/`folder-move`
  tags stamped at pair time (overlay unions tags, can't retract) — see the
  `retract_tags` pre-merge decision; and the `stacked-csv-broken-out` child-orphan
  rough edge below.
- **CFM-72 split/merge via partition identities.** Verbatim row-partition
  splits/merges are representable and claimed only when exact. SDK ships an opaque
  `IdentityToken` + format-keyed `IdentityExtractor` (`tabular_v1` token = row
  cell-values), registered on `CorrespondenceEngineConfig.identity_extractors`,
  plus a generic `disjoint_cover` coverage query (`Clean`/`NearMiss`/`None`). Core
  dispatches it JIT through `EngineView::identity_tokens` over the unmatched residue
  (stays type-ignorant; tokens opaque, never stored). Stdlib `PartitionPair` claims
  a split/merge **iff** complete + disjoint + unambiguous — emitting a settled
  1→N/N→1 link fan + a `binoc.tabular_split`/`_merge` `Changeset.claims` entry (the
  first concrete claim producer, via `PairRule::final_claims` collected
  post-saturation) — else declines with `binoc.possible_split`. Proven by
  `observations-split-by-year`, `enforcement-actions-merge-years`,
  `observations-split-residual` (near-miss decline), `stacked-csv-broken-out`
  (confirms whole-table rehoming stays reshape, not split). (ADR: partition
  identities.) *Punchlist follow-ups:* (a) **DONE — the interim `humanize_numbers`
  identifier-guard rider was removed by CFM-84**; (b) identity tokens are
  recomputed over the residue each saturation round (residue-capped at 256/side);
  per-run caching is the next optimization — see punchlist; (c) residue admits
  unsettled *cross-path* links while excluding settled and *same-path* links; (d) a
  single participant covering the whole returns `None` (1:1 move); (e) the
  `stacked-csv-broken-out` reshape pairs only one child cleanly and orphans the
  other — see punchlist.
- **CFM-84 diagnostics as structured `Summary` segments.** `Diagnostic.message:
  String → Summary`; constructors take `impl Into<Summary>` (the `From<&str>`/
  `From<String>` impls keep every plain-string call site unchanged). The Markdown
  renderer routes diagnostics through `render_summary` instead of the fragile
  `humanize_numbers` prose scan, so embedded filenames (`actions_2023.csv`) are
  never digit-mangled and standalone counts are always grouped. `humanize_numbers`
  is back to a plain thousands-grouper — the CFM-72 identifier-guard and its
  `humanize_numbers_groups_only_standalone_quantities` test are deleted. Count/
  path-bearing producers rebuilt as segment chains: declared-correspondence
  warnings, fuzzy/tabular/partition limits, `possible_split` near-miss (snapshot
  `Side` threaded through `near_misses`), `rule_failure`, and `bounded_index_list`
  (typed `Uint` segments). Proven by `renderers::markdown::tests::
  diagnostic_summary_renders_filename_safely_with_grouped_count` (the year-bearing
  filename proof CFM-72 follow-up asked for) + reblessed `observations-split-residual`
  / `csv-keyed-null-duplicate` snaps. **API note:** `Diagnostic` lost its `Eq`/
  `Hash` derive (`Summary` carries `f64`); no in-tree consumer relied on it.
- **`ProjectionHint::retract_tags` — tag retraction (closes the last open IR
  question).** Tag overlay was union-only, so a CFM-71 reshape couldn't drop the
  pair-time `binoc.move`/`binoc.folder-move` it superseded, leaving contradictory
  tags in the IR (inert in rendering, incoherent in JSON). New
  `ProjectionHint.retract_tags` (+ `.retract_tag` builder), honored at the single
  tag-merge point (`merge_tags`, shared by `merge_from`/`overlay_from`) and at node
  materialization (`project.rs`) for cross-line coherence; `container_reshape_hint`
  retracts the move-family tags. Proven by the reblessed `csv-dir-to-sqlite-reshape`
  and `stacked-csv-broken-out` vectors (reshape node drops `binoc.move`; a
  genuinely-moved sibling keeps its own) + SDK unit tests.
- **Equal-arity boundary coverage + CFM-83(e) stem-collision fix.** Added
  `observations-repartition-equal-arity` (2→2 N→M repartition → `disjoint_cover`
  correctly declines both ways, no false split/merge) and
  `shapefile-fusion-equal-arity-tiebreak` (two distinct-stem arity-5 fusions, no
  contention). The third vector `shapefile-stem-collision` surfaced a real **silent
  member-misattribution bug**: `SharedStem` keyed on the *first-dot* stem, collapsing
  `roads.*` and `roads.v2.*` onto one stem so two `.shp` anchors fought over
  `roads.dbf`. **Fixed:** `stem_key` now strips only the *final* extension, so a
  sidecar set shares its full basename and each member binds to its most-specific
  anchor; the deferred suffix-sidecar case (`data.tif.aux.xml`) is unchanged. The
  vector flipped from a known-bug pin to a correctness assertion (two independent
  layers, own attributes, no orphans).

## Pre-merge gaps — all resolved

1. **`retract_tags`** ✅ landed (above). The one open IR-coherence question is
   closed: reshapes drop superseded framings; JSON tag sets are coherent.
2. **Partition / fusion boundary coverage** ✅ landed (above). Equal-arity vectors
   added; the CFM-83(e) stem-collision seam was a silent-corruption bug and is now
   *fixed*, not just pinned.
3. **`Diagnostic` Eq/Hash drop** — accepted (greenfield, AGENTS.md rule 9). No
   action; recorded as an API note on the CFM-84 entry.

## Post-merge punchlist (known rough edges, acceptable to defer)

1. **Reshape child-orphan — a parse/pair round-ordering race, not a reshape or
   partition bug.** In `stacked-csv-broken-out`, `FuzzyPair` links the still-
   *unparsed* container (`report.csv ↔ products.csv`) on raw-byte similarity before
   `CsvParse` decomposes it; the second child then has no partner and surfaces as a
   child `remove`. Confirmed by trace replay (disabling `FuzzyPair` yields the ideal
   whole-table rehoming). The honest fix is non-local — force parse-before-fuzzy for
   decomposable nodes, or add endpoint-contention/backtracking to pair dispatch —
   which is **explicitly rejected as over-engineering** in
   `docs/adr/2026-06-13-derived_requires_link.md` (Known Limitation). File against
   the CFM-71 reshape line if/when it's worth a scoped dispatch/ABI change.
2. **CFM-83 dispatch-contract tidies.** (b) invalid anchor declines via the *error*
   channel rather than the clean empty-output decline — reconcile; (c) `parse`/
   `parse_group` boilerplate on every fusing rule — ergonomics; (a) the rule-scoped
   decline-memoization invariant to keep in mind.
3. **Identity-token per-run caching (CFM-72b).** Tokens recomputed over the residue
   each saturation round (capped at 256/side). Token inputs are immutable within a
   run, so a cache keyed `(side, node index, artifact format)` is low-difficulty —
   but this is **not a measured bottleneck**; do it only if a profile shows it.
   Sits with the parked perf items below.
4. **Composed-edit raw order (CFM-82).** Artifact-format-sorted then structural-last;
   reader-facing order comes from the renderer, not raw composed order — revisit
   only if a consumer depends on raw order.

## Future feature arcs (the rewrite-rule iteration phase)

- **CFM-74/75/76 replayable claims — reclassified as next-arc feature work, not
  core plumbing.** Two distinct pieces bundled under one number: (a) replay-
  *verification* of existing claims — likely largely redundant, since split/merge
  claims are already emitted only on an *exact* `disjoint_cover`; and (b) net-new
  claim *producers* (numeric unit-conversion, precision-rounding, bounded
  near-duplicate row hints) that are rewrite-rule territory. Build on the cost
  ratchet, not render hacks, when a real consumer needs them. The
  `binoc.tabular_split`/`_merge` claim (verb/params from/to/covered/residual) is the
  shape to generalize from; `LinkDescriptionCost` is the cost infrastructure.
- **Parked formats / text-section split.** FASTQ, VCF, TIFF/image equality,
  XBRL-like JSON; text-section split (needs conservative section children from the
  parser first). The partition machinery (opaque tokens, `disjoint_cover`, the
  `IdentityExtractor` seam) is format-generic — a new format registers its own
  extractor and gets split/merge for free. Pull one only when a user needs it.
- **ABI graduations & interchange.** Pair-rule ABI (blocked on `EngineView` transit
  shape; affected by split/merge pairing); expand/parse ABI once shapes settle;
  Arrow IPC `tabular_v2` (only when extraction/interchange justifies it);
  N-snapshot / k-tree lineages; true graph renderer output (a renderer option, not
  an engine change); dirty-set / frontier scheduling (not the measured bottleneck).

## References

- Fuzz arc detail and scenario→item mapping: prior long-form tracker in git
  history; report at `binoc-fuzz-vectors.KujG6V/REPORT.md`.
- ADRs (`docs/adr/`): correspondence-first engine · parsed children and decompose
  boundaries · typed records · tiered artifact metadata · composable per-artifact
  writers · multi-input claims · partition identities (CFM-72) · structured summary
  segments (the typed-`Segment` model CFM-84 extended to diagnostics) ·
  `2026-06-13-derived_requires_link` (the parse/pair-ordering Known Limitation
  behind the reshape child-orphan punchlist item).
