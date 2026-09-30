# Vintage benchmark — target experience

This file is the north star for the *vintage* (different-edition) audience. It is
**not** checked by the harness; it is the hand-authored target that
`expected-output/changelog.snap` should converge toward as the vintage story
improves. Compare the two whenever you touch tabular significance, vocabulary
detection, or summary statistics.

A vintage reader is comparing two editions of the same published dataset. They
care about the *shape* of the data — did a column appear, did a category
vocabulary shift — and they deliberately do **not** want to read the bulk
cell/row churn. (This is the opposite stance from the same-data-with-edits
reader binoc is primarily tuned for today, who wants every cell.)

## What binoc renders today

See `expected-output/changelog.snap`. Abbreviated:

```
## Schema & vocabulary changes
- facilities.csv: Column added: 'region'; 1 cell changed
  - row 2, column 'status': 'active' -> 'decommissioned'
  - Set Headers: ...; Add Column: 'region' ...
## Bulk data updates
- inspections.csv: 2 rows added; 3 cells changed
  - row 1, column 'score': '82' -> '85'
  - ... every changed cell and added row, in full ...
```

The file-level separation is right. Three things fall short.

## What great looks like

```
# Changelog: 2021 edition -> 2022 edition

## Schema & vocabulary changes
- facilities.csv
  - Column added: 'region'  (4 values: north, east, south, west)
  - Vocabulary 'status' gained a value: 'decommissioned'
    (now: active, inactive, decommissioned)

## Bulk data updates — summarized, not enumerated
- facilities.csv:  4 rows, 1 cell changed
- inspections.csv: 4 -> 6 rows (+2), 3 cells changed
```

## The three gaps between today and the target

1. **Within-node significance / edit-level keep-drop.**
   `facilities.csv`'s `region` addition and its `status` cell edit are edits on
   one node, so the renderer cannot put the structural change in the top section
   and hold the cell back. The vintage reader still sees the cell bullet.
   *Needs:* a config-driven, edit-level drop/keep on the renderer (the data path
   already has `EditProjection.visible`, but only writers set it). This is the
   single smallest unlock and it lives entirely in the renderer — no engine or
   IR change.

2. **Vocabulary as a first-class change.**
   `active -> decommissioned` is reported as `binoc.cell-change`, not "the
   `status` vocabulary gained a value." Columns are not first-class nodes and
   distinct-value-set diffing does not exist.
   *Needs:* a plugin `EditListWriter` over `tabular_v1` that computes the set of
   distinct values per categorical column on each side and emits the set-delta
   as a tagged edit (`binoc.vocabulary-change`). No engine change — a plugin
   pack, exactly like the standard library is.

3. **Summary statistics instead of enumeration.**
   The bulk section dumps every changed cell and added row. A vintage reader
   wants "4 -> 6 rows, 3 cells changed."
   *Needs:* the same plugin writer emitting an aggregate via `Edit::with_summary`
   (or `GlobalClaim` for a dataset-level roll-up). The seam already carries such
   facts — binoc-stdlib uses `with_summary` for binary string-diffs today; no
   rule emits a tabular roll-up yet.

## Why this benchmark exists

It demonstrates that the *engine* does not foreclose the vintage audience: the
target above is reachable with (1) one renderer-local keep/drop filter and (2)
one plugin pack that emits vocabulary + statistic facts — no change to the
type-ignorant controller, the IR, or the correspondence engine. The vintage vs.
same-data distinction is a renderer-config + plugin-pack concern, which is the
architecture's whole thesis (AGENTS rules 1 and 3).

It is kept as a passing benchmark so the gap stays visible and measurable. We
are deliberately **not** building the unlocks yet (we want to nail the
same-data audience first), but the channel is provably clear.
