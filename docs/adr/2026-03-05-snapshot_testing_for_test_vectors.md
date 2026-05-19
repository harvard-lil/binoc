# Snapshot Testing for Test Vectors

**Date:** 2026-03-05
**Status:** Implemented

## Problem

Test vectors had structural assertions in their manifests (`root_kind`, `child_count`, `has_tags`, `significance`) but no record of full output. This meant:

- You couldn't look at a test vector and see what binoc actually produces for it. The vectors were documentation of *inputs* but not *outputs*.
- Coarse structural assertions missed unintended changes to formatting, field ordering, detail content, or tag sets. A refactor that changed the JSON shape or the Markdown rendering could pass all tests while silently altering output.
- DESIGN.md described optional `expected-changeset.json` gold files but they were never implemented — the stub was a no-op.

## Decision

**Each test vector stores full expected output as `insta` snapshots in `expected-output/`.** The test harness renders both JSON (the changeset IR) and Markdown (the changelog) and asserts them via `insta`. Manifest structural assertions remain the primary stability check; snapshots are the secondary full-output check.

### Layout

```
test-vectors/csv-column-reorder/
  manifest.toml
  snapshot-a/
  snapshot-b/
  expected-output/
    changeset.snap      # full IR as JSON
    changelog.snap      # rendered Markdown changelog
```

### Path normalization

`Changeset.from_snapshot` and `to_snapshot` contain absolute paths that vary per machine. The test harness clones the changeset and replaces these with `"snapshot-a"` and `"snapshot-b"` before snapshotting, so `.snap` files are machine-independent.

### Workflow

- `cargo test` — asserts snapshots match (the CI default; fails on drift).
- `just snapshot-review` — runs tests, then opens insta's interactive accept/reject UI.
- `just snapshot-update` — bulk-accepts all changes (`INSTA_UPDATE=always`).

## Why `insta`?

It was already a dev-dependency (unused) and is the dominant Rust snapshot crate (~11M downloads/90 days). Key features:

- `Settings::set_snapshot_path` lets us store `.snap` files alongside the test vectors they document, rather than next to the test source file.
- `assert_json_snapshot!` serializes the Changeset struct directly, giving deterministic JSON formatting.
- `cargo insta test --review` provides an interactive diff-and-accept workflow for intentional changes.
- The `INSTA_UPDATE` env var gives a clean "regenerate all" escape hatch.

## Alternatives considered

- **`expect-test` with `expect_file!`:** Lighter weight (no CLI tool), but no interactive review, no JSON-aware diffing, and the file path syntax is relative to the source file rather than configurable per-test.
- **`trycmd` / `snapbox`:** Good for CLI binary testing but oriented around stdout/stderr capture, not library-level output. Could complement this for CLI-specific snapshot tests later.
- **Hand-managed gold files with a custom diff script:** What DESIGN.md originally described. `insta` gives this for free with better tooling.
