---
audience: Rust plugin author
---

# Write a Rust comparator

**Goal.** Build a Rust comparator that runs at native speed via the
C ABI, ending with a working plugin packaged as a Python extension
module that `pip install` makes available to the `binoc` CLI
automatically.

**Prerequisites.**
- Rust toolchain ([rustup](https://rustup.rs/)).
- [`maturin`](https://www.maturin.rs/) (installed automatically when
  you build via `uv`).
- Familiarity with [Plugin model](../explanation/plugin-model.md).

For a complete reference implementation, read
[`model-plugins/binoc-sqlite`](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite)
alongside this recipe.

## Project layout

```
biobinoc/
├── Cargo.toml
├── pyproject.toml
├── src/
│   ├── lib.rs          # export_plugin! + pub use
│   └── fasta.rs        # comparator implementation
└── tests/
    └── test_vectors.rs # optional; see "Test a plugin with vectors"
```

## `Cargo.toml`

```toml
[package]
name = "biobinoc"
version = "0.1.0"
edition = "2021"

[lib]
name = "biobinoc"
crate-type = ["cdylib", "rlib"]

[features]
default = []
python = ["dep:pyo3"]

[dependencies]
binoc-sdk = "0.1"
serde_json = "1"
pyo3 = { version = "0.27", features = ["extension-module"], optional = true }
```

The `python` feature is only needed so the `export_plugin!` macro can
generate the PyO3 module stub maturin requires. Your plugin code
never touches PyO3 directly.

## `src/fasta.rs` — the comparator

```rust
use binoc_sdk::*;

#[derive(Default)]
pub struct FastaComparator;

impl Comparator for FastaComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("biobinoc.fasta").with_extensions(
            vec![".fasta".into(), ".fa".into(), ".fna".into()],
        )
    }

    fn compare(
        &self,
        pair: &ItemPair,
        data: &dyn DataAccess,
    ) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let l = data.read_bytes(left)?;
                let r = data.read_bytes(right)?;
                if l == r {
                    return Ok(CompareResult::Identical);
                }
                let node = DiffNode::new("modify", "fasta", pair.logical_path())
                    .with_tag("biobinoc.sequence-changed")
                    .with_summary("FASTA sequences changed");
                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => Ok(CompareResult::Leaf(
                DiffNode::new("add", "fasta", &right.logical_path),
            )),
            (Some(left), None) => Ok(CompareResult::Leaf(
                DiffNode::new("remove", "fasta", &left.logical_path),
            )),
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
```

Key points:

- **Plugin structs must implement `Default`.** The `export_plugin!`
  macro constructs them.
- **All I/O goes through `&dyn DataAccess`.** Do not use `std::fs`
  directly. `data.read_bytes(item)` returns the content;
  `data.local_path(item)` returns a filesystem path (for libraries
  that require one, like SQLite); `data.open_read(item)` streams.
- **Dispatch is declarative.** `ComparatorDescriptor` declares
  extensions, media types, and scope. If the descriptor matches but
  the data turns out to be unsuitable, return `CompareResult::Skip`
  and the controller tries the next candidate. See "Skip cost" in
  the [plugin model](../explanation/plugin-model.md) for why
  descriptors should be specific.
- **`pair.logical_path()`** returns the user-facing path (prefers
  the right side, falls back to left).

## `src/lib.rs` — the export macro

```rust
mod fasta;

pub use fasta::FastaComparator;

binoc_sdk::export_plugin! {
    module: biobinoc,
    comparators: [FastaComparator],
}
```

The macro generates every C ABI entry point
(`_binoc_plugin_describe`, `_binoc_comparator_compare`, …) and — when
the `python` feature is active — an empty `#[pymodule]` so maturin
recognizes the build. One plugin pack can export any combination:

```rust
binoc_sdk::export_plugin! {
    module: my_plugin,
    comparators: [FooComparator, BarComparator],
    transformers: [BazTransformer],
}
```

## `pyproject.toml`

```toml
[project]
name = "biobinoc"
version = "0.1.0"
dependencies = ["binoc>=0.1"]

[project.entry-points."binoc.plugins"]
biobinoc = "biobinoc"

[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[tool.maturin]
features = ["python"]
```

Note the entry point value is just the module name — no
`module:function`. Discovery detects that the entry point is a native
module and loads it through the C ABI. See
[Plugin discovery](../reference/plugin-discovery.md) for the exact
strings.

## Build and try it

```bash
cd biobinoc
uv venv
uv run --extra dev maturin develop
binoc diff snapshot-a snapshot-b
```

`.fasta` files are now claimed by your comparator.

## Publish a plugin

See [Publish a plugin](publish-a-plugin.md) for the release story.
Briefly: `binoc-*` is the PyPI ecosystem namespace; versioning is
independent per published package; native plugin compatibility is
checked at runtime via `binoc-sdk`'s `sdk_version`, so depend on
`binoc-sdk` tightly and on `binoc` (the host) loosely.

## Cross-phase composition: artifacts

If your comparator produces data another transformer (yours or
someone else's) might use, publish it as an **artifact** — a typed,
versioned byte blob keyed by `(format, subject)`. Artifacts are the
primary cross-phase mechanism; the thin-comparator pattern in the
stdlib (CSV comparator publishes `tabular_v1`, the tabular analyzer
transformer consumes it) is the reference. See
[Artifacts and composition](../explanation/artifacts-and-composition.md).

```rust
let tabular = parse_to_tabular(&data.read_bytes(left)?)?;
let bytes = serde_json::to_vec(&tabular).unwrap();
let desc = data.publish_artifact(
    &tabular_v1(),
    ArtifactSubject::Left,
    "biobinoc.fasta",
    &bytes,
)?;
node = node.with_artifact(desc);
```

Any transformer matching on the `tabular_v1` artifact format sees
your data without re-parsing the source file.

## Testing

Rust plugins use the shared test-vector harness. Write a
`tests/test_vectors.rs` that discovers the vectors in your crate and
runs them against a registry that includes your plugin. See
[Test a plugin with vectors](test-a-plugin-with-vectors.md).

## Where to go next

- [Publish a plugin](publish-a-plugin.md) — packaging, entry points,
  PyPI.
- [Write a Rust transformer](write-a-rust-transformer.md) — add a
  pattern-detection pass across IR nodes your comparator produces.
- [Test a plugin with vectors](test-a-plugin-with-vectors.md) — share
  the built-in test harness.
- [Plugin SDK and ABI ADR](../adr/2026-03-12-plugin_sdk_and_abi.md) — the
  design of the boundary between host and native plugins.
