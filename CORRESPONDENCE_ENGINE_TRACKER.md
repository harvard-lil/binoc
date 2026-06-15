# Correspondence Engine Tracker

Lineage: `CORRLINK_MIGRATION_TRACKER.md` → `CORRESPONDENCE_ENGINE_TRACKER.md`.
The single-tree → correspondence-first migration and its first cleanup/fuzz arc
are complete. This is now a **compact status board** for the new engine: what has
landed, and the ordered list of open and newly-needed work. The long-form arc
planning this supersedes (per-task checklists, the scenario→item mapping table)
lives in git history and the fuzz report `binoc-fuzz-vectors.KujG6V/REPORT.md`.

Principles (unchanged): core stays type-ignorant; format knowledge lives in
stdlib/plugin rules; honest partial output beats false precision; greenfield, no
back-compat effort (AGENTS.md rule 9). Every change keeps
`just fmt && just check && just test` green and names the vector/test that proves
it.

## Handoff (working state — for a fresh session)

- **Branch `pre-refactor`, all green** (`just check` + `just test`). Last shepherd
  run landed CFM-72 (split/merge); prior runs landed CFM-78/80/81/82/83/71 +
  path-boundary-escape. The working tree has a few **untracked** files from a
  separate trace-visualizer effort (`docs/users/explanation/replays*`,
  `scripts/build_replays.py`) — leave them.
- **Shepherding workflow (full autopilot on git):** hand each item to a subagent
  in an isolated worktree; merge the branch into `pre-refactor`; run
  `just fmt && just check && just test`; regenerate gold via the justfile
  `INSTA_UPDATE=always cargo test -p <crate> --test test_vectors` bless recipe;
  update this tracker (move item to Landed, renumber); delete the merged worktree
  + branch.
- **Worktree gotcha:** agent worktrees may be created from `main` (an outdated
  layout). Every worktree agent must `git reset --hard pre-refactor` *first* and
  confirm `git log` shows recent CFM commits before working.
- **Concurrent sessions** share this checkout — stage by explicit path
  (`git add <file>`), never `git add -A`. A foreman worktree at
  `.foreman/worktrees/path-boundary-escape` exists; its work is already
  cherry-picked in — leave it.
- **Next up:** **CFM-84** (item 1) — migrate the diagnostics channel to structured
  `Summary` segments. This is now also the **immediate cleanup for CFM-72**: that
  run shipped an interim `humanize_numbers` identifier-guard rider that this
  tracker's own diagnostic-rendering note says not to do — CFM-84 should delete
  that guard (and its `humanize_numbers_groups_only_standalone_quantities` unit
  test in `binoc-stdlib/src/renderers/markdown.rs`) once `Diagnostic.message`
  becomes a `Summary`. Items carry per-item carry-overs/follow-ups inline; the
  parked `retract_tags` question is the one small open IR-coherence decision.

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
  Container nodes carry a named `item_type` ("SQLite database", "Excel workbook",
  "stacked tables", "SAS transport file") set via `ParseOutput.projection`
  (overlaid onto the node at parse time) and surfaced through a projection-time
  container-path→`item_type` map, so interior container nodes default to
  "container" — never the old bare "item". `item_type` stays render-facing only:
  writer/shape dispatch keys off actual children, not the string. (ADR: parsed
  children and decompose boundaries.)
- **CFM-80 Tiered artifact metadata — channels only.** Per-column +
  per-table metadata on `tabular_v1`; new `parser_metadata_v1` artifact and
  `ParseOutput.artifacts` (a second artifact on a node); stat-binary restores its
  dropped labels/formats/value-labels/version facts into the three tiers.
  Carried, not yet rendered. (ADR: tiered artifact metadata.)
- **CFM-81 composable per-artifact writer dispatch.** The artifact is the
  rendering unit: per-link dispatch now runs one writer per *present* artifact
  format and concatenates, composing with structural writers (`ContainerWriter`/
  `TextWriter`/`FallbackWriter`, marked by a `fallback` descriptor flag).
  `Edit.provenance` tags each edit with its producing format/writer; compaction
  is format-scoped; `writer_used`/`extract`/trace/`LinkDescriptionCost` migrated
  to per-link writer **sets**. Single-artifact nodes behave as before. (ADR:
  composable per-artifact writers.)
- **CFM-78 bounded diagnostics.** `bounded_index_list` caps inline index/key
  lists to a few examples + remaining count; `binoc.table_splitter.ambiguous` no
  longer fires on flat single-table CSVs (gated on a detected banner/title), only
  on genuine stacked layouts. Audit confirmed no other unbounded per-item
  diagnostic interpolations remain.
- **CFM-82 metadata rendering + significance.** `ParserMetadataWriter`
  (`parser_metadata_v1`) and tabular column/table metadata diffing emit
  `metadata.value_change`, composed per-artifact (CFM-81) alongside table/content
  edits; Markdown renders them after the content blocks; significance is mapped
  from scope/semantic tags (`binoc.metadata.column-label`/`.value-label-set`/
  `.display-format`/`.provenance`) via renderer-group config, not the IR. Proven
  by stat-binary `stata-metadata-change` (`.dta` leaf) and `xpt-dataset-label-change`
  (`.xpt` container). Note: composed-edit order is artifact-format-sorted then
  structural-last; correct reader-facing order comes from the renderer choosing
  block order — revisit only if a consumer relies on raw composed order. (ADR:
  tiered artifact metadata.)
- **Path-boundary escaping.** A logical-path segment literally starting with `>`
  (or `\`) is now escaped with a leading backslash by the `binoc_sdk::path`
  helpers (`escape_segment`), so `dir/>q1.csv` is unambiguously a decompose
  boundary while a real file `>q1.csv` is written `dir/\>q1.csv`. Resolves the
  previously-parked `/>` ambiguity; documented in the parsed-children ADR.
- **CFM-83 multi-input claims (file-set fusion; `subsume`).** A parse claim can
  span a **correlated set of nodes**: extra members + correlation live on the
  `ParseRule` trait (defaults + `From<NodeMatch>` keep single-input ergonomic;
  blast radius = core + shapefile only). Default correlation = same-parent
  first-dot lowercased stem; arity-descending precedence, registration tiebreak,
  decline releases members. New store primitive **`subsume`** (a `subsumed_by`
  flag, not deletion): subsumed members are excluded from expand/parse dispatch,
  per-artifact dispatch, and sibling projection, but retained as result-node
  provenance. Shapefile fusing parser fuses `{.shp,.shx,.dbf,.prj,.cpg}` into one
  "Shapefile layer" node with the `.dbf` as a `tabular_v1` child + CRS as carried
  `parser_metadata_v1`; declines cleanly (standalone `.dbf` stays a plain table;
  bare `.shp` served by the single-input parser). Proven by
  `shapefile-fusion-roads` / `shapefile-fusion-decline`. (ADR: multi-input claims.)
  *Follow-ups (minor, not blocking):* (a) decline memoization is now rule-scoped
  (node+format+rule) rather than format-scoped — necessary for decline-and-release,
  JSON-split unaffected, but a core-dispatch invariant change to keep in mind;
  (b) an invalid anchor declines via the *error* channel, not the clean
  empty-output decline channel; (c) `parse`/`parse_group` boilerplate on every
  fusing rule; (d) no equal-arity-overlap test yet (only 5-vs-1 shapefile/dbf);
  (e) `roads.v2.shp` vs `roads.shp` collide on stem `roads` — capture/template
  correlation seam left but not built.
- **CFM-71 container reshape + parent reconciliation.** `merge_projected_collision`
  is generalized into one reconciliation pass in `project.rs` (same-path "Merged
  from" N→1 kept as the degenerate case); linked containers of *differing kind*
  now render as a representation change, not move+add/remove. Wording stays in
  stdlib: a `container_reshape_hint` annotator reads both endpoints' `item_type`
  (core passes `source_item_type` but never inspects it), emits action
  `container_representation_change` + tags `binoc.container-reshape`/
  `.serialization-change` ("Reshaped from `data` (directory → SQLite database)").
  No new pair rule needed — `TabularPair` + `ContainerFromChildEvidence` already
  link members and parents. Proven by `csv-dir-to-sqlite-reshape`. *Follow-up:*
  the reconciled node still carries inert `binoc.move`/`folder-move` tags stamped
  at pair time (overlay unions tags, can't retract); harmless in rendering but
  incoherent in JSON — see the open `retract_tags` question.
- **CFM-72 split/merge via partition identities.** Verbatim row-partition
  splits/merges are now representable and claimed only when exact, instead of the
  fuzzy rule mis-linking a split as a 1:1 move. SDK ships an opaque `IdentityToken`
  + format-keyed `IdentityExtractor` (`tabular_v1` token = row cell-values),
  registered on `CorrespondenceEngineConfig.identity_extractors`, plus a generic
  `disjoint_cover` coverage query (`Clean`/`NearMiss`/`None`). Core dispatches it
  JIT through `EngineView::identity_tokens` over the unmatched residue (stays
  type-ignorant; tokens opaque, never stored). Stdlib `PartitionPair` (registered
  before the fuzzy tabular/file rules) claims a split/merge **iff** complete +
  disjoint + unambiguous — emitting a settled 1→N/N→1 link fan + a
  `binoc.tabular_split`/`_merge` `Changeset.claims` entry (the first concrete claim
  producer, via a new `PairRule::final_claims` hook collected post-saturation) —
  else declines with `binoc.possible_split`. Splits render "Split from `X`";
  merges reuse the CFM-71 "Merged from" reconciliation. Proven by
  `observations-split-by-year`, `enforcement-actions-merge-years`,
  `observations-split-residual` (near-miss decline), and `stacked-csv-broken-out`
  (confirms whole-table rehoming stays reshape, not split). (ADR: partition
  identities — now Implemented, with realization notes.) *Follow-ups:* (a) **CFM-84
  must remove** the interim `humanize_numbers` identifier-guard rider this run
  shipped (see Next up); (b) identity tokens are recomputed over the residue each
  saturation round (residue-capped; per-run caching is the next optimization);
  (c) residue admits unsettled *cross-path* links so a clean claim outranks a
  premature fuzzy link, while excluding settled and *same-path* links; (d) a single
  participant covering the whole returns `None` (1:1 move), never a near miss;
  (e) the `stacked-csv-broken-out` baseline declines correctly but the underlying
  CFM-71 reshape pairs only one child cleanly and orphans the other as a
  child-remove — a pre-existing reshape/child-pairing rough edge, not a partition
  bug; (f) the ADR/this tracker named a split vector `enforcement-actions-split-by-
  year`; the landed merge vector is `enforcement-actions-merge-years` and the
  near-miss is `observations-split-residual` (whose diagnostic path carries no
  digits, so CFM-84 needs a year-bearing-filename near-miss vector to prove the
  diagnostic-segment fix).

## Open work, in order

1. **CFM-84 — diagnostics as structured `Summary` segments (hygiene + CFM-72
   cleanup; independent of the arity arc below).** The structured-summary-segments migration
   (ADR `2026-06-03-structured-summary-segments`, commit `43844d1`) retired the
   `humanize_numbers` prose scan for `DiffNode.summary` by giving it typed
   `Segment`s (`Text`/`Path`/`Uint`/`Float`): a `Uint` is always digit-grouped,
   `Text`/`Path` are verbatim, so years in filenames are never mangled. The
   **diagnostics channel was never migrated** — `Diagnostic.message` is still a bare
   `String` (`binoc-sdk/src/ir.rs:205`), and the Markdown renderer still runs the
   old prose scan on it (`humanize_numbers(&diagnostic.message)`,
   `binoc-stdlib/src/renderers/markdown.rs:201`). That call is the **last** free-text
   caller of `humanize_numbers`; the other three operate on bare `Uint`/`Float`
   strings. The reparse mangles embedded filenames (e.g. CFM-72 split/merge
   diagnostics turn `actions_2023.csv` into `actions_2,023.csv`). **Fix:** migrate
   `Diagnostic.message: String → Summary`, update the `Diagnostic` constructors and
   the `markdown.rs:201` call site to `render_summary`, and have producers that embed
   paths/counts in diagnostic text build segments (`.path(..)`, `.count(..)`) instead
   of `format!`. Then `humanize_numbers` only ever sees bare numeric values and no
   identifier-digit-skipping guard is needed. Greenfield, no back-compat (rule 9).
   **First delete CFM-72's interim rider** (the identifier-guard in
   `humanize_numbers` + its `humanize_numbers_groups_only_standalone_quantities`
   unit test) as part of this migration. Prove with a vector whose diagnostic
   embeds a year-bearing filename: CFM-72 landed only `observations-split-residual`
   (a near-miss whose diagnostic path carries no digits), so **add** a year-bearing
   near-miss vector (e.g. an `actions_2023.csv`-style `binoc.possible_split`) to
   demonstrate the fix. **Self-contained:** touches the diagnostics type + renderer
   + diagnostic producers, not the partition/arity machinery.

The remaining items form the engine **arity** arc. The **arity matrix** — split
and merge now landed (CFM-72):

| | within-snapshot (structure) | across-snapshot (correspondence) |
|---|---|---|
| **1 → N** | expand / parsed children (done, CFM-69/70) | split (done, CFM-72) |
| **N → 1** | multi-input claims / `subsume` (done, CFM-83) | merge (done, CFM-72) · reconciliation (done, CFM-71) |
| **1 → 1** | parse leaf (done) | move / modify (done) |

The engine has had unfold (`add_child`) since day one and never had fold;
`subsume` (CFM-83) is the structural fold. Separately, the input/content/output
"the node was a proxy" correction is landing across CFM-83 (input: the *claim* is
the parse unit), CFM-80 (content: a node carries N artifacts), and CFM-81
(output: the *artifact* is the render unit).

2. **CFM-74 / CFM-75 / CFM-76 — replayable claims.** Finalize the
   `Changeset.claims` payload (scope/verb/params/covered/evidence/residual) with
   replay verification; numeric unit-conversion and precision-rounding claims;
   bounded near-duplicate row hints. Build on the cost ratchet, not render hacks.
   *Builds on CFM-72:* the `binoc.tabular_split`/`_merge` claims (verb/params
   from/to/covered/residual, no replay verification yet) are the first concrete
   `Changeset.claims` producer to generalize from.

3. **CFM-73 — text-section split.** Deferred from CFM-72: needs conservative
   section children from the parser first. The partition machinery (opaque tokens,
   `disjoint_cover`, the `IdentityExtractor` seam) is format-generic and ready — a
   text-section format would register its own extractor and get split/merge free.

4. **CFM-77+ — domain format plugins.** FASTQ, VCF, TIFF/image equality,
   XBRL-like JSON — after the generic machinery is coherent, unless a user needs
   one first. (Shapefile geometry already shipped as `binoc-shapefile`; its
   fusion is CFM-83, not here.)

## Parked

- **Pair-rule ABI graduation** — blocked on `EngineView` transit shape; affected
  by split/merge pairing.
- **Dirty-set / frontier scheduling** — not the measured bottleneck; revisit only
  if CFM-72 adds broad many-to-many pressure.
- **True graph output** — a renderer option, not an engine change.
- **N-snapshot / k-tree lineages** — future product feature, not a correctness gap.
- **Arrow IPC `tabular_v2`** — only when extraction/interchange justifies it.
- **`ProjectionHint::retract_tags`** — overlay only *unions* tags, so an annotator
  that supersedes an earlier framing (e.g. CFM-71 reshape over a pair-time
  `binoc.move`) can't drop the stale tag. Inert in rendering today; build a
  retraction channel only if a consumer needs coherent JSON tag sets. (Open
  question raised by CFM-71.)

## References

- Fuzz arc detail and scenario→item mapping: prior long-form tracker in git
  history; report at `binoc-fuzz-vectors.KujG6V/REPORT.md`.
- ADRs (`docs/adr/`): correspondence-first engine · parsed children and decompose
  boundaries · typed records · tiered artifact metadata · composable per-artifact
  writers · multi-input claims · partition identities (CFM-72) · structured summary
  segments (the typed-`Segment` model CFM-84 extends to diagnostics).
