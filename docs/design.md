This document tracks design goals, user stories, and architectural decisions needed to develop Binoc.

Detailed design decisions are also recorded in docs/adr/

## Design Goals

**1. Fast.** The core engine is written in Rust. Large datasets with thousands of files should diff in seconds, not minutes. Parallel processing of independent subtrees is a default, not an optimization. Plugins are expected to be computationally lean — streaming I/O, minimal allocations, no unnecessary re-parsing.

**2. Portable.** Rust compiles to libraries callable from other languages. A Python wrapper (PyO3) is a first-class distribution target. The Rust CLI serves batch and CI use; the Python library serves interactive exploration in Jupyter notebooks and integration into data science workflows. The library is the product; the CLI is one consumer of it.

**3. Extensible by community.** The archival community is small and lightly funded. Binoc must be a platform that other communities (biology, climate science, public health) can extend with their own format-specific plugins. The target distribution model: vanilla Binoc ships a standard library of plugins; BioBinoc, ClimateBinoc, etc. are pip-installable packages that register domain-specific comparators, transformers, and renderer configs. The core engine has zero domain knowledge.

**4. Correct by default, tunable by experts.** Out of the box, Binoc produces accurate diffs using sensible defaults. Dataset-specific configuration allows experts to control comparator selection, transformer ordering, and significance classification, and add custom handlers.

**5. Parsimonious.** The controller does not know about directories, zip files, CSVs, or any data format. It knows about a tree of items and a pipeline of plugins. Separation of concerns is enforced at the architecture level. As much as possible, internal architecture should minimize the number of concepts that exist and maximize the ways they can be combined.

---

## User Stories

### Data Rescue Archivist

> "I have 12 snapshots of a Census Bureau demographic dataset scraped over three years. I need to know which snapshots are identical, which have trivial changes (column reordering, whitespace), and which reflect genuine policy shifts (new categories, removed fields). I want a changelog I can put in our archive metadata."

### Data Scientist

> "In a Jupyter notebook, I want to load two versions of a dataset, diff them, and drill into specific changes — show me the actual rows that were added, the columns that were renamed. I want the diff result as a dataframe I can filter and analyze."

### Bio Researcher

> "Our lab tracks genomic reference databases that update quarterly. We need a tool that understands FASTA and alignment formats and can tell us when sequences are added, removed, or modified. The standard Binoc doesn't know about these formats, but we can write plugins."

### CI Pipeline Operator

> "I want to run `binoc diff snapshots/2024-03/ snapshots/2024-06/ --config census.yaml -o migration.json -q` in a GitHub Action and fail the build if substantive changes are detected without a corresponding human review."

### Interactive Explorer

> "The changelog says '3,217 rows added to data.csv inside archive.zip.' I want to run `binoc extract migration.json archive.zip/data/data.csv rows_added` and see those actual rows."

---

## Conceptual Model

### Core Objects

| Object | Definition |
|---|---|
| **Dataset** | A named resource that humans think of as a consistent semantic entity. |
| **Snapshot** | A set of files representing the state of the dataset at a moment in time. Concretely, a directory on disk (primary), or potentially a manifest, archive, or other container. |
| **Migration** | A structured description of how to get from one snapshot to the next. A tree of diff nodes. Does not contain data, but can reconstruct details on demand given access to the original snapshots. |
| **Changelog** | A human-level summary of a series of migrations. Output in Markdown (default), rendered from migrations by a renderer (template-driven or LLM-summarized). |
| **Dataset Config** | Optional YAML file specifying the comparator pipeline, transformer pipeline, renderer settings, significance rules, and any format-specific configuration for a particular dataset. |

### Program Components

| Component | Role |
|---|---|
| **Controller** | Accepts two input snapshots and produces a migration. Processes a work queue of item pairs, dispatching to comparators, assembling the diff tree, then running transformers. Type-ignorant — it does not know what a directory, zip, or CSV is. |
| **Comparator** | A plugin that claims an item pair and either emits a leaf diff or expands the pair into child items for further processing. Comparators are the parser: raw data → IR. They have data access. |
| **Transformer** | A plugin that rewrites the completed diff tree. Transformers are optimization passes: IR → IR. They operate on structure, not raw data. |
| **Diff Node** | The unit of the intermediate representation (IR). A node in a tree representing one change or container of changes. |
| **Renderer** | A plugin that renders migrations into a presentation format (Markdown changelog, HTML, custom). Each renderer receives its own config section and handles its own concerns (e.g. significance classification for the Markdown renderer). Raw JSON is the canonical migration format, not a renderer. |

---

## Architecture

### The Controller Loop

The controller is a work loop over a tree of item pairs. It does not know about any data format. Its algorithm:

```
1. Receive a root item pair (snapshot A, snapshot B).
2. Process the pair: walk the comparator pipeline; first comparator to claim it wins.
3. The comparator either:
   - Reports the items as identical (no diff produced), or
   - Emits a leaf diff (item is fully handled), or
   - Emits a container diff node and expands into child item pairs.
4. For expansions, process all child pairs recursively.
   Independent siblings are processed in parallel (rayon).
5. Once the tree is fully expanded, walk the transformer pipeline.
   Each transformer runs in declared order over the completed tree.
6. The finalized tree is the migration.
```

This handles arbitrary nesting naturally. A zip comparator expands into directory entries. A directory comparator matches files by path and expands into file pairs. A nested zip is expanded again. A CSV comparator diffs at the column/row level. The controller doesn't know or care about any of this.

### The Diff IR (Intermediate Representation)

The IR is a tree of `DiffNode` values. This is the central data structure of the system — every comparator emits it, every transformer rewrites it, every renderer reads it.

```
DiffNode:
    kind: string            # open enum: "add", "remove", "modify", "move",
                            #   "rename", "reorder", "schema_change", ...
                            #   Plugins may define new kinds.
    item_type: string       # open string: "directory", "file", "tabular",
                            #   "zip_archive", "tar_archive", "alignment", ...
                            #   No built-in types. Conventions, not enforcement.
    path: string            # location within snapshot (logical path, including
                            #   interior paths like "archive.zip/data/file.csv")
    source_path: string?    # for moves/renames: the original path
    tags: set<string>       # open bag of semantic tags, namespaced by convention
                            #   e.g. "binoc.column-reorder", "biobinoc.gap-change"
    summary: string?         # optional human-readable one-liner describing the
                             #   change, set by comparator or transformer
                             #   (see Changelog Rendering Philosophy)
    children: list<DiffNode>
    details: map<string, any>   # comparator-specific payload, schema determined
                                #   by item_type convention
    annotations: map<string, any>  # transformer-added metadata
```

**Design decisions embedded here:**

- **`kind` is an open enum.** Plugins can define new kinds. Transformers that don't recognize a kind pass nodes through unchanged. A future `custom` subkind mechanism may be added if collision becomes a problem.
- **`item_type` is an open string.** The core system does not interpret it. Type conventions are documented (e.g., "if you emit `item_type: tabular`, your `details` should conform to this schema"). This avoids hardcoding the wrong primitives while enabling shared tooling (e.g., all tabular comparators emit a common detail schema, so a single tabular transformer works across CSV, Excel, Parquet).
- **`tags` are an open bag.** No built-in semantics. Comparators attach tags describing what they observed ("column-reorder", "row-addition"). Significance classification maps tags to user-facing categories in the renderer config.

### Comparator Interface

A comparator claims item pairs it knows how to handle and either produces a terminal diff or expands the pair into sub-items for further processing.

**Claiming items.** Comparators declare which file extensions they handle (e.g., `[".csv", ".tsv"]`) and/or which MIME media types they handle (e.g., `["application/zip"]`). For items that can't be identified by extension or media type (like directories), a `can_handle` method inspects the item pair directly.

**Dispatch order per comparator:** extension match → media type match → `can_handle`. The first comparator to claim by any method wins. Media type matching enables content-aware dispatch: a `.dat` file whose bytes are a zip archive will be claimed by the zip comparator if it declares `handles_media_types: ["application/zip"]`, even though the extension doesn't match.

**Media type detection.** Expanding comparators (e.g. the directory comparator) populate each child `Item`'s `media_type` field at the same time they read the file for hashing. Detection uses content sniffing from magic bytes (via the `infer` crate) with an extension-based fallback (via `mime_guess`). This piggybacks on the single `fs::read` call already performed for BLAKE3 hashing — no additional I/O. The field is `Option<String>`; items without a detectable type (no magic bytes, no recognized extension) carry `None` and fall through to `can_handle` dispatch.

**Comparing.** The `compare` method takes an item pair and returns one of:

- **Identical** — the items are the same; no diff node is produced.
- **Leaf** — a terminal diff node (the comparator fully handled this pair).
- **Expand** — a container diff node plus child item pairs that are added back to the work queue for further processing.

**Dispatch semantics:** The controller walks the comparator pipeline in config order. For each comparator it checks extension filters, then media type filters, then `can_handle`. The first comparator to claim the item wins. Directories skip both extension and media type matching (they are claimed solely via `can_handle`) to prevent extracted archive contents from being re-claimed by the archive comparator. This is URL-routing semantics: specificity is controlled by ordering in config, not by plugin self-declaration.

**The `extract` verb:** An optional method for on-demand detail retrieval. Migrations are descriptive — they record what changed but not the data itself. When a user wants to see the actual changed rows, the `extract` method is called with the original snapshots and a selector addressing the change of interest. Not all comparators implement it.

### Transformer Interface

A transformer rewrites the completed diff tree. Transformers are optimization passes — they operate on IR structure, not raw data.

**Matching nodes.** Transformers declare which nodes they care about via filters on `item_type`, `tags`, and/or diff `kind`. A `can_handle` method provides an imperative escape hatch.

**Scope.** A transformer operates in either Node scope (receives individual matched nodes; the controller recurses into children) or Subtree scope (receives the whole subtree rooted at the matched node and can rewrite it freely).

**Transforming.** The `transform` method returns one of:

- **Unchanged** — the node is left as-is (zero-cost).
- **Replace** — substitute a new node.
- **ReplaceMany** — replace one node with multiple siblings.
- **Remove** — delete the node entirely.

Each transformer declares a `suggested_phase` (e.g., "structural", "semantic", "cleanup") as a hint for default ordering, but config overrides everything.

**Dispatch semantics:** Transformers run in the order declared in the dataset config. For each transformer, the controller walks the diff tree (depth-first by default), finds nodes matching the transformer's declared filters (plus `can_handle` fallback), and invokes `transform` on each match.

**Transformer examples:**

| Transformer | Matches | Does |
|---|---|---|
| Move detector | Container nodes with `add` and `remove` children | Correlates adds/removes by content hash; collapses matching pairs into `move` nodes |
| Copy detector | Container nodes with `add` and `identical` children | Detects adds whose content hash matches an existing unchanged file; collapses into `copy` nodes |
| Column reorder detector | `item_type: tabular` with column-level `modify` children | Detects pure column reordering; collapses into a single `reorder` node |

### Renderer Interface

An renderer renders finalized migrations into a presentation format. Renderers are the final stage of the pipeline: IR → presentation. They are plugins, registered in the same registry as comparators and transformers.

**Identity.** Each renderer declares a unique `name` (e.g. `"binoc.markdown"`) and a `file_extension` (e.g. `"md"`) used for automatic format inference when the user writes `-o changelog.md`.

**Rendering.** The `render` method receives a slice of migrations and a `serde_json::Value` config object — the renderer-specific section from the dataset config's `output` block. The renderer is responsible for deserializing this into its own config type and applying its own defaults for missing fields.

**Per-renderer config.** The dataset config's `output` section is a map from renderer names to renderer-specific config objects. Each renderer defines its own config schema. For example, the standard Markdown renderer expects:

```yaml
output:
  markdown:
    significance:
      clerical: [binoc.column-reorder, ...]
      substantive: [binoc.column-addition, ...]
```

When the user provides no config for a renderer, the renderer receives an empty object and applies its own defaults. Name resolution is flexible: `markdown`, `binoc.markdown`, or the fully-qualified name all resolve to the same section.

**Dispatch.** The CLI resolves output destinations by file extension (`.md` → the renderer claiming `"md"`) or by explicit format name (`--format markdown`, `-o markdown:output.dat`). When multiple renderers claim the same extension, the CLI errors with an ambiguity message.

**Standard renderer: `binoc.markdown`.** Groups changes by significance category (using the tag-to-category mapping from its own config section), renders Markdown with sections like "Clerical Changes", "Substantive Changes", "Other Changes". Unclassified changes (tags not in any category) appear under "Other Changes".

**JSON is not a renderer.** Raw migration JSON is the canonical serialization format for migrations and is handled directly by the CLI, not through the renderer trait. JSON output is always available via `--format json` or `-o migration.json`.

### Changelog Rendering Philosophy

The changelog is the primary human-facing output. Its purpose is to answer: "What changed, and should I care?" The design of the standard Markdown renderer reflects this — it is a narrative document, not a schema dump.

**Principles:**

1. **Narrative over schema.** A changelog entry describes a change in natural language. "Column 'email' added" — not `columns_added: ["email"]`. A reader should understand what happened without knowing the IR field names.

2. **Appropriate detail.** Name what changed; quantify the magnitude; stop there.
   - **Columns:** Name them. The user needs to know *which* column was added or removed.
   - **Rows:** Give counts and direction. "2 rows added (1→3 rows)." The user needs the magnitude, not the row contents. Full data is available via `binoc extract`.
   - **Cells:** Give counts. For small numbers, locations (row/column) are helpful; for large numbers, a count suffices.
   - **Files:** Name the file and describe the change type. "2 lines added, 1 removed" is informative; a 64-character content hash is not.
   - **Containers (directories, archives):** Generally not reported in the changelog on their own. Their children carry the meaningful changes. A zip archive node that merely wraps changed CSV files adds no information.

3. **No implementation artifacts.** Content hashes, raw left/right counters for both sides of the comparison, internal field names, test fixture paths — none of these belong in user-facing output. The JSON migration format is the machine-readable representation. The changelog is for humans.

4. **Logical paths only.** Paths in the changelog refer to user-meaningful locations: `archive.zip/data.csv`, not temporary extraction directories or build-system conventions.

**Plugin-provided summaries.** The Markdown renderer cannot know how to describe every possible change type. A genomics comparator might emit `item_type: fasta_alignment` with domain-specific details; the standard renderer can't render that meaningfully. To solve this, DiffNode carries an optional `summary` field — a pre-formatted human-readable one-liner describing the change.

This is the same pattern as Rust's `Display` trait or Python's `__str__`: the type that understands the data provides the human representation.

- `summary` is optional. When absent, the renderer renders a generic description from kind, item_type, and tags.
- `summary` is a hint, not a mandate. Renderers may ignore it and render their own description from raw details.
- The last plugin to touch a node should set the summary. Transformers that rewrite a node's meaning (e.g. collapsing add+remove into a move) should update or clear the summary from the original comparator.
- Standard library comparators and transformers always set `summary` so that the default renderer produces good output without any configuration.

This keeps domain-specific rendering knowledge near the domain knowledge (in comparators and transformers), while letting each renderer control final presentation. Third-party plugin packs get good changelogs for free — they describe their own changes, and any renderer can display those descriptions.

### Comparator/Transformer Ordering in Config

Ordering is a config concern, not a plugin concern. The dataset config declares explicit pipelines:

```yaml
# dataset.binoc.yaml

comparators:
  # Tried in order for each unprocessed item. First to claim wins.
  - my_project.custom_comparator   # project-specific, uses can_handle
  - binoc.zip
  - binoc.directory
  - binoc.csv
  - binoc.text
  - binoc.binary                   # catch-all fallback, always claims

transformers:
  # Run in order on the completed diff tree.
  - binoc.move_detector
  - binoc.copy_detector
  - binoc.column_reorder_detector
  - my_project.custom_normalizer

# Per-renderer config. Keys are renderer names; each renderer
# receives its own section and defines its own schema and defaults.
output:
  markdown:
    significance:
      clerical:
        - binoc.column-reorder
        - binoc.whitespace-change
        - binoc.folder-rename
        - binoc.encoding-change
      substantive:
        - binoc.column-addition
        - binoc.column-removal
        - binoc.schema-change
        - binoc.row-addition
        - binoc.row-removal
```

If no config is provided, a default pipeline is used (the standard library ordering). Plugins declare a `suggested_phase` as a hint for default ordering, but config overrides everything. Each renderer applies its own defaults when its config section is absent.

This parallels web framework middleware/URL routing: declarative, inspectable, order-controlled by the deployer.

---

## Significance Classification

One usecase is significance classification — creating a changelog that reports or automatically alerts on some changes but not others. Significance is an **renderer concern**, not a core IR concern. The migration IR carries only factual tags; the judgment of what those tags _mean_ for a given audience belongs to each renderer's config.

The design is layered:

1. **Comparators** attach semantic tags to diff nodes describing what they observed. A CSV comparator that detects column reordering tags the node `binoc.column-reorder`. This is factual reporting, not judgment.
2. **Transformers** may add additional tags or annotations.
3. **The renderer's own config section** maps tags to user-facing significance categories (`clerical`, `substantive`, or any user-defined categories). Different renderers, communities, datasets, or use cases can define different mappings over the same tags. For example, the standard Markdown renderer reads its significance mapping from `output.markdown.significance` in the dataset config.
4. **LLM summarizers** (optional) can further interpret unclassified changes, clearly marked as inferred.

Because significance lives in per-renderer config rather than in the core IR, the same migration can be rendered by different renderers with different significance judgments. A CI-check renderer might classify `binoc.row-addition` as `critical` while the Markdown changelog calls it `substantive`.

Tags are namespaced by convention: `binoc.*` for the standard library, `biobinoc.*` for bio plugins, `myproject.*` for project-specific plugins.

---

## Data Interop: The Core API Boundary

The boundary between `binoc-core` and consumers (CLI, Python, other languages) is where performance for interactive use lives or dies. Key design choices:

### IR Tree Access from Python

Python wrappers (`PyDiffNode`, `PyMigration`) own a copy of the underlying Rust node. Attribute access (`.children`, `.tags`, `.details`) reads from the owned Rust struct via PyO3 getters. Tree traversal uses standard Python iteration and indexing (`for child in node`, `node[0]`). `find_node(path)` searches the subtree by logical path. `to_dict()` and `to_json()` convert to Python-native representations for detailed exploration.

This clone-on-access model is simple and adequate for realistic diff sizes. If profiling reveals a bottleneck for very large diffs (tens of thousands of nodes), a future optimization could hold a shared reference to the Rust-owned tree with lazy attribute access, but the current approach avoids lifetime complexity at the FFI boundary.

### Tabular Data Interop

The CSV comparator uses the `csv` crate internally and diffs via string comparison. The `extract` trait method exists on `Comparator` for future on-demand detail retrieval (e.g. "show me the actual added rows"), returning `ExtractResult::Text` or `ExtractResult::Binary`. No comparator implements `extract` yet.

If the `extract` verb is built out and users need to pull large tabular slices into Python (e.g. thousands of added rows into a DataFrame), Arrow-based zero-copy transfer via `pyarrow` would be a reasonable transport optimization at that boundary. This is deferred until there is a measured need — the dependency weight of `arrow-rs` is significant and the current string-based comparison is adequate for the diff algorithm itself.

### Core API Design Principles

- **No global state.** Configuration is passed in, not read from the environment.
- **No process-level side effects.** The library is embeddable in any context.
- **Avoid unnecessary cloning.** Partial traversal from FFI is cheap; Python wrappers read directly from owned Rust structs.

---

## Distribution Model

### Crate/Package Structure

```
binoc-core          Rust crate: controller, IR types, plugin traits, PluginRegistry
binoc-stdlib        Rust crate: standard library plugins (dir, zip, csv, text, binary)
binoc-cli           Rust crate (lib + bin): CLI porcelain, parameterized on PluginRegistry
binoc-python        Python package (PyO3): bindings to core + stdlib, plugin discovery, CLI entry point
```

`binoc-stdlib` is architecturally identical to any third-party plugin pack. The core engine has zero domain knowledge — not even about directories or text files.

### Two Distribution Channels

**Python (primary):** `pip install binoc` provides the `binoc` command via a console script entry point. The CLI discovers third-party plugins via Python entry points at startup, then delegates to the Rust engine. All per-file work is pure Rust — Python is involved only once at startup for plugin discovery.

**Rust (secondary):** `cargo install binoc-cli` provides the `binoc-cli` command as a standalone binary with standard library plugins only. No Python runtime, no plugin discovery. For minimal containers, embedded systems, or CI environments where Python is unavailable.

Both channels share the same Rust engine (`binoc-cli::run` accepts a `PluginRegistry`). The Python entry point populates the registry with discovered plugins before calling the same `run` function.

### Plugin Discovery via Entry Points

Third-party plugin packages register with binoc through Python [entry points](https://packaging.python.org/en/latest/specifications/entry-points/). A plugin package declares an entry point in its `pyproject.toml`:

```toml
[project.entry-points."binoc.plugins"]
biobinoc = "biobinoc:register"
```

Where `biobinoc:register` is a callable that accepts a `PluginRegistry` and populates it:

```python
def register(registry):
    from biobinoc.fasta import FastaComparator
    registry.register_comparator("biobinoc.fasta", FastaComparator())
```

At startup, the Python CLI scans `importlib.metadata.entry_points(group="binoc.plugins")` and invokes each discovered `register` function. This is the same pattern used by pytest, `llm`, and other Python tools with plugin ecosystems.

**Why entry points, not pluggy:** Pluggy adds hook specifications, call ordering, and wrappers. Binoc doesn't need these — ordering is a config concern, and the "hook" is just `register(registry)`. Raw entry points keep the mechanism trivial (~10 lines) with no runtime dependency. Pluggy can be adopted later if the plugin ecosystem grows to need it.

### Rust Plugins via Python Entry Points

A key design property: **Rust-authored plugins discovered via Python entry points register native trait objects, not Python bridge objects.** The per-file performance cost is zero — only startup discovery goes through Python.

A third-party Rust plugin (e.g., BioBinoc) is a PyO3 package. Its `register` function calls Rust code that registers Rust `Comparator` trait objects directly into the `PluginRegistry`:

```rust
// biobinoc/src/lib.rs (PyO3 module)
#[pyfunction]
fn register(registry: &mut PyPluginRegistry) {
    registry.inner.register_comparator(
        "biobinoc.fasta",
        Arc::new(FastaComparator),  // pure Rust trait object
    );
}
```

The runtime flow:

1. **Startup (once):** Python entry point discovers `biobinoc`, calls `biobinoc.register(registry)`. One PyO3 boundary crossing.
2. **Per-file dispatch (many times):** Controller dispatches to `FastaComparator::compare()` via Rust trait object vtable. No GIL, no serialization, same speed as stdlib plugins.

Python-authored plugins continue to work via `PyComparatorBridge` / `PyTransformerBridge`, with the expected GIL cost per call.

### Third-Party Plugin Packs

A community extension (e.g., BioBinoc) is a pip-installable package providing:

- Comparators (FASTA comparator, alignment comparator, etc.)
- Transformers (domain-specific simplifications)
- Renderers with their own config schemas (e.g. a domain-specific significance mapping)
- Optionally, a default dataset config template
- An entry point in `pyproject.toml` that registers all of the above

The user experience: `pip install biobinoc`, then reference `biobinoc.fasta` in a dataset config file. No config editing to "enable" the plugin — entry point discovery makes it available automatically.

For pure-Rust deployments without Python, a "distro binary" pattern is available: a custom `main.rs` that builds a `PluginRegistry` with the desired plugins compiled in. This is an advanced path for niche use cases.

### Tag and Type Namespacing

To prevent collisions across plugin packs:

- Tags: `binoc.column-reorder`, `biobinoc.alignment.gap-change`
- Item types: `binoc.tabular`, `biobinoc.fasta-alignment`
- Kinds: standard kinds (`add`, `remove`, `modify`, `move`, `reorder`) are unnamespaced; plugin-defined kinds are namespaced.

---

## Performance Architecture

### Parallelism

The controller processes independent subtrees in parallel by default (work-stealing, rayon-style). Two sibling files expanded from a directory comparator have no dependency on each other; their comparators run concurrently.

Constraint: transformers may need to see all siblings (e.g., move-detector needs the full add/remove set). The tree must be fully expanded at a given level before transformers for that level can run.

v1 model: all expansion completes, then all transformation runs. The interfaces do not preclude future level-by-level processing for memory-bounded operation on very large trees.

### Plugin Performance Contract

Documented expectations for plugin authors:

- **Comparators** should stream I/O where possible, not load entire files into memory.
- **Transformers** should avoid cloning subtrees when they don't modify them. Returning `Unchanged` from `transform` is zero-cost.
- **Hashing** (for identity/move detection) uses BLAKE3 for speed. Streaming, parallelizable per-file.

### Cross-Phase Caching (Future)

Known inefficiency in v1: a CSV comparator parses a file; a transformer or the `extract` verb may need the parsed representation again, requiring re-parsing. A content-addressed cache service is a future optimization, deferred until the performance cost is measured.

---

## v1 Implementation Scope

### Non-goals

- Replayable migrations (applying a migration to reconstruct snapshot B from A). We assume snapshots are preserved.

## Open Questions

1. **Transformer fixed-point.** The v1 single-pass model means transformer B's output cannot be input for transformer A if A runs before B. Config ordering solves known cases. The trigger to revisit is a real-world case where mutual transformer dependencies arise.

2. **Python plugin GIL cost.** PyO3's `allow_threads` is the assumed mitigation. Needs benchmarking on a real mixed Rust/Python pipeline to confirm.

3. **Tag collision in practice.** Namespacing by convention (`binoc.*`, `biobinoc.*`) may be sufficient, or may need enforcement. Monitor as third-party plugins emerge.

---

## Testing Strategy

### Shared Test Vectors

Test vectors live in `test-vectors/` at the workspace root, shared across all crates and (future) Python bindings. Each vector is a directory containing two snapshot directories, a TOML manifest, and optionally a gold-file migration output:

```
test-vectors/
├── README.md
├── trivial-identical/
│   ├── manifest.toml
│   ├── snapshot-a/
│   └── snapshot-b/
├── csv-column-reorder/
│   ├── manifest.toml
│   ├── snapshot-a/
│   ├── snapshot-b/
│   └── expected-migration.json   (optional gold file)
└── ...
```

The manifest declares what the vector tests and what assertions to check:

```toml
[vector]
name = "csv-column-reorder"
description = "Columns shuffled, content identical"
tags = ["csv", "column-reorder", "clerical"]

[config]
comparators = ["binoc.directory", "binoc.csv"]
transformers = ["binoc.column_reorder_detector"]

[expected]
root_kind = "modify"
child_count = 1
has_tags = ["binoc.column-reorder"]
significance = "clerical"
```

Structural assertions in the manifest are the primary check — they survive IR schema evolution. Gold files (`expected-migration.json`) are a secondary, opt-in check for vectors where exact output matters.

Vectors are named for what they test, not how they test it: `csv-column-reorder`, not `test-comparator-csv-3`. They double as documentation — someone reading the vector directory should understand a Binoc capability.

Zip vectors use `.zip.d/` directories that the test harness builds into `.zip` files, including nested zips. This keeps binary files out of version control.

### Testing by Crate

**`binoc-core`** — Controller, IR types, traits, dispatch logic. No format knowledge.

- *Inline unit tests:* IR construction and traversal, dispatch logic with mock comparators, config parsing, transformer dispatch with mock transformers, output serialization and significance classification.
- *Integration tests:* Mock comparators and transformers exercise the full controller loop. Test parallel expansion, Expand result, transformer scoping.
- *Snapshot testing:* `insta` for IR tree serialization regression detection.

**`binoc-stdlib`** — Where most test vectors are consumed.

- *Inline unit tests:* Each comparator in isolation (given two files → verify DiffNode output). Each transformer in isolation (given a hand-constructed tree → verify rewrite).
- *Integration tests:* A test vector runner discovers all vectors, loads each manifest, runs the full pipeline (controller + stdlib), and checks structural assertions. A macro generates one test case per vector.
- *Comparator tests:* Binary (hash identity, add/remove with hash), text (line counting, identical detection), CSV (column add/remove/reorder, row add/remove, cell changes), directory (expansion, file correspondence), zip (extraction, expansion).
- *Transformer tests:* Move detector (hash correlation, non-matching hashes, preserving non-moved children), column reorder detector (pure reorder conversion, mixed change passthrough).

**`binoc-cli`** — CLI binary tested as subprocess.

- *Subprocess tests:* `assert_cmd` crate runs the CLI binary. Tests cover `diff` (JSON and Markdown output, file output, config files), `changelog` (from saved migrations), error cases (missing snapshots, invalid config), and help flags.
- *Test vectors consumed via CLI:* Feed vector snapshot paths to `binoc diff`, verify exit codes and stdout content.

**`binoc-python`** — pytest suite consuming the same test vectors.

- Same vectors, consumed from Python via pytest parameterization.
- Tests exercise Python API: tree traversal, IR construction, JSON/Markdown output.
- Python-authored plugins tested via registration and invocation through the pipeline.

### CI Structure

```yaml
test:
  - cargo test -p binoc-core        # unit + integration, fast
  - cargo test -p binoc-stdlib       # unit + integration + vectors
  - cargo test -p binoc-cli          # CLI integration + subprocess tests
```

### Gold File Maintenance

When the IR schema evolves (new fields, renamed fields), gold files may break. Mitigations:

1. Structural assertions in manifests are more stable and are the primary check. Gold files are secondary.
2. A script can re-run all vectors and offer to update gold files with diff review, similar to `cargo insta review`.

---

## Design Principles

For reference and for contributors:

1. **The controller is type-ignorant.** It processes a work queue of item pairs, dispatching to comparators. It does not know about files, directories, archives, or any data format.
2. **Comparators are the parser.** They turn raw item pairs into IR (diffs and/or expanded sub-items). They need data access.
3. **Transformers are optimization passes.** They rewrite the IR tree. They don't need raw data. (Revisit if this breaks.)
4. **The IR is tree-structured, openly typed, and tag-annotated.** No built-in types or significance levels. Conventions, not enforcement.
5. **Dispatch is declarative-first with an imperative escape hatch.**
6. **Significance is a renderer concern**, mapped from semantic tags via per-renderer config. The IR carries only factual tags.
7. **Migrations are descriptive. Detail extraction is on-demand** via comparator `extract` methods with access to original snapshots.
8. **The standard library is a plugin pack**, architecturally identical to third-party packs.
9. **Distribution unit = core engine + plugin packs.** Community extensions are just plugin packs with configs.
10. **Ordering is a config concern, not a plugin concern.** Plugins suggest; config decides.
11. **Fast.** Rust core. Parallel subtrees. Streaming I/O. Plugins are computationally lean.
12. **Portable.** Rust library with Python bindings. No global state. Embeddable.
13. **The library is the product; the CLI is porcelain.**

