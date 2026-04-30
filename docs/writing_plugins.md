# Writing Binoc Plugins

This guide covers how to write a binoc plugin — a comparator, transformer, or renderer — and distribute it so `uv pip install your-package` (or `pip install your-package`) makes it available to the `binoc` CLI automatically.

Plugins can be written in **Python** (quick to prototype, GIL cost per file) or **Rust** (zero per-file overhead via C ABI, more boilerplate). Both use the same distribution mechanism: Python entry points.

## Concepts

Before writing a plugin, understand what each type does:

- **Comparator**: Claims item pairs by file extension, media type, or scope (files vs. containers). Produces a diff (leaf node or expansion into children). This is the parser — it turns raw data into IR. If a comparator is matched by its descriptor but discovers at compare-time that it can't handle the item, it returns `Skip` and the controller tries the next candidate.
- **Transformer**: Rewrites the completed diff tree. Operates on structure, not raw data. For example, the move detector correlates add/remove pairs by content hash. Transformers that need source data can re-read it via `source_items` (see [Cross-phase data access](#cross-phase-data-access)).
- **Renderer**: Renders finalized changesets into a presentation format (Markdown, HTML, etc.).

The IR is a tree of `DiffNode` values.

## Python Plugins

### A minimal comparator

```python
import binoc

class FastaComparator(binoc.Comparator):
    name = "biobinoc.fasta"
    extensions = [".fasta", ".fa", ".fna"]

    def compare(self, pair):
        if pair.left_path and pair.right_path:
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
            return binoc.Leaf(binoc.DiffNode(
                action="add",
                item_type="fasta",
                path=pair.logical_path,
            ))

        else:
            return binoc.Leaf(binoc.DiffNode(
                action="remove",
                item_type="fasta",
                path=pair.logical_path,
            ))
```

**Key points:**

- Set `name` to a namespaced string (e.g. `"biobinoc.fasta"`, not `"fasta"`).
- Set `extensions` for declarative dispatch. The controller matches item pairs to comparators by extension, media type, and scope — no imperative pre-check method.
- `compare()` must return `Identical()`, `Leaf(node)`, or `Expand(node, children)`.
- `pair.left_path` / `pair.right_path` are physical paths on disk (or `None` for add/remove). `pair.logical_path` is the user-facing path.

### A minimal transformer

```python
import binoc

class SequenceNormalizer(binoc.Transformer):
    name = "biobinoc.sequence_normalizer"
    match_types = ["fasta"]

    def transform(self, node):
        if node.action == "modify" and node.details.get("sequences_left") == node.details.get("sequences_right"):
            return binoc.Replace(node.with_tag("biobinoc.whitespace-only"))
        return binoc.Unchanged()
```

**Key points:**

- Declare what nodes your transformer matches using one or more of: `match_tags`, `match_actions`, `match_types`, `match_artifacts`, or `node_shape` (`"any"`, `"container"`, `"leaf"`). The controller dispatches to your transformer when **all** non-empty criteria match (AND-of-ORs: within each field any value suffices, but every populated field must match).
- `transform()` must return `Unchanged()`, `Replace(node)`, `ReplaceMany(nodes)`, or `Remove()`.
- Transformers see the completed tree. They can access typed data published by comparators via artifacts — see [Cross-phase data access](#cross-phase-data-access).

### DiffNode API (Python)

Nodes are immutable-ish. Builder methods return new nodes:

```python
node = binoc.DiffNode(action="modify", item_type="fasta", path="seqs.fa")
node = node.with_tag("biobinoc.gap-change")
node = node.with_detail("gap_count", 42)
node = node.with_source_path("old_seqs.fa")  # for moves/renames
node = node.with_children([child1, child2])

# Reading
node.action        # "modify"
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

### Known limitations of Python plugins

Python plugins receive a simplified interface compared to Rust plugins:

- **No `DataAccess`**: Python comparators receive physical file paths but not the `DataAccess` trait. They can't publish artifacts or call `workspace()` for scratch space.
- **No `content_hash` or `media_type`**: The `ItemPair` passed to Python comparators omits these fields.
- **No `source_items`**: Python transformers can't re-read source data. They operate on the `DiffNode` tree only.

For plugins that need these capabilities, write a Rust plugin instead.

## Distributing a Python plugin

To make a plugin available via `uv pip install` (or `pip install`), declare an entry point in your package's `pyproject.toml`:

```toml
[project]
name = "biobinoc"
version = "0.1.0"
dependencies = ["binoc>=0.1"]

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

After installing, the `binoc` CLI automatically discovers and loads the plugin at startup. No configuration needed to "enable" it — entry-point discovery handles that. The user just references `biobinoc.fasta` in their dataset config:

```yaml
comparators:
  - binoc.directory
  - biobinoc.fasta     # claimed by your plugin
  - binoc.text
  - binoc.binary

transformers:
  - biobinoc.sequence_normalizer
  - binoc.correlation_detector
```

Versioning note:

- For Python plugins, `binoc` is a real Python API dependency. Set a minimum host version for the Python APIs you use.
- Do not add an upper bound unless you know your plugin depends on a host-side Python API or behavior that may break across Binoc releases.

## Rust Plugins

Rust plugins have zero per-file Python overhead. Python is involved once at startup for entry-point discovery; after that, all dispatch goes through the C ABI (JSON serialization at the boundary, no GIL involvement per-file).

A Rust plugin depends on `binoc-sdk` (not `binoc-core`), implements the plugin traits, and uses the `export_plugin!` macro to generate all transport glue. No PyO3 code in the plugin itself — the macro handles everything.

### Project structure

```
biobinoc/
├── Cargo.toml
├── pyproject.toml
├── src/
│   ├── lib.rs          # export_plugin! + pub use
│   └── fasta.rs        # Comparator implementation
└── tests/
    └── test_vectors.rs # Rust test vector suite (optional)
```

### Cargo.toml

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
binoc-sdk = { version = "0.1" }
serde_json = "1.0"
pyo3 = { version = "0.27", features = ["extension-module"], optional = true }
```

The `python` feature is optional — it's only needed for the `export_plugin!` macro to generate the PyO3 module stub that maturin requires. Your plugin code never touches PyO3 directly.

### Implementing a Rust comparator

```rust
// src/fasta.rs
use binoc_sdk::*;

#[derive(Default)]
pub struct FastaComparator;

impl Comparator for FastaComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("biobinoc.fasta")
            .with_extensions(vec![".fasta".into(), ".fa".into(), ".fna".into()])
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let left_data = data.read_bytes(left)?;
                let right_data = data.read_bytes(right)?;

                if left_data == right_data {
                    return Ok(CompareResult::Identical);
                }

                let node = DiffNode::new("modify", "fasta", pair.logical_path())
                    .with_tag("biobinoc.sequence-changed")
                    .with_summary("FASTA sequences changed");

                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => {
                Ok(CompareResult::Leaf(
                    DiffNode::new("add", "fasta", &right.logical_path),
                ))
            }
            (Some(left), None) => {
                Ok(CompareResult::Leaf(
                    DiffNode::new("remove", "fasta", &left.logical_path),
                ))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
```

**Key points:**

- Plugin structs must implement `Default` (the `export_plugin!` macro constructs them).
- All I/O goes through `&dyn DataAccess` — never use `std::fs` directly. Use `data.read_bytes(item)` for content, `data.local_path(item)` when a filesystem path is required (e.g. for SQLite or other libraries that need a path), and `data.open_read(item)` for streaming.
- `ComparatorDescriptor` declares routing metadata: extensions, media types, scope. No `can_handle` method — if the descriptor matches but the data turns out to be unsuitable, return `CompareResult::Skip` (see [Performance: skip cost](#skip-cost) below).
- `pair.logical_path()` returns the user-facing path (prefers the right side, falls back to left).

### The export macro

```rust
// src/lib.rs
mod fasta;

pub use fasta::FastaComparator;

binoc_sdk::export_plugin! {
    module: biobinoc,
    comparators: [FastaComparator],
}
```

This generates all C ABI entry points (`_binoc_plugin_describe`, `_binoc_comparator_compare`, etc.) plus an empty `#[pymodule]` when the `python` feature is active. A single plugin pack can export any combination:

```rust
binoc_sdk::export_plugin! {
    module: my_plugin,
    comparators: [FooComparator, BarComparator],
    transformers: [BazTransformer],
}
```

### pyproject.toml

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

Note the entry point value is just the module name (`"biobinoc"`), not a `module:function` callable. The discovery code detects that it's a native module and loads it via the C ABI automatically.

Versioning note:

- The Rust compatibility boundary is `binoc-sdk`, not the `binoc` package version.
- Depend on the `binoc-sdk` minor line you build against in `Cargo.toml`.
- In `pyproject.toml`, use `binoc` as a host-package dependency with a minimum version floor for the loader/runtime features you need.
- Do not add a `binoc<next-minor` cap just to mirror the SDK minor. Native plugin compatibility is checked at runtime through the plugin `sdk_version`.

### Runtime flow

1. User runs `binoc diff snapshot-a snapshot-b`.
2. Python CLI starts, scans `binoc.plugins` entry points, finds `biobinoc`.
3. Discovery detects a native module, loads the `.so`/`.dylib` via `libloading`.
4. Reads the plugin description (descriptors for all comparators/transformers/renderers).
5. Registers `NativeComparator`/`NativeTransformer`/`NativeRenderer` wrappers in the registry.
6. For every `.fasta` file in the snapshots, the controller dispatches to `FastaComparator::compare()` through the C ABI — JSON serialization at the boundary, but no Python GIL involvement per file.

### Testing

Rust plugins use the shared test-vector harness. Depend on `binoc-stdlib` with the `test-vectors` feature in `[dev-dependencies]`:

```toml
[dev-dependencies]
binoc-core = { path = "../../binoc-core" }
binoc-sdk = { path = "../../binoc-sdk", features = ["test-support"] }
binoc-stdlib = { path = "../../binoc-stdlib", features = ["test-vectors"] }
```

Then write a test that discovers and runs your vectors, passing a list of `VectorMaterializer`s so `.zip.d` / `.tar.gz.d` / plugin-specific staging directories get built into real artifacts before the diff runs:

```rust
use binoc_stdlib::test_vectors::{
    discover_vectors, run_vector, stdlib_materializers, VectorMaterializer,
};

#[test]
fn test_vectors() {
    let stdlib = stdlib_materializers();
    let materializers: Vec<&dyn VectorMaterializer> =
        stdlib.iter().map(|m| &**m).collect();
    for vector in discover_vectors("path/to/your/test-vectors") {
        run_vector(
            &vector,
            "path/to/your/test-vectors".as_ref(),
            || {
                let mut r = binoc_stdlib::default_registry();
                // register your plugin into r
                r
            },
            &materializers,
        );
    }
}
```

#### Custom staging directories (`VectorMaterializer`)

If your plugin's test vectors commit source trees instead of opaque binaries — `.sqlite.d/*.sql` scripts instead of a `.sqlite` file, say — implement `VectorMaterializer` once and reuse it for both tests and `just materialize`. The trait is test-harness-only (never shipped through the plugin ABI):

```rust
use std::path::Path;
use binoc_stdlib::test_vectors::VectorMaterializer;

pub struct FastaBundleMaterializer;

impl VectorMaterializer for FastaBundleMaterializer {
    // Dirs this builder claims, each including the leading dot.
    fn suffixes(&self) -> &[&'static str] { &[".fabundle.d"] }

    // Build `out_path` (a single .fabundle file) from the sources in `staging_dir`.
    // `all_staging_suffixes` is the union across all registered materializers — use it
    // to skip any nested staging directories that will be built separately.
    fn build(&self, staging_dir: &Path, out_path: &Path, _all: &[&str]) {
        // ... walk staging_dir, write out_path ...
    }
}
```

Put the type behind a `test-support` feature on your crate so it doesn't ship in the cdylib / Python wheel, and add a tiny `src/bin/materialize_test_vectors.rs` that composes it with `stdlib_materializers()`:

```rust
use binoc_stdlib::test_vectors::{
    discover_vectors, materialize_snapshots, stdlib_materializers, VectorMaterializer,
};
use my_plugin::test_support::FastaBundleMaterializer;

fn main() {
    let stdlib = stdlib_materializers();
    let mine = FastaBundleMaterializer;
    let mut materializers: Vec<&dyn VectorMaterializer> =
        stdlib.iter().map(|m| &**m as &dyn VectorMaterializer).collect();
    materializers.push(&mine);

    for vector in discover_vectors("path/to/your/test-vectors".as_ref()) {
        let dest = /* output_root */ Path::new("test-vectors-materialized")
            .join(vector.file_name().unwrap());
        materialize_snapshots(&vector, &dest, &materializers);
    }
}
```

Users then run `just materialize` (or `cargo run -p my-plugin --features test-support --bin materialize-test-vectors`) to get a browsable `test-vectors-materialized/` tree that the tutorial, debugging sessions, and CI can reference directly. See `model-plugins/binoc-sqlite/src/test_support.rs` and `model-plugins/binoc-sqlite/src/bin/materialize_test_vectors.rs` for a complete example, and [`docs/adr/test_vector_materialization.md`](adr/test_vector_materialization.md) for the design.

## Cross-phase data access

The architecture cleanly separates phases: comparators parse raw data into IR, transformers rewrite IR. Typed **artifacts** are the primary channel between them — comparators publish structured data, and transformers consume it without re-parsing.

### Artifacts — the primary cross-phase mechanism

Artifacts are the unified mechanism for both private reuse and cross-plugin composition. A comparator publishes zero or more artifacts per node; downstream transformers retrieve them by format and subject.

The standard library demonstrates this with the **thin comparator pattern**: the CSV comparator parses the file into `TabularData`, publishes `tabular_v1` artifacts, and emits a bare node (action, item type, artifacts — no tags or summary). The format-agnostic `TabularAnalyzer` transformer then reads those artifacts and adds all the semantic tags, details, and summary text. This means any future comparator that publishes `tabular_v1` (Parquet, Excel, etc.) gets tabular analysis for free.

```rust
use binoc_sdk::{ArtifactFormat, ArtifactSubject, ArtifactDescriptor, tabular_v1};

// In a comparator's compare(): publish an artifact
let tabular = parse_to_tabular(data)?;
let bytes = serde_json::to_vec(&tabular).unwrap();
let artifact = data.publish_artifact(
    &tabular_v1(),
    ArtifactSubject::Left,
    "binoc.csv",
    &bytes,
)?;
node = node.with_artifact(artifact);

// In a downstream transformer: consume the artifact
let fmt = tabular_v1();
let descriptor = node.artifacts.iter()
    .find(|a| a.format == fmt && a.subject == ArtifactSubject::Left);
if let Some(desc) = descriptor {
    if let Some(bytes) = data.get_artifact(desc)? {
        let tabular: TabularData = serde_json::from_slice(&bytes).unwrap();
    }
}
```

**Artifact formats are structured tuples** — `(package, name, version)` rather than dotted strings. The `package` field is a package name resolvable through the language's normal package system (e.g. `("binoc", "tabular", 1)` is owned by the `binoc` SDK package, `("binoc-csv", "table", 1)` is owned by the `binoc-csv` package). Given a format's package, a developer can mechanically determine which package to depend on to get the codec. The `version` is a single integer — bump only for breaking schema changes; adding optional fields does not require a bump.

**Public vs. private artifacts:** An artifact whose format is documented and stable is a public artifact — the cross-plugin composition contract. An artifact whose format is undocumented or plugin-internal is a private artifact. They use the same storage and API; the difference is whether the format carries a stability guarantee.

**Artifacts are transient session data** — they are not serialized into the changeset JSON. Like `source_items`, they exist only during the live diff/transform session. The `extract` chain can regenerate them by replaying the compare/reopen chain.

The artifact store is filesystem-backed under `<data_root>/.artifacts/` so data written by the host is visible to separately-compiled plugins sharing the same `data_root` across the C ABI boundary.

### `source_items` — re-parse source data

The controller sets `DiffNode.source_items` on every node during the diff. Transformers that need the original data can re-parse it via `data.local_path()` or `data.read_bytes()` on the `ItemRef`s in the pair. This is a fallback for cases where no artifact is available and the transformer must work directly with the raw file.

```rust
fn transform(&self, node: DiffNode, data: &dyn DataAccess) -> TransformResult {
    let Some(ref pair) = node.source_items else {
        return TransformResult::Unchanged;
    };
    // Re-parse source files when no artifact is available ...
}
```

**Prefer artifacts over `source_items`** when your data requires parsing. Artifacts avoid redundant re-parsing across multiple transformers and enable cross-plugin composition (a transformer doesn't need to know which comparator produced the data). Use `source_items` only when you need raw byte access (e.g. hashing for move detection) or the comparator doesn't publish a suitable artifact.

## Naming and namespacing

### Package naming

On PyPI, the `binoc-*` namespace is the shared ecosystem namespace, similar to `pytest-*` or `llm-*`.

### Plugin names, tags, and types

To prevent collisions across plugin packs, namespace internal identifiers:

| Thing | Convention | Examples |
|---|---|---|
| Plugin names | `package.name` | `biobinoc.fasta`, `climate.netcdf` |
| Tags | `package.tag-name` | `biobinoc.sequence-changed`, `binoc.column-reorder` |
| Item types | `package.type-name` | `biobinoc.fasta-alignment`, `binoc.tabular` |
| Actions | Standard actions unnamespaced; custom actions namespaced | `add`, `remove`, `modify` (standard); `biobinoc.gap-shift` (custom) |

Standard `binoc.*` names are reserved for the standard library.

## Summary field

The `DiffNode.summary` field is an optional human-readable one-liner describing the change. Renderers use it for narrative rendering. If your plugin produces a domain-specific diff, set `summary` so the standard Markdown renderer can describe it without understanding your format:

```python
node = binoc.DiffNode(
    action="modify",
    item_type="fasta",
    path="sequences.fa",
    summary="3 sequences added, 1 removed",
)
```

In Rust, use `.with_summary()` on the `DiffNode` builder. Note that `summary` is a top-level field, not a detail entry.

When `summary` is absent, renderers fall back to a generic description from `action`, `item_type`, and `tags`. Setting it is optional but improves changelog quality.

## Performance expectations

- **Comparators** should stream I/O where possible. Don't load entire large files into memory when you can process incrementally.
- **Transformers** should avoid cloning subtrees they don't modify. Returning `Unchanged()` / `TransformResult::Unchanged` is zero-cost.
- **Hashing** for identity/move detection uses BLAKE3. If your comparator needs content hashing, use the same algorithm for consistency.
- Python plugins pay a GIL acquisition cost per `compare()` / `transform()` call. For high-throughput scenarios (thousands of files), consider a Rust implementation.

### Skip cost

Dispatch is fully declarative — comparators declare which extensions, media types, and scope (files vs. containers) they handle. There is no separate `can_handle` pre-check method. If a comparator is matched by its descriptor but discovers at compare-time that it can't handle the item (e.g. a `.db` file that isn't actually SQLite), it returns `CompareResult::Skip` and the controller tries the next comparator.

This means the "skip" path involves real work: the comparator opens the file, inspects it, and then bails. For separately-compiled plugins crossing the C ABI it includes JSON serialization of the request and response. Design your descriptors to be specific enough that false matches are rare:

- Use precise file extensions (`.sqlite3` not `.db`) when possible.
- Use media types for content-based dispatch where extension is ambiguous.
- Use `scope: Containers` or `scope: Files` to avoid being dispatched for the wrong item shape.

If your plugin handles a format that requires content sniffing (magic bytes), the skip path is unavoidable — just make the detection fast (read the first few bytes, not the whole file).

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
    transformers=["binoc.correlation_detector"],
)
config.add_comparator(FastaComparator())
changeset = binoc.diff("test-data/snapshot-a", "test-data/snapshot-b", config=config)
```

You can also create test vectors following the pattern in `test-vectors/` — see `test-vectors/README.md` for the manifest format.
