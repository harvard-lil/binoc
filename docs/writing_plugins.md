# Writing Binoc Plugins

This guide covers how to write a binoc plugin — a comparator, transformer, or renderer — and distribute it so `pip install your-package` makes it available to the `binoc` CLI automatically.

Plugins can be written in **Python** (quick to prototype, GIL cost per file) or **Rust** (zero per-file overhead, more boilerplate). Both use the same distribution mechanism: Python entry points.

## Concepts

Before writing a plugin, understand what each type does:

- **Comparator**: Claims item pairs by file extension, media type, or imperative `can_handle` logic. Produces a diff (leaf node or expansion into children). This is the parser — it turns raw data into IR.
- **Transformer**: Rewrites the completed diff tree. Operates on structure, not raw data. For example, the move detector correlates add/remove pairs by content hash.
- **Renderer**: Renders finalized changesets into a presentation format (Markdown, HTML, etc.).

The IR is a tree of `DiffNode` values. See `docs/design.md` for the full schema.

## Python Plugins

### A minimal comparator

```python
import binoc

class FastaComparator(binoc.Comparator):
    name = "biobinoc.fasta"
    extensions = [".fasta", ".fa", ".fna"]

    def compare(self, pair):
        if pair.left_path and pair.right_path:
            # Both sides present — compare them
            left = open(pair.left_path).read()
            right = open(pair.right_path).read()
            if left == right:
                return binoc.Identical()

            node = binoc.DiffNode(
                action="modify",
                item_type="fasta",
                path=pair.logical_path,
                tags=["biobinoc.sequence-changed"],
                details={"sequences_left": left.count(">"),
                         "sequences_right": right.count(">")},
            )
            node = node.with_detail(
                "summary_text",
                f"{right.count('>')} sequences in new version",
            )
            return binoc.Leaf(node)

        elif pair.right_path:
            # Added
            return binoc.Leaf(binoc.DiffNode(
                action="add",
                item_type="fasta",
                path=pair.logical_path,
            ))

        else:
            # Removed
            return binoc.Leaf(binoc.DiffNode(
                action="remove",
                item_type="fasta",
                path=pair.logical_path,
            ))
```

**Key points:**

- Set `name` to a namespaced string (e.g. `"biobinoc.fasta"`, not `"fasta"`).
- Set `extensions` for declarative dispatch. Override `can_handle(self, pair)` for imperative logic.
- `compare()` must return `Identical()`, `Leaf(node)`, or `Expand(node, children)`.
- `pair.left_path` / `pair.right_path` are physical paths on disk (or `None` for add/remove). `pair.logical_path` is the user-facing path.

### A minimal transformer

```python
import binoc

class SequenceNormalizer(binoc.Transformer):
    name = "biobinoc.sequence_normalizer"
    match_types = ["fasta"]

    def transform(self, node):
        # Collapse trivial whitespace-only changes
        if node.action == "modify" and node.details.get("sequences_left") == node.details.get("sequences_right"):
            return binoc.Replace(node.with_tag("biobinoc.whitespace-only"))
        return binoc.Unchanged()
```

**Key points:**

- Set `match_types`, `match_tags`, and/or `match_actions` for declarative matching. Override `can_handle(self, node)` for imperative logic.
- `transform()` must return `Unchanged()`, `Replace(node)`, `ReplaceMany(nodes)`, or `Remove()`.
- Transformers see the completed tree. They don't have access to raw file data.

### DiffNode API (Python)

Nodes are immutable-ish. Builder methods return new nodes:

```python
node = binoc.DiffNode(action="modify", item_type="fasta", path="seqs.fa")
node = node.with_tag("biobinoc.gap-change")
node = node.with_detail("gap_count", 42)
node = node.with_source_path("old_seqs.fa")  # for moves/renames
node = node.with_children([child1, child2])

# Reading
node.action          # "modify"
node.item_type     # "fasta"
node.path          # "seqs.fa"
node.tags          # ["biobinoc.gap-change"]
node.details       # {"gap_count": 42}
node.children      # [child1, child2]
node.annotations   # {} — set by transformers
```

### Using plugins without packaging

For scripts and notebooks, register plugins directly:

```python
import binoc

config = binoc.Config.default()
config.add_comparator(FastaComparator())
config.add_transformer(SequenceNormalizer())
changeset = binoc.diff("snapshot-a", "snapshot-b", config=config)
```

This bypasses entry-point discovery entirely. The plugin doesn't need to be packaged or installed.

## Distributing a Python plugin

To make a plugin available via `pip install`, declare an entry point in your package's `pyproject.toml`:

```toml
[project]
name = "biobinoc"
version = "0.1.0"
dependencies = ["binoc"]

[project.entry-points."binoc.plugins"]
biobinoc = "biobinoc:register"
```

Then implement the `register` function:

```python
# biobinoc/__init__.py

def register(registry):
    from biobinoc.fasta import FastaComparator
    from biobinoc.normalizer import SequenceNormalizer

    registry.register_comparator("biobinoc.fasta", FastaComparator())
    registry.register_transformer("biobinoc.sequence_normalizer", SequenceNormalizer())
```

After `pip install biobinoc`, the `binoc` CLI automatically discovers and loads the plugin at startup. No configuration needed to "enable" it — entry-point discovery handles that. The user just references `biobinoc.fasta` in their dataset config:

```yaml
comparators:
  - binoc.directory
  - biobinoc.fasta     # claimed by your plugin
  - binoc.text
  - binoc.binary

transformers:
  - biobinoc.sequence_normalizer
  - binoc.move_detector
```

## Rust Plugins

Rust plugins have zero per-file Python overhead. Python is involved once at startup for entry-point discovery; after that, all dispatch goes through Rust trait object vtables.

A Rust plugin is a PyO3 crate that registers native trait objects into the `PluginRegistry`.

### Project structure

```
biobinoc/
├── Cargo.toml
├── pyproject.toml
├── src/
│   ├── lib.rs          # PyO3 module + register function
│   └── fasta.rs        # Comparator implementation
└── python/
    └── biobinoc/
        └── __init__.py
```

### Cargo.toml

```toml
[package]
name = "biobinoc"
version = "0.1.0"
edition = "2021"

[lib]
name = "biobinoc"
crate-type = ["cdylib"]

[dependencies]
binoc-core = { version = "0.1" }
pyo3 = { version = "0.27", features = ["extension-module"] }
serde_json = "1.0"
```

### Implementing a Rust comparator

```rust
// src/fasta.rs
use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;

pub struct FastaComparator;

impl Comparator for FastaComparator {
    fn name(&self) -> &str { "biobinoc.fasta" }

    fn handles_extensions(&self) -> &[&str] {
        &[".fasta", ".fa", ".fna"]
    }

    fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let left_data = std::fs::read_to_string(&left.physical_path)
                    .map_err(BinocError::Io)?;
                let right_data = std::fs::read_to_string(&right.physical_path)
                    .map_err(BinocError::Io)?;

                if left_data == right_data {
                    return Ok(CompareResult::Identical);
                }

                let node = DiffNode::new("modify", "fasta", &right.logical_path)
                    .with_tag("biobinoc.sequence-changed")
                    .with_summary("FASTA sequences changed");

                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => {
                let node = DiffNode::new("add", "fasta", &right.logical_path);
                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let node = DiffNode::new("remove", "fasta", &left.logical_path);
                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
```

### Registering via PyO3

```rust
// src/lib.rs
mod fasta;

use std::sync::Arc;
use pyo3::prelude::*;

#[pyfunction]
fn register(registry: &mut binoc::PluginRegistry) {
    registry.inner.register_comparator(
        "biobinoc.fasta",
        Arc::new(fasta::FastaComparator),
    );
}

#[pymodule]
fn _biobinoc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(register, m)?)?;
    Ok(())
}
```

The `register` function receives a `binoc.PluginRegistry` (which is `PyPluginRegistry` from `binoc-python`). It accesses the underlying Rust `PluginRegistry` via `.inner` and registers native `Arc<dyn Comparator>` objects. No Python bridge layer is involved at runtime.

### pyproject.toml

```toml
[project]
name = "biobinoc"
version = "0.1.0"
dependencies = ["binoc"]

[project.entry-points."binoc.plugins"]
biobinoc = "_biobinoc:register"

[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[tool.maturin]
python-source = "python"
module-name = "_biobinoc"
features = ["pyo3/extension-module"]
```

### Runtime flow

1. User runs `binoc diff snapshot-a snapshot-b`.
2. Python CLI starts, scans entry points, finds `biobinoc`.
3. Python calls `_biobinoc.register(registry)` — one PyO3 boundary crossing.
4. Rust `FastaComparator` is registered as a native trait object in the `PluginRegistry`.
5. For every `.fasta` file in the snapshots, the controller dispatches to `FastaComparator::compare()` via Rust vtable. No GIL, no serialization.

## Naming and namespacing

To prevent collisions across plugin packs:

| Thing | Convention | Examples |
|---|---|---|
| Plugin names | `org.name` | `biobinoc.fasta`, `climate.netcdf` |
| Tags | `org.tag-name` | `biobinoc.sequence-changed`, `binoc.column-reorder` |
| Item types | `org.type-name` | `biobinoc.fasta-alignment`, `binoc.tabular` |
| Kinds | Standard kinds unnamespaced; custom kinds namespaced | `add`, `remove`, `modify` (standard); `biobinoc.gap-shift` (custom) |

Standard `binoc.*` names are reserved for the standard library.

## Summary field

The `DiffNode.summary` field is an optional human-readable one-liner describing the change. Renderers use it for narrative rendering. If your comparator produces a domain-specific diff, set `summary` so the standard Markdown renderer can describe it without understanding your format:

```python
node = binoc.DiffNode(
    action="modify",
    item_type="fasta",
    path="sequences.fa",
).with_detail("summary", "3 sequences added, 1 removed")
```

When `summary` is absent, renderers fall back to a generic description from `action`, `item_type`, and `tags`. Setting it is optional but improves changelog quality.

## Performance expectations

- **Comparators** should stream I/O where possible. Don't load entire large files into memory when you can process incrementally.
- **Transformers** should avoid cloning subtrees they don't modify. Returning `Unchanged()` is zero-cost.
- **Hashing** for identity/move detection uses BLAKE3. If your comparator needs content hashing, use the same algorithm for consistency.
- Python plugins pay a GIL acquisition cost per `compare()` / `transform()` call. For high-throughput scenarios (thousands of files), consider a Rust implementation.

## Testing

Test your plugin by constructing item pairs and calling `compare()` / `transform()` directly:

```python
import binoc

comp = FastaComparator()
pair = binoc.ItemPair.both(
    "test-data/old.fasta", "test-data/new.fasta",
    "old.fasta", "new.fasta",
)
result = comp.compare(pair)
assert isinstance(result, binoc.Leaf)
assert result.node.action == "modify"
assert "biobinoc.sequence-changed" in result.node.tags
```

For integration testing, use `binoc.diff()` with a config that includes your plugin:

```python
config = binoc.Config(
    comparators=["biobinoc.fasta", "binoc.text", "binoc.binary"],
    transformers=["binoc.move_detector"],
)
config.add_comparator(FastaComparator())
changeset = binoc.diff("test-data/snapshot-a", "test-data/snapshot-b", config=config)
```

You can also create test vectors following the pattern in `test-vectors/` — see `test-vectors/README.md` for the manifest format. To avoid duplicating harness code, depend on `binoc-stdlib` (with its default `test-vectors` feature) and use `binoc_stdlib::test_vectors::{discover_vectors, run_vector}` with a registry that includes your plugin; see `binoc-sqlite/tests/test_vectors.rs` for a minimal example.
