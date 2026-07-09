---
audience: core developers surveying prior art for the edit-list-compaction engine
---

# Edit lists, edit distance, and compaction: prior art

!!! note "Research note, not normative documentation"
    This is a background research survey, not a description of how binoc
    currently behaves. It maps the fields that have already wrestled with the
    problem the [correspondence-first engine
    ADR](../../adr/2026-06-12-correspondence_first_engine.md) takes on in its
    Pass 2: representing edits as data, deriving the edit script from a
    correspondence rather than storing it, and *adding richer verbs to shorten
    the description*. It is a focused companion to the broader [prior art and
    architecture precedents](precedents.md) survey — where that document and
    this one overlap (GumTree, difftastic, JSON Patch, daff, Darcs/Pijul,
    e-graphs, the MLIR phase-ordering arc), this one looks through the
    edit-list/edit-distance lens specifically. Facts were web-verified against
    primary sources in June 2026; load-bearing uncertainties are flagged
    inline. Poke holes in it. A sequel, [Searching and verifying edit-list
    rewrites](edit-list-search-and-verification.md), takes up three questions
    this survey leaves open: rule ordering, a cost function that is the sole
    acceptance test, and verification by replay.

*The ADR's Pass 2 is a wager that the field has tested many times under many
names. Every correspondence link gets an **edit list** in open-vocabulary
verbs; a naive writer emits the dumbest true list (per-cell, per-line,
"contents differ"); **compaction rules rewrite the list into shorter
equivalents by introducing richer verbs** ("columns reordered by σ",
"find-replace sneakers→shoes"), accepted only when a structural
description-cost — verbs + parameter sizes + residual edits — strictly
decreases. Five distinct research traditions have hit pieces of this. The
single most important finding: **the moment you add move/copy/block verbs, the
optimal edit script becomes NP-hard**, and every practical system responds the
same way binoc does — cheap unambiguous anchors first, then greedy,
cost-gated, never-backtracking rewrites. binoc is not improvising; it is
re-deriving the standard answer.*

## The hardness spine — why richer verbs force greedy rewriting

This is the backbone argument and it deserves to be stated precisely, because
it is the formal justification for the entire compaction design.

- **The minimal verb set is exactly what makes diffing tractable.**
  Levenshtein distance (Levenshtein 1966) over {insert, delete, substitute}
  is computed exactly by the Wagner–Fischer dynamic program (Wagner & Fischer,
  "The string-to-string correction problem," *JACM* 1974) in `O(mn)`, made
  linear-*space* by [Hirschberg
  1975](https://dl.acm.org/doi/10.1145/360825.360861). The DP works *because
  each operation is local and position-independent* — every edit acts on one
  position with an additive cost. binoc's "dumbest true edit list" is precisely
  a Levenshtein script: it always exists and always has a defined cost. Treat
  {insert, delete} as the residual fallback the cost function falls back to.
- **Adding a *bounded, local* verb stays tractable.**
  [Damerau–Levenshtein](https://en.wikipedia.org/wiki/Damerau%E2%80%93Levenshtein_distance)
  (Damerau 1964) adds exactly one verb — adjacent transposition — and remains
  polynomial. This is the encouraging case and the precise boundary: a new verb
  keeps the problem easy *iff* its effect is local and bounded and you forbid
  overlapping application. (Caveat worth stating carefully: the cheap
  "optimal-string-alignment" DP only stays correct under a no-region-edited-twice
  restriction, and it stops being a metric — people routinely conflate the
  restricted and unrestricted variants.)
- **Adding *unbounded* move/copy/block verbs makes the optimum NP-hard.** Two
  independent 2002 proofs, worth citing together: Shapira & Storer, ["Edit
  distance with move
  operations"](https://www.cs.brandeis.edu/~shapird/publications/cpm2002.pdf)
  (CPM 2002 / *J. Discrete Algorithms* 2007) prove {insert, delete,
  arbitrary-substring move} is NP-complete and give an `O(log n)`-approximation;
  Cormode & Muthukrishnan, ["The string edit distance matching problem with
  moves"](https://dimacs.rutgers.edu/~graham/pubs/papers/editmoves.pdf) (SODA
  2002 / *TALG* 2007) prove the same set NP-hard with an almost-linear
  approximation. The contrast they draw is the key fact: *single-character*
  moves stay polynomial; it is the *arbitrary-span* move that flips it. Shapira
  & Storer's later "multiple block operations" work extends this — essentially
  every richer block verb (move, copy, reversal) pushes you to NP-hard.
- **The MDL restatement is NP-hard too, by a different reduction.** binoc's
  cost objective is, formally, a mini smallest-grammar problem: each richer verb
  is a rule that compresses the edit list. [The smallest grammar
  problem](http://web.cs.ucla.edu/~sahai/work/web/2005%20Publications/TransOnInfoTheory2005.pdf)
  (Charikar et al., *IEEE TIT* 2005; hardness from Storer & Szymanski 1982) is
  NP-hard and hard to approximate below a constant factor. This independently
  re-confirms the "don't seek the optimum" conclusion — and it tells you what to
  steal: grammar compressors (RePair, Sequitur) all use *greedy "replace the
  most-compressive repeated pattern, repeat until no net gain,"* which is the
  exact shape of binoc's greedy strictly-cost-decreasing loop.

**The transferable conclusion.** binoc's architecture is correct in principle:
once rich verbs are in the vocabulary, *do not search for the global minimum* —
it is provably intractable by two independent routes. Use greedy,
strictly-cost-decreasing rewrites over an already-valid Levenshtein-style list,
each a local move in description-cost space accepted only if it strictly lowers
the MDL cost. This is the posture Tichy, Heckel, and the 2002 approximation
results all adopt. The honest one-line version for reviewers: *"the optimization
version is NP-hard, so the field uses greedy/approximate cost-reducing methods;
binoc does the same."*

## The diff and delta engineering lineage — what the practitioners do

The byte/line tradition has spent thirty years answering "what is the smallest
set of instructions that turns A into B?" — and converged on patterns binoc
should reuse rather than reinvent.

**Diff algorithms: anchor on the unambiguous, then work the gaps.**

- Classic `diff` computes a *longest common subsequence*, not edit distance
  directly. [Hunt & McIlroy
  1976](https://www.cs.dartmouth.edu/~doug/diff.pdf) made it fast by anchoring
  on *unique* lines first and only aligning the gaps — the original
  exact-before-fuzzy move, and the original "performance degrades exactly where
  matches are non-unique" warning (the same place compaction verbs are hardest
  to assign).
- Myers' [`O(ND)` algorithm](https://www.fabiensanglard.net/git_code_review/diff.php)
  (1986), git's default, is *output-sensitive*: effort scales with the edit
  distance `D`, near-linear when little changed. The lesson for a compaction
  engine that runs on *every* correspondence: make cost proportional to the
  amount of change, not the input size — a tiny diff should cost almost nothing
  to compact.
- [Patience diff](https://blog.jcoglan.com/2017/09/19/the-patience-diff-algorithm/)
  (Bram Cohen) and histogram diff (the git/JGit refinement — do *not* repeat the
  common "built by Bram Cohen" attribution for histogram; its lineage is
  git-internal) generalize the anchor idea from "occurs exactly once" to "occurs
  *rarely*", ranking candidate correspondences by inverse frequency. The
  peer-reviewed ["How different are different diff algorithms in
  Git?"](https://arxiv.org/pdf/1902.02467) (Nugroho et al., *EMSE* 2020)
  recommends histogram for code. The deeper lesson is binoc's whole thesis:
  **minimal-cost ≠ best explanation.** Myers gives a provably shortest script
  that can read worse than a slightly longer one anchored on salient uniques.
  Tie-breaking and anchor salience are first-class, not afterthoughts.
- Move detection is a *separate pass layered on a plain diff*, not baked into
  alignment: Heckel's linear-time algorithm (CACM 1978) recognizes moved blocks
  from out-of-order unique-line correspondences, and git's `--color-moved`
  (Git 2.17, 2018) post-processes a finished diff, *refusing* to call anything
  under ~20 alphanumeric characters a move. binoc's MDL acceptance test is the
  principled generalization of that ad-hoc 20-char threshold: a rewrite must pay
  for itself, and trivially small moves won't.

**Delta encoding: a tiny COPY/ADD verb core, justified by encoded length.**

- [VCDIFF (RFC 3284)](https://www.rfc-editor.org/rfc/rfc3284.html), the
  standardized delta format, has exactly three verbs — `COPY` a range, `ADD` a
  literal, `RUN` a repeated byte — and `RUN` exists *purely because it is cheaper
  to describe* than the equivalent `ADD`. That is the MDL move in miniature: add
  a verb only when it shortens descriptions more than it costs. VCDIFF also
  separates verbs from parameters into distinct streams, which is what makes the
  "verb + parameter size" accounting cheap. Independently, git packfiles,
  Mercurial revlogs, Subversion svndiff, and [Fossil's delta
  format](https://fossil-scm.org/home/doc/tip/www/delta_encoder_algorithm.wiki)
  *all* reduce to {COPY range, INSERT literal}. Four systems converging on the
  same two verbs is strong evidence that is the irreducible core — and that
  binoc's richer verbs should be justified as compaction shorthands *over* that
  core. Mercurial's "store the delta only if it is smaller than the full text"
  is binoc's strictly-decreasing acceptance test already shipping in production;
  Subversion's skip-deltas are the cautionary case that *reconstruction* cost
  and *description* cost are different objectives — be explicit about which one
  binoc optimizes.
- [bsdiff](https://www.daemonology.net/bsdiff/) (Percival 2003) tolerates
  *approximate* matches: it copies a near-matching region and encodes a bytewise
  residual that is mostly zeros (and therefore compresses away). The lesson:
  distinguish exact `COPY` from *near-COPY-with-residual*, and price the residual
  as a small separate parameter rather than shattering a near-match into many
  literal edits — exactly binoc's "verbs + residual edits" split.
- **[Courgette](https://www.chromium.org/developers/design-documents/software-updates-courgette/)
  (Chrome updater) is the headline analogy.** Small source changes cause huge
  binary diffs because internal pointers shift en masse, so Courgette
  *transforms the representation before diffing*: it disassembles, references
  pointers by symbol-table index rather than raw address, and **reorders the
  symbol table specifically to maximize long common substrings** — i.e. it
  rewrites the representation to make the delta small, then diffs in that space
  and inverts. Verified numbers on one Chrome update: 10.4 MB full → 704 KB
  bsdiff → **79 KB Courgette**. This *is* binoc's parse-then-compact bet in
  another domain: don't diff the surface form; parse into a representation where
  corresponding things line up, then describe the small difference. (Flag:
  Courgette is x86-era and partly superseded by its successor *zucchini* for
  modern architectures — cite carefully if claiming current status.)
- The rsync rolling checksum (Tridgell & Mackerras 1996) and content-defined
  chunking ([FastCDC](https://www.usenix.org/conference/atc16/technical-sessions/presentation/xia),
  USENIX ATC 2016) are the mechanism for *finding* COPY ranges cheaply, and they
  make explicit that finding matches (a hashing problem) and encoding them (the
  cost objective) are separable concerns — supporting binoc's split of cheap
  candidate-correspondence detection from MDL-priced verb selection.

## Derive the script from a mapping — don't store it

The ADR's structural move is that `add`/`remove`/`move`/`modify` are *queries*
over a stored correspondence, not state maintained by tree surgery. Three
literatures converged on the same idea, and the most recent is the strongest
endorsement.

- **GumTree** (Falleri et al., ASE 2014; [2024
  follow-up](https://hal.science/hal-04855170v1/document)) computes a *mapping*
  between two ASTs, then *derives* an edit script — including `MOVE` — as a
  readout of the mapping. The mapping is primary; the script is a view. This is
  binoc's architecture stated in the source-code-diffing literature, and it
  shows `MOVE` can be made first-class in a script even though optimal
  move-diffing is intractable — by deriving moves from a heuristic mapping
  rather than solving for them. ([precedents.md](precedents.md) cites GumTree for
  the nodes-vs-tags tension; here the angle is edit-script-as-derived-view.)
- **Sequence CRDTs** reached the same place from the concurrency side. The arc:
  persist CRDT metadata (RGA, Logoot, LSEQ, Yjs/YATA) → persist transformed ops
  (OT) → **persist only the original event graph and derive everything by
  replay**. That last step is [Eg-walker
  ("event-graph-walker")](https://arxiv.org/html/2409.14252v1) (Gentle &
  Kleppmann, EuroSys 2025), which stores an append-only DAG of original edits and
  computes state as a *pure function* `replay(G)` — building a transient internal
  CRDT during replay and *discarding it* when done, reclaiming memory at
  "critical versions" where history is linear. It is the strongest available
  precedent for "the mapping/history is the artifact; the edit script is derived"
  — and it shows derivation can beat surgical maintenance on *both* memory and
  speed (their figures: peak memory near a CRDT's steady state; merges that take
  an hour under OT in ~24 ms). The transferable optimizations: keep per-link edit
  lists and partial compactions *transient*, persist only the mapping plus rules,
  and look for binoc's analogue of critical versions (points where a link is
  settled and compaction needn't re-replay). Automerge's git-like change DAG sits
  on the same "history primary, views derived" side, with the same honest cost
  (history size, load time). Stable operation *identity* is the load-bearing idea
  throughout: anchor edit verbs to stable identities of corresponded elements,
  not to volatile indices, so a derived script recomputes deterministically.

## Composing independently-authored rewrite rules — the central bet

The ADR's prototype must demonstrate "robust, general composability of
independently authored rules." That is the hardest claim, and four literatures
have formal or hard-won things to say about exactly it. The throughline: *"does
the order of two rules matter?"* is not a vibe — it is a decidable, named
property, and the systems that pretended otherwise shipped bugs for two decades.

- **Term rewriting gives the rigorous version of the question.** A system that
  is *terminating* and *confluent* yields a unique normal form **independent of
  rule-application order** (Church–Rosser). Knuth & Bendix (1970) made this
  mechanical: the finite set of *critical pairs* (minimal overlaps between two
  rules' left-hand sides) determines confluence — a terminating system is
  confluent iff all critical pairs are joinable. So "can these two rules be
  authored independently of order?" reduces to "are their critical pairs
  joinable?" binoc's deliberate choice of an *ordered pipeline rather than a
  fixpoint solver* (the ADR's "LLVM/MLIR lesson") is precisely an admission that
  the rule set is *not* known to be confluent: order is used to *impose* the
  determinism confluence would otherwise give for free. The transferable
  discipline: for any rule pair that provably commutes, their order cannot
  matter and they are safe to author independently; the ordered pipeline only
  needs to encode judgment for the genuinely non-confluent overlaps.
  Knuth–Bendix completion — *trying to add rules until the system is confluent*,
  and its ability to fail or diverge — is itself evidence for why binoc
  reasonably declined to require global confluence. The road not taken,
  worth naming honestly, is to engineer the rules into a convergent system with
  canonical normal forms; that buys order-independence and testability but
  constrains how judgment-laden each rule can be.
- **Patch theory gives the complementary "when can order *not* matter."** Darcs's
  theory of patches makes *commutation* a first-class partial operation: two
  patches commute iff they are independent, and that is exactly what determines
  clean reorderability. [Mimram & Di Giusto, "A Categorical Theory of
  Patches"](https://arxiv.org/abs/1311.3903) (MFPS 2013, the basis for Pijul)
  models files as objects and patches as arrows with merge as a *pushout*, and
  states the slogan binoc wants: *in a category where all patches commute,
  pushouts are found by applying patches in any order*. binoc's rewrites are
  themselves patches-on-an-edit-list; this gives a vocabulary to *classify* rules
  into an order-free core plus a small ordered residue of genuinely non-commuting
  rules. (Homotopical patch theory, Angiuli et al., ICFP 2014, is the elegant but
  heavyweight HoTT version — cite as conceptual grounding, not as something to
  build. [precedents.md](precedents.md) cites Darcs/Pijul for "do these
  operations commute?" as a rule-ordering test; this is the same point
  with the categorical machinery.)
- **Equality saturation is the maximalist alternative the ADR explicitly
  rejected — frame it precisely as a trade.** [egg](https://arxiv.org/abs/2004.03082)
  (Willsey et al., POPL 2021) applies rewrites *non-destructively* into an
  e-graph until saturation, then *extracts the cheapest* term against a cost
  function — dissolving the phase-ordering problem (NP-hard; the LLVM `-O3`
  60-pass lineage) rather than solving it. binoc keeps the *good half* —
  minimum-cost selection against a cost function — while discarding the expensive
  half (materializing an exponential space of equivalent edit lists, e-graph
  memory blowup). The honest framing: binoc's greedy ordered pipeline is a
  *destructive* rewrite system that re-incurs phase-ordering on purpose, betting
  careful pass ordering is cheaper than saturation for its domain, and recovering
  equality saturation's "pick the cheapest" only locally — each rewrite accepted
  iff it strictly lowers cost — rather than via global extraction. ([precedents.md](precedents.md)
  records the e-graph rejection from the controller side; this adds the
  phase-ordering and cost-extraction detail.)
- **Operational Transformation is the cautionary tale — and a built example of
  binoc's open-vocabulary verbs.** OT transforms a concurrent operation so it
  applies correctly after another; the convergence conditions are TP1 (two-op
  diamond) and TP2 (three-op path-independence). The load-bearing finding:
  [Randolph et al., "On Consistency of the Operational Transformation
  Approach"](https://arxiv.org/abs/1302.3292) (2013) showed the canonical IT
  functions across the literature (Ellis 1989, Ressel 1996, Sun 1998, Suleiman
  1998, Imine 2003) are *all* incorrect, and proved that for the classical
  insert/delete signature **TP1 and TP2 cannot both be satisfied** — an
  impossibility, not just a difficulty. The escape that made OT shippable
  (Jupiter 1995 → Google Wave) was to *centralize the order*: a single
  server-defined sequence needs only the pairwise TP1, sidestepping the entire
  TP2 bug class. The two transfers for binoc: (1) the two-rule case is the easy
  case; the bugs live in *three-way* composition, and if rule order is *not*
  canonical, binoc inherits the TP2 problem where hand-written transforms are
  almost always wrong. A **canonical rule ordering is the single highest-value
  decision** — it is exactly what binoc's ordered pipeline already buys. (2) If
  verbs carry too little context, a correct order-independent compaction can be
  *provably impossible*; OT's fix was richer operation signatures (identifiers,
  priorities), echoing the CRDT "stable identity" lesson. Concretely, ShareDB's
  [`json1` OT type](https://github.com/ottypes/json1) is the closest existing
  analogue to binoc's open, namespaced verbs: it passes *embedded sub-edits it
  does not understand* through transform/compose, recursing into registered
  subtypes only when forced — proof the pattern is buildable. But its own author
  documents that the fuzzer still finds convergence bugs after ~a million
  iterations, while the deliberately-less-ambitious, central-server
  [Etherpad Easysync](https://github.com/ether/etherpad-lite/wiki/Changeset-Library)
  (with its attribute-pool interning of open vocabularies — a pattern worth
  stealing) is considered solid. The blunt lesson: **budget for a property-based
  fuzzer of the composition/transform laws from day one** (round-trip + two-rule
  diamond + three-rule path-independence). Human reasoning about three-way
  composition of opaque verbs is exactly what failed across the whole OT
  literature.

A final OT warning binoc should internalize: **convergence is not correctness.**
The CCI model (Sun et al., *TOCHI* 1998) separates *convergence* (everyone ends
at the same state) from *intention preservation* (everyone ends at the state the
operations *meant*), and proves intention preservation has no general
serialization fix. A rule pipeline that deterministically converges to *a*
compacted edit list is not the same as one that preserves what each rewrite
*meant* — imposing a canonical order to dodge TP2 can silently violate a
lower-priority rule's intent. Make "what must this verb still mean after another
verb composed ahead of it?" an explicit, tested property of the vocabulary.

## Inferring named operations — and verifying them by replay

binoc's flagship compaction ("these 10,000 cell edits *are* one find-replace
sneakers→shoes") is operation inference: recovering the high-level process
behind low-level edits. The software-engineering field has done this for a
decade, and its single most useful contribution is the honesty about how often
it is wrong.

- **The forward direction exists and is production-grade.**
  [Coccinelle/SmPL](https://coccinelle.gitlabpages.inria.fr/website/) (Lawall &
  Muller; actively maintained, v1.3.0 Nov 2024, standard in the Linux kernel)
  lets you write one *semantic patch* — a before→after pattern with
  metavariables — that rewrites thousands of sites a line diff never could.
  That validates the granularity binoc's compaction rules should emit: a single
  parameterized verb, not a pile of literal edits.
- **The inference direction is the direct ancestor — and it is imperfect.**
  spdiff (Andersen & Lawall, ASE 2012) *infers* a semantic patch from concrete
  (before, after) examples; its maintained successor
  [SPINFER](https://www.usenix.org/system/files/atc20-serrano.pdf) (USENIX ATC
  2020) infers Coccinelle rules from kernel changes and reports **86% precision,
  69% recall**. That number is the anchor: automated recovery of "the operation
  behind the edits" is genuinely error-prone even from a strong research group.
  The refactoring miners tell the same story at higher accuracy but with the same
  framing — [RefactoringMiner](https://github.com/tsantalis/RefactoringMiner)
  (Tsantalis et al., ICSE 2018 / TSE 2020) detects 40+ named refactorings
  (Rename Method, Extract Method) from commit diffs, and RefDiff (TSE 2020) does
  it multi-language over a normalized Code Structure Tree — both reported in
  *precision/recall*, not truth, and both benchmark-bound. ChangeDistiller
  (Fluri et al., *TSE* 2007) is the earliest clean statement of "map raw AST
  edits to named, significance-weighted change *categories*," which parallels
  binoc's verbs *and* its significance classification.
- **So binoc's distinguishing move must be to verify by replay.** Every
  inference technique above is fallible; the disciplined response is to *close
  the loop*. When a rule claims "these edits = find-replace sneakers→shoes,"
  binoc can execute that operation on the old side and assert it reproduces the
  new side. [OpenRefine](https://openrefine.org/) proves operations can be made
  deterministically replayable (its history extracts as a JSON recipe and
  re-applies to other data) — and offers a guardrail binoc should copy: it
  *refuses to generalize one-off single-cell edits* into recipe operations.
  Don't promote idiosyncratic edits to "operations."
- **The edit list deserves to be a first-class algebraic object — and there is a
  production existence proof.** SQLite's
  [session extension](https://sqlite.org/sessionintro.html) records row-level
  changes as a *changeset* that can be serialized, **inverted**
  (`sqlite3changeset_invert`), applied with programmable conflict resolution, and
  **rebased** (`sqlite3rebaser`) onto another applied changeset. Its *patchset*
  is a literally-lossy compaction of a changeset (DELETEs keep only the primary
  key; UPDATEs drop unchanged fields) — a direct analogue of binoc's raw edit
  list → compacted changeset, *including the honest tradeoff that compaction
  loses invertibility and weakens conflict detection*. This is the design pattern
  to point at when arguing binoc's edit lists should be first-class objects with
  an algebra rather than ephemeral diff output: get the object model right and
  undo (invert) and composition (rebase) come nearly for free.

## The cost function — minimum description length, named

binoc's acceptance criterion is not an aesthetic preference; it is textbook
two-part MDL, and saying so earns the strongest available justification for
"shorter is better."

- **MDL** (Rissanen, "Modeling by shortest data description," *Automatica*
  1978; Grünwald, *The Minimum Description Length Principle*, MIT Press 2007)
  selects the model minimizing `L(model) + L(data | model)`. Map directly:
  `L(model)` = the verbs and their parameters; `L(data | model)` = the residual
  low-level edits. binoc's "strictly-decreasing description cost" *is* the MDL
  objective, and it comes with a built-in Occam guarantee against overfitting the
  edit list. The honest caveat: binoc's cost is a hand-designed *structural
  proxy* (verb counts, parameter sizes, residual counts), not a true
  bit-length code, so it inherits MDL's spirit without its formal optimality.
- **Kolmogorov complexity** (Li & Vitányi, *An Introduction to Kolmogorov
  Complexity*, 4th ed. 2019) is the uncomputable ideal behind "shortest
  description" — the length of the shortest program producing the object. Its
  uncomputability is itself the justification for a greedy heuristic: you
  *cannot* compute the shortest edit list, so you approximate it with cheap
  accepted rewrites. The "shortest description of a diff" is conceptually the
  conditional complexity `K(new | old)`, and every delta codec in the section
  above is a practical, restricted-language upper bound on it.
- **Normalized Compression Distance** (Cilibrasi & Vitányi, "Clustering by
  Compression," *IEEE TIT* 2005) is the precedent for *using description length
  as a distance metric* — two states are close when the compressed edit between
  them is short. It legitimizes the framing but is a conceptual cousin, not a
  binoc method: NCD measures undirected blob similarity, whereas binoc produces a
  structured, directional, replayable edit list.
- **KRIMP / "patterns that compress"** (Vreeken, van Leeuwen, Siebes, "Krimp:
  mining itemsets that compress," *DMKD* 2011, plus the sequence variants SQS /
  GoKRIMP) is the closest formal precedent for binoc's *actual* algorithm: an
  *open vocabulary of structures, each admitted to the model only when it reduces
  total description length, chosen greedily.* The most honest framing of binoc's
  novelty: it applies the **KRIMP "patterns that compress" principle** to the
  **VCDIFF/Courgette "COPY/ADD over a transformed representation" substrate** —
  and a targeted search did *not* find prior work framing file/AST diff as an MDL
  verb-selection problem under that name. (Flag: treat that as "we didn't find
  one," not "none exists" — worth a literature check before claiming novelty in
  print.)

## What to steal, what to fear

The fields converge on a single arc that validates binoc's design and supplies
its guardrails.

**Steal:**

1. **Exact/unique anchors before fuzzy search** (Hunt–McIlroy, patience,
   histogram, git diffcore-rename). Lock the unambiguous correspondences cheaply;
   reserve expensive verb search for the ambiguous residue.
2. **Output-sensitive cost** (Myers): a correspondence with a tiny diff should
   cost almost nothing to compact.
3. **A minimal COPY/ADD verb core** (VCDIFF, git, Fossil), with richer verbs
   justified as description-shortening shorthands over it, and *near-COPY +
   priced residual* rather than shattering near-matches (bsdiff).
4. **Transform the representation so the diff shrinks** (Courgette) — binoc's
   parse-then-compact in another domain.
5. **Derive the script from a stored mapping; keep intermediates transient**
   (GumTree, Eg-walker), anchored on stable element identity (CRDTs).
6. **A canonical rule ordering** (Jupiter/Easysync) to dodge the three-way TP2
   bug class, and the **attribute-pool** pattern (Etherpad) for interning open
   vocabularies cheaply.
7. **Verify inferred operations by replay** (OpenRefine), and treat edit lists as
   **first-class invertible/rebasable objects** (SQLite session/patchset).
8. **Name the cost function as MDL** (Rissanen → Grünwald → KRIMP) and the
   acceptance test as strictly-decreasing description length.

**Fear:**

1. **The global optimum is NP-hard** the moment moves/copies enter the vocabulary
   (Shapira–Storer, Cormode–Muthukrishnan; smallest-grammar). Never chase it.
2. **Three-way composition of opaque verbs is where correctness dies** — the
   entire OT literature shipped TP2 bugs; the most ambitious open-vocabulary
   implementation (json1) is still fuzzed-buggy by its own author. Budget for a
   property-based fuzzer of the composition laws from day one.
3. **Convergence is not correctness** (CCI / intention preservation): a
   deterministic compaction can still violate what a rule *meant*. Make verb
   intention an explicit, tested property.
4. **Inference is imperfect** (SPINFER 86%/69%; refactoring miners are
   benchmark-bound). Don't trust an inferred verb without the replay check, and
   don't generalize one-off edits.
5. **Reconstruction cost ≠ description cost** (SVN skip-deltas), and **stable
   identity has a metadata-cost failure mode** (CRDT tombstones, Logoot
   identifier bloat) — watch the size of the stored mapping as edits accumulate.

**The one-paragraph synthesis.** With a minimal, local verb set, optimal
alignment is exact and polynomial; adding a bounded local verb keeps it
polynomial; adding unbounded move/copy/block verbs makes it NP-hard (two
independent 2002 proofs, and again via the smallest-grammar problem).
Consequently every practical system that wants the richer verbs abandons global
optimality for cheap unambiguous anchors plus greedy, threshold-gated,
cost-reducing rewrites — which is exactly binoc's naive-writer-then-compaction
design. The structural moves binoc makes around that core are each
independently validated: deriving the script from a stored mapping (GumTree,
Eg-walker), imposing a canonical rule order to keep composition tractable
(Jupiter, term-rewriting confluence, patch commutation), verifying inferred
operations by replay (OpenRefine, SQLite session), and pricing the whole thing
by minimum description length (Rissanen, KRIMP). The risks are equally
well-documented, and the loudest one — that independently-authored rules
composing into a wrong-but-convergent result is the failure mode no serializer
fixes — is the prototype's real test.
