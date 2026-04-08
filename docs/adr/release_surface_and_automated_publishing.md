# Release Surface And Automated Publishing

**Date:** 2026-04-08
**Status:** Implemented

## Context

Binoc now has a clearer packaging split than the early workspace had:

- `binoc` is the primary user-facing Python distribution.
- `binoc-sqlite` is the first public plugin package.
- `binoc-sdk` is the stable Rust surface for plugin authors.
- `binoc-core`, `binoc-stdlib`, `binoc-cli`, and the remaining model plugins are useful internally, but publishing all of them immediately would create a larger compatibility surface than the project needs at this stage.

The project also needs a modern release path:

- Python packages should publish wheels, not source-only archives.
- Registry credentials should not be long-lived repository secrets.
- Release mechanics should be repeatable and documented.

## Decision

Binoc publishes exactly three artifacts for now:

- PyPI: `binoc`
- PyPI: `binoc-sqlite`
- crates.io: `binoc-sdk`

All other Rust crates in the workspace set `publish = false`.

Releases are automated from GitHub Actions on `v*` tags:

- `binoc` and `binoc-sqlite` build abi3 wheels and sdists with maturin.
- PyPI publishing uses GitHub OIDC trusted publishing.
- `binoc-sdk` publishing uses crates.io trusted publishing via GitHub OIDC.

The root `just release` task is the canonical release entry point. It runs the test suite, verifies that the published package versions are synchronized, pushes the current branch, creates an annotated tag, and pushes that tag to trigger publishing.

## Alternatives Considered

### Publish every Rust crate now

Rejected because it would commit the project to a broader Rust support surface before there is a concrete need for external consumers of `binoc-core`, `binoc-stdlib`, or `binoc-cli`.

### Publish only PyPI packages and skip crates.io for now

Rejected because `binoc-sdk` is now the documented dependency for Rust plugin authors and should exist where those authors expect to obtain it.

### Use API tokens stored as GitHub secrets

Rejected in favor of trusted publishing. OIDC-based publishing is narrower in scope, avoids long-lived credentials, and matches the current deployment direction used elsewhere in the project ecosystem.

### Keep per-Python-version wheels

Rejected in favor of `abi3-py310`. The Binoc Python packages do not need per-version extension builds, and abi3 substantially reduces the wheel matrix while preserving support for Python 3.10+.
