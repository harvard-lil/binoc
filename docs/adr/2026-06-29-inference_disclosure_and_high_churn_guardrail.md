# Inference Is Allowed, But Always Disclosed and Overridable; the High-Churn Guardrail Is the Backstop

**Date:** 2026-06-29
**Status:** Accepted

## Context

A cluster of queued issues makes binoc *guess* something about a dataset rather
than be told it:

- **#92** — auto-detect a row key (or fall back to order-independent matching) so
  a re-sorted table isn't reported as wholesale change.
- **#93** — warn when a tabular diff is near-total churn (a likely re-sort or two
  disjoint cohorts), rather than emitting the per-cell story as fact.
- **#104 / #107 / #69** — sniff a header preamble, sniff text-vs-binary, sniff
  CSV dialect.

These share one failure mode, stated bluntly in #92: a user running plain
`binoc diff` on a re-sorted file "gets a confident multi-million-cell changelog
that is mostly fiction." CDC BRFSS positional-diffs to **37 million** changed
cells (~77% of the table) that are almost entirely a re-sort artifact; USDA
FoodData Central positional-diffs to **134 million**. The real edits are a few
hundred. The danger is not that binoc guesses — it is that a *wrong* guess is
rendered indistinguishably from declared truth.

Before any of these issues fan out in parallel, binoc needs a single stated
posture on (a) how aggressively it infers and (b) how an inference is disclosed,
so five agents don't each pick a different answer.

## Decision

Adopt one principle across every inference in the system:

> **binoc may infer, but every inference is (1) disclosed in the output as
> provenance, (2) overridable by explicit config, and (3) never permitted to
> render a high-confidence wrong story — the high-churn guardrail converts a
> failed correspondence into a useful signal instead of fiction.**

Three concrete commitments follow:

1. **Explicit config wins and is silent.** A path keyed/shaped/typed by config
   (the per-path model) is *declared*, not inferred — it carries no provenance
   noise. Config is always the no-guess escape hatch.

2. **Absent config, binoc may infer — and says so.** Auto-detected keys, sniffed
   dialects, content-sniffed projections produce a provenance note carried *in
   the IR* as a self-describing finding/annotation: "matched rows by inferred key
   `ent_num`"; "treated `northamerica` as text (content sniff, no extension)."
   Because provenance lives in the changeset, every renderer and the JSON output
   agree, and it is testable as an invariant. (It is emitted as a structured
   summary segment and rendered generically — see the renderer-ignorance ADR; it
   is *not* a renderer special-case.)

3. **The high-churn guardrail (#93) is both the safety valve and #92's
   success gate.** When a tabular diff's changed-cell (or changed-row) fraction
   exceeds a threshold, binoc does **not** publish the per-cell edit list as the
   change story. It reports "these two tables do not appear to correspond
   row-for-row" plus actionable suggestions (candidate key columns, the churn
   fraction). The same churn signal is what tells auto-keying (#92) whether the
   key it picked actually reduced churn — so **#92 and #93 are one work item**,
   not two parallel ones.

Net posture: **the guardrail is always on** (it is cheap and pure safety);
**auto-key detection is on by default but disclosed**, with the guardrail
catching its misfires. A user never has to opt in to be protected from fiction,
and never has to wonder whether a clean-looking diff was declared or guessed.

## Alternatives Considered

**Aggressive silent inference** — just do the smart thing and don't mention it.
Rejected: a silent guess is indistinguishable from declared truth, so when it is
wrong the error is invisible and the damage (a confident fictional changelog) is
exactly the showcase-killing outcome these issues exist to prevent.

**Literal-only — require explicit config for every non-default shape or key.**
Rejected: #92 measured the floor this sets — "without auto-detection, every such
dataset needs hand-tuned config," and plain `binoc diff` stays "mostly fiction."
Too high a bar for the showcase promise of useful output out of the box.

**Put the guardrail in the renderer as a display threshold.** Rejected: whether
two tables correspond is a *correspondence judgment*, not a presentation choice.
It must live in the IR so the JSON output and every renderer agree, so it can be
asserted as a test-vector invariant, and so it can drive the suggestion machinery
(#89's extract command, key candidates) rather than only suppressing bullets.

**Significance/confidence baked into the IR as a typed level.** Rejected per
AGENTS.md rules #3–#4: significance is a renderer/config concern mapped from
open-string tags. Provenance is a *fact* (this was inferred); how loudly to render
it stays a renderer/config decision.

## Open sub-decisions (flagged for review)

- The churn **threshold** and whether it is per-table, per-bundle, or both (#93
  names both re-sort *and* disjoint-cohort cases, which may want different cuts).
- Whether order-independent fallback matching (#92's second idea) ships this round
  or only auto-key-detection does, with sorted-fallback deferred.
- Exact provenance vocabulary (the tag/segment names for "inferred key",
  "content-sniffed type") — coordinate with the cell/tag work so the namespace is
  consistent.
