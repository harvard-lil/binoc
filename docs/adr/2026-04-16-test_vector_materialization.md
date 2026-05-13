# Test vector materialization: plugin trait, not a runtime plugin point

**Date:** 2026-04-16
**Status:** Implemented

## Context

Test vectors under `test-vectors/` commit *source* trees (e.g.
`archive.zip.d/` with the text files that should go into `archive.zip`)
instead of opaque binaries, so that diffs are reviewable and vectors don't
balloon the repo. Something has to turn those staging directories into the
real artifacts before the diff engine sees them. Initially this logic lived
in three places:

1. The Rust test harness (`binoc-stdlib::test_vectors`) built `.zip`/`.tar`
   inline into a tempdir for each test run.
2. The Python test harness (`binoc-python/python/binoc/testing.py`)
   reimplemented the same builders in Python.
3. `binoc-sqlite`'s tests passed a custom `prepare(snap_a, snap_b)` callback
   to the Rust harness to build `.sqlite` from `.sqlite.d/*.sql`.
4. `docs/tutorial.md` had an ad-hoc shell loop for the SQLite demo and a
   separate ad-hoc `cargo run ... materialize-test-vectors` for the zip demo.

Four ways to do the same thing is too many. We also wanted `just materialize`
to produce a browsable `test-vectors-materialized/` tree that users and the
tutorial could reference directly.

## Decision

Introduce a small `VectorMaterializer` trait inside
`binoc_stdlib::test_vectors`:

```rust
pub trait VectorMaterializer: Send + Sync {
    fn suffixes(&self) -> &[&'static str];
    fn build(&self, staging_dir: &Path, out_path: &Path, all_staging_suffixes: &[&str]);
}
```

Plugins instantiate a materializer and pass it as `&[&dyn VectorMaterializer]`
into `materialize_snapshots` / `run_vector` / `run_vector_with_abi_log`
alongside `stdlib_materializers()`. A single walker processes staging dirs
innermost-first so nested staging is handled correctly.

`materialize_snapshots` now copies the full vector directory (including
`manifest.toml` and `expected-output/`) so `test-vectors-materialized/` is a
drop-in replacement for `test-vectors/`.

Every workspace crate that ships vectors adds a thin `src/bin/
materialize_test_vectors.rs` that composes `stdlib_materializers()` with its
plugin-specific builders. `just materialize` runs each one. The Rust test
harness uses the same materializer slice, so `just test` and `just materialize`
produce byte-identical trees.

## Alternatives considered

**(A) Materializer as an ABI plugin registration point.** Would let a future
`binoc materialize` CLI auto-discover materializers from installed plugins.
Rejected because materialization is strictly a test/dev-time concern: the
shipped cdylib / Python wheel never needs this code, and crossing the C ABI
for in-process shell-out-free file I/O buys nothing. Registration is instead
done at the call site with a Rust `Vec<&dyn VectorMaterializer>`. If we ever
need cross-process discovery we can graduate the trait into the ABI then —
greenfield rule applies.

**(B) One materialize binary in `binoc-stdlib` that dlopens each plugin.**
Would let `just materialize` be one command, but pulls the whole native
plugin loading stack into a dev-only path. Each plugin shipping its own tiny
binary (composing `stdlib_materializers()`) is simpler and keeps the plugin
repo self-contained.

**(C) Keep the Python zip/tar builders in `binoc.testing`.** They were unused
by any in-repo test and duplicated the Rust builders. Dropped. Python plugin
authors shell out to `cargo run ... materialize-test-vectors` once per pytest
session; the docstring shows the pattern.

**(D) Keep the `prepare(&Path, &Path)` callback.** A closure is slightly less
code at the call site but makes the tests harness and the `just materialize`
binary use different mechanisms, which was the original problem. The trait
collapses them.

## Consequences

- **One walker.** All staging dirs across stdlib + plugins are built in a
  single innermost-first traversal, so nested staging (e.g. a `.zip.d`
  sibling inside a `.sqlite.d`) works correctly if it ever arises.
- **`test-vectors-materialized/` is a drop-in replacement for
  `test-vectors/`** (manifests and expected-output included). The tutorial
  references it directly.
- **Plugins document their contract.** A materializer declares its suffixes,
  so plugin READMEs and `writing_plugins.md` can point at a single trait
  rather than a grab-bag of conventions.
- **No impact on shipped artifacts.** `SqliteMaterializer` lives behind
  `binoc-sqlite`'s `test-support` feature; the published cdylib / wheel does
  not compile it.
