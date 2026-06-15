# Correspondence-First Migration Completion

**Date:** 2026-06-12
**Status:** Implemented

## Context

Binoc has completed the teardown of the original single-tree comparison engine.
That engine dispatched source item pairs to format-specific comparison plugins,
then rewrote the completed tree with optimization passes. The replacement is
the correspondence-first engine: two side trees, link proposals, edit-list
writers, strict-cost compaction, and final projection to the public changeset
tree.

The migration was tracked in `CORRLINK_MIGRATION_TRACKER.md` as CFM-01 through
CFM-58. The clean-room prototype crates under `prototypes/corrlink` and
`prototypes/corrlink-extras` were deleted after their durable findings were
ported into production tests and this ADR.

## Decision

The correspondence-first engine is the only live diff architecture. The legacy
comparison plugin traits, descriptors, ABI entrypoints, Python bridge classes,
stdlib comparison/rewrite packs, compatibility docs, prototype crates, and
legacy-named changeset provenance fields were removed.

Current plugin authoring vocabulary is:

- expand rules for containers and wrappers;
- parse rules for bytes-to-artifact conversion;
- pair rules for correspondences;
- edit-list writers for explaining links;
- compaction rules for strict-cost-decreasing rewrites;
- projection annotators for factual projection hints;
- renderers for output.

The retired comparator/transformer vocabulary remains only in historical ADRs
and research notes.

## Prototype Findings Preserved

The prototype validated the model with these outcomes:

- Settled links can suppress work beneath them generically. The settled scope is
  rule policy, not engine knowledge; production keeps
  `expand_renamed_unchanged_collections` as the conservative default and allows
  dataset config to choose the faster short-circuit posture.
- Late links replace recompare machinery. When a fuzzy or declared link forms
  after expansion, parse work that requires a link can fire afterward without
  re-entering any legacy merge/recompare phase.
- Links are the claims ledger. Container writers derive child adds/removes from
  link queries, so moved or copied children are not double-counted as dangling
  removals.
- Third-party rule injection is configuration-owned. A rule authored against the
  public API can improve projection and emit namespaced evidence or verbs
  without engine changes; unknown verbs flow through projection.
- Pair-rule priority is supplied by config order. Different rule orderings can
  produce different but deterministic winning evidence; this is an explicit
  user/configuration control point.
- Termination is structural. Pair proposals are idempotent, link revisions only
  move to strictly higher priority, and compaction rewrites must strictly reduce
  cost.
- Compaction power depends on naive edit vocabulary. The engine can compose
  multiple judgments in one edit list, but rule authors still need to choose
  edit verbs and parameters that preserve enough structure for later rules.
- Root-scope compaction remains intentionally deferred. Per-link edit-list
  rewrites are sufficient for the release arc; whole-run claims such as
  find/replace need a separate global strict-cost-decrease design.

These findings are now covered by production tests or carried as explicit
follow-up issues.

## Performance Baseline

The last recorded debug baseline on a synthetic 1,000-file / 20-directory
fixture was:

- legacy engine: 56.3 ms;
- correspondence engine: 146.6 ms;
- ratio: 2.60x.

The slower debug baseline was accepted for correctness and architectural
simplicity. Any optimization work should be measured and tracked under CFM-44,
with likely focus on parallel subtrees, dirty-set rescans, and analysis caches.

## Follow-Ups

- CFM-45: replace the stacked-table writer stopgap with a real parse rule and
  consolidate collection-writer helpers.
- CFM-42: add an MDL metric regression warning to the vector harness.
- CFM-43: finish declared container correspondences end to end.
- CFM-27b: define the stable ABI tier and SDK major bump for rule surfaces that
  have settled.
- CFM-44: pursue only measured performance follow-ups.
- CFM-41: post-release root-scope find/replace compaction.

## Alternatives Considered

**Keep compatibility shims for the old plugin names.** Rejected. The project is
pre-1.0 and explicitly not optimizing for backwards compatibility. Keeping
shims would make docs, generated references, and tests dual-engine aware.

**Keep prototype crates as executable documentation.** Rejected. Their durable
claims are now in production tests and this ADR; git history is the archive.

**Retain changeset fields named after the old phases.** Rejected. The fields did
not carry useful provenance after projection and kept obsolete vocabulary in the
public schema.
