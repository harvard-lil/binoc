---
name: lint-plugin
description: Review checklist for binoc correspondence rule packs, renderers, and core changes. Verifies the invariants mechanical lints cannot: descriptor honesty, deterministic proposals, security, settled behavior, layering, and projection/extract honesty.
---

# Lint a binoc plugin or rule pack

This is tier 3 of binoc's invariant scheme: the checks that need judgment
rather than mechanism. Read the automated tiers first so review effort goes to
the gaps, not to properties already covered by tests.

| Tier | What | Where | Failure mode |
|---|---|---|---|
| 1. Harness and runtime invariants | Changeset structural invariants on every test vector; correspondence driver hard checks such as pair-rule emits honesty and compaction monotonicity | `binoc_stdlib::test_vectors::check_changeset_invariants`, `binoc-core/src/correspondence/driver.rs` | hard test failure |
| 2. Mechanical lints | Descriptor lints for legacy transformers and correspondence pair rules; source scans for tag wipes and core read/write-set misuse | `binoc_sdk::lints`, each crate's `tests/lints.rs`, `just lint` for warnings | errors fail, warnings print |
| 3. Agent lints | Everything below | this file | your review finding |

Scope note: tier 1 only audits paths that test vectors exercise, and tier 2
only sees descriptor data or literal source patterns. Your job is the
remainder: untested branches, computed values, and properties that are not
expressible as a mechanical pattern.

## How to Report

For each finding give severity (`violation` = breaks a stated contract, ADR,
or AGENTS.md rule; `smell` = legal but worth a question), `file:line`, the
contract it offends, and the smallest suggested fix. If everything passes, say
which checks you ran. Silence is not a clean bill.

## 1. Layering

- `binoc-core` stays type-ignorant. Core code must not mention file formats,
  extensions, media types, dataset-specific tags, or stdlib/plugin semantics.
- Rule packs are plugin packs. They depend on `binoc-sdk` for plugin-facing
  contracts; they do not smuggle host internals into descriptors or payloads.
- Raw bytes are read only through `DataAccess` by expand/parse/pair rules that
  explicitly need them. Writers, compaction rules, annotators, and renderers
  should operate on correspondence artifacts or projected IR.
- Significance remains renderer-side. Rules may emit factual tags and evidence,
  not importance rankings such as "critical" or "substantive".
- No global state or process-level side effects. Configuration is passed in.

## 2. Descriptor Honesty

Read each rule implementation and confirm every value it can emit is declared,
including values from error, diagnostic, and fallback branches.

- Expand rules: selectors match what the rule can actually expand; outputs use
  stable logical child paths; diagnostics are bounded and factual.
- Parse rules: input selectors, output artifact format, and `requires_link`
  match the parser's real behavior. A parser must not claim generic formats it
  cannot parse deterministically.
- Pair rules: `PairDescriptor.emits` contains every possible evidence string.
  The driver fails closed on undeclared evidence, but vectors may not cover
  every branch. Declarations are facts, not aspirations.
- Writers: claimed artifact formats, verbs, item types, tags, details, and
  extract aspects match the emitted projection. A writer that owns a projected
  node must also own or deliberately delegate extraction semantics.
- Compaction rules: rewrites strictly reduce driver cost, preserve or remap
  links, diagnostics, tags, and extract ownership, and never erase evidence
  needed to explain the projection.
- Annotators: annotations are projection-only facts. They must not change
  pairing, settled state, raw artifacts, or rule scheduling.
- Renderers: significance/grouping config maps from semantic facts already in
  the IR; renderers do not invent parser facts.

## 3. Producer-Kind Self-Filtering

Writer and rule dispatch is usually by artifact format. Artifact format says
nothing about which producer created the payload, so a specialized writer or
rule claiming a shared format must imperatively decline foreign payloads and
return `None` so dispatch falls through to the generic fallback.

Review question: does the specialized rule prove the producer kind it expects,
decline everything else, and sit before a generic fallback? Use
`is_sqlite_collection` in `model-plugins/binoc-sqlite/src/sqlite.rs` as the
current pattern.

## 4. Correspondence Semantics

- Proposals are deterministic: identical inputs and config produce identical
  links, ordering, priorities, and diagnostics.
- Settled behavior is explicit. A rule with `sees_beneath_settled = false`
  must not rely on hidden descendants under settled links; a rule that can see
  beneath settled links must not mutate settled links except through declared
  proposals allowed by the driver.
- Late links and third-party rule injection preserve unknown evidence and edit
  verbs. Open vocabularies must flow through without lossy normalization.
- Pairing semantics are clear. Keyed, positional, fuzzy, copy-aware, and
  declared correspondences must not silently substitute for one another.
- No hidden mutable cache may affect rule decisions across runs.

## 5. Projection and Extract Honesty

- Projected actions, item types, paths, source paths, summaries, tags, and
  detail blocks describe the actual edits, not implementation convenience.
- Writer projections are complete for the artifacts they claim. If an edit list
  has rows, columns, cell edits, content edits, or container children, the
  projection either represents them or documents why it intentionally elides
  them.
- Extract/reopen paths are anchored in source items or live links, including
  nested archives. A projected node should not expose an extract aspect unless
  the owning writer or compaction rule can resolve it.
- Compaction that combines edits preserves enough evidence for diagnostics and
  future extract ownership. If it changes aspect semantics, it becomes the new
  owner and must implement extraction explicitly.

## 6. Performance

- No full-data scan on a broad dispatch path. Expensive work is gated behind
  cheap descriptors, selectors, hashes, sizes, or already parsed artifacts.
- Prefer streaming I/O for potentially large inputs. Avoid unbounded
  decompress-to-memory or whole-file buffering where the format permits
  streaming.
- No sibling-by-sibling quadratic work without bucketing, bounds, or an ADR
  explaining why the domain size is capped.
- Diagnostics and examples are capped. Per-node allocations scale with reported
  evidence, not input size.

## 7. Security

- Container expansion rejects absolute paths and `..` traversal in logical
  child paths.
- Archive and compressed formats have bomb resistance: streaming, size caps, or
  explicit documented bounds.
- No environment, network, or filesystem reads outside `DataAccess`, except in
  test/materialization code.
- Summaries and diagnostics quote small truncated previews only, not whole
  files or large cells.
- `unsafe` is limited to generated ABI boundary code, not plugin logic.
- External format libraries are reviewed for maintenance, licensing, and
  sandbox posture when they parse untrusted input.

## 8. Docs and ADR Hygiene

- Behavior or contract changes update the docs that state the old behavior.
- Design decisions with durable alternatives get an ADR, plus a short entry in
  `docs/adr/README.md`.
- Generated docs are regenerated with the relevant `just` target.

## 9. General Quality

- The rule pack should serve its stated purpose with focused tests named for
  what they prove.
- Gold-file diffs are intentional and explained by structural assertions or
  nearby tests.
- If a repeat review issue is not covered here, update this skill.
