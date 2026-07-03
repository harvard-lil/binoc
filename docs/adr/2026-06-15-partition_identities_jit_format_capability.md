# Partition Identities: a JIT, Format-Owned Capability for N<->M Correspondence (CFM-72)

**Date:** 2026-06-15
**Status:** Implemented

## Context

Some changes turn one artifact into several of the same shape, or several into
one: a table split by year (`observations.csv` -> `observations_2024.csv` +
`observations_2025.csv`), or merged. Binoc must be able to *represent* that —
"`X` split into `A`, `B`" should not be inexpressible — without diving into
detection tuning that needs a corpus of test cases first.

Two prior results bound the problem:

- **The 1:1 rules already handle whole-artifact moves.** With tables now
  first-class nodes (CFM-69/70), `HashPair` links a verbatim whole table that
  moved, and `TabularPair` links a reformatted one by parsed content (CFM-62).
  These are correct and stay.
- **The collection<->broken-out-tables case is reshape, not split.** A stacked CSV
  or SQLite DB broken out into one file per table moves each table **whole, 1:1**;
  that is a container reshape (CFM-71) plus ordinary 1:1 child pairing, already
  handled. The discriminator is **whether rows are partitioned (split) or whole
  tables are re-homed (reshape)**.

So the genuinely missing piece is the **N-arity, row-partition** case — where *no*
output artifact equals the input, but the set does. There, the existing fuzzy
rule actively does the wrong thing: `TabularPair` fuzzy-links the split input to
*one* output and leaves the rest as adds — the "false move + add/remove" the fuzz
report flagged as the riskiest behavior. The job is to make split/merge
representable, claim it only when it is unambiguous, and stop the fuzzy rule from
lying when it is.

This ADR defines that machinery. It is deliberately **representation-complete but
detection-conservative**: lock the durable representation now; keep the detector
dial-free; defer everything that needs tuning.

## Decision

### 1. Partition identity is an artifact-*format* capability, derived JIT

The knowledge of "what are the atomic sub-units of this artifact, and what is each
one's identity" belongs to the **artifact format**, not the parser that produced
it. A registered `IdentityExtractor` keyed by `ArtifactFormat` (dispatched like
writers/compaction/annotators) yields an **ordered sequence of opaque identity
tokens** for an artifact. The SDK/stdlib ships the extractor for `tabular_v1`
(token = content hash of a row's cell values); third-party formats register their
own; a format with no extractor is simply not partition-capable.

- **It rides the format**, so all six `tabular_v1` producers (CSV, SQLite, Excel,
  Parquet, Avro, DBF) gain partition capability for free — the parsers do nothing.
- Tokens are **opaque** to the engine (equality / membership / disjointness /
  union only) and **globally comparable** — the same row in artifact A and B
  yields the same token (content- or key-derived, never a positional index). The
  engine stays type-ignorant; the format owns the meaning.

Identities are **JIT and ephemeral**: derived only when (a) the 1:1 rules have
left a residue of *unmatched* nodes and (b) a partition-capable pair rule wants to
match a sequence-capable set of them. They are never published as an artifact,
never stored in the IR, never serialized into gold. Most runs (everything matched
1:1) compute nothing. Cost is bounded to the residue and is pure hashing plus a
hashmap, cached within a run.

### 2. An SDK disjoint-union / coverage query

Given the unmatched partition-capable nodes, the SDK builds `token -> owning node`
once (O(total atoms)) and answers the set questions: is X's token multiset the
**disjoint union** of some set of others (split), or the symmetric union (merge)?
It flags **ambiguity** (a token owned by more than one candidate). The query is
generic over opaque tokens, so it serves any format and any location — siblings
and non-siblings alike, because the **rows carry the correspondence**, not the
tree structure.

### 3. One generic, conservative consumer

A single format-ignorant pair rule (`reads: [tabular_v1, …]`) consumes the query
and claims a split/merge **iff** the relationship is:

- **complete and disjoint** — the outputs' tokens exactly reconstruct the input's,
  residual = 0; and
- **unambiguous** — no token maps to more than one candidate input; and
- **not a whole-table 1:1** — no single output ≈ the whole input (that is
  reshape/move, left to the 1:1 rules).

Otherwise it **declines** and emits a `binoc.possible_split` diagnostic, leaving
the nodes to honest add/remove. There are **no similarity thresholds, no scoring,
no multi-candidate contest** — it is the exact-tier analog of `HashPair`/`CopyPair`:
the tokens reconstruct exactly or they do not.

Ordering: exact 1:1 settles first (`HashPair`) -> the partition rule runs on the
residue and claims clean partitions *before* the fuzzy rule can mis-link -> fuzzy
1:1 (`TabularPair`/`FuzzyPair`) runs last on whatever remains.

### 4. Representation: a link set + a claim, projected through CFM-71

A claimed split is a **1->N link fan-out** (merge: N->1; the general case is N->M)
plus a `Changeset.claims` entry — `binoc.tabular_split` / `binoc.tabular_merge`
with `from`/`to` and `evidence` (covered tokens, residual = 0, and the partition
column when one cleanly explains it, reported as evidence but never *required*).
Rendering reuses the **CFM-71 reconciliation pass**: split (1->N) is the
across-snapshot dual of the within-snapshot N->1 "Merged from" collision that pass
already handles. This makes split/merge the first concrete producer of the
`Changeset.claims` slot reserved by CFM-60 (CFM-74 later generalizes the payload).

### 5. The decompose<->partition unification (and the reserved delivery hook)

On-demand sub-artifact delivery generalizes parsed children: a parsed child
(CFM-69) is a *parser-chosen, pre-materialized identity subset surfaced as a
node*; a partition is an *engine-discovered identity subset across artifacts*;
delivery is *materialize any subset on demand*. The future edited/keyed tier needs
it — when tokens are stable **keys** rather than content hashes, "same key,
different content" is a residual edit, and rendering it means fetching and diffing
the actual sub-content. So a second format capability is **reserved but not
implemented**: `format + identity-subset -> sub-artifact`, keyed and JIT like the
extractor. v1 uses content-hash tokens and needs no delivery.

## Alternatives Considered

- **Parser-published eager identity sidecar.** Have each parser emit a partition
  sequence as an artifact. Rejected: it stores derivable data (IR/gold bloat),
  forces per-parser opt-in, and pays the cost on every run even when everything
  matches 1:1. The format-capability + JIT model computes nothing until needed and
  stores nothing.
- **A tabular-specific split rule** (hash rows inside a `tabular_v1`-aware rule).
  Rejected: it does not generalize. The opaque-token + format-extractor seam is
  barely more code and gives JSON-record / text-section / any-future-format split
  for free, with the format owning identity semantics.
- **Fuzzy/similarity detection in v1.** Rejected: that is the dial-optimization
  swamp. Exact, disjoint, unambiguous is correct and conservative; fuzzy/partial
  coverage is a named later tier that does not change the representation.
- **Restrict candidates to siblings.** Rejected: locality was only a crutch for
  bounding a *similarity* search. The exact rule is content-addressed, so it is
  naturally global; restricting to siblings would be extra code and would make
  cross-container splits inexpressible.
- **Treat collection<->broken-out-tables as split.** Rejected: that is whole-table
  rehoming (reshape, CFM-71), not row partitioning. The detector explicitly
  declines when an output equals a whole input.

## Consequences

- **`binoc-sdk`:** an `IdentityExtractor` trait keyed by `ArtifactFormat` and a
  registration slot (alongside writers/compaction/annotators); the `tabular_v1`
  extractor; a disjoint-union/coverage query over opaque tokens; `Changeset.claims`
  payload for `binoc.tabular_split`/`_merge`; a reserved (unimplemented)
  sub-artifact-delivery capability.
- **`binoc-core`:** JIT identity extraction over the unmatched residue (cached per
  run); 1->N / N->1 links in the store/projection; split rendering folded into the
  CFM-71 reconciliation pass; the `possible_split` diagnostic.
- **Behavior:** verbatim row-partition splits/merges render as a coherent claim
  ("`observations.csv` split by `year` into …"), cross-container included; the
  fuzzy rule no longer mis-links a split as a 1:1 move; ambiguous or
  residual-bearing cases degrade to add/remove with a diagnostic.

## Implementation notes (landed)

What shipped matches the decision; a few realization choices are worth recording:

- **Identity is engine-mediated, not rule-held.** The `tabular_v1` extractor is
  registered on `CorrespondenceEngineConfig.identity_extractors`; the pair rule
  reads tokens through a new `EngineView::identity_tokens`, which dispatches to the
  first registered extractor whose format the node carries. The rule stays
  format-ignorant (opaque `IdentityToken`s only); core never interprets a token.
- **Claims come from a `PairRule::final_claims` hook**, the claim analog of the
  existing `final_diagnostics`: called once on the converged link graph, so the
  `binoc.tabular_split`/`_merge` `GlobalClaim` is produced from the settled fan
  rather than re-emitted every round. The domain verb/wording lives in stdlib;
  core only collects and hoists onto `Changeset.claims`.
- **Rendering split vs. merge is asymmetric.** A merge (N->1) collides on the
  target path and reuses the CFM-71 "Merged from" reconciliation unchanged. A
  split (1->N) lands its targets at *distinct* paths (no collision), so each target
  carries a `tabular_split` action + "Split from `X`" summary stamped on the link
  projection; the shared claim ties them together. Both clean-partition links are
  **settled**, so no spurious whole-vs-part content diff is written.
- **Residue admission and the fuzzy-rule race.** Partition runs before the fuzzy
  tabular/file rules but needs parsed artifacts that materialize a round later, so
  the fuzzy rule often links a split input first. The residue therefore admits
  nodes carrying only *unsettled, cross-path* links (a fuzzy rename) — a clean
  split outranks and upgrades them — while excluding settled links and *same-path*
  links (an in-place modify, which a shared unchanged row must not turn into a
  false near miss).
- **A single participant is never a split.** The coverage query returns `None`
  (not `NearMiss`) when only one other-side table shares rows with the whole — that
  is a 1:1 move/modify the exact/fuzzy rules own. `binoc.possible_split` fires only
  when ≥2 tables *together* almost-but-not-quite partition the whole, and is
  suppressed for any node a later scan ultimately claims.
- **Rider fix:** the Markdown `humanize_numbers` helper grouped thousands inside
  identifiers, mangling year-bearing filenames in diagnostics (`actions_2023.csv`
  -> `actions_2,023.csv`); it now groups only standalone quantities. Surfaced by
  CFM-72's split-by-year diagnostics.
- **Deferred / known limits:** identity tokens are recomputed over the residue
  each saturation round (bounded by a residue cap; per-run caching is the obvious
  next optimization). The stacked-CSV->broken-out reshape baseline
  (`stacked-csv-broken-out` vector) confirms partition correctly *declines*, but
  the underlying reshape pairs only one child cleanly and surfaces the other as a
  container-reshape plus an orphaned child-remove — a pre-existing CFM-71/child-
  pairing rough edge, not a partition bug.

## Open Questions

- **Identity API surface.** Exact shape of the `IdentityExtractor` trait and the
  SDK query (return type for candidates; how ambiguity is surfaced); perf cap on
  residue size.
- **Key-based identities + on-demand delivery (deferred tier).** The contract for
  stable-key tokens and `format + subset -> sub-artifact`, which together unlock
  edited/residual splits.
- **N->M re-partitioning contests.** v1 claims clean 1<->N pivots; the general
  many-to-many re-partition (quarterly -> yearly with overlap) needs a contest
  policy — deferred.
- **Partition-column evidence.** Opportunistically detected and reported, not
  required; how prominently it renders is a renderer-config question.
- **CFM-73 text sections.** Tabular first; text-section split needs conservative
  section children from the parser — a separate follow-up.
