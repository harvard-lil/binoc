---
name: lint-plugin
description: Review checklist for binoc plugins and core changes. Verifies the invariants mechanical lints cannot — behavioral write-set completeness, judgment semantics, performance, security, and layering. Use when writing or reviewing a comparator, transformer, renderer, or a change to the controller/harness.
---

# Lint a binoc plugin (agent checklist)

This is tier 3 of binoc's invariant scheme — the checks that need judgment
rather than mechanism. Read what's already automated first, so you verify
the gaps instead of re-deriving what a test already proves:

| Tier | What | Where | Failure mode |
|---|---|---|---|
| 1. Harness invariants | Changeset structural invariants on every test vector; per-call write-set enforcement; direct-vs-ABI wire parity | `binoc_stdlib::test_vectors::check_changeset_invariants`, `binoc_sdk::test_support` (`AbiTransformer`, `undeclared_emissions`) | hard test failure |
| 2. Mechanical lints | Descriptor lints (single-producer/single-consumer tags, undeclared write-sets) and source scans (tag wipes, core reading write-sets) | `binoc_sdk::lints`, each crate's `tests/lints.rs`, `just lint` for warnings | errors fail, warnings print |
| 3. Agent lints | Everything below | this file | your review finding |

Scope note: tier 1 only audits code paths that test vectors exercise, and
tier 2 only sees literal source patterns. Your job is the remainder:
untested branches, computed values, and properties that aren't expressible
as a pattern.

## How to report

For each finding give: severity (**violation** = breaks a stated contract
/ ADR / AGENTS.md rule; **smell** = legal but worth a question),
`file:line`, the contract it offends, and the smallest suggested fix. If
everything passes, say which checks you ran — silence is not a clean bill.

## 1. Layering (AGENTS.md rules 1–3)

- No format knowledge in `binoc-core`: a core diff must not mention file
  formats, extensions, media types, or plugin-specific tags/actions.
- Comparators are the only readers of raw bytes. Transformers may re-parse
  only via `node.source_items` / artifacts through `DataAccess` — grep the
  plugin for `std::fs`, `File::open`, `physical_path`: any hit outside a
  `DataAccess` implementation or test code is a violation.
- Significance/grouping is renderer-side. A comparator or transformer
  writing words like "substantive", "critical", or otherwise ranking
  importance into the IR is a violation; tags state facts only.
- Plugins must not depend on `binoc-core` (SDK only). Check `Cargo.toml`.

## 2. Descriptor honesty (behavioral write-sets)

The harness verifies declarations dynamically, but only on vector-covered
paths. Read the `transform()`/`compare()` implementation and confirm:

- Every tag/action/item-type/artifact-format the code *can* emit — on any
  branch, including error and diagnostic paths — appears in
  `emits_tags`/`emits_actions`/`emits_item_types`/`publishes_artifacts`.
  Watch computed values: tags built from variables, actions copied from
  inputs, `DiffNode::new(action, …)` where `action` is a parameter.
- The converse: nothing declared that the code can no longer emit
  (declarations are facts, not aspirations — stale entries hide drift).
- Dispatch (`match_*`) is as narrow as correctness allows; a transformer
  doing its own filtering that the descriptor could express is a smell.
- New tags follow the namespace convention (`package.tag-name`) and don't
  create a single-producer/single-consumer dispatch channel; if the tier-2
  lint is allowlisted, the allowlist comment must say *why* (e.g. renderer
  configs also consume the tag).

## 3. Judgment semantics and test vectors

- Tags-as-facts: a transformer may remove a tag only when it is changing
  the fact's truth (e.g. demoting move→modify removes `binoc.move`).
  Removing or clearing tags it doesn't own is the
  `ColumnReorderDetector tags.clear()` bug class — tier 2 catches the
  wholesale form; you check targeted `tags.remove(...)` calls.
- Keyed vs positional: any tabular judgment must state which pairing it
  uses and not silently mix them (a re-sorted keyed table is not a pure
  positional reorder).
- Every behavior the plugin claims has a vector named for *what* it tests;
  every gold-file diff in the change is explained in the commit message.
  An unexplained gold diff means the change is wrong or undertested.
- New judgments derivable from an existing analyzer's single pass should
  be inlined there, not added as a tag-handoff transformer (see the
  pure-reorder collapse ADR); patterns needing their own scan are
  legitimately separate transformers.

## 4. Performance

- No full-data re-scan on the dispatch path: expensive checks must be
  gated behind cheap facts already computed (compare how the keyed
  pure-reorder check is gated in `tabular_analyzer.rs`).
- Streaming I/O for potentially large inputs (`open_read` over
  `read_bytes` where the format allows); no unbounded buffering of
  container members.
- No quadratic sibling × sibling work without a bound or bucketing;
  correlation-style detectors should index by hash/size first.
- Per-node allocations proportional to what the node reports (capped
  examples — see `MAX_CAPTURED_CELL_EXAMPLES`), not to input size.

## 5. Security

- Container expansion must not allow path traversal: child logical paths
  derived from archive entries need `..`/absolute-path handling — check
  against how the zip/tar comparators sanitize.
- Decompression bombs: expanding comparators should bound or stream
  output; flag unbounded decompress-to-memory.
- No environment/system reads (env vars, network, paths outside
  `DataAccess`) — AGENTS.md rule 7, and it breaks across the ABI anyway.
- Diagnostics and summaries may quote *cell-sized* data previews
  (truncated, like `capture_value_preview`), not whole file contents.
- `unsafe` only at the generated ABI boundary (`export_plugin!`), never in
  plugin logic.

## 6. Docs and ADR hygiene

- Behavior or contract changes update the explanation/reference docs that
  state the old behavior (grep `docs/` for the plugin and tag names).
- Design decisions with rejected alternatives get an ADR
  (`just adr "Title"`); superseded ADR sections get a superseding note,
  not a rewrite.
- Generated docs are regenerated, not hand-edited (`just docs-adr-index`,
  `just docs-vectors`, etc.).
