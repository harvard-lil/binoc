# Derive Parse-Rule Link Gating from Pair Reads

**Date:** 2026-06-13
**Status:** Implemented

## Context

A parse rule turns raw bytes into a published artifact. Most parse rules
should only run on nodes that have already been paired (linked): there is
no point parsing a SQLite file or a stacked-CSV collection on a leaf that
will never have a counterpart. The engine modeled this with a per-rule
`ParseDescriptor.requires_link: bool` flag.

The cross-format tabular pairing feature broke that model. `TabularPair`
compares parsed `tabular_v1` content to detect a CSV→TSV reformat as a
single reformatted table. To do that it must read the `tabular_v1`
artifact *before any link exists* — pairing is exactly what produces the
link. So `CsvParse.requires_link` was hand-flipped to `false`.

That hand-flip exposed a deeper fact: **`requires_link` is not a local
property of a parse rule.** It is a statement about the whole ruleset —
"no pre-link consumer needs my output." A parse rule author cannot
correctly set it in isolation, because the correct value depends on which
pair rules happen to be configured alongside it. Two facts encoded in two
places that can never legally disagree (the parse rule's flag and the pair
rules' declared reads) are a latent inconsistency waiting to bite.

## Decision

Remove `ParseDescriptor.requires_link` and **derive** the gate from the
ruleset. Once per run, before the fixed-point loop, the driver
(`binoc-core/src/correspondence/driver.rs`) computes:

    preconsumed_formats: BTreeSet<ArtifactFormat>
        = union of every CoreRule::Pair rule's descriptor().reads

A parse rule is link-gated iff its `descriptor.output` is **not** in
`preconsumed_formats`:

    if !preconsumed_formats.contains(&descriptor.output)
        && store.links.of_node(id).is_empty()
    {
        continue;
    }

Pair rules are the only pre-link artifact consumers today: writers and
annotators run post-link, and expand rules read raw data, not artifacts.
So the union of pair `reads` is the complete set of formats that must
exist on unlinked nodes. The derivation reproduces the prior behavior
exactly: `CsvParse` (output `tabular_v1`) is read by `TabularPair` → un-
gated (the former hand-flip, now formalized); every other parse rule
(`CsvStackedTablesParse`, `SqliteParseRule`, the stat-binary collection
parsers, all output `tabular_collection_v1`, read by no pair rule) →
stays link-gated.

This stays a static pre-pass: it sets a per-format gate and does not
reorder or schedule rule execution. The engine remains a fixed-point
loop.

## Alternatives Considered

- **Keep `requires_link` and add a lint that cross-checks it against pair
  reads.** Rejected: validating a redundant field is strictly worse than
  deriving the only consistent value. The field could still be authored
  wrong; the lint would just catch it later.
- **Schedule parse rules explicitly relative to pair rules instead of a
  static gate.** Rejected as over-engineering. The fixed-point loop
  already converges; a per-format boolean gate is sufficient and keeps the
  engine's execution model unchanged.

## Known Limitation

`descriptor.output` is a parse rule's **primary** format only. Child
artifacts published via `ParsedChild` (e.g. the stacked-table parser
emits child `tabular_v1` artifacts) are not declared in the descriptor,
so the derivation cannot see them. For the current ruleset this is
harmless: no pair rule needs a format that is produced *only* as an
undeclared child pre-link. If a future pair rule needed to read such a
child format on unlinked nodes, the producing parser would have to declare
that format (e.g. via descriptor-level child-format declarations) so the
derivation could un-gate it. Noted here as a future edge rather than
solved speculatively.
