# Rust MSRV and dependency update policy

**Date:** 2026-04-10
**Status:** Implemented

## Context

Binoc is a library-first Rust workspace with published and unpublished crates, plus Python packages built on top of Rust components. Before this ADR, the repository had no explicit minimum supported Rust version (MSRV). In practice, the supported compiler version was whatever happened to work with the current dependency set on the latest stable toolchain.

That left two problems:

1. Dependency upgrades could silently raise the compiler floor without an explicit decision.
2. Plugin authors and embedders had no clear answer for which Rust version Binoc expects.

At the same time, the project is still pre-1.0 and greenfield. We do not need to preserve support for old toolchains at high maintenance cost, but we do want a clear and intentional compatibility story.

## Decision

Binoc declares and tests an explicit MSRV.

- The workspace MSRV is `1.88`.
- All workspace crates inherit that value through `rust-version.workspace = true`.
- CI tests both the latest stable Rust toolchain and the MSRV toolchain.
- MSRV bumps are allowed, but they must be intentional and called out in dependency or release work rather than arriving accidentally through routine updates.

Dependency policy follows from that decision:

- Minor and patch dependency updates are routine when they preserve behavior and keep the repository on the declared MSRV.
- Major dependency updates receive manual review for API churn, MSRV impact, and release stability.
- Python dependency metadata should remain broadly compatible where safe and reasonable; lockfiles and CI tooling may track newer versions without narrowing published runtime constraints unnecessarily.

## Alternatives Considered

- **No explicit MSRV:** Simplest in the short term, but it lets transitive dependency changes define compiler support accidentally and leaves users guessing.
- **Support only latest stable Rust:** Lowest maintenance burden, but too vague for a library-first project with third-party plugin authors and downstream embedders.
- **Support a much older compiler line:** More conservative for downstream users, but not justified for a pre-1.0 project moving quickly with dependencies such as PyO3, zip, and libloading.
