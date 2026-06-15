# Correspondence-First Engine: Two Trees, Links, and Edit-List Compaction

**Date:** 2026-06-12
**Status:** Accepted (prototype validated; migration gated by structural projection)

## Context

The engine's IR is a single diff tree whose nodes carry actions
(`add`/`remove`/`move`/`modify`). That representation bakes the edit script
into the structure: whenever a later pass improves our knowledge of
*correspondence* (which left item is which right item), the tree must be
surgically rewritten. A series of design investigations (June 2026) found
that every hard problem on the roadmap is a symptom of that choice:

- **Recompare machinery.** `pending_recompare` + `inflate_pending_recompares`
  + bespoke merge semantics exist solely because a pairing discovered in the
  transformer phase must re-enter the comparator phase and have its result
  spliced into the tree. The merge wholesale-replaces children
  (`controller.rs`), which would clobber previously derived work the moment
  container pairing lands.
- **Cross-subtree claims.** A file moved *out* of a renamed zip produces a
  move node outside the zip's subtree. Any future recompare of the zip pair
  re-manufactures a `remove` for that file from raw bytes, double-counting a
  change already claimed elsewhere. Fixing this in the current model requires
  a claims ledger keyed by item identity — i.e., maintaining the
  correspondence mapping as separate state that the tree must be reconciled
  against.
- **Rollup surgery.** `FolderMoveDetector` infers container moves from leaf
  evidence, then rewrites the tree to express them. The rewrite layer
  operates on path strings and has accumulated guards (source-ancestor
  checks, same-path shell hoisting, dangling-remove cleanup) because nothing
  structurally prevents an incoherent rollup from being applied.
- **Leaf-only pairing.** All three pairing mechanisms (exact hash, fuzzy
  similarity, declared correspondence) only pair childless nodes. Containers
  are second-class in every one of them; user-declared container
  correspondences are silently ignored (now diagnosed, still unsupported).
  Test vector `zip-rename-contents-rewritten` documents the resulting gap.
- **Refinements can't compose.** Transformers that recognize *total*
  patterns ("pure column reorder") fall through on conjunctions ("reorder
  *and* rows added"). Composing them requires either a god-pass or a
  normalization/residual mechanism, which the single tree makes expensive
  (see Alternatives in
  [transformer composition](2026-03-20-transformer_composition_and_artifact_flow.md)).

Each problem had been given a local patch design (claims ledger, scoped
delta-loop re-evaluation, replace-then-re-derive merge, per-rule evaluation
scopes). Laid side by side, all four patches approximate the same underlying
structure — the standard representation in the tree-diff literature
(GumTree et al.): **two trees plus a mapping, with the edit script derived
from the mapping rather than stored as the structure.**

The recently landed declared write-sets
([ADR](2026-06-11-declared_write_sets_on_transformer_descriptor.md)) give
every transformer machine-readable input/output declarations — the
prerequisite for dispatching rules by what they consume and produce.

## Decision

Adopt, as the target engine model, an IR of **two immutable side trees plus
first-class correspondence links**, processed in two passes plus a
projection. The standalone prototype validated this model enough to begin
the staged migration of `binoc-core`; the structural projection gate below
remains the migration guardrail.

### The IR

- **Side trees.** Left and right snapshots are each expanded into a tree of
  `ItemRef`-backed nodes. Side trees are append-only and immutable once
  expanded: snapshots don't change, so node-keyed computations (hashes,
  parses, artifacts) are pure and memoizable with no invalidation concern.
  (This soundly revives what the
  [cross-phase cache](2026-03-05-cross_phase_data_cache.md) attempted
  against a mutating tree.)
- **Links.** A many-to-many relation between left and right nodes, each link
  carrying the evidence kind that produced it (hash, name-under-paired-
  parent, declared, fuzzy, child-evidence, copy) and a priority. Links are
  the claims ledger, promoted into the IR. `ItemPair` survives as the
  payload of a link, not as the engine's input. The applied link records
  its proposing rule as provenance so the harness can enforce evidence
  declaration honesty; the engine does not interpret the rule name.
- **Derived actions.** `add`/`remove`/`move`/`modify` are queries, not
  stored state: an unlinked left node is removed; a link whose endpoints
  have different paths is a move; a many-to-one link with copy evidence is
  a copy; etc. The bug class "derived state maintained by tree surgery got
  out of sync" becomes unrepresentable.

### Pass 1 — saturation (expand, parse, pair)

Rules with declared input/output types, run by a worklist driver to
quiescence:

- **expand**: side node → child side nodes (zip, tar, gzip, directory,
  sqlite-to-tables). Needs no pairing; bounded by input depth.
- **parse**: side node → artifacts (tabular_v1 etc.). Fires at most once per
  node; memoized.
- **pair**: evidence → link. Exact-hash pairing is the cheapest, strongest
  rule and **is** the generalized short-circuit: a hash-link formed early
  suppresses parse/expand work beneath it, unifying the controller's hash
  short-circuit, `CorrelationDetector`'s exact stage, and comparators'
  child pre-hashing into one rule. The other stdlib pairing rules: declared
  correspondence, name-under-paired-parent (today buried inside `Expand`),
  fuzzy content similarity, container-from-child-evidence (the rollup
  detector reborn as a link *proposer* rather than a tree rewriter).
  Containers become first-class pairable items in every mechanism for free.
  Link priority comes from configuration — config order = evidence
  priority, the same rule as comparator dispatch today — not from the
  engine knowing which rule is which.

Two scheduling axioms from the prototype are part of the target semantics:

- **Frontier rule.** Pairing gets first claim on every new node. Expand and
  parse rules only see nodes that existed at the start of the saturation
  round, so all pair rules get one turn to settle or link a new node before
  any rule works beneath it. Config order alone is insufficient to make the
  short-circuit effective.
- **First-claim-wins expand.** An expand rule claims a node when it matches,
  even if it returns zero children. Later expand rules do not get a second
  chance. This mirrors comparator dispatch and keeps expansion priority a
  configuration concern.

### The driver is rule-ignorant (C12, strengthened)

The controller's type-ignorance carries over and is a hard constraint on
the model, not an aspiration:

- The driver dispatches rules **solely on their declared input/output
  types** (node attributes and media types, artifact formats, link kinds,
  edit-list verbs). It contains no knowledge of any specific rule. The
  stdlib rule set is a plugin pack with no special status; every rule named
  in this ADR is plausibly third-party.
- **Link evidence kinds and edit-list verbs are open, namespaced
  vocabularies**, with exactly the posture tags and actions have today: a
  genomics plugin introduces `genomics.gap-shift` edit verbs or a new
  pairing evidence kind without touching the engine, and unknown verbs flow
  through compaction and projection unharmed.
- **Pairing priority is configuration**, not engine code. "Hash before
  declared before name before fuzzy" is the stdlib's shipped default
  config, and link revision's "strictly higher priority only" rule works
  over whatever total order the config supplies.
- **The short-circuit is a generic mechanism**: a pairing rule may flag a
  link as *settled* (content-identical by its evidence). The driver's only
  built-in policy is that no rule fires beneath a settled link unless its
  descriptor opts in — `handles_identical` generalized. The driver does not
  know what hashing is. Settledness is link-scoped, not node-scoped:
  work rules suppress the endpoint and descendants; pair-rule views hide
  only strict descendants so the settled link itself remains usable as
  evidence. Cross-path hash links remain rule policy (`settle_renames` in
  the prototype), not engine policy; the stdlib migration must choose its
  default explicitly before deleting old copy/correlation behavior.
- **Negation is stratified by configuration.** Rules that query absence
  ("unlinked") observe the links already produced by stronger/config-earlier
  rules. This is a semantic consequence of config-owned priority and must be
  documented for rule authors.
- **Stdlib hash-pair default.** The migrated stdlib defaults to correctness
  over speed for exact cross-path hash links: renamed unchanged collections
  are linked but not settled, so expansion can recover copy/move-out
  provenance inside them. The fast short-circuit posture is an explicit
  opt-in (`expand_renamed_unchanged_collections = false`). The future
  fast-and-correct path is a provenance rule that opts into
  `fires_beneath_settled`, keeping the short-circuit while recovering
  provenance on demand.
- **Evidence declarations are fail-closed.** Pair descriptors declare the
  open evidence kinds they may emit; a rule that proposes an undeclared
  evidence kind fails the run. This is stricter than legacy transformer
  write-set enforcement, but it is appropriate for correspondence links:
  undeclared evidence changes the claims ledger, so continuing would project
  confident add/remove/move facts from unverified rule behavior. The
  declarations remain verification and capability metadata, not driver
  dispatch logic.
- **Naive edit-list writers register per artifact format**, like rules; the
  engine knows "format F has a writer," never "this is the tabular writer."
  The prototype proved that naive vocabulary design is real product work:
  for example, per-position header edits could not compact reorder plus
  column-add, while a whole-header statement
  `tabular.set_headers {from, to}` could. Each artifact format needs an
  intentionally designed naive vocabulary that preserves enough information
  for independent compaction rules to compose.
- **The cost function is structural** (verb count, parameter sizes,
  residual edit counts) over any vocabulary; it requires no verb registry
  and no engine knowledge of what any verb means.

Termination is structural, not budgeted: expansion is bounded by the input,
parse is once-per-node, links and annotations are add-only. The one
non-monotone operation — link revision — is restricted to strictly
increasing priority, which preserves termination. A "late" pairing (fuzzy
or declared, discovered after both sides were parsed) simply adds a link;
parse/pair rules waiting on links fire then. **Recompare, inflation, and
merge semantics cease to exist** — there is nothing to splice, because no
derived tree is being maintained.

Expansion rules that decompress or unpack containers must preserve the
project's C10 posture: no path traversal, bounded output, and no silent
truncation. Zip and tar rules need per-entry and total output caps; gzip cap
overflow is an error or reportable diagnostic, never a partial diff.

### Pass 2 — edit-list compaction

Every link gets an **edit list**: the description of what changed across
that correspondence, in open-vocabulary verbs.

- **Naive writers ship with artifact formats.** Each artifact format is
  packaged with a baseline writer that emits the dumbest true edit list:
  tabular_v1's writer emits per-cell edits and row/column adds; the
  container writer emits child-level adds/removes; the binary writer emits
  "contents differ." This generalizes the thin-comparator pattern: parsers
  parse, the naive writer states the obvious, and *all* compactness is
  earned by rewrite rules.
- **Compaction rules rewrite edit lists to shorter equivalents**: N cell
  edits in a consistent pattern → "columns reordered by σ"; M per-link edit
  lists across the dataset → one "find-replace sneakers→shoes" claim (the
  vision's flagship becomes an ordinary root-scope compaction rule reading
  residual edit lists). Acceptance requires strictly decreasing description
  cost (structure-counted: verbs + parameter sizes + residual edits, with
  parameters charged so a fat permutation can't "explain" anything), which
  is both the termination measure and the product's own MDL objective made
  operational. Compaction runs as an explicit ordered pipeline (the
  LLVM/MLIR lesson: judgment ordering stays curated; only the monotone
  dataflow of Pass 1 gets a fixpoint driver).
- Significance remains a renderer concern (unchanged;
  [renderer config](2026-03-09-renderer_config.md)).

### Projection — the changeset contract survives

The changeset tree (C7's stable JSON contract) is **derived at the output
boundary** from (side trees, links, edit lists, annotations), the way
`prune_identical`/`strip_transient` already project today. External
consumers, renderers, and the existing gold files are therefore the
regression harness for any migration rather than casualties of it. Extract
(C8) reads the side trees and links directly — per-side reopen chains plus
the link, with no merged-tree ancestry to disentangle.

Projection owns one product-level normalization decision: do not state the
same child change twice. A paired container may carry child add/remove edit
verbs, or the projection may emit per-child add/remove nodes, but user-facing
changesets must pick one representation for a given fact.

In the main port, projection metadata is supplied by rule packs rather than
inferred by core. `ItemRef` carries an optional `projection_hint` for
product-facing item type and tags known during expansion, link proposals carry
projection hints for correspondence judgments such as moves/copies, and edit
list entries carry projection hints plus a visibility bit for detail output.
Core projection overlays link/edit hints on node hints, renders only visible
edits in the `edits` detail, and otherwise treats all item types, tags,
evidence kinds, and edit verbs as opaque strings. This replaces the prototype's
extension matching and hardcoded `binoc.*` projection helper.

The carried-child path-change judgment is a projection default in the main
port: if two linked children retain the same relative name and their parents
are linked, the child path change is considered carried by the parent link and
does not independently project as a move. This is structural and does not
inspect rule names or evidence kinds. If product policy later needs to expose
both parent and child moves, that should be added as renderer/config policy on
top of the structural projection rather than as engine vocabulary knowledge.

The migration gate is structural before it is byte-for-byte product parity:
the prototype must project to real `Changeset` / `DiffNode`, pass shared
changeset invariants, and satisfy every mappable manifest assertion across
the corpus. Byte-gold parity is then proved during the main projection port,
where product summaries, detail blocks, diagnostics, and extract hints are
owned by the real stdlib writers/compaction rules.

### Prototype before migration

This model was adopted as *direction*, then validated by a minimal
clean-room prototype. The prototype crates were retired after migration; their
durable findings are preserved in
[Correspondence-First Migration Completion](2026-06-12-correspondence_first_migration_completion.md).
The hardened prototype demonstrated the key bet — robust, general
composability of independently authored rules — on the discriminator vectors
and the full mappable test-vector corpus.

Prototype status at acceptance:

- acceptance discriminator tests pass;
- breadth is **39 pass / 0 partial / 3 n-a / 0 errors**;
- the previous partials (`csv-keyed-row-diff`,
  `csv-keyed-null-duplicate`, `csv-mixed-changes`) pass after keyed-row
  writer flow and header-vocabulary redesign;
- `directory-file-copy` passes via a many-to-many copy pair rule;
- the prototype emits real `Changeset` / `DiffNode` and runs shared
  changeset invariants across the corpus;
- pair-rule evidence honesty and archive decompression bounds are enforced.

This is sufficient to proceed with migration. If the main port later fails
the structural projection gate, the recorded single-tree patch designs
(claims ledger, scoped delta loop) remain the fallback path, but they are no
longer the preferred target.

## Consequences for existing decisions

Because the prototype validated, this decision supersedes
[transformer-initiated recompare](2026-06-03-transformer_initiated_recompare.md)
(late links replace the recompare bounce); subsumes
[full comparison tree](2026-03-05-full_comparison_tree_and_content_hashes.md)
(side trees naturally contain identical nodes; pruning moves to
projection); revisits the "sequential undo" rejection in
[transformer composition](2026-03-20-transformer_composition_and_artifact_flow.md) —
compaction rewrites *edit lists*, not artifacts, dissolving the
data-amplification objection, while the cost guard answers ordering and
the MDL objective answers narrative ambiguity. The retired
comparator/transformer vocabulary now remains only in historical records; live
plugin authoring uses the correspondence rule-family names.

## Alternatives Considered

**Keep the single tree and land the patch designs** (claims ledger keyed by
item identity, per-rule-scope delta loop, replace-then-re-derive merge).
Workable — each patch was individually designed and reviewed. Rejected as
the *target* because the four patches are approximations of the two-tree
model maintained by hand inside a representation that fights them; the
patch path keeps paying the surgery tax on every future container feature.
It remains the fallback if the main migration fails the structural
projection gate.

**Top-down container pairing as another Root transformer + rewrite pass.**
Fastest route to closing the `zip-rename-contents-rewritten` gap. Rejected:
it creates a second pairing mechanism that can disagree with rollup about
whether `archive.zip` corresponds to `data.zip`, adds ordering sensitivity
between them, and deepens the tree-surgery layer the rollup work showed is
already accumulating guards.

**Full equality saturation (e-graphs).** The maximalist version of
"derive the cheapest description at the end." Rejected: nodes are fat,
stateful records and rules perform real I/O; saturation costs are
prohibitive. The adopted design keeps the e-graph *shape* only where it is
cheap — minimum-cost choice during projection — and uses greedy
cost-decreasing compaction elsewhere.

**One general scheduler for everything, including judgment.** The LLVM
legacy-pass-manager arc argues against declared-dependency scheduling where
rule interactions are semantic. Adopted hybrid instead: fixpoint driver
only for Pass 1's monotone dataflow (where build-system/Datalog-style
scheduling demonstrably works), explicit ordered pipeline for Pass 2's
judgment-laden compaction.
