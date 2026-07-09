# Tiered Plugin Surface During Pre-1.0: In-Process Proposed Tier, ABI Stable Tier

**Date:** 2026-06-12
**Status:** Accepted; catalog reference updated by unified registry consolidation

## Context

The correspondence-first migration
([ADR](2026-06-12-correspondence_first_engine.md)) dissolves the
comparator/transformer taxonomy that the
[plugin SDK and ABI](2026-03-12-plugin_sdk_and_abi.md) is shaped around:
its entry points, request structs, and `CompareResult::Skip` dispatch all
assume coarse-grained roles that no longer exist as engine phases. The ABI
therefore breaks wholesale regardless of any policy choice; the open
question was how much of it to rebuild, and when, given that the deletion
pass (tracker CFM-26) was blocked on plugin support while the ABI rework
(old CFM-27) was sequenced after it.

Three facts bear on the answer:

- **The new engine is chattier than the old contract's pricing.** The
  original ABI was tuned for coarse calls — it removed `can_handle`
  specifically to avoid an IPC round-trip per candidate. Pass 1 is the
  opposite shape: a worklist driver firing many small rules to quiescence,
  with pair rules making callback-style `EngineView` queries across the
  side trees. Arm's-length transit for those families requires a
  snapshot/batch view design that does not exist yet.
- **Phase 3 changes the shapes that an ABI would freeze.** Root-scope
  compaction (CFM-41) changes the `CompactionRule` signature
  (`rewrite_all`); LCS alignment (CFM-40) tunes the tabular vocabulary;
  `ProjectionAnnotator` is days old. Transit machinery built before those
  land is built twice.
- **Every known plugin is regenerable.** All real plugins are in-tree
  (`model-plugins/`), enumerated in the unified `plugin_registry.json`, and
  covered by the shared vector harness. The
  [plugin ecosystem precedents research note](../core-developers/research/plugin-ecosystem-precedents.md)
  surveys how platforms with unstable APIs allocate breaking-change costs:
  the relevant patterns are author-pays co-evolution (Linux in-tree
  drivers, Google LSCs), stability tiers (VS Code proposed APIs), and
  version-the-seam-before-splitting (Terraform, Kubernetes). Skew
  tolerance — compat fields, version ranges, grandfather exemptions — is
  insurance for a constituency of stale external builds that does not
  exist during this phase.

## Decision

Split the plugin surface into two tiers for the remainder of pre-1.0.

**The seam is mandatory everywhere.** Every plugin-facing type —
descriptors, rule inputs/outputs, hints, diagnostics, edits — stays
serializable data with no host types or internals, so any extension point
*can* move behind a process boundary later. This is nearly free and
preserves the extraction option that every successful precedent built
first.

**Transit machinery exists only at the stable tier.** C ABI entry points,
wire payloads, and native wrappers are built per rule family, only after
that family's trait signature and naive vocabulary have stopped moving.
The graduation signal is two release windows with no changes to either.
The initial stable tier is renderers (untouched by the migration; the one
real Python plugin is a renderer), followed by expand/parse packs.
Pair rules, writers, compaction rules, and annotators are the proposed
tier: in-process only, with the chatty-view transit design deferred until
a family graduates.

**Skew posture is lockstep during 0.x.** The SDK version floor tracks the
host; there are no tolerance ranges and no grandfather exemptions (the
correspondence engine's fail-closed evidence enforcement already adopted
this posture). The wire-safety half of version checking stays — mismatched
builds must fail with a clear error, never undefined behavior. Breaking
changes fix all in-tree plugins atomically in the same change; registered
out-of-tree plugins (none yet) get generated fix PRs validated by the
vector harness.

**Sequencing follows from the tiers.** The old CFM-27 splits: CFM-27a
(registration types into the SDK; model plugins ported to in-process rule
traits) precedes the CFM-26 deletion, so deletion has no remaining
consumers; CFM-27b (ABI for graduated families, Python rule authoring,
SDK major bump) follows CFM-26 and the Phase 3 trait-shape changes.

**Accepted cost:** until CFM-27b, in-tree plugins are first-party
consumers of the SDK traits rather than proof of the arm's-length
boundary — a deliberate, time-boxed loosening of the old AGENTS.md rule 8,
which is amended to match this ADR.

**Exit condition:** the first external plugin we cannot regenerate — a
real third-party author who won't take fix PRs — is the signal to widen
the version floor and graduate more surface. That change merits its own
ADR.

## Alternatives Considered

**Rebuild full ABI equivalence during the port, pare back later.**
Rejected. Phase 3 changes the shapes (CFM-41 alone changes
`CompactionRule`), so full coverage schedules guaranteed rework; the
fine-grained families need a genuinely hard transit design (snapshot or
batch `EngineView`) that is wasted effort if vocabularies move; and paring
a published surface is a breaking change against future constituents,
while widening a narrow one costs nothing. With zero external consumers,
full equivalence buys nothing today.

**Sequence all of CFM-26 after all of CFM-27.** Rejected. It designs the
new ABI while both engines coexist and the legacy taxonomy still distorts
the design, and it gates deletion on the expensive half of the work when
only the in-process half actually blocks it.

**Blessed in-tree plugins only (the Elm pole).** Drop the ABI ambition
for 0.x entirely. Rejected. Language-agnostic plugins behind a
transit-swappable SDK are a core product bet, and the seam that preserves
them is nearly free to keep; only the transit machinery is deferred. The
research note records the community cost of the closed pole.
