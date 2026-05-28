# Test Vector Root Defaults and Plugin Test Vectors

**Date:** 2026-03-09
**Status:** Implemented

## Context

We wanted (1) a way to set default config/assertions for all vectors under a test-vector root, with per-vector overrides, and (2) plugin crates (e.g. binoc-sqlite) to ship and run their own test vectors without checking in binary artifacts.

## Decision

### Root manifest defaults

- **Workspace** `test-vectors/manifest.toml` may define optional `[config]` and `[expected]` with no `[vector]` section. This file is the "root defaults" for that tree.
- When loading a vector's `manifest.toml`, the harness loads the root manifest (if present) and merges: the vector's `[config]` and `[expected]` override the root's when present; otherwise root values are used.
- Only subdirectories that contain both `manifest.toml` and `snapshot-a/` and `snapshot-b/` are considered vectors; the root `manifest.toml` is not itself a vector.

### Plugin test vectors

- Plugin crates may ship test vectors under their own tree (e.g. `binoc-sqlite/test-vectors/`) with the same layout and manifest format. The same root-defaults merge applies: `binoc-sqlite/test-vectors/manifest.toml` provides default config (e.g. comparators including `binoc-sqlite.sqlite`); individual vectors can override (e.g. `without-plugin` uses a config that omits the SQLite comparator).
- Each plugin that has test vectors adds an integration test (e.g. `binoc-sqlite/tests/test_vectors.rs`) that discovers vectors in that crate's `test-vectors/`, builds any binary artifacts from source (see below), and runs the same run/assert/snapshot flow using a registry that includes the plugin.
- `just test` / `cargo test` therefore runs both workspace test-vectors (stdlib) and plugin test-vectors (e.g. binoc-sqlite) without extra wiring.

### Building binaries on the fly

- **Zips:** Existing convention: `.zip.d/` directories are built into `.zip` files by the harness, then the `.zip.d` dirs are removed so comparators only see the zip. Workspace vectors keep `.zip` in repo or use `.zip.d`; the harness mutates the vector dir (stdlib) or, for plugins, we copy snapshots to a temp dir first so the repo is not mutated.
- **SQLite:** Building SQLite from `.sqlite.d`/`.db.d` is a **plugin** concern, not part of the shared harness. The harness copies to temp and builds zips only; it accepts an optional `prepare(snap_a, snap_b)` callback. The binoc-sqlite plugin passes a prepare closure that builds `.sqlite` from `.sqlite.d` (using rusqlite) and removes the `.d` dirs. The shared harness stays free of rusqlite and other plugin-specific deps.

## Alternatives considered

- **Single global test-vector root with env/config to enable plugins:** Would require the main test runner to know about every plugin and its registry. Keeping plugin vectors and their test in the plugin crate keeps the plugin self-contained and movable.
- **Leaving `.sqlite.d` in place and not removing it:** The directory comparator would then see both `data.sqlite` and `data.sqlite.d/`, producing extra nodes and confusing expectations. Building in a temp copy avoids mutating the repo and keeps the comparator input clean.
