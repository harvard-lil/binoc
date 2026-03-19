# Binoc: Contributor Onboarding Tutorial

*2026-03-05T18:42:00Z by Showboat 0.6.1*
<!-- showboat-id: ed87b449-228b-4460-9c83-ae0417fc327a -->

<!-- ### Regenerating This Tutorial

This tutorial is executable — every code block is verified against the current code by [Showboat](https://github.com/simonw/showboat). After making changes that affect tutorial output, regenerate it:

    just docs

This runs `uvx showboat verify` to re-execute each block and update the expected output in place. -->

Binoc generates changelogs for datasets that don't have them. Given a series of snapshots of a dataset downloaded at different times, it detects what changed, expresses those changes as a minimal structured diff, and produces human-readable summaries.

This tutorial walks through the project from daily usage to internal architecture. By the end, you should be ready to contribute code, write plugins, and extend the test suite.

## Who This Is For

You are (or aspire to be) someone who:
- Wants to understand the details of how Binoc works
- Wants to extend Binoc with format-specific plugins for your domain
- Wants to contribute to Binoc

## Dev Setup

### Project Layout

Binoc is a Rust workspace with four crates, plus shared test vectors:

```bash
cat Cargo.toml
```

```output
[workspace]
members = ["binoc-core", "binoc-stdlib", "binoc-cli", "binoc-python", "binoc-sqlite"]
default-members = ["binoc-core", "binoc-stdlib", "binoc-cli", "binoc-sqlite"]
resolver = "2"
```

| Crate | Role |
|---|---|
| `binoc-core` | Controller, IR types, plugin traits. Zero domain knowledge. |
| `binoc-stdlib` | Standard library plugins: directory, zip, CSV, text, binary comparators; move detector and column reorder transformers. Architecturally identical to any third-party plugin pack. |
| `binoc-cli` | CLI library + standalone Rust binary. The library exposes `binoc_cli::run(registry, args)` so both the Rust binary and the Python entry point share the same CLI logic. |
| `binoc-python` | PyO3 bindings, Python plugin discovery via entry points, and the `binoc` console script (the primary user-facing CLI). |
| `binoc-sqlite` | Demo plugin: SQLite schema and row-count diffing (`.sqlite`/`.db`). Reference for plugin authors; test vectors in `binoc-sqlite/test-vectors/`. |
| `test-vectors/` | Shared test data consumed by all crates. Each vector is a pair of snapshots plus a TOML manifest. |

### Prerequisites

You'll need [Rust](https://rustup.rs/), [just](https://github.com/casey/just) (`brew install just` on macOS), and [uv](https://docs.astral.sh/uv/) (for Python bindings and tests).

### Building and Testing

Build everything (Rust workspace + Python bindings):

    just build

Run the full test suite (Rust + Python):

    just test

For fast Rust-only iteration you can also use `cargo build` and `cargo test` directly, which skip the Python crate.

### Local Dev Install

To put the local Python CLI in your path, create a virtual environment and install the package in editable mode:

```bash
uv venv
uv pip install -e ./binoc-python
source .venv/bin/activate
```

## Usage Walkthrough

### Your First Diff: Identical Snapshots

The simplest case: two identical directories. The `trivial-identical` test vector has the same file in both snapshots:

```bash
binoc diff test-vectors/trivial-identical/snapshot-a test-vectors/trivial-identical/snapshot-b
```

```output
# Changelog: test-vectors/trivial-identical/snapshot-a → test-vectors/trivial-identical/snapshot-b

No changes detected.

```

No changes detected — `root` is `null`. The directory comparator expanded the root pair into child file pairs and pre-computed BLAKE3 hashes. The controller saw matching content hashes and short-circuited each pair to `identical` without invoking any leaf comparator. After transformers ran, pruning removed all identical nodes and the now-empty directory container, leaving nothing.

### Text File Changes

When a text file changes between snapshots, the text comparator detects line-level differences:

```bash
cat test-vectors/single-file-modify-text/snapshot-a/story.txt
```

```output
Line 1
Line 2
Line 3
Line 4
Line 5
```

```bash
cat test-vectors/single-file-modify-text/snapshot-b/story.txt
```

```output
Line 1
Line 2 revised
Line 3
Line 4
Line 5
Line 6
```

```bash
binoc diff test-vectors/single-file-modify-text/snapshot-a test-vectors/single-file-modify-text/snapshot-b
```

```output
# Changelog: test-vectors/single-file-modify-text/snapshot-a → test-vectors/single-file-modify-text/snapshot-b

## Substantive Changes

- **story.txt**: 2 lines added, 1 removed

```

Key observations:
- The **directory comparator** expanded the root pair into child file pairs.
- The **text comparator** claimed `story.txt` (by `.txt` extension) and produced a leaf diff with the summary you see above.
- Under the hood, the diff node carries **semantic tags** like `binoc.content-changed` and `binoc.lines-added` (factual observations, not judgments) and **details** with exact line counts. The markdown renderer renders these into the human-readable summary; the JSON changeset artifact preserves the full structured data.

### File Addition and Removal

Binoc handles files that exist in only one snapshot:

```bash
binoc diff test-vectors/single-file-add/snapshot-a test-vectors/single-file-add/snapshot-b
```

```output
# Changelog: test-vectors/single-file-add/snapshot-a → test-vectors/single-file-add/snapshot-b

## Substantive Changes

- **new_file.txt**: New file (1 line)

```

```bash
binoc diff test-vectors/single-file-remove/snapshot-a test-vectors/single-file-remove/snapshot-b
```

```output
# Changelog: test-vectors/single-file-remove/snapshot-a → test-vectors/single-file-remove/snapshot-b

## Substantive Changes

- **removed_file.txt**: File removed (1 line)

```

Files present only in snapshot B get `action: "add"`. Files only in snapshot A get `action: "remove"`. The directory comparator creates `ItemPair::added()` or `ItemPair::removed()` entries, and downstream comparators handle the one-sided case.

### CSV Comparisons

CSV is the most common format in data archiving, and an example of Binoc's value over generic diff tools. The CSV comparator understands columns and rows.

#### Column Reordering

Columns shuffled but content identical — a clerical change that generic diff would flag as a total rewrite:

```bash
cat test-vectors/csv-column-reorder/snapshot-a/data.csv
```

```output
name,age,city
Alice,30,NYC
Bob,25,LA
```

```bash
cat test-vectors/csv-column-reorder/snapshot-b/data.csv
```

```output
city,name,age
NYC,Alice,30
LA,Bob,25
```

```bash
binoc diff test-vectors/csv-column-reorder/snapshot-a test-vectors/csv-column-reorder/snapshot-b
```

```output
# Changelog: test-vectors/csv-column-reorder/snapshot-a → test-vectors/csv-column-reorder/snapshot-b

## Clerical Changes

- **data.csv**: Columns reordered (content unchanged)

```

Notice:
- The output says "Columns reordered (content unchanged)" — the **column reorder transformer** detected that the only change was column ordering and collapsed it from a `modify` to a `reorder`.
- This appears under **Clerical Changes**, not Substantive. The tag `binoc.column-reorder` maps to the "clerical" significance category — housekeeping, not a policy change.
- Under the hood, the diff tree records exactly which columns were in which order, with zero cells changed and zero rows added/removed. That structured detail is available in the JSON changeset artifact and via the `extract` command.

#### Mixed CSV Changes

Real-world dataset updates often combine multiple kinds of changes. Here, columns are reordered AND a new column and row are added:

```bash
cat test-vectors/csv-mixed-changes/snapshot-a/data.csv
```

```output
name,age,city
Alice,30,NYC
Bob,25,LA
```

```bash
cat test-vectors/csv-mixed-changes/snapshot-b/data.csv
```

```output
city,name,age,email
NYC,Alice,30,a@test.com
LA,Bob,25,b@test.com
SF,Charlie,35,c@test.com
```

```bash
binoc diff test-vectors/csv-mixed-changes/snapshot-a test-vectors/csv-mixed-changes/snapshot-b
```

```output
# Changelog: test-vectors/csv-mixed-changes/snapshot-a → test-vectors/csv-mixed-changes/snapshot-b

## Substantive Changes

- **data.csv**: Column added: 'email'; columns reordered; 1 row added

```

The summary line packs three distinct changes into one sentence. Under the hood, these map to separate semantic tags: `binoc.column-addition`, `binoc.column-reorder`, `binoc.row-addition`, and `binoc.schema-change`. The column reorder transformer only collapses a diff to `reorder` when reordering is the *only* change — here there's also an added column and row, so it stays as `modify` with the full detail preserved.

Because the tags include `binoc.column-addition` (substantive), not just `binoc.column-reorder` (clerical), this appears under **Substantive Changes**. The significance classification maps tags to categories independently — a single node with both clerical and substantive tags gets classified by the highest-priority match.

### Markdown Changelog Output

By default, `binoc diff` prints Markdown to stdout. You can also save specific output formats to files with `-o`. Here we save the raw changeset JSON and separately save a Markdown changelog:

```bash
binoc diff test-vectors/csv-mixed-changes/snapshot-a test-vectors/csv-mixed-changes/snapshot-b -o /tmp/changeset.json -o /tmp/changeset.md -q && cat /tmp/changeset.md
```

```output
# Changelog: test-vectors/csv-mixed-changes/snapshot-a → test-vectors/csv-mixed-changes/snapshot-b

## Substantive Changes

- **data.csv**: Column added: 'email'; columns reordered; 1 row added

```

The changeset JSON is the canonical machine-readable artifact. The Markdown is a derived view produced by the output formatter. You can also generate changelogs from saved changesets using `binoc changelog`.

The markdown output groups changes by significance category. Here, `binoc.column-addition` and `binoc.schema-change` match the default `substantive` significance rules, so the change appears under "Substantive Changes." The column reorder alone (as in the earlier example) would appear under "Clerical Changes."

### Extracting Changed Data

A changeset tells you *what* changed — "2 rows were added to data.csv." But sometimes you want the actual data: which rows? `binoc extract` reopens the original snapshots and pulls out the changed content. Extract requires *both* snapshots to be present, as well as the json changeset file, so it can reopen the data through the correct comparator layers.

#### New CSV Rows

The `csv-row-addition` test vector adds two rows to a CSV. First, generate the changeset:

```bash
binoc diff test-vectors/csv-row-addition/snapshot-a test-vectors/csv-row-addition/snapshot-b -o /tmp/tut-csv.json -q
```

Now extract the added rows:

```bash
binoc extract /tmp/tut-csv.json data.csv rows_added
```

```output
name,age
Bob,25
Charlie,35
```

The output is valid CSV — the added rows with their headers. You could pipe this into another tool, load it into a database, or just eyeball what changed.

The extract command works by walking the provenance chain recorded in the changeset. Each node knows which comparator produced it (`comparator` field) and which transformers modified it (`transformed_by` field). During extract, the controller reopens the snapshot files through each layer (directory → file), then asks the responsible plugin to format the result.

#### Text File Diff

For text files, `extract` can produce a unified diff. First generate and save the changeset:

```bash
binoc diff test-vectors/single-file-modify-text/snapshot-a test-vectors/single-file-modify-text/snapshot-b -o /tmp/tut-text.json -q
```

Then extract:

```bash
binoc extract /tmp/tut-text.json story.txt diff
```

```output
 Line 1
-Line 2
+Line 2 revised
 Line 3
 Line 4
 Line 5
+Line 6
```

This works through nested containers too. A text file inside a zip inside a directory — the extract chain reopens each layer to reach the source data.

Available aspects depend on the node type. For tabular nodes: `rows_added`, `rows_removed`, `cells_changed`, `columns_added`, `columns_removed`, `content`. For text: `diff`, `content_left`, `content_right`, `content`. For column reorder nodes: `column_order`.

## Nested Structures: Directories Within Directories

Binoc handles arbitrary nesting naturally. The controller's work loop processes independent subtrees in parallel:

```bash
find test-vectors/directory-nested/snapshot-a -type f | sort
```

```output
test-vectors/directory-nested/snapshot-a/data/records.csv
test-vectors/directory-nested/snapshot-a/docs/readme.txt
```

```bash
find test-vectors/directory-nested/snapshot-b -type f | sort
```

```output
test-vectors/directory-nested/snapshot-b/data/extra.csv
test-vectors/directory-nested/snapshot-b/data/records.csv
test-vectors/directory-nested/snapshot-b/docs/readme.txt
```

```bash
binoc diff test-vectors/directory-nested/snapshot-a test-vectors/directory-nested/snapshot-b
```

```output
# Changelog: test-vectors/directory-nested/snapshot-a → test-vectors/directory-nested/snapshot-b

## Substantive Changes

- **data/extra.csv**: New table (2 columns, 1 rows)
- **data/records.csv**: 1 row added
- **docs/readme.txt**: 2 lines added, 1 removed

```

The diff tree mirrors the directory structure. The `data/` subtree and `docs/` subtree are processed in parallel by rayon. Each file is dispatched to the appropriate comparator by extension: `.csv` files go to the CSV comparator, `.txt` files go to the text comparator. The controller doesn't know about any of this — it just processes item pairs.

### File Moves (The Move Detector Transformer)

When a file appears with a different name but identical content, the move detector transformer correlates adds and removes by content hash:

```bash
find test-vectors/directory-file-move/snapshot-a -type f | sort
```

```output
test-vectors/directory-file-move/snapshot-a/old_name.bin
```

```bash
find test-vectors/directory-file-move/snapshot-b -type f | sort
```

```output
test-vectors/directory-file-move/snapshot-b/new_name.bin
```

```bash
binoc diff test-vectors/directory-file-move/snapshot-a test-vectors/directory-file-move/snapshot-b
```

```output
# Changelog: test-vectors/directory-file-move/snapshot-a → test-vectors/directory-file-move/snapshot-b

## Other Changes

- **new_name.bin**: Moved from old_name.bin

```

Without the move detector, this would appear as an `add` of `new_name.bin` and a `remove` of `old_name.bin`. The transformer collapsed them into a single `move` node with `source_path` showing where it came from. This is the transformer pattern: comparators report raw facts (add + remove), transformers detect higher-level patterns (move).

### Zip Archives

Binoc looks inside zip files. The zip comparator extracts both sides to temp directories and re-enters the controller queue, so any comparator that works on regular files also works on files inside zips:

```bash
binoc diff test-vectors/zip-simple/snapshot-a test-vectors/zip-simple/snapshot-b
```

```output
# Changelog: test-vectors/zip-simple/snapshot-a → test-vectors/zip-simple/snapshot-b

## Substantive Changes

- **archive.zip/data.txt**: 1 line added, 1 removed
- **archive.zip/extra.txt**: New file (1 line)

```

The zip comparator extracted both archives, then the controller processed the extracted directories with the same pipeline. The path `archive.zip/data.txt` shows the logical path through the archive. This nesting is recursive — the `zip-nested` test vector has a zip inside a zip inside a CSV, and it all works.

### The SQLite Plugin

The **binoc-sqlite** plugin is an example of a third-party plugin. It compares schema (tables, columns, types) and row counts — not row-by-row content — so you see "1 table modified", "1 row added (1 → 2 rows)", or "Table added (2 columns, 5 rows)" instead of a raw binary change.

To try it from the repo (requires `sqlite3` on PATH): build two SQLite DBs from the test-vector SQL sources, then run the Python CLI with the plugin:

```bash
# sqlite file setup
if [ -d /tmp/binoc-sqlite-demo ]; then rm -rf /tmp/binoc-sqlite-demo; fi
mkdir -p /tmp/binoc-sqlite-demo/snapshot-a /tmp/binoc-sqlite-demo/snapshot-b
for f in binoc-sqlite/test-vectors/sqlite-row-addition/snapshot-a/data.sqlite.d/*.sql; do sqlite3 /tmp/binoc-sqlite-demo/snapshot-a/data.sqlite < "$f"; done
for f in binoc-sqlite/test-vectors/sqlite-row-addition/snapshot-b/data.sqlite.d/*.sql; do sqlite3 /tmp/binoc-sqlite-demo/snapshot-b/data.sqlite < "$f"; done

# run the CLI with the plugin
uv pip install -e ./binoc-sqlite/
binoc diff /tmp/binoc-sqlite-demo/snapshot-a /tmp/binoc-sqlite-demo/snapshot-b
```

```output
Resolved 2 packages in 1.13s
   Building binoc-sqlite @ file:///Users/jcushman/Documents/binoc/binoc-sqlite
      Built binoc-sqlite @ file:///Users/jcushman/Documents/binoc/binoc-sqlite
Prepared 1 package in 5.21s
Installed 1 package in 1ms
 + binoc-sqlite==0.1.0 (from file:///Users/jcushman/Documents/binoc/binoc-sqlite)
# Changelog: /tmp/binoc-sqlite-demo/snapshot-a → /tmp/binoc-sqlite-demo/snapshot-b

## Substantive Changes

- **data.sqlite/users**: 1 row added (1 → 2 rows)

```

Snapshot-a has one row in `users`, snapshot-b has two. Without the plugin, the same files would be compared as binary and reported as "Content changed". See [binoc-sqlite’s README](../binoc-sqlite/README.md) and [Writing Binoc Plugins](writing_plugins.md).

## Architecture

Now that you've seen the user-facing behavior, let's look under the hood.

### Putting It All Together: A Mental Model for Contributors

Here's the flow for a single `binoc diff` invocation, from input to output:

    Snapshot A ──┐                                          ┌── JSON changeset
                 ├── Controller ─── Comparators ─── IR ─── Transformers ─── Renderer ──┤
    Snapshot B ──┘   (type-ignorant)  (format-aware) (tree) (pattern-aware)  (format)   └── Markdown changelog

**Separation of concerns**:
- The **controller** is a generic tree-processing engine. It never imports or references any data format.
- **Comparators** are the parsers: raw data → IR. They have data access. They're where format knowledge lives.
- **Transformers** are optimization passes: IR → IR. They detect patterns (moves, reorders) across the tree. They don't need raw data.
- **Renderers** serialize the IR. Significance classification (clerical vs. substantive) lives here.
- **Config** controls ordering and composition. Plugins suggest; config decides.

### The Controller Loop

The central insight: **the controller is type-ignorant**. It processes a work queue of item pairs without knowing what a directory, zip, or CSV is.

The algorithm:
1. Receive a root item pair (snapshot A, snapshot B).
2. Walk the comparator pipeline. First comparator to claim the item wins.
3. The comparator either emits a **Leaf** diff (terminal), expands into **child item pairs** (recursive), or reports **Identical** (filtered out).
4. Independent subtrees are processed in parallel (rayon).
5. Once the tree is fully expanded, run the transformer pipeline in order.

In pseudocode, the core `process_pair` method (in `binoc-core/src/controller.rs`):

    process_pair(pair):
        if both sides have matching content hashes:
            return Identical (skip comparator entirely)

        comparator = first comparator in pipeline that claims this pair
        result = comparator.compare(pair)

        match result:
            Identical  → mark node "identical"
            Leaf(node) → return the comparator's diff node
            Expand(container, children) →
                process each child pair in parallel (recurse)
                attach children to container node

The controller has no knowledge of what constitutes a "directory" or "CSV" — that's all in the comparators. Three possible outcomes: `Identical` (filtered out after transformers), `Leaf` (terminal diff), or `Expand` (recurse into children).

### The DiffNode IR

Every comparator emits `DiffNode` values. Every transformer rewrites them. The CLI, serializers, and Python bindings all consume the same structure. Here are its fields (defined in `binoc-core/src/ir.rs`):

| Field | Type | Purpose |
|---|---|---|
| `action` | open enum | `"add"`, `"remove"`, `"modify"`, `"move"`, `"reorder"`, etc. Plugins may define new actions. |
| `item_type` | open string | `"directory"`, `"file"`, `"tabular"`, `"zip_archive"`, etc. The core never interprets it. |
| `path` | string | Logical path within snapshot, e.g. `"archive.zip/data/file.csv"`. |
| `source_path` | optional | For moves/renames: the original path. |
| `summary` | optional | Human-readable one-liner (e.g. "2 lines added, 1 removed"). Set by comparators/transformers, rendered by renderers. |
| `tags` | set of strings | Semantic observations: `binoc.column-reorder`, `binoc.content-changed`, etc. Open and namespaced by convention. |
| `children` | list | Child diff nodes forming the tree structure. |
| `details` | map | Comparator-specific structured data (column lists, row counts, hashes). |
| `annotations` | map | Transformer-added metadata, separate from comparator data. |
| `comparator` | optional | Which comparator produced this node (provenance for extract chain). |
| `transformed_by` | list | Transformers that modified this node, in order (provenance for extract chain). |

Key design decisions:
- **Everything is openly typed.** `action`, `item_type`, and `tags` are plain strings — conventions, not enforcement. A genomics plugin can define `action: "gap-change"` without touching core.
- **Tags are factual observations, not judgments.** Significance classification maps tags to categories in output config, not in the IR.
- **Provenance is tracked.** `comparator` and `transformed_by` record who produced and modified each node, enabling `binoc extract` to reopen data through the right plugin chain.

### The Plugin Dispatch System

Comparators are tried in pipeline order. The first to claim an item wins — URL-routing semantics. For each comparator in order, the controller checks three things:

1. **Extension match** — does the item's file extension match `handles_extensions()`?
2. **Media type match** — does the item's MIME type match `handles_media_types()`?
3. **Imperative claim** — does `can_handle()` return true?

First match wins. Directories skip steps 1 and 2 entirely — this prevents a directory named `archive.zip` (extracted zip contents) from being re-claimed by the zip comparator. The directory comparator claims it via `can_handle` instead.

The default pipeline order (from `DatasetConfig::default_config()` in `binoc-core/src/config.rs`):

| # | Comparator | Claims by |
|---|---|---|
| 1 | `binoc.zip` | `.zip` extension |
| 2 | `binoc.directory` | `can_handle` (is a directory?) |
| 3 | `binoc.csv` | `.csv` extension |
| 4 | `binoc.text` | `.txt` and other text extensions |
| 5 | `binoc.binary` | `can_handle` (catch-all fallback) |

Order matters. Zip comes first because `.zip` extension match must happen before directory `can_handle`. CSV comes before text because `.csv` files should use the column-aware comparator, not line-level diff. Binary is the catch-all fallback.

After comparators, transformers run in order: `binoc.move_detector` → `binoc.copy_detector` → `binoc.column_reorder_detector`.

**This is a config concern, not a plugin concern.** Plugins suggest ordering via `suggested_phase`, but config decides. A custom dataset config can reorder, add, or remove any plugin.

### Significance Classification

Significance is an output concern, not a core IR concern. The mapping is layered:

1. **Comparators** attach semantic tags (`binoc.column-reorder`, `binoc.row-addition`) — factual reporting.
2. **Transformers** may add more tags or annotations.
3. **The output config** maps tags to categories — different users and domains can define different mappings over the same tags.

The default significance mapping lives in the markdown renderer (`binoc-stdlib/src/renderers/markdown.rs`) — since classification is a renderer concern, not a core concern:

| Category | Tags |
|---|---|
| **Clerical** | `binoc.column-reorder`, `binoc.whitespace-change`, `binoc.folder-rename`, `binoc.encoding-change` |
| **Substantive** | `binoc.column-addition`, `binoc.column-removal`, `binoc.schema-change`, `binoc.row-addition`, `binoc.row-removal`, `binoc.content-changed` |

Column reordering is clerical (housekeeping). Column addition is substantive (policy change). A bio research lab could define `biobinoc.alignment-gap` as clerical and `biobinoc.sequence-deletion` as substantive, using the same mechanism. Users override these defaults via the `output.markdown.significance` section of their dataset config YAML.

### The Transformer Pattern

Transformers rewrite the completed diff tree. They run in declared order, matching nodes by type, tag, action, or imperative `can_handle`. Two scopes:
- **Node**: the controller recurses into children first, then the transformer sees each matched node individually.
- **Subtree**: the transformer receives the entire subtree and can rewrite it freely.

The trait interfaces (in `binoc-core/src/traits.rs`) — everything a plugin author needs to implement:

**Comparator** — claims an item pair and either emits a leaf diff or expands into child items:

| Method | Required? | Purpose |
|---|---|---|
| `name()` | yes | Unique identifier, e.g. `"binoc.csv"` |
| `compare(pair, ctx)` | yes | Compare an item pair. Returns `Identical`, `Leaf(node)`, or `Expand(container, children)`. |
| `handles_extensions()` | no | Declarative dispatch by file extension, e.g. `[".csv", ".tsv"]` |
| `handles_media_types()` | no | Declarative dispatch by MIME type, e.g. `["application/zip"]` |
| `can_handle(pair)` | no | Imperative dispatch — return true to claim |
| `handles_identical()` | no | Override to true for containers that need expanding even when byte-identical (e.g. zip, directory) |
| `reopen(pair, child_path, ctx)` | no | Container comparators implement this to reconstruct physical access during `binoc extract` |
| `extract(data, node, aspect)` | no | Extract user-facing data (e.g. added rows, unified diff) from a node this comparator produced |

**Transformer** — rewrites the completed diff tree, matching nodes by declarative filters or imperative `can_handle`:

| Method | Required? | Purpose |
|---|---|---|
| `name()` | yes | Unique identifier, e.g. `"binoc.move_detector"` |
| `transform(node, ctx)` | yes | Rewrite a matched node. Returns `Unchanged`, `Replace(node)`, `ReplaceMany(nodes)`, or `Remove`. |
| `match_types()` | no | Match nodes by `item_type` |
| `match_tags()` | no | Match nodes that have any of these tags |
| `match_actions()` | no | Match nodes by `action` |
| `scope()` | no | `Node` (bottom-up, default) or `Subtree` (receives entire subtree) |
| `can_handle(node)` | no | Imperative filter |
| `extract(data, node, aspect)` | no | Extract data from nodes this transformer modified |

### The Standard Library

The stdlib (`binoc-stdlib`) registers itself into a `PluginRegistry` by name — exactly as a third-party plugin pack would. Its `register_stdlib()` function registers each comparator, transformer, and renderer under its `binoc.*` name. A third-party pack (e.g., BioBinoc for genomics) would do the same: implement the traits, register by name, and users reference them in their dataset config YAML.

## Test Vectors: How Testing Works

Test vectors live in `test-vectors/`. Each is a pair of snapshots plus a TOML manifest declaring what the vector tests and what assertions to check:

```bash
cat test-vectors/csv-column-reorder/manifest.toml
```

```output
[vector]
name = "csv-column-reorder"
description = "Columns shuffled, content identical"
tags = ["csv", "column-reorder", "clerical"]

[config]
comparators = ["binoc.directory", "binoc.csv"]
transformers = ["binoc.column_reorder_detector"]

[expected]
root_action = "modify"
child_count = 1
has_tags = ["binoc.column-reorder"]
significance = "clerical"
```

Structural assertions in the manifest (`root_action`, `child_count`, `has_tags`) are the primary check — they survive IR schema evolution. The `[config]` section specifies which plugins to use, so vectors test specific comparators in isolation.

The test vector runner discovers all vectors, loads each manifest, runs the full pipeline, and checks assertions. Let's see how many vectors we have:

```bash
ls -1d test-vectors/*/manifest.toml | wc -l && echo 'test vectors:' && ls -1d test-vectors/*/ | sed 's|test-vectors/||;s|/||'
```

```output
      17
test vectors:
csv-cell-changes
csv-column-addition
csv-column-removal
csv-column-reorder
csv-mixed-changes
csv-row-addition
directory-file-copy
directory-file-move
directory-nested
single-file-add
single-file-modify-binary
single-file-modify-text
single-file-remove
text-file-move
trivial-identical
zip-nested
zip-simple
```

Vectors are named for what they test, not how they test it: `csv-column-reorder`, not `test-comparator-csv-3`. They double as documentation.

**Adding a new test vector** is one of the easiest ways to contribute:
1. Create a directory under `test-vectors/` with snapshot-a and snapshot-b.
2. Write a `manifest.toml` with the expected behavior.
3. Run `cargo test -p binoc-stdlib` — the vector runner auto-discovers it.

## Extending Binoc: A Worked Example

The built-in comparators handle common formats — directories, text files, CSVs, zip archives. But datasets often contain domain-specific formats that a generic diff can't interpret meaningfully. This is where Python plugins come in.

### The Problem: Noisy Diffs on Domain Files

Suppose you're tracking a genomics dataset that contains FASTA sequence files. Between two releases, a database re-export rewrote all the header annotations but left the actual sequences unchanged:

```bash
cat docs/examples/fasta-demo/snapshot-a/sequences.fasta
```

```output
>gene_1 src=GenBank date=2024-01
ATCGATCGATCG
>gene_2 src=GenBank date=2024-01
GCTAGCTAGCTA
```

```bash
cat docs/examples/fasta-demo/snapshot-b/sequences.fasta
```

```output
>gene_1 source=NCBI retrieved=2024-06
ATCGATCGATCG
>gene_2 source=NCBI retrieved=2024-06
GCTAGCTAGCTA
```

Out of the box, Binoc doesn't know what FASTA is. The `.fasta` extension isn't claimed by any comparator except the binary catch-all:

```bash
binoc diff docs/examples/fasta-demo/snapshot-a docs/examples/fasta-demo/snapshot-b
```

```output
# Changelog: docs/examples/fasta-demo/snapshot-a → docs/examples/fasta-demo/snapshot-b

## Substantive Changes

- **sequences.fasta**: Content changed (92 bytes → 102 bytes)

```

"Content changed" — true, but unhelpful. An archivist seeing this can't tell whether the actual genomic data changed or if it's just metadata noise. And because `binoc.content-changed` is classified as substantive, this would show up alongside real data changes.

### The Solution: A Python Comparator

A custom comparator that understands FASTA can parse the format and report what actually changed. Here's the complete plugin — about 30 lines of Python:

    import binoc

    class FastaComparator(binoc.Comparator):
        name = "bio.fasta"
        extensions = [".fasta", ".fa"]

        def compare(self, pair):
            left = self._parse(open(pair.left_path).read()) if pair.left_path else {}
            right = self._parse(open(pair.right_path).read()) if pair.right_path else {}

            ids = sorted(set(left) | set(right))
            seqs_changed = sum(
                1 for i in ids
                if left.get(i, {}).get("seq") != right.get(i, {}).get("seq")
            )
            hdrs_changed = sum(
                1 for i in ids
                if left.get(i, {}).get("hdr") != right.get(i, {}).get("hdr")
            )

            if not seqs_changed and not hdrs_changed:
                return binoc.Identical()

            tags = []
            if seqs_changed:
                tags.append("bio.sequence-change")
            if hdrs_changed:
                tags.append("bio.header-change")

            if seqs_changed:
                summary = f"{seqs_changed} sequence(s) changed"
            else:
                summary = f"Headers updated ({hdrs_changed} records); sequences unchanged"

            return binoc.Leaf(binoc.DiffNode(
                action="modify", item_type="fasta", path=pair.logical_path,
                summary=summary, tags=tags,
            ))

        @staticmethod
        def _parse(text):
            records = {}
            current = None
            for line in text.strip().split("\n"):
                if line.startswith(">"):
                    current = line.split()[0][1:]
                    records[current] = {"hdr": line, "seq": ""}
                elif current:
                    records[current]["seq"] += line.strip()
            return records

Register it and run:

    config = binoc.Config.default()
    config.add_comparator(FastaComparator())

    changeset = binoc.diff(
        "docs/examples/fasta-demo/snapshot-a",
        "docs/examples/fasta-demo/snapshot-b",
        config=config,
    )
    print(binoc.to_markdown([changeset]))

Now the output reads:

    ## Other Changes

    - **sequences.fasta**: Headers updated (2 records); sequences unchanged

The archivist immediately knows this is metadata noise, not a data change.

### Classifying Custom Tags with Config

The output says "Other Changes" because the default significance mapping doesn't know about `bio.header-change`. A dataset config YAML file can teach the renderer about your domain-specific tags:

    # dataset.yaml
    output:
      markdown:
        significance:
          clerical:
            - binoc.column-reorder
            - binoc.whitespace-change
            - bio.header-change       # header-only changes are housekeeping
          substantive:
            - binoc.column-addition
            - binoc.content-changed
            - bio.sequence-change     # actual sequence changes matter

Load this config alongside your custom comparator:

    config = binoc.Config.from_file("dataset.yaml")
    config.add_comparator(FastaComparator())

    changeset = binoc.diff("snapshot-a", "snapshot-b", config=config)
    print(binoc.to_markdown([changeset]))

Now the same diff appears under **Clerical Changes** instead, clearly separated from real data changes in the changelog.

The `add_comparator()` call above is the scripting path — great for Jupyter notebooks and one-off analysis. For reusable plugins, Binoc uses **Python entry points** for automatic discovery: `pip install biobinoc` makes the plugin available to the `binoc` CLI automatically, and the config file can reference it by name (`comparators: [biobinoc.fasta]`).

This is the separation of concerns in practice: the **comparator** reports facts (which tags apply), **config** decides what those facts mean (clerical vs. substantive), and the **renderer** renders the result. Different teams tracking the same dataset can use different configs to prioritize different kinds of changes.

For the full story on packaging plugins (Python and Rust), entry-point discovery, the `PluginRegistry` API, naming conventions, and the two-CLI architecture (`binoc` vs `binoc-cli`), see [Writing Binoc Plugins](writing_plugins.md).

## Quick Reference: Contributing

| Task | Where to Start |
|---|---|
| Add a new test vector | `test-vectors/` — create a dir with snapshot-a, snapshot-b, manifest.toml |
| Write a new comparator | `binoc-stdlib/src/comparators/` for stdlib; see [Writing Plugins](writing_plugins.md) for third-party |
| Write a new transformer | `binoc-stdlib/src/transformers/` for stdlib; see [Writing Plugins](writing_plugins.md) for third-party |
| Fix a CLI bug | `binoc-cli/src/lib.rs` — the CLI library; `main.rs` just calls `binoc_cli::run()` |
| Add Python API surface | `binoc-python/src/lib.rs` (PyO3) + `binoc-python/python/binoc/__init__.py` |
| Change the IR | `binoc-core/src/ir.rs` — affects everything downstream |
| Modify dispatch logic | `binoc-core/src/controller.rs` — the `find_comparator` and `process_pair` methods |
| Tune default significance | `binoc-stdlib/src/renderers/markdown.rs` — the `default_significance` function |

Run the full test suite before submitting:

    cargo test

All green. Welcome to the project.
