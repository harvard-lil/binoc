# Invariant and Lint Tiers: Harness, Mechanical, Agent

**Date:** 2026-06-12
**Status:** Implemented

## Context

Binoc accumulated invariant checks in scattered, single-purpose homes:
changeset structural invariants hardcoded inside the test-vector harness,
write-set enforcement inside `AbiTransformer`, a descriptor lint in
`test_support`, and one-off assertions in per-crate test files. Each new
check faced the same questions with no settled answers: where does it
live, how does a plugin author adopt it, can it warn without failing, and
what about properties that no mechanism can check — behavioral
completeness on untested branches, performance posture, security of
container expansion, layering discipline?

The write-sets work sharpened the gap: the dynamic harness only audits
code paths that vectors exercise, source scans only see literal patterns,
and a class of contracts (the never-for-scheduling rule, "tags are
facts") was enforced purely by ADR prose.

## Decision

Three tiers, each with a designated home, a failure mode, and an adoption
path. The tier table lives in `binoc_sdk::lints` (module docs) and
`.agents/skills/lint-plugin/SKILL.md` so both humans and agents find it.

**Tier 1 — harness invariants (hard fail, every vector).** Properties of
every produced changeset, checked between diff and snapshot so gold files
cannot silently enshrine bugs. Home:
`binoc_stdlib::test_vectors::check_changeset_invariants` (now public —
plugins call it, plus their own domain checks, on changesets they build),
and the per-call write-set enforcement in `AbiTransformer`. Adding an
invariant here costs every harness user a hard failure, so it must be a
true always-property, cheap, and self-explanatory when it fires.

**Tier 2 — mechanical lints (errors fail, warnings print).** Static
checks over descriptor data and source text in `binoc_sdk::lints`
(test-support feature): a `LintReport { errors, warnings }` whose
`assert_clean()` panics on errors and prints warnings to stderr. The
convention is one `tests/lints.rs` per crate calling the helpers it cares
about; `binoc-sqlite`'s is the model for plugin authors. Initial lints:
single-producer/single-consumer tags and undeclared write-sets
(descriptor-level — these also work on closed-source plugins, since
descriptors cross the ABI), wholesale tag wipes (`tags.clear()` bug
class), and the tripwire that `binoc-core` never reads write-set
declarations (mechanizing the write-sets ADR's never-for-scheduling
constraint). Source scans honor a `binoc-lint: allow(<rule>)` comment on
or above the flagged line — except the dispatch tripwire, where the
answer is "don't".

Warnings ride the test suite rather than a separate tool: zero new
infrastructure, colocated with what they check, and lint *errors* already
fail `just test`. The cost is that libtest hides stderr from passing
tests, so `just lint` runs the lint targets with `--nocapture` to surface
advisory warnings on demand.

**Tier 3 — agent lints (review findings).** Contracts that need judgment,
not mechanism: behavioral write-set completeness on branches vectors
don't reach, tags-as-facts for targeted removals, keyed-vs-positional
judgment hygiene, performance posture (no unguarded full-data scans,
streaming, bounded captures), security (path traversal, decompression
bombs, no ambient I/O), layering (AGENTS.md rules), and docs/ADR hygiene.
Documented as an operational checklist in
`.agents/skills/lint-plugin/SKILL.md`, including a reporting format
(violation vs. smell, file:line, offended contract, smallest fix). The
skill explicitly lists what tiers 1–2 already prove so agents verify the
gaps instead of re-deriving the automated parts.

The tiers are ordered by preference: a check that *can* move down a tier
should. An agent-lint observation that turns out to be greppable becomes
a tier-2 lint; a tier-2 pattern that is really an always-property of
changesets becomes a tier-1 invariant.

## Alternatives Considered

**A separate lint binary / `cargo xtask` pass.** A dedicated runner could
report warnings without the `--nocapture` wrinkle and gain flags
(`--deny-warnings`). Rejected for now: it adds a build target and a CI
step for what three test files express today, and lint errors must fail
`just test` anyway — the runner would duplicate that path. Revisit if
warnings need machine-readable output.

**Custom rustc lints (dylint/clippy plugins).** True AST-level precision
(e.g. resolving that a computed `with_tag(tag)` value is declared).
Rejected: heavy toolchain coupling for marginal gain over substring scans
plus the dynamic harness, which together already cover both sides
(patterns statically, behavior dynamically).

**Type-level enforcement (sealed tag emission via declared-set tokens).**
Strongest static guarantee, but it conflicts with the open-IR
architecture (AGENTS.md rule 4: open strings, conventions not
enforcement) and would break the `DiffNode.tags: BTreeSet<String>` plugin
surface. Deliberately not pursued.

**Putting agent checks in CLAUDE.md / AGENTS.md prose.** The checklist is
operational (per-review, per-plugin), not ambient project context;
AGENTS.md stays the pointer, the skill holds the procedure. A skill file
can also be invoked selectively by tooling rather than occupying every
session's context.
