# Renderers Are Ecosystem-Ignorant; Ugly Output From an Unknown Rule Is an Upstream Bug

**Date:** 2026-06-29
**Status:** Accepted

## Context

Three queued issues (#102 binary byte-range edits, #108 cell-type-shift, #120 the
reduced-precision verb inventory) each introduce new kinds of change, and the
natural-but-wrong reflex is "register each new tag's significance and pretty-print
in the stdlib renderer." Pursuing that would deepen a coupling that already exists
and that we want to prove unnecessary.

The intended principle: **a baseline renderer knows next to nothing about the
ecosystem that produced its input tree.** It is *fine* for a renderer to offer a
user-facing setting that sorts tags into significance categories, and *fine* for
it to carry optional pretty-print shortcuts for very common rewrites. The point to
prove in this phase is that it does **not need to** — generic processing of the
self-describing information rewrite rules send downstream (the typed user-facing
message sequences, i.e. `Segment` summaries, ADR 2026-06-03) is enough to produce
good output. If a renderer's output is ugly or hard to interpret because it
isn't provided sufficient information about a particular rewrite rule, **that is a rewrite-rule or engine bug, not a renderer bug** — the rule failed to describe its own change.

The current code does not yet reflect this. `binoc-stdlib/src/renderers/markdown.rs`
runs two patterns side by side:

- **Generic (the model):** `Segment::{Text,Path,Uint,Float}` summaries formatted
  without knowledge of which rule produced them; `humanize_numbers`/`format_float`.
- **Ecosystem-coupled (the debt):** branches on specific stdlib vocabulary —
  `node.tags.contains("binoc.move.modified")` (lines 386, 442),
  `"binoc.folder-move"` (431), and a `match` that maps specific item-types to
  specific tags to decide phrasing: `"tabular.rename_column" => binoc.column-rename`,
  `"tabular.reorder_columns"`, `"document.serialization_change"` (884-886).

Each coupled branch is the renderer knowing something only a rewrite rule should
know. Left in place, it is the template #102/#108/#120 would copy.

## Decision

Establish the contract and retire the debt.

1. **Rule contract.** A rewrite rule that produces a change a human should read
   MUST emit a self-describing summary (a `Segment` sequence) that renders legibly
   under a renderer with **zero** knowledge of that rule. The phrasing and
   structure of a change live in the rule's emitted segments, never in the
   renderer.

2. **What tags are for.** Tags carry **significance and categorization only** —
   the ecosystem-agnostic axis a user may remap. The renderer's
   `tag_map`/`classify_tags` mechanism stays: it is config-driven and lets a user
   declare which tags are "major," exactly the allowed setting. Tags do **not**
   carry "how to phrase this."

3. **What the renderer may do.** Generic segment formatting; number humanizing;
   the configurable tag→category significance map; and *optional* pretty-print
   shortcuts for very common rules — permitted only when the generic path already
   renders that change legibly without them, so the shortcut is polish and never
   load-bearing.

4. **What the renderer may not do (debt to retire).** Branch on specific
   `binoc.*` tag strings or specific item-type strings to decide phrasing or
   structure. The audit list is the three sites above (`move.modified`,
   `folder-move`, the item-type→tag `match`). For each, the fix is to move the
   description upstream into the rule's `Segment` output, then delete the branch.
   NOTE: third party renderers are allowed to do these things. Enforcing this rule
   on built in renderers is to make sure the generic path continues to work well.

5. **Mechanize it as an invariant.** Add a test-vector check (an invariant tier,
   per the 2026-06-12 invariant-and-lint ADR): render every vector with an empty
   `tag_map` and the rule-specific branches disabled; the output must still be
   legible for every vector — no raw tag names leaking, no opaque
   "contents differ" fallback where a rule actually described the change. This
   turns "renderers are ecosystem-ignorant" from a principle into a failing test
   when violated.

**Consequence for the queued work (this is the corrected framing of the "new tags
need renderer significance" question).** #102, #108, and #120 each add tags *for
significance/categorization* and emit `Segment` summaries for their human-readable
story. They add **zero** renderer branches. A new rewrite rule should never
require a renderer change to produce good output; if it does, the design is wrong
and the fix belongs in the rule.

## Alternatives Considered

**Rich renderer with per-rule pretty-printers** (extend the status-quo
`.contains("binoc.…")` pattern for each new rule). Rejected: it couples the
renderer to the rule ecosystem, does not scale to third-party packs (which cannot
edit the stdlib renderer), and forfeits the very property pre-1.0 is meant to
prove — that the generic downstream channel is sufficient.

**Bake significance into the IR as a typed level so the renderer needs no map.**
Rejected by AGENTS.md rules #3-#4: the IR is openly typed and significance-free;
classification is a renderer/config concern mapped from open-string tags.

**Keep the existing special-cases, reclassified as "common-rule polish."**
Partially accepted, conditionally: a shortcut may remain *only* if the invariant
in (5) passes with it disabled — i.e. the generic path already produces legible
output and the shortcut is genuinely optional. Any branch that is load-bearing
(its removal makes output ugly or opaque) is by definition an upstream bug: the
rule isn't describing its change, and the fix is to emit the missing segments, not
to keep the branch.

**Defer this until after Track C ships.** Rejected: Track C (#108, #120) and #102
are precisely the work that would add new coupled branches. Settling the contract
first is what lets those three issues fan out without each re-litigating where a
change's description belongs.
