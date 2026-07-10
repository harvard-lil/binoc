---
audience: core developers weighing search-based compaction, rule composition, and verification
---

# Searching and verifying edit-list rewrites: the questions the first survey didn't ask

!!! note "Research note, not normative documentation"
    Background research, not a description of how binoc behaves. It is the
    sequel to [Edit lists, edit distance, and
    compaction](edit-list-compaction.md), which surveyed the *offline* prior art
    for Pass 2 — hardness bounds, delta codecs, MDL. This note takes up three
    questions that survey left open, each of which has since acquired empirical
    motivation from binoc's own shipped rule set. Citations were web-verified
    against primary sources in July 2026; load-bearing uncertainties are flagged
    inline. Poke holes in it.

*The first survey concluded that binoc "is not improvising; it is re-deriving the
standard answer" — greedy, cost-gated, never-backtracking rewrites over a naive
edit list. That conclusion holds. This note examines the three places where the
standard answer is load-bearing and binoc has not yet paid for it: the rule
pipeline's **order** is doing work nobody wrote down, the cost function is the
**only** acceptance criterion and it is gameable, and the correctness of a
compaction is currently checked by nothing but gold files. It also revisits the
project's e-graph rejection, which was decided on Pass 1's premises and has never
been evaluated on Pass 2's. The cross-artifact case is treated last and
deliberately lightly: it is a curiosity, not a commitment.*

## 1. The e-graph rejection was about a different phase

The [correspondence-first engine
ADR](../../adr/2026-06-12-correspondence_first_engine.md) rejects full equality
saturation in one sentence: *"nodes are fat, stateful records and rules perform
real I/O; saturation costs are prohibitive."*

Every clause of that is an argument about **Pass 1**. Side-tree nodes hold file
handles; expand and parse rules decompress archives and read bytes; saturating
over them would mean re-running I/O across an exponential space. The rejection is
correct and should stand.

None of it describes **Pass 2**. An `Edit` is a small, pure, serializable record:
a verb string and a JSON parameter bag. A link's edit list is bounded by the
artifact's size, and rewriting one performs no I/O — of the six shipped
`tabular_v1` compaction rules, most never touch `DataAccess` at all. The
combinatorial object a Pass 2 e-graph would saturate is a list of a few hundred
pure records, not a tree of open files.

So the honest status is: **binoc rejected e-graphs for Pass 1 with good reasons,
and has never evaluated them for Pass 2.** The same ADR half-concedes this, noting
it wants to keep "the e-graph *shape* only where it is cheap — minimum-cost choice
during projection." No minimum-cost choice exists at projection today.

This matters because the ADR's stated defense of the ordered pipeline is the
LLVM legacy-pass-manager argument: *keep judgment ordering curated.* That is a
real argument, and the counter-argument — that e-graphs exist precisely to dissolve
phase ordering ([egg](https://arxiv.org/abs/2004.03082), Willsey et al., POPL
2021) — was never joined, because the discussion was about a phase where
saturation was independently impossible.

## 2. Phase ordering is not hypothetical here; it has already bitten

The reason to reopen the question is not that e-graphs are fashionable. It is that
binoc's Pass 2, at **six rules**, already exhibits the pathology, and does so
undocumented. The shipped order in `binoc-stdlib/src/correspondence/mod.rs` is
`ColumnRename → ColumnReorder → TypeOnlyColumnChange → RowAlignment →
ReducedPrecision → RowAdditionConsolidation`, with no comment explaining it, while
at least four ordering facts are load-bearing:

- **`ColumnRename` must precede `RowAlignment`.** The rename scorer reads a hidden
  `tabular.row_alignment_basis` pseudo-edit that the writer emits; `RowAlignment`
  deletes that basis. Swap them and rename scoring silently degrades.
- **`ColumnReorder` after `ColumnRename` is itself a compromise, and the pair is
  lossy.** `ColumnReorder` consumes `set_headers` and emits a permutation over
  names common to *both* sides, so a renamed column is absent from the permutation
  and `ColumnRename`'s header cleanup — which only recognizes `set_headers` — can
  no longer fire. A reorder *and* a rename in one table degrade each other. This is
  precisely the "conjunctions must compose" property the engine overhaul was built
  to deliver.
- **`ReducedPrecision` must follow `RowAlignment`.** It was originally committed
  before it, and moved ten minutes later; the regression test
  (`row_alignment_precedes_reduced_precision_for_inserted_sentinels`) records that
  the old order manufactured a spurious `tabular.values_suppressed` claim out of
  inserted-row artifacts.
- **`RowAdditionConsolidation` must follow `RowAlignment`**, which synthesizes the
  consecutive `add_row` runs it consumes.

The first survey named the theory for exactly this: a terminating, **confluent**
rewrite system has a unique normal form independent of rule order (Church–Rosser),
and Knuth–Bendix reduces confluence to the joinability of finitely many critical
pairs. binoc's ordered pipeline is an admission that its rule set is not known to
be confluent — order *imposes* the determinism confluence would give for free. That
is a legitimate engineering choice. But it comes with an obligation the project has
not met: **if order is load-bearing, the constraints must be written down and
checked**, because the failure mode is not a crash but a quietly worse changelog.

Three responses are available, in increasing ambition:

1. **Declare and check.** Give `CompactionRule` optional `before`/`after`
   constraints, validate the configured order at engine-construction time, and
   document the basis side-channel as a protocol. Cheap; solves nothing about
   *lossiness*, only about accidental misordering.
2. **Engineer for confluence.** Identify which rule pairs provably commute (patch
   theory's commutation, [Mimram & Di Giusto](https://arxiv.org/abs/1311.3903)),
   author those freely, and keep a small ordered residue. This is the road the
   first survey called "not taken."
3. **Dissolve it.** Apply rules non-destructively into an e-graph over edit lists
   and extract the cheapest result. Order stops mattering by construction; the
   reorder+rename conjunction composes because both rewrites coexist in the
   e-graph until extraction picks the cheapest consistent whole.

Option 3 is the one that has never been costed, and Pass 2's purity is what makes
it costable.

## 3. Equivalence and induction are different problems, and conflating them is the trap

Discussions of "e-graphs for binoc" tend to slide between two things that need
different mathematics.

**Per-link compaction is an equivalence problem.** Given one edit list, find a
cheaper edit list that *means the same thing*. The candidate space is generated by
rewrite rules assumed sound; nothing is being learned. This is term rewriting, and
e-graphs are the canonical machinery: apply rewrites non-destructively, extract the
minimum-cost representative. `egg` is the reference implementation; `egglog`
([Zhang et al., PLDI 2023](https://arxiv.org/abs/2304.04332)) unifies that with
Datalog, which matters if rules ever need to query relational context.

**Cross-artifact compaction is an induction problem.** Given ten thousand edit
lists, find the *hypothesis* — "someone ran find-replace sneakers→shoes" — that
explains many at once. Nothing here is an equivalence-preserving rewrite, and the
generalization can be **wrong** in a way a sound rewrite cannot. The right
machinery is anti-unification (Plotkin, "A Note on Inductive Generalization,"
*Machine Intelligence* 5, 1970; independently Reynolds, same volume; see
[Cerna & Kutsia's survey](https://arxiv.org/abs/2302.00277)) and version-space
algebras — a VSA is a compact representation of *all hypotheses consistent with the
examples*, the inductive twin of an e-graph's "all terms equivalent to this one."

That inductive lineage is absent from the first survey and is the directly relevant
prior art for the named-inference claim:

- **[FlashMeta](https://doi.org/10.1145/2814270.2814310)** (Polozov & Gulwani,
  OOPSLA 2015) is the framework for programming-by-example synthesis, built on VSAs.
  (The version space *algebra* itself is Lau, Domingos & Weld, ICML 2000 / *Machine
  Learning* 2003; the underlying version-space concept is Mitchell 1982.)
- **[Refazer](https://arxiv.org/abs/1608.09000)** (Rolim et al., ICSE 2017) learns
  syntactic program transformations from example edits, on FlashMeta/PROSE.
- **[Getafix](https://arxiv.org/abs/1902.06111)** (Bader, Scott, Pradel & Chandra,
  OOPSLA 2019) is the closest thing to a spec for the sneakers→shoes rule:
  hierarchically cluster concrete edits, anti-unify each cluster into a
  parameterized pattern, rank candidates. It is a production Meta tool.
- **[Revisar](https://arxiv.org/abs/1803.03806)** (Rolim et al., SBES 2021 —
  preprint 2018) does the unsupervised version: mine repositories, cluster edits by
  anti-unification, emit generalized rewrite rules.

Two consequences follow.

First, **the first survey already contained the cross-artifact answer without
labelling it.** [KRIMP](https://doi.org/10.1007/s10618-010-0202-x) (Vreeken, van
Leeuwen & Siebes, *DMKD* 2011) admits a pattern to its code table only when the
pattern compresses the **whole database** of transactions — dataset-scope
compaction with an MDL acceptance test, which is exactly the shape wanted. So does
the smallest-grammar framing: a grammar rule earns its keep by being *reused* across
the corpus. The systems analogue is shared-dictionary compression — `zstd --train`
builds one dictionary across a corpus of small files precisely because per-file
compression cannot see the shared structure — and git's packfile delta selection
(window/depth heuristics, per `git-pack-objects` docs; an engineering heuristic,
not a paper) chooses which objects to delta against which, to amortize sharing.

Second, and more interestingly, **the two formalisms have already been combined.**
[babble](https://arxiv.org/abs/2212.04596) (Cao, Kunkel, Nandi, Willsey, Tatlock &
Polikarpova, POPL 2023) does *library learning* with e-graphs **plus**
anti-unification: it compresses a corpus of programs by discovering shared
abstractions, using e-graph saturation to see past syntactic accidents and
anti-unification to generalize. That is structurally the cross-artifact problem —
compress a corpus by finding one abstraction that many members instantiate — and it
is the single closest published neighbor to what binoc's named inference would
be.

**Negative result worth recording.** A targeted search found **no published work
applying e-graphs or equality saturation to structural diffing, edit-script
minimization, or diff/patch compaction.** The nearest neighbors are babble
(corpus compression, not diffing), KestRel and HEC (e-graphs to *align* or prove
equivalence of two programs — the closest thing to diffing, but producing proofs
rather than edit scripts), and [rewrite-rule inference via
saturation](https://arxiv.org/abs/2108.10436) (Nandi et al., OOPSLA 2021). Treat
this as "we looked and did not find one," not "none exists" — but the gap looks
real, and if binoc took the e-graph route for Pass 2 it would be doing something
that has not been written up.

## 4. If cost drives the search, cost has to be honest

Today the cost function is a tiebreak on a rewrite a rule already decided to
propose. Under any search-based extraction it becomes the *objective*, and every
imprecision becomes an exploit. binoc's `cost()` currently has three:

- **Verb names are free.** Cost is `1 + value_size(params)`; the verb string is
  uncharged, as are JSON object *keys*. A rule can launder description mass into a
  long verb name.
- **Strings are discounted 16:1** (`1 + len/16`), so a large string parameter is
  nearly free relative to the residual edits it replaces.
- **The hidden basis subsidises its own consumer.** The writer emits
  `tabular.row_alignment_basis` carrying row signatures and captured rows. Because
  cost is parameter-size-based, *any* rewrite that deletes the basis is
  automatically cheaper — `RowAlignment` can be accepted for deleting it, having
  explained nothing. The strict-decrease gate holds formally and means little on
  that path. (Worse, in the keyed variant the basis captures the entire right table
  and, above the 512-row compaction cutoff, is never consumed — so it persists into
  the published changeset.)

There is also a structural fact worth internalizing before anyone gets ambitious:
**extraction from an e-graph is polynomial under tree cost and NP-hard under
sharing-aware (DAG) cost**, where a shared subterm is paid for once. Under tree
cost, greedy bottom-up extraction is optimal because each e-class's choice is local;
sharing makes the choice global. See [Sun, Coward et al., "E-Graphs as Circuits, and
Optimal Extraction via Treewidth"](https://arxiv.org/abs/2408.17042); the practical
answers are ILP extractors and the
[extraction-gym](https://github.com/egraphs-good/extraction-gym) benchmark suite.

That result is not an obstacle so much as an explanation. Cross-artifact compaction
*is* sharing-aware cost — a `find_replace` verb is worth introducing only if its
cost is amortized across the many links that instantiate it. So the moment binoc
wants corpus-level compaction it inherits an NP-hard extraction problem, which is
the same conclusion the first survey reached from the smallest-grammar direction,
arriving from the other side. The response is the same: don't chase the optimum.

## 5. The missing safety net: verify by replay

The first survey stated the discipline plainly — *"binoc's distinguishing move must
be to verify by replay"* — citing OpenRefine's replayable recipes and SQLite's
session extension. **binoc does not do this.** Compaction acceptance tests
`cost(rewritten) < cost(edits)` and nothing else. A cheaper *false* rewrite is
accepted, and the only thing between it and a published changelog is the gold-file
corpus.

This is the highest-value item in the area and it is independent of every other
question here. Applying a compacted `tabular_v1` edit list to the left artifact and
checking that it reproduces the right artifact is concrete, cheap, and sound.

It has a precise name in the compiler literature: **translation validation**
(Pnueli, Siegel & Singerman, TACAS 1998) — rather than proving a transformation
correct for all inputs, check that *this* run of it produced a correct output.
[Alive2](https://doi.org/10.1145/3453483.3454030) (Lopes, Lee, Hur, Liu & Regehr,
PLDI 2021) applies it to LLVM's peephole optimizations, catching unsound rewrites
that survived years of review. binoc's version is strictly easier: no SMT solver is
needed, because the ground truth — the right-hand artifact — is sitting on disk. The
check is per-instance rather than universal, which is exactly what an
open-vocabulary, third-party-extensible rule set can actually support.

Three payoffs, in order of importance:

1. **It closes the soundness hole.** A rule that produces a cheaper, wrong edit
   list is caught by construction, not by a reviewer noticing a snapshot moved.
2. **It licenses ambition.** Greedy search, saturation, inductive generalization —
   all become safe to attempt, because anything accepted is checked against ground
   truth. This is what makes the e-graph and PBE directions responsible rather than
   reckless. It is also the obligation `egg` itself does *not* discharge: there,
   rewrite-rule soundness is on the author's honor. (`egg` can emit
   *explanations* — proofs that two terms are equal under the rule set — which
   shrink the trusted base but still presuppose the rules. Whether `egglog` exposes
   the same is **UNVERIFIED**.)
3. **It disciplines the vocabulary.** A verb that cannot be replayed is a verb that
   does not fully describe its own change — which is the same defect the
   [renderer-ignorance ADR](../../adr/2026-06-29-renderer_ecosystem_ignorance.md)
   attacks from the presentation side.

Two caveats. Replay verifies *what*, never *why*: it can confirm that a
find-replace reproduces the right artifact, not that a human ran one. And replay
requires the edit list to be **executable**, which compaction can destroy. SQLite's
session extension is the honest precedent: its *patchset* is a lossy compaction of a
*changeset* (an UPDATE keeps only changed fields; a DELETE keeps only the primary
key), and `sqlite3changeset_invert()` returns `SQLITE_CORRUPT` when handed one.
Compaction trades invertibility for size. binoc should decide, per verb, which side
of that line it sits on — and a replay harness is what forces the decision to be
explicit.

## 6. Cross-artifact compaction: a stretch goal, and an honest look at its price

The dataset-wide find-replace has had outsized gravity in binoc's planning
documents relative to its evidence base. Two things are worth separating.

**Is it wanted?** Unclear. The vision documents treat "someone ran a find-replace
from sneakers to shoes" as a flagship, but no user research establishes that
archivists and stewards need cross-artifact process inference more than they need
a bulletproof per-artifact changelog. Treat it as a stretch goal, not a
scheduled feature. The [vintage-audience
ADR](../../adr/2026-06-22-vintage_audience_and_metadata_only_benchmark.md) is
instructive here: its three gaps are all *within-artifact*, and it concludes the
whole audience is reachable with one renderer filter plus one plugin pack. Nothing
in it wants cross-artifact inference.

**What would it cost?** More than it looks, and the reason is structural rather
than algorithmic. `CompactionRule::rewrite(&self, ctx: &LinkCtx, edits: &[Edit],
data)` takes one link's edits and returns one link's edits; its `format() -> None`
escape hatch means "across artifact formats *within* a link," not across links.
Meanwhile the only dataset-scope output channel is `GlobalClaim`, produced by
`PairRule::final_claims` — and in the driver, claims are collected **before**
`build_edit_lists` runs. **A `GlobalClaim` therefore cannot, today, be a function of
any edit list.** The rule family that can see all edit lists is per-link by
signature; the family that is dataset-scope by design executes before edit lists
exist.

So cross-artifact compaction is not merely unimplemented — the phase structure
forbids expressing it. That the partition-identity split/merge claim works is not
counter-evidence: split/merge is a fact about *links and node identities*, which
`final_claims` can see. It does not generalize to inferences over edits, and its
success may have obscured the gap. If this ever becomes worth pursuing, it needs
a new dataset-scope phase after `build_edit_lists`, not an ordinary compaction
rule.

The unlock, if it is ever wanted, is a **phase, not an algorithm**: a dataset-scope
stage after `build_edit_lists` that reads every link's edit list, may emit
`GlobalClaim`s, and may rewrite per-link residuals, accepted iff total description
cost across the corpus strictly decreases. Whether its internals are greedy
KRIMP-style pattern mining, babble-style e-graph + anti-unification, or VSA
synthesis is a replaceable detail behind that seam. The ordering follows: get the
phase and a corpus-level cost function right; the search algorithm is swappable.

The recommendation this note makes is to **hold it as a stretch goal and stop
letting it steer**. It is a genuinely interesting research direction with a clean
formal home (babble is nearly a blueprint), and it is also a heavy change to the
engine's phase structure in service of a user need nobody has demonstrated. Those
two facts are compatible. What is not defensible is planning per-link work around a
cross-link feature that the type system does not admit and the audience has not
asked for.

## 7. What to steal, what to fear, what would settle it

**Steal:**

1. **Translation validation, per instance** (Pnueli et al.; Alive2). Replay the
   compacted edit list against the right artifact. Cheap, sound, no solver.
2. **Anti-unification as the generalization primitive** (Plotkin; Getafix; Revisar)
   if inductive rules are ever attempted — not ad-hoc pattern matching.
3. **Sharing-aware cost as the definition of "corpus-level compaction"** (KRIMP;
   smallest grammar; zstd dictionaries), and the knowledge that its extraction is
   NP-hard, so greedy is again the answer.
4. **`egg`'s explanations** as a model for making a rewrite auditable, even if the
   engine never adopts e-graphs.
5. **The changeset/patchset distinction** (SQLite session): decide per verb whether
   compaction may destroy invertibility.

**Fear:**

1. **Cost-only acceptance.** It is the current design, and it admits cheap lies.
   Everything else in this note is downstream of fixing it.
2. **A cost function that becomes an objective without being audited first.** Verb
   names free, strings 16:1, and a writer-emitted subsidy that pays its own
   consumer. Search would exploit all three.
3. **Confusing equivalence with induction.** An e-graph cannot tell you a
   generalization is *true*; only replay (or a human) can.
4. **The first survey's loudest warning, still unmet:** three-way composition of
   opaque verbs is where correctness dies, and there is still no property-based
   fuzzer over the composition laws. The `fuzz-vectors` skill fuzzes *scenarios*,
   not the algebra.

**What would settle it — three experiments, in dependency order:**

1. **The replay spike.** Implement replay verification for the existing
   `tabular_v1` compaction rules and run it across the vector corpus. If it passes
   everywhere, a safety net was bought for the price of a harness. If it fails
   anywhere — and the reorder+rename lossiness suggests it might — a real bug has
   been found and the harness has paid for itself immediately.
2. **The cost audit.** Charge verb length and object keys; cap or relocate the
   `row_alignment_basis` side channel; re-run the corpus and diff the accepted-rule
   statistics. This tells you how much of today's compaction is earned and how much
   is subsidised.
3. **The saturation bake-off.** Only if 1 and 2 land: express the six `tabular_v1`
   rules as non-destructive rewrites over an edit-list e-graph, extract under tree
   cost, and compare output on the vectors where conjunctions currently degrade
   (reorder+rename, add+remove mid-table). The measurable question is whether
   saturation recovers conjunctions the ordered pipeline loses. If it does not, the
   ADR's original instinct is vindicated for the per-link case and the matter is
   closed with evidence rather than by inheritance.

**The one-paragraph synthesis.** binoc's Pass 2 is a destructive, ordered rewrite
system whose order is undocumented and already lossy on a two-rule conjunction, whose
acceptance test is a gameable cost proxy, and whose correctness rests on gold files.
Each of those is fixable independently, and the fixes are ordered: verify by replay,
then make cost honest, then — and only then — ask whether a cost-driven search
(equality saturation over pure edit records, a direction the project's own rejection
never actually evaluated) recovers the compositions the curated pipeline drops. The
inductive, cross-artifact cousin of that question has a clean formal home in the
anti-unification and library-learning literature, and no home at all in binoc's
current phase structure; it is worth reading about and not worth planning around.
