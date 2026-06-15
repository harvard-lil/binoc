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

## Open work, in order

Orienting frame — the **arity matrix** ties several items together. Node
operations come in fan-out / fan-in / 1:1, on two axes (within-snapshot
structure, across-snapshot correspondence):

| | within-snapshot (structure) | across-snapshot (correspondence) |
|---|---|---|
| **1 → N** | expand / parsed children (done, CFM-69/70) | split (CFM-72) |
| **N → 1** | **multi-input claims / `subsume`** (CFM-83) | merge (CFM-72) · reconciliation (CFM-71) |
| **1 → 1** | parse leaf (done) | move / modify (done) |

The engine has had unfold (`add_child`) since day one and never had fold;
`subsume` (CFM-83) is the structural fold. Separately, the input/content/output
"the node was a proxy" correction is landing across CFM-83 (input: the *claim* is
the parse unit), CFM-80 (content: a node carries N artifacts), and CFM-81
(output: the *artifact* is the render unit).

1. **CFM-78 tail — bound diagnostics.** Inline index cap done; still open: audit
   other per-row message interpolations; gate/quiet `binoc.table_splitter.ambiguous`
   on flat single-table CSVs (false positive on showcase brfss/fda).

2. **CFM-81 — composable per-artifact writer dispatch.** *Architectural enabler;
   prerequisite for CFM-82 and a cleaner CFM-71.* Dispatch today is
   one-writer-per-link (first match wins) — a degenerate case that worked only
   while each node had exactly one content artifact. Move to: **the artifact is
   the rendering unit** — run one writer per present artifact format and
   concatenate, composing with node-level structural writers (container
   child-tracking, fallback). Needs: writer taxonomy (artifact vs structural);
   **edit provenance** (tag each `Edit` with its producing format) so
   compaction/extract/summary stay per-content-type in a merged list;
   deterministic ordering; format-scoped compaction; the per-link→per-writer-set
   bookkeeping migration (`writer_used`, `extract`, trace, perf_report). (ADR:
   composable per-artifact writers — this decision.)

3. **CFM-82 — metadata rendering + significance** (needs CFM-81). A real
   `ParserMetadataWriter`, plus tabular column/table-metadata rendering, emitting
   `metadata.value_change`; significance mapping so a relabeled column, a dropped
   value-label set, and a creator rename weigh differently — via renderer config,
   not the IR. First proof: an `.xpt` container metadata change renders alongside
   child edits; a `.dta` label/format change renders on the leaf.

4. **CFM-83 — multi-input claims (file-set fusion; `subsume`).** Generalize the
   parse claim from one node to a **correlated set of nodes**: `ParseDescriptor`
   input becomes an ordered member-match list (single-file = size-1 degenerate)
   with a correlation key (default: same container + shared basename stem;
   escalates to `DeclaredPair`'s capture/template form when a format needs it).
   The engine enumerates candidate sibling groups by the key; the parser's
   ordinary parse-or-**decline** is the authority (one source of truth — no
   separate grouping registry). Precedence is **arity-descending, largest
   *successful* claim wins, decline releases members to smaller claims**. The one
   new core primitive is **`subsume`** — mark member nodes claimed-by (a flag,
   not a deletion: `NodeId`s are index-stable, and subsumed members survive as
   result-node provenance); it is the structural N→1 fold dual to `add_child`.
   The fused node is an ordinary CFM-69/80/81 node (named `item_type`, e.g.
   "Shapefile layer"; `tabular_v1` `.dbf` child; `parser_metadata_v1` CRS).
   *Rendering needs CFM-81* (a shapefile carries geometry + attributes + CRS at
   once). Reduces pressure on CFM-72 (a file set is one node per side, so 1:1
   pairing suffices). Proof: `roads.{shp,shx,dbf,prj}` whose CRS changes while
   geometry is unchanged; a standalone `.dbf` sharing a stem must still parse
   alone (decline path). (ADR: multi-input claims.)

5. **CFM-71 — container-type-change projection + parent reconciliation.** Linked
   containers whose representation changed (directory↔SQLite, file↔section dir)
   render as a container/serialization change, not move+add/remove; reconcile
   linked parents before projecting children. Direction (from the parsed-children
   ADR): **generalize `merge_projected_collision` into one parent-reconciliation
   pass**, with the existing same-path "Merged from" collision as its degenerate
   case — not a second code path. Named container `item_type` already flows
   (CFM-70), so a reshaped container can render with an honest kind. Benefits
   directly from CFM-81 (a container node carrying both structural and metadata
   edits); shares member-attribution provenance with CFM-83.

6. **CFM-72 / CFM-73 — split/merge correspondence.** Pair rule over `tabular_v1`
   children proposing one-to-many / many-to-one when row sets partition cleanly;
   split/merge claim with residual edits; gate fuzzy one-to-one when a stronger
   split explanation exists; prove via description cost. CFM-73 extends to text
   sections if the parser stays conservative. Now unblocked: parsed children are
   linkable endpoints with content hashes, so `HashPair`/`CopyPair` can already
   match a verbatim table moved between containers. Open design question to settle
   first: what the split/merge rule owns vs. what already falls out of generic
   hash/name pairing, and whether the generic pair rules *should* reach into
   parsed children at all. Vectors: `*-split-by-year`,
   `report-split-into-section-files`.

7. **CFM-74 / CFM-75 / CFM-76 — replayable claims.** Finalize the
   `Changeset.claims` payload (scope/verb/params/covered/evidence/residual) with
   replay verification; numeric unit-conversion and precision-rounding claims;
   bounded near-duplicate row hints. Build on the cost ratchet, not render hacks.

8. **CFM-77+ — domain format plugins.** FASTQ, VCF, TIFF/image equality,
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
- **Decompose-separator escaping** — a logical-path segment literally starting
  with `>` is ambiguous with the `/>` boundary marker. Accepted as a known
  limitation (greenfield); revisit only if real data hits it.

## References

- Fuzz arc detail and scenario→item mapping: prior long-form tracker in git
  history; report at `binoc-fuzz-vectors.KujG6V/REPORT.md`.
- ADRs (`docs/adr/`): correspondence-first engine · parsed children and decompose
  boundaries · typed records · tiered artifact metadata · composable per-artifact
  writers · multi-input claims.
