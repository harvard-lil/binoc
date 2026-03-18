# Standard Library Boundary Policy

**Date:** 2026-03-09
**Status:** Accepted

## Context

`binoc-stdlib` is architecturally identical to a third-party plugin pack — the core engine has zero format knowledge. This raises the question: which comparators, transformers, and renderers belong in the standard library versus external plugin crates?

The answer affects first-run UX (what works out of the box), build cost (every stdlib dependency is paid by every user), and maintenance commitment (the `binoc.*` namespace implies long-term support).

## Decision

A plugin belongs in `binoc-stdlib` when it satisfies **all three** of the following criteria:

### 1. Structural necessity or audience expectation

- **Container plugins** that feed items back into the work queue (directory, zip) are structurally necessary — without them the tree walk produces nothing.
- **Universal fallback plugins** (text, binary) are the bottom of the dispatch chain — without them most leaf items produce no output.
- **Formats so common in data distribution that their absence would feel like a bug** to the target audience of archivists, data scientists, and dataset stewards. CSV is the canonical example: a tool that diffs dataset snapshots but reports `.csv` files as "binary: content changed" would feel broken.

### 2. Modest dependency cost

Dependencies must be pure Rust (or nearly so), small, and well-maintained. No bundled C libraries, no heavyweight native code. Every stdlib dependency increases compile time, cross-compilation friction, and supply-chain surface for all users.

### 3. Sustainable maintenance scope

The core team must be willing to maintain the plugin indefinitely under the `binoc.*` namespace. Domain-specific formats with evolving specs, niche audiences, or complex edge cases are better served by dedicated plugin maintainers who can release independently.

## Applying the criteria

| Format | Verdict | Reasoning |
|---|---|---|
| directory | stdlib | Structural container — the walk requires it |
| zip | stdlib | Structural container — common archive format in data distribution |
| text | stdlib | Universal leaf fallback |
| binary | stdlib | Universal leaf fallback |
| CSV | stdlib | Expected by target audience; tiny pure-Rust dep (`csv`) |
| tar/tar.gz | strong candidate | Container format common in data distribution; pure-Rust crates exist |
| JSON/YAML | borderline | Common but text-diff is an acceptable fallback; structural diff semantics are debatable (tree diff vs line diff) |
| SQLite | plugin | Application format; requires bundled C library (`rusqlite`) |
| Excel, Parquet, HDF5 | plugin | Domain-specific; heavy or native dependencies |

Transformers and renderers follow the same logic: move/copy detection and Markdown output are generally useful and have no external dependencies, so they belong in stdlib. A domain-specific transformer (e.g., database migration generation) belongs in the plugin that understands the domain.

## Alternatives Considered

- **"No format-specific libraries"**: Too restrictive — excludes `csv` and `zip`, which are essential. The real line is dependency *weight*, not dependency *existence*.
- **"Top N most requested formats"**: No principled stopping point. Leads to stdlib bloat and forces every user to compile formats they don't need.
- **"Everything, behind feature flags"**: Technically possible but adds conditional-compilation complexity, confuses the plugin story ("why is sqlite a feature flag here but a plugin there?"), and still increases maintenance burden.
