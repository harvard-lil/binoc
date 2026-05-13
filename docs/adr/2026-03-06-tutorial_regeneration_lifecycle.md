# Tutorial Regeneration Is a Build Step, Not a Test

**Date:** 2026-03-06
**Status:** Implemented

## Problem

`docs/TUTORIAL.md` is executable documentation: [Showboat](https://github.com/jcushman/showboat) runs every fenced `bash` block and verifies the output matches. When code changes, the tutorial needs to be regenerated so outputs stay in sync.

The original approach put regeneration inside `cargo test` as `binoc-cli/tests/tutorial.rs`. This had three problems:

1. **It wasn't a test.** The function never failed — it silently rewrote the file and returned `ok`. A test that always passes and has side effects is a build step hiding in test clothing.
2. **It was slow.** The tutorial's executable blocks included `cargo build --release` and `cargo test`, so running `cargo test` triggered a nested build-and-test cycle.
3. **It invited recursion.** Any executable block in the tutorial that itself invokes showboat (e.g., `just docs`) creates an infinite fork bomb. The original code had an `BINOC_INSIDE_SHOWBOAT` env-var guard, but the real fix is not to mix the two lifecycles.

## Decision

**Tutorial regeneration lives in the justfile; `cargo test` doesn't touch it.**

- `just docs` runs `uvx showboat verify docs/TUTORIAL.md --output docs/TUTORIAL.md`. Contributors run it after code changes that affect tutorial output.
- The tutorial's "Building and Testing" section uses indented code blocks (not fenced `bash`) for commands like `cargo build`, `cargo test`, and `just docs`. Showboat only executes fenced blocks, so these are rendered as code for the reader but skipped during regeneration.
- There is no staleness check in `cargo test`. The tutorial is documentation, not a correctness invariant — it should reflect current code rather than gate CI.

## Alternatives considered

- **Verify-only test (fail if stale):** Would catch drift during local development, but adds ~30s to every `cargo test` run for a doc-freshness check. Not worth the cost for a v1 project where the tutorial changes infrequently.
- **Pre-commit hook:** Attractive for automation, but showboat executes every block in the tutorial (including builds), making it too slow to gate every commit.
- **`build.rs` script:** `cargo build` runs `build.rs` on every compile. Regenerating docs on every build is far too aggressive.
