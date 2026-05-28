# Shared Test-Vector Harness for Plugins

**Date:** 2026-03-09
**Status:** Implemented

## Context

Plugin crates (e.g. binoc-sqlite) wanted to run test vectors with the same manifest format, root-defaults merge, zip/sqlite build-from-source, assertions, and snapshot flow as the workspace test-vectors. Duplicating that logic in each plugin would be brittle and would not scale for third-party plugins that might live in a separate repo.

## Decision

**The test-vector harness lives in `binoc-stdlib` and is exposed as a public API when the default feature `test-vectors` is enabled.** Plugins (and the stdlib’s own integration test) call `binoc_stdlib::test_vectors::discover_vectors(vectors_dir)` and `binoc_stdlib::test_vectors::run_vector(vector_dir, vectors_root, registry_builder)` instead of reimplementing manifest parsing, copy-to-temp, zip/sqlite building, assertions, and insta snapshots.

- **No new crate:** Keeping the harness in binoc-stdlib avoids a separate `binoc-test-harness` crate. Plugins that extend the default pipeline already depend on binoc-stdlib (or can add it as a dev-dependency); they enable the default feature (or explicitly `features = ["test-vectors"]`) to get the harness.
- **Registry builder:** `run_vector` takes a `registry_builder: impl FnOnce() -> PluginRegistry` so each test can supply a registry that includes the plugin (e.g. `default_registry()` for stdlib, or `|| { let mut r = default_registry(); register_sqlite(&mut r); r }` for binoc-sqlite).
- **Uniform behavior:** The harness always copies snapshot-a and snapshot-b to a temp dir, then builds zips from `.zip.d` there, so the repo is never mutated. Building other artifacts (e.g. SQLite from `.sqlite.d`) is a plugin concern: the plugin passes an optional `prepare(snap_a, snap_b)` callback that runs after the zip step. That keeps the stdlib free of plugin-specific deps (e.g. rusqlite).
- **`just test`:** Runs `cargo test`, which runs tests for all workspace crates including binoc-sqlite. No auto-discovery of plugins is required; the demo plugin is simply part of the workspace.

## Alternatives considered

- **Dedicated `binoc-test-harness` crate:** Would centralize the logic without pulling insta/toml/rusqlite into binoc-stdlib’s default build, but adds a crate to maintain and would require the harness to depend on binoc-core (and possibly binoc-stdlib for markdown/significance). Keeping the harness in binoc-stdlib with an optional feature keeps one fewer crate and matches the fact that many plugins already want stdlib for `default_registry()`.
- **Making the feature opt-in instead of default:** Would avoid pulling test deps for users who use `default-features = false`, but would require `just test` (or CI) to pass `--features test-vectors` for binoc-stdlib. We chose default-on so that `cargo test` and `just test` work without extra flags.
