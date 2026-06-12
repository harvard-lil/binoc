---
audience: core contributor
---

# Contribute to binoc

**Goal.** Go from a fresh clone to a merged PR.

**Prerequisites.**
- [Rust](https://rustup.rs/)
- [just](https://github.com/casey/just) (`brew install just` on
  macOS)
- [uv](https://docs.astral.sh/uv/)

If you only want to use binoc, you don't need any of this — see
[Diff two snapshots](../../users/howto/diff-two-snapshots.md) instead.

## Architectural ground rules

Read [`AGENTS.md`](https://github.com/harvard-lil/binoc/blob/main/AGENTS.md)
first. It's the architectural contract for both human and AI
contributors. Headlines:

- **The controller is type-ignorant.** Never add format knowledge to
  `binoc-core`. See [Architecture overview](../../plugin-developers/explanation/architecture.md).
- **The standard library is a plugin pack.** It has no special
  status relative to third-party plugins.
- **The library is the product; the CLI is porcelain.** Design APIs
  for embedding first.
- **Greenfield.** Note breaking changes, but don't spend effort on
  backwards compatibility.

Longer form: the [architecture overview](../../plugin-developers/explanation/architecture.md)
narrates the system; the [ADR set](../../adr/README.md) is the canonical
record of every design decision with rejected alternatives.

## First-time setup

```bash
git clone https://github.com/harvard-lil/binoc
cd binoc
just build       # Rust workspace + Python bindings (debug)
just test        # Full suite: Rust + Python
```

For a local CLI in your path:

```bash
uv venv
uv pip install -e ./binoc-python
source .venv/bin/activate
```

Or run the dev CLI with every local plugin wired up, no global
install:

```bash
just binoc diff path/to/snapshot-a path/to/snapshot-b
```

See the existing [tutorial](../../tutorial.md) for a fuller walkthrough
that regularly re-verifies against the current code via
[Showboat](../../adr/2026-03-06-tutorial_regeneration_lifecycle.md).

## The everyday loop

```bash
# 1. Make your change

just fmt           # auto-format Rust + Python
just check         # mirror CI (clippy, rustfmt --check, ruff)
just test          # Rust + Python test suites
```

For focused iteration: `cargo build` or `cargo test -p binoc-core`
(etc.) skip the Python crate and are much faster than `just test`.

## Where things live

| You want to… | Start in… |
|---|---|
| Add a new test vector | [`test-vectors/`](https://github.com/harvard-lil/binoc/tree/main/test-vectors) — create `snapshot-a/`, `snapshot-b/`, `manifest.toml`. See [Test vectors explanation](../../plugin-developers/explanation/test-vectors.md). |
| Add a stdlib comparator or transformer | `binoc-stdlib/src/comparators/` or `.../transformers/`. For third-party plugins, see [Write a Python comparator](../../plugin-developers/howto/write-a-python-comparator.md) / [Write a Rust comparator](../../plugin-developers/howto/write-a-rust-comparator.md). |
| Change the IR | `binoc-sdk/src/ir.rs` (wire types) and `binoc-core/src/ir.rs` (in-memory additions). IR changes are high-leverage; read [IR and changesets](../../plugin-developers/explanation/ir-and-changesets.md) first. |
| Fix the controller / dispatch logic | `binoc-core/src/controller.rs`. See [Dispatch model](../../plugin-developers/explanation/dispatch-model.md). |
| Fix the CLI | `binoc-cli/src/lib.rs` (the library); `binoc-cli/src/main.rs` is a thin wrapper around `binoc_cli::run()`. |
| Add a Python API surface | `binoc-python/src/lib.rs` (PyO3) and `binoc-python/python/binoc/__init__.py`. |
| Change Markdown grouping behavior | `binoc-stdlib/src/renderers/markdown.rs` — `MarkdownRendererConfig`, `MarkdownGroup`, and `render_markdown`. See [Significance classification](../../users/explanation/significance-classification.md). |

## Test vectors are the cheap contribution

Adding a new test vector is the lowest-friction way to contribute a
meaningful improvement. You get:

1. A named fixture that documents a capability (or a bug) in
   `test-vectors/`.
2. Coverage in `just test` via the shared harness.
3. Reference material that appears in the documentation and
   tutorial.

The convention: name vectors for **what they test**
(`csv-column-reorder`), not how (`test-comparator-csv-3`). See
[Test vectors](../../plugin-developers/explanation/test-vectors.md).

## Documentation

Docs live in `docs/` and are published via MkDocs. See
[Write and maintain docs](write-docs.md) for the full guide.
Quick reference:

```bash
just docs          # regenerate all generated inputs
just docs-serve    # live preview at 127.0.0.1:8000
just docs-build    # strict build (CI mirror); fails on broken links or orphan pages
```

If you change behavior the tutorial demonstrates, regenerate it with
`just docs-tutorial` ([Showboat](../../adr/2026-03-06-tutorial_regeneration_lifecycle.md)).

## PR conventions

- **Small, focused PRs.** Easier to review, easier to roll back.
- **Tests for new behavior.** A test vector often suffices.
- **Update docs** when you change user-visible behavior.
- **Note breaking changes** in the PR description, but don't spend
  effort on backwards compatibility — see `AGENTS.md`.
- **ADR** any design decision whose rejected alternatives are
  non-obvious.

## Where to go next

- [Architecture overview](../../plugin-developers/explanation/architecture.md) — build
  the mental model before making a structural change.
- [Test vectors](../../plugin-developers/explanation/test-vectors.md) — the easy-first
  contribution path.
- [Cut a release](cut-a-release.md) — once your change is merged and
  deserves a version bump.
