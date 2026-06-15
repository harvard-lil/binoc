# AGENTS.md

## Project Overview

Binoc generates changelogs for datasets that don't ship with them. Given two snapshots of a dataset, it detects structural and content changes, records them as a changeset tree (the IR), and renders changes as JSON or Markdown. The primary audience is archivists, data scientists, and stewards tracking undocumented changes to published datasets.

Rust workspace with five crates:

| Crate | Role |
|---|---|
| `binoc-core` | Controller loop, correspondence driver, config, output |
| `binoc-stdlib` | Standard correspondence rule pack and renderers (directory, zip, CSV, text, binary, move/copy detection) |
| `binoc-cli` | CLI porcelain over the core library |
| `binoc-python` | PyO3 bindings and Python plugin support |
| `binoc-sqlite` | Demo plugin: SQLite schema and row count diffing (also a reference for plugin authors) |

Shared test fixtures live in `test-vectors/`. ADRs live in `docs/adr/`.

## Key Architectural Rules

1. **The controller is type-ignorant.** It hands snapshots to the correspondence engine and projects the result. It does not know about files, directories, archives, or any data format. Never add format knowledge to `binoc-core`.

2. **The standard library (`binoc-stdlib`) is a plugin pack**, architecturally identical to third-party packs. The core engine has zero domain knowledge—not even about directories or text files.

3. **Correspondence rules own domain knowledge.** Expand/parse rules turn raw data into side-tree items and artifacts; pair rules propose links; edit-list writers explain links; compaction rules optimize edit lists; projection annotators add factual projection hints. **Significance classification is a renderer concern**, mapped from semantic tags via config—not baked into the IR.

4. **The IR is tree-structured, openly typed, and tag-annotated.** `action`, `item_type`, and `tags` are open enums/strings. No built-in types or significance levels. Conventions, not enforcement.

5. **Dispatch is declarative-first** (shape/type/extension/media/artifact filters) **with narrow rule-level self-filters** for unavoidable content or producer-kind checks. First successful expand/parse claim wins for that node/format; pair-rule priority is configuration order. Ordering is a config concern, not a plugin concern.

6. **The library is the product; the CLI is porcelain.** Design APIs for embedding first, CLI consumption second.

7. **No global state. No process-level side effects.** Configuration is passed in, not read from the environment.

8. **Build the plugin seam everywhere; build transit machinery only at the stable tier.** Every plugin-facing type stays serializable and free of host internals, so any extension point *can* later move behind a process boundary. During pre-1.0, only the stable tier (renderers; expand/parse packs once their shapes settle) gets the C ABI/wire machinery; fine-grained rule families (pair, writer, compaction, annotator) are in-process until their trait signatures and vocabularies stop moving. In-tree plugins like binoc-sqlite are first-party consumers of the SDK traits during this phase, not proof of the arm's-length boundary. See [the tiered plugin surface ADR](docs/adr/2026-06-12-tiered_plugin_surface_pre_1_0.md).

9. **As of now this is greenfield development.** You can note backwards incompatible changes, but don't put any effort into backwards compatibility.

## Build & Test

```bash
just build                    # Python package (primary target), debug mode
just build-release            # optimized Rust binaries + Python package
just test                     # full suite: Rust crates + Python tests
just docs                     # regenerate tutorial after code changes
just binoc diff snap-a snap-b # run binoc CLI with auto-rebuild
```

For Rust-only iteration: `cargo build`, `cargo test`, or by crate: `cargo test -p binoc-core`, etc.

**After making changes**, run `just fmt` to auto-format, then `just check && just test` to verify CI will pass. `just check` runs clippy, rustfmt (verify), and ruff (lint + format verify). `just test` runs the full Rust and Python test suites.

## Test Vectors

Each vector in `test-vectors/` has a `manifest.toml` declaring what it tests and structural assertions. Vectors are named for what they test (`csv-column-reorder`), not how (`test-csv-3`). Structural assertions in manifests are the primary check; gold files are secondary. To keep binaries out of version control, vectors commit *source* trees (`archive.zip.d/`, `data.sqlite.d/*.sql`, …) that are built into real artifacts by [`VectorMaterializer`](docs/adr/2026-04-16-test_vector_materialization.md) implementations. The stdlib ships `ZipMaterializer` and `TarMaterializer`; plugins contribute their own (e.g. `binoc_sqlite::test_support::SqliteMaterializer`). Both `just test` (via the test harness) and `just materialize` (which produces a gitignored `test-vectors-materialized/` tree) go through the same builders.

Plugins (including those in a separate repo) can run test vectors without duplicating harness code: depend on `binoc-stdlib` with the default `test-vectors` feature and call `binoc_stdlib::test_vectors::{discover_vectors, run_vector_with_correspondence_engine_config}` with a `CorrespondenceEngineConfig` that includes the plugin and a `&[&dyn VectorMaterializer]` slice. See `model-plugins/binoc-sqlite/tests/test_vectors.rs` and its sibling `src/bin/materialize_test_vectors.rs`. `just test` runs all workspace crates’ tests, including the demo plugin binoc-sqlite; no auto-discovery is required.

## Invariants and Lints

Invariant checks live in three tiers (see [the ADR](docs/adr/2026-06-12-invariant_and_lint_tiers.md)): (1) changeset invariants run on every test vector — add always-properties to `binoc_stdlib::test_vectors::check_changeset_invariants`; (2) mechanical lints in `binoc_sdk::lints`, called from each crate's `tests/lints.rs` (copy `model-plugins/binoc-sqlite/tests/lints.rs`); errors fail `just test`, warnings surface via `just lint`; (3) judgment-level checks for reviewing plugins — behavioral write-set completeness, performance, security, layering — documented in [.agents/skills/lint-plugin/SKILL.md](.agents/skills/lint-plugin/SKILL.md). When reviewing or adding a plugin, run that checklist.

## Performance Expectations

Rust core, parallel subtrees (rayon), streaming I/O, BLAKE3 hashing, arena-allocated IR nodes. Plugins should be computationally lean—streaming I/O, minimal allocations, no unnecessary re-parsing.

## Logging design decisions

If you make a design decision worth recording separately from the code (the alternatives rejected may arise again), log it in `docs/adr/`:

    # Title

    **Date:** 2026-03-06
    **Status:** Implemented

    ## Context

    ## Decision

    ## Alternatives Considered

Add a short summary with date to `docs/adr/README.md`.

## Writing docs

See [2026-04-17-documentation_platform_and_info_design.md](docs/adr/2026-04-17-documentation_platform_and_info_design.md) for the documentation platform and information design decisions.
