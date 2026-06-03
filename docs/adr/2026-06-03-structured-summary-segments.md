# Structured Summary Segments

**Date:** 2026-06-03
**Status:** Implemented

## Context

`DiffNode::summary` was a free-text string built by whichever comparator or
transformer produced the node (for example `format!("{a} lines added, {r}
removed")`). By the time a renderer saw it, the *type* of every value in it was
gone — it was just characters. To present numbers well (thousands grouping) the
markdown renderer had to reverse-engineer structure the producer had already
thrown away: it scanned the prose for digit runs and used heuristics
(neighbouring unit words, adjacency to path/identifier characters, a set of
"known count" values pulled from `details`) to guess which digits were counts to
group and which were years, IDs, or path fragments to leave alone. Each new edge
case (a date inside a folder name, a leading-zero code) needed another guard.

A parallel smell sat next to it. Rename and copy headlines were detected with
`summary_is_path_statement`, which string-matched the `"Moved from "` /
`"Copied from "` prefixes that the renderer's own fallback emitted — the renderer
pattern-matching its own output to recover a fact (`action`, `source_path`) that
was already a typed field on the node.

Both are the same anti-pattern: producers flatten typed values to prose, then the
renderer parses the prose back into types it can format. The middle two steps are
pure loss and can only ever be heuristic.

A further constraint: rename/move/copy detection lives in transformers, some of
them out-of-tree plugins, and `action` is an open set. A renderer must not encode
what "move" means, or it cannot compose with a plugin that invents a new
relational action.

## Decision

`DiffNode::summary` is now `Option<Summary>`, where a `Summary` is an ordered
list of typed `Segment`s. Producers build it; renderers format each segment by
its type and never parse prose.

```rust
pub enum Segment {
    Text(String),                       // verbatim
    Path { value: String, snapshot: Side }, // linkable; which side it resolves in
    Uint(u64),                          // digit-grouped by the renderer
    Float(f64),                         // decimal/precision policy by the renderer
}
pub struct Summary(pub Vec<Segment>);   // serializes transparently as a JSON array
```

Design rules that keep the variant set small and the layering clean:

- **Variants track render behaviour, not meaning.** A variant exists only when a
  renderer would do something to the value it cannot infer from `Text`: group an
  integer (`Uint`), apply decimal policy (`Float`), hyperlink/shorten a path
  (`Path`). Currency, percent, and units are `Text` plus a number, never their
  own variant — this is the line that stops an "Excel zoo" of format types.
- **Path/date are not how we avoid mangling.** Digits inside `Text` (and inside a
  `Path` value, such as a year in a folder name) are never reformatted. A number
  that should be grouped is a `Uint`; everything else is left exactly alone. The
  old heuristics disappear because the question "is this digit run a count?" was
  answered upstream, where the value was still a `u64`.
- **Producers own concept wording; renderers own typography.** A rename detector
  emits `Text("Moved from ") + Path(src, Side::From)`. The renderer formats a
  `Path` (today: verbatim; a richer renderer can hyperlink it) without knowing it
  is a rename. A future `split`/`merge` plugin emits its own wording and the same
  renderer formats it with no new code. `summary_is_path_statement` and the
  renderer's concept knowledge are gone.
- **Direction is a property of the value.** `Segment::Path` carries `snapshot:
  Side` (`From`/`To`) so a renderer that dereferences a path targets the correct
  tree — framed as "which snapshot does this resolve in", not "rename direction".

The ergonomic shortcut is `impl Into<Summary>`: `with_summary("plain text")` (and
`with_summary(format!(...))`) still compiles and produces a single
`Segment::Text`. Plain-string summaries remain valid and render verbatim;
producers opt into structured segments only where formatting matters. Wire and
Python surfaces preserve a plain-text view via `Summary::plain_text()`.

This carries across the plugin ABI unchanged in mechanism — `DiffNode` already
serializes to JSON for native plugins — and the persisted changeset now exposes
`summary` as a typed segment array, which machine consumers can read directly or
flatten.

## Alternatives Considered

**Keep the prose summary and harden the number scanner.** This was the prior
direction (constrain humanization with more guards). It makes the reverse-parse
safer but never removes the need to reverse-parse; every new summary phrasing is
a new risk. Rejected as treating the symptom.

**Push number formatting into the SDK so producers emit grouped strings.**
Grouping is a render-time/locale decision the producer cannot make (it does not
know the output sink). The formatter must run in the renderer, which means the
renderer needs the raw number — i.e. structured segments. So this collapses back
into the chosen design.

**Generate the move/rename headline in the renderer from `source_path`/`path`.**
This removes the prose reparse but reintroduces concept coupling: the renderer
would `match` on `action` and could not compose with plugin-defined relational
actions. Rejected in favour of producers emitting the segments (including their
own `Path`s) and the renderer staying concept-free.

**A semantic enum (`Count`, `Currency`, `Percent`, `Date`, `FileSize`, ...).**
This is the unbounded "Excel zoo". Keyed on meaning, the set never closes.
Rejected; variants are keyed on render behaviour instead. New *typographic*
policies (humanized bytes, durations) remain possible as justified future
additions, or via an optional format-hint axis, without encoding domain meaning.

**Carry the trailing content summary on move nodes as structured segments too.**
The `tabular_summary` / `content_summary` annotation trailers stay plain strings
for now, because the folder-move detector reads them with `.as_str()`; they
render verbatim (counts in a trailer are not grouped). A deliberate, documented
boundary — the primary `summary` is structured; annotation trailers are prose.
