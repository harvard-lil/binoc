# The Vintage Audience: a Kept Benchmark for Metadata-Over-Data Reading

**Date:** 2026-06-22
**Status:** Accepted (benchmark landed; features deliberately deferred)

## Context

Binoc is tuned for one audience today: readers comparing two snapshots of the
*same* dataset who want every change explained — every edited cell, every added
row. Call them the **same-data** audience.

There is a second, latent audience: readers comparing two **vintages** — two
editions of the same published dataset (a yearly facilities register, a
re-released survey). A vintage reader cares about the *shape* of the data: did a
column appear, did a categorical vocabulary shift. They deliberately do **not**
want to read the bulk cell/row churn — for them it is noise, possibly millions
of rows of it.

We are not ready to serve the vintage audience. The same-data experience needs
to be bulletproof first, and inviting vintage feedback now would split our
attention and our optics before the core is solid. But we do need confidence
that the *engine* does not foreclose the vintage audience — that when we choose
to open that channel, it is a matter of configuration and plugins, not a
re-architecture. The risk we want to retire is a silent one: that some
assumption baked into the controller, the IR, or the correspondence engine
quietly assumes the same-data stance.

## Decision

Land a **kept benchmark vector**, `test-vectors/csv-vintage-benchmark`, that
exercises the vintage stance end to end and stays green, rather than building any
vintage feature. The vector is a two-CSV "published dataset" across two editions:
`facilities.csv` gains a `region` column and one row's `status` moves to a new
category value (`decommissioned`); `inspections.csv` changes only in its data
(edited scores, appended rows). A markdown `groups` config expresses the vintage
stance as significance — schema/vocabulary tags are the high-priority group, bulk
cell/row tags the low-priority group.

The benchmark confirms what already works: because significance is a renderer
concern (per [2026-03-09 renderer config](2026-03-09-renderer_config.md)) and
`classify_tags` promotes a node to the highest-priority group among its tags, the
schema-touching `facilities.csv` floats up to the structural section while the
pure-data `inspections.csv` sinks to the bulk section. That file-granularity
separation is the best vintage view binoc offers today, and it is pure
config — the type-ignorant controller (AGENTS rule 1) never participates.

The vector ships two renderings side by side: `expected-output/changelog.snap`
is the real, harness-checked engine output; `VINTAGE-IDEAL.md` is a hand-authored
target that is *not* harness-checked. The benchmark is kept green so the gap
between the two stays visible and measurable. The ideal names three gaps, each
reachable without touching the controller, the IR, or the correspondence engine:

1. **Within-node keep/drop.** A CSV's `region` addition and its `status` cell
   edit are edits on one node, so the renderer cannot surface the structural
   change while holding the cell back. The fix is a config-driven, edit-level
   keep/drop filter in the renderer. The data path already carries
   `EditProjection.visible`; today only writers set it. This is the smallest
   unlock and it lives entirely in the renderer.

2. **Vocabulary as a first-class change.** The `active -> decommissioned` shift
   is reported as an ordinary `binoc.cell-change`, not as "the `status`
   vocabulary gained a value." The fix is a plugin `EditListWriter` over
   `tabular_v1` that diffs the distinct-value set of each categorical column and
   emits a `binoc.vocabulary-change` edit — a plugin pack, exactly like the
   standard library is (AGENTS rule 2).

3. **Summary statistics over enumeration.** The bulk section enumerates every
   changed cell and added row; a vintage reader wants "4 -> 6 rows, 3 cells
   changed." The fix is the same plugin emitting an aggregate via
   `Edit::with_summary` or a dataset-level `GlobalClaim`. The seam already
   carries such facts — stdlib uses `with_summary` for binary string-diffs — but
   no rule emits a tabular roll-up yet.

The conclusion we are recording: the vintage-vs-same-data distinction is a
renderer-config + plugin-pack concern, which is the architecture's whole thesis.
The minimum to open the channel is one renderer-local filter plus one plugin
pack. No engine surgery. The channel is provably clear, and we are choosing not
to walk through it yet.

## Alternatives Considered

**Build the metadata-only filter and a sample statistics plugin now.** This is
the natural next step and the benchmark is designed to make it cheap. We are
deferring it for social and focus reasons, not technical ones: shipping a vintage
surface would invite vintage feedback before the same-data experience is solid.
The benchmark captures the design so the work is shovel-ready when we choose it.

**Write the rationale as prose only, with no vector.** A document can claim the
engine is ready; a passing benchmark proves it and keeps proving it. Without an
executable artifact, a future change could quietly regress the vintage stance
(e.g., bake a same-data assumption into a writer) with nothing to catch it.

**Make the benchmark aspirational — hand-author the ideal as the gold file.**
A snapshot that encodes output the engine does not produce would fail CI, forcing
us to either disable the test (dead weight) or special-case it (harness
complexity). Instead the harness-checked snapshot tracks reality and a separate,
unchecked `VINTAGE-IDEAL.md` holds the target. The benchmark stays honest and
green, and the gap is documented rather than asserted.

**Promote columns (and their vocabularies) to first-class IR nodes now.** This
would make within-node significance and vocabulary diffing fall out naturally,
but it is a substantial IR change in service of an audience we are deliberately
not yet serving. The benchmark shows the same outcomes are reachable with a
plugin writer emitting tagged edits, deferring any IR commitment until the
vintage audience is real.
