---
audience: plugin author
---

# Test a plugin with vectors

**Goal.** Use the shared test-vector harness to run end-to-end tests
for your plugin — same manifest format, same assertions, same
snapshot flow as the standard library.

**Prerequisites.** A Rust plugin (see
[Write a Rust comparator](write-a-rust-comparator.md)). The shared
harness is Rust-first; Python-only plugins can still write vectors
and run them through `binoc.diff()` in pytest.

For the design record, see
[Shared test-vector harness ADR](../adr/2026-03-09-plugin_test_vector_harness.md)
and [Test vector materialization ADR](../adr/2026-04-16-test_vector_materialization.md).

## What a test vector is

A vector is a directory containing:

```
my-vector/
├── manifest.toml
├── snapshot-a/   # files or source trees to diff
└── snapshot-b/
```

`manifest.toml` declares what the vector tests and what to assert:

```toml
[vector]
name = "fasta-header-change"
description = "Header metadata changed; sequences identical"
tags = ["fasta", "clerical"]

[config]
comparators = ["binoc.directory", "biobinoc.fasta"]
transformers = ["biobinoc.sequence_normalizer"]

[expected]
root_kind = "modify"
child_count = 1
has_tags = ["biobinoc.whitespace-only"]
significance = "clerical"
```

Structural assertions (`root_kind`, `child_count`, `has_tags`,
`significance`) are the primary check — they survive IR schema
evolution. Golden output files are optional and secondary. Vectors
are named for **what they test**, not how
(`fasta-header-change`, not `test-fasta-3`).

## Wire the harness into your crate

Add a dev dependency on `binoc-stdlib` with the `test-vectors`
feature (on by default):

```toml
[dev-dependencies]
binoc-core   = { version = "0.1" }
binoc-sdk    = { version = "0.1", features = ["test-support"] }
binoc-stdlib = { version = "0.1", features = ["test-vectors"] }
```

Then write a `tests/test_vectors.rs` that discovers your vectors and
runs each one against a registry that includes your plugin:

```rust
use binoc_stdlib::test_vectors::{
    discover_vectors, run_vector, stdlib_materializers, VectorMaterializer,
};

#[test]
fn test_vectors() {
    let stdlib = stdlib_materializers();
    let materializers: Vec<&dyn VectorMaterializer> =
        stdlib.iter().map(|m| &**m).collect();

    let vectors_root = "tests/test-vectors";
    for vector in discover_vectors(vectors_root) {
        run_vector(
            &vector,
            vectors_root.as_ref(),
            || {
                let mut r = binoc_stdlib::default_registry();
                // register your plugin into r
                my_plugin::register(&mut r);
                r
            },
            &materializers,
        );
    }
}
```

`cargo test` is now all you need — the harness handles manifest
parsing, temp-dir setup, running the full pipeline, and checking
assertions.

## Source-tree vectors and `VectorMaterializer`

Binary artifacts (`.zip`, `.tar.gz`, `.sqlite`, …) are kept out of
git. Vectors commit *source* trees — `archive.zip.d/`,
`data.sqlite.d/*.sql` — that the harness builds into real artifacts
at test time. The stdlib ships builders for `.zip.d`, `.tar.gz.d`, and
`.gz.d`. If your plugin's format has no suitable builder, implement the
`VectorMaterializer` trait once and reuse it for both tests and the
`just materialize` CLI:

```rust
use std::path::Path;
use binoc_stdlib::test_vectors::VectorMaterializer;

pub struct FastaBundleMaterializer;

impl VectorMaterializer for FastaBundleMaterializer {
    fn suffixes(&self) -> &[&'static str] { &[".fabundle.d"] }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all: &[&str]) {
        // walk staging_dir, write out_path
    }
}
```

Gate this type behind a `test-support` feature on your crate so it
stays out of the production cdylib / wheel, and wire it into your
test and into a `src/bin/materialize_test_vectors.rs` that composes
with `stdlib_materializers()`. The SQLite reference plugin is the
canonical example — see
[`model-plugins/binoc-sqlite/src/test_support.rs`](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite/src/test_support.rs)
and its sibling `materialize_test_vectors.rs` binary.

## Testing Python-only plugins

If your plugin is pure Python, skip the Rust harness and run a
pytest that invokes `binoc.diff()`:

```python
import binoc
import my_plugin

def test_fasta_header_change():
    config = binoc.Config.default()
    config.add_comparator(my_plugin.FastaComparator())
    config.add_transformer(my_plugin.SequenceNormalizer())

    changeset = binoc.diff(
        "tests/fixtures/fasta-header-change/snapshot-a",
        "tests/fixtures/fasta-header-change/snapshot-b",
        config=config,
    )

    root = changeset.root
    assert root is not None
    assert root.action == "modify"
    assert "biobinoc.whitespace-only" in _all_tags(root)

def _all_tags(node):
    yield from node.tags
    for child in node.children:
        yield from _all_tags(child)
```

A future follow-up may expose the manifest-based vector harness to
Python plugins directly; until then, pytest + `binoc.diff()` is the
recommended path.

## Where to go next

- [Test vector materialization ADR](../adr/2026-04-16-test_vector_materialization.md)
  — the design of the source-tree-to-artifact builders.
- [Test vectors explanation](../explanation/test-vectors.md) — the
  broader picture of how the project's test corpus is organized.
- [Write a Rust comparator](write-a-rust-comparator.md) — the
  plugin you'll be testing here.
