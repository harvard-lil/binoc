# Plugin SDK, ABI Safety, and Native Plugin Loading

**Date:** 2026-03-12
**Status:** Implemented

## Context

Binoc's plugin interface was tightly coupled to in-process assumptions. Plugin authors depended on `binoc-core`, which bundled stable concepts (traits, IR types) with unstable internals (controller, dispatch, `CompareContext` mutexes). Comparators accessed data through `Item.physical_path` and `std::fs`, cross-phase data sharing used a closed `ReopenedData` enum in core, and the `CompareContext` struct was process-local shared mutable state. These couplings would fight any move to IPC or WASM, and already caused problems at the PyO3 boundary — Python plugins received a degraded interface missing `content_hash`, `media_type`, and `CompareContext`.

The `binoc-sqlite` plugin demonstrated the cost: its Rust comparator results were serialized to JSON, bounced through a Python wrapper class, deserialized back into Rust — a round-trip through two serialization boundaries to get a Rust `DiffNode` from one Rust function to another.

## Decisions

### 1. New `binoc-sdk` crate separates plugin surface from engine

Plugin authors (including `binoc-stdlib` and third-party plugins) depend on `binoc-sdk`. The engine (`binoc-core`) also depends on `binoc-sdk` and implements its abstractions. This separates what plugin authors program against from how the engine implements dispatch, data access, and communication.

```
SDK["binoc-sdk"] ← Core["binoc-core"]
SDK ← Stdlib["binoc-stdlib"]
SDK ← Sqlite["binoc-sqlite"]
```

### 2. Three-layer SDK design

**Registration layer (descriptors).** Each plugin type has a serializable descriptor struct (`ComparatorDescriptor`, `TransformerDescriptor`, `RendererDescriptor`). Descriptors carry static metadata: name, extensions, media types, scope, `sdk_version` (auto-set from `CARGO_PKG_VERSION`). All structs are `#[non_exhaustive]` so new fields can be added without breaking compiled plugins.

**Typed data layer (IR and results).** `DiffNode`, `Changeset`, `CompareResult`, `TransformResult`, `ExtractResult` live in the SDK. These were already `Serialize`/`Deserialize` and wire-friendly. `CompareResult` gains a `Skip` variant (see below). Enums are `#[non_exhaustive]` for forward compatibility.

**Raw data layer (`DataAccess` trait).** Replaces both `Item.physical_path` and `CompareContext`. Plugin I/O goes through `read_bytes`, `open_read`, `local_path`, `provide`, `workspace`, `register_local`, `store`, `load`, and `data_root` methods on a `&dyn DataAccess` argument. The engine supplies `LocalDataAccess` (filesystem-backed) in three modes:
- `new()` — owns a session temp dir as `data_root` (used by the controller)
- `for_plugin(data_root, workspace)` — shares the host's `data_root` for cache access and uses a host-provided workspace for expansion (used by the `export_plugin!` macro across the C ABI)
- `with_data_root(data_root)` — read-only access to an existing `data_root` cache (used for extract-only scenarios)

### 3. Declarative-only routing — `can_handle` removed

Every current use of `can_handle` was checking `is_dir()` — a metadata check that belongs in the descriptor. The `scope: ItemScope` field (`Files` / `Containers` / `Any`) in `ComparatorDescriptor` replaces it. The `Comparator` trait has no `can_handle` method.

If a comparator is dispatched by descriptor match but discovers at compare-time it can't handle the item (e.g., invalid zip despite `.zip` extension), it returns `CompareResult::Skip`. The controller tries the next comparator. This eliminates a separate pre-check round-trip (important for IPC/WASM) while preserving the escape hatch as part of the real work.

### 4. Source-item access replaces cross-phase cache for tabular data; `store()`/`load()` retained for expensive parses

The closed `ReopenedData { Tabular, Text, Binary }` enum was an extensibility bottleneck — third-party plugins couldn't add variants. Replaced by two mechanisms:

**`DiffNode.source_items` — direct source access.** The controller sets `source_items: Option<ItemPair>` on every node it processes. Transformers and extractors that need the original data re-parse it via `data.local_path()` or `data.read_bytes()` on the `ItemRef`s. The field is `#[serde(skip)]` — present at runtime, absent from changeset JSON (handles are ephemeral temp paths). For the C ABI, `source_items` is carried explicitly in `TransformRequest` and `ExtractRequest`, and the `export_plugin!` macro reconstructs it on the plugin side.

This is the preferred pattern for tabular transformers. The CSV comparator no longer caches its parsed data — the row-reorder transformer and column-reorder detector re-parse the source CSVs directly. This avoids writing a JSON-serialized copy of the CSV to disk (which was strictly larger and slower to parse than the original).

**`DataAccess::store(key, &[u8])` / `load(key)` — filesystem-backed cache.** Still available for plugins where caching genuinely helps: expensive parses (e.g., SQLite schema introspection), binary format conversions, or cases where the cached representation is meaningfully more efficient than the source (e.g., Arrow IPC for large columnar data). The cache is filesystem-backed under `<data_root>/.cache/` so data written by the host is visible to separately-compiled plugins sharing the same `data_root` across the C ABI boundary.

`TabularData`/`TabularDataPair` remain as SDK convenience types (convention, not protocol).

The `reopen` method remains on the `Comparator` trait. Container comparators (directory, zip, tar) implement `reopen(pair, child_path, data)` to reconstruct physical access to a child item without re-diffing — directory resolves a child path, zip re-extracts to a workspace.

`Controller::extract()` walks the ancestor chain from root to target node, calling `reopen()` at each container level to reconstruct the scratchpad. At the leaf, it calls `compare()` to re-derive the data, sets `source_items` on the target node, then calls `extract()` on the last transformer (or the comparator itself if no transformer modified the node). See the [provenance and extract ADR](provenance_and_extract.md) for the design rationale.

### 5. SDK version checking at registration

The controller validates every plugin's `sdk_version` against the host's SDK version at registration time (`Controller::new`). Incompatible plugins produce a `BinocError::SdkVersion` error naming the plugin and both versions.

During 0.x, the accepted range is `[MIN_COMPATIBLE_MINOR, host_minor]` within the same major version — patch may differ. The SDK declares a `MIN_COMPATIBLE_MINOR` constant (the compatibility floor). Adding new `#[non_exhaustive]` fields with `#[serde(default)]` doesn't require bumping the floor, since older plugins that omit those fields deserialize correctly. A breaking protocol change (renamed or removed field, changed enum variant semantics) requires bumping the floor so older plugins are rejected with a clear error rather than silent data corruption.

After 1.0, standard semver applies: same major, plugin minor <= host minor.

### 6. ABI safety — two plugin loading modes

Rust has no stable ABI. `#[non_exhaustive]` is source-compatible but not binary-compatible — separately compiled plugins exchanging Rust trait objects risk undefined behavior from layout mismatch.

**Same-build plugins (direct dispatch).** Plugin and engine compile in the same cargo workspace. Types share memory layout by construction. Dispatch is via Rust trait object vtable. This is `binoc-stdlib` and anything in the workspace.

**Separately-compiled plugins (C ABI + JSON serialization).** The SDK provides an `export_plugin!` macro that generates `#[no_mangle] extern "C"` entry points. All data crossing the boundary is serialized as JSON. Version mismatch produces a runtime deserialization error, never UB.

The macro conditionally generates entry points based on declared plugin types:
- `_binoc_plugin_describe` — returns a JSON `PluginDescription` containing descriptors for all comparators, transformers, and renderers in the plugin (always generated)
- `_binoc_free_string` — deallocator for returned strings (always generated)
- `_binoc_comparator_compare`, `_binoc_comparator_reopen`, `_binoc_comparator_extract` — comparator entry points (generated when comparators are declared)
- `_binoc_transformer_transform`, `_binoc_transformer_extract` — transformer entry points (generated when transformers are declared)
- `_binoc_renderer_render` — renderer entry point (generated when renderers are declared)

Each entry point takes an `index` (selecting which plugin within the pack) and a JSON request, and returns a JSON response. Request types carry the necessary context for cross-process operation: `CompareRequest` and `ReopenRequest` include `data_root` and `workspace` paths; `TransformRequest` and `ExtractRequest` include `data_root`; `RenderRequest` includes serialized changesets and config.

The host (`binoc-python`) uses `libloading` to dlopen the plugin `.so`, reads the descriptor, and wraps each plugin in a `NativeComparator`, `NativeTransformer`, or `NativeRenderer` that implements the corresponding trait by serializing/deserializing through the C ABI. Before each comparator call, the host allocates a workspace in the controller's `DataAccess` and passes its path in the request. The controller doesn't know the difference between a same-build and a native-loaded plugin.

### 7. Plugin authors write one line of transport glue

The `export_plugin!` macro generates all transport glue:

```rust
// binoc-sqlite — comparator plugin
binoc_sdk::export_plugin! {
    module: binoc_sqlite,
    comparators: [SqliteComparator],
}

// binoc-row-reorder — transformer plugin
binoc_sdk::export_plugin! {
    module: binoc_row_reorder,
    transformers: [RowReorderDetector],
}
```

The `module:` argument is the Python module name (must match crate lib name). When the `python` feature is active, the macro also generates an empty `#[pymodule]` so maturin can package the `.so`. The plugin has no `pyo3` dependency in its own code, no knowledge of Python, and no `binoc-core` dependency. A single plugin pack can export any combination of comparators, transformers, and renderers.

### 8. Unified discovery — single entry point group

Both Python and native plugins register under `binoc.plugins` in `pyproject.toml`. The discovery code (`_discovery.py`) distinguishes by what `ep.load()` returns: if it's a callable, it's a Python plugin (call `register(registry)`); if it's a module with a native extension, it's a native plugin (call `registry.load_native_plugin(module_name)`). One group, one mental model.

For native plugins, the entry point points directly to the native module:

```toml
# Rust cdylib loaded via C ABI
[project.entry-points."binoc.plugins"]
binoc-sqlite = "binoc_sqlite"
```

For pure Python plugins, the entry point points to a register function:

```toml
# Pure Python plugin
[project.entry-points."binoc.plugins"]
binoc-html = "binoc_html:register"
```

### 9. Three plugin archetypes demonstrated in `model-plugins/`

The `model-plugins/` directory contains example plugins that are architecturally identical to third-party plugins (separate crates, no `binoc-core` dependency). They serve as reference implementations for the three plugin types:

**`binoc-sqlite` — Rust comparator** (native C ABI). Compares SQLite databases by schema and row counts. Demonstrates `export_plugin!` with comparators, test vector harness, and the native loading smoke test.

**`binoc-row-reorder` — Rust transformer** (native C ABI). Detects re-sorted CSV tables by loading cached `TabularDataPair` via `data.load()` and checking whether rows are a permutation. Demonstrates `export_plugin!` with transformers and cross-phase cache access.

**`binoc-html` — Pure Python renderer**. Renders changesets as self-contained HTML changelogs. Demonstrates the Python plugin authoring path: a class with `name`, `file_extension`, and `render()`, plus a `register(registry)` entry point.

Layout of a Rust plugin:

```
model-plugins/binoc-sqlite/
├── Cargo.toml
├── pyproject.toml
├── src/
│   ├── lib.rs          # export_plugin! + pub use
│   └── sqlite.rs       # Comparator impl
├── tests/
│   ├── test_vectors.rs # Rust test vector suite
│   └── test_python.py  # single smoke test: "does native loading work?"
└── test-vectors/       # plugin-specific vectors
```

Layout of a pure Python plugin:

```
model-plugins/binoc-html/
├── pyproject.toml
├── binoc_html/
│   └── __init__.py     # Renderer class + register()
└── tests/
    └── test_html.py    # Python tests
```

Plugin-specific test vectors live within the plugin's own `test-vectors/` directory, not the shared `test-vectors/` root. This prevents the `binoc-stdlib` test harness from attempting to run vectors that require plugins it doesn't know about.

## Alternatives Considered

**Shared-filesystem scratchpad (Plan 9 style).** An early exploration proposed making a structured directory the universal plugin interface — all data access as file reads/writes, parsed data cached as files in conventional formats. Rather than adopting the full Plan 9 model, the scratchpad idea was absorbed into the `DataAccess` trait: the controller owns a workspace directory tree, plugins write into it via `workspace()` and `provide()`, and the controller manages cleanup. Cross-ABI plugins receive their workspace path in the request. The filesystem-backed `store()`/`load()` cache goes further, making the scratchpad the cross-ABI data channel — but in-process plugins can still use the same API without filesystem overhead for hot paths.

**Raw pointer handoff for plugin registration.** An intermediate proposal had plugins expose a C function taking a raw pointer to `PluginRegistry`, called via `ctypes`. Rejected because it leaks host internals (`PluginRegistry` layout) into the plugin boundary and risks UB if compiled separately. The plugin shouldn't push into a host data structure — it should just export itself.

**`register(&mut PluginRegistry)` as the SDK contract.** The original pattern had plugins depend on `PluginRegistry` and call `registry.register_comparator(Arc::new(...))`. Rejected because `PluginRegistry` is a host-internal type. The SDK-clean version: the plugin exports via `export_plugin!`, the host imports via its native loader. The plugin never names a host type.

**Two entry point groups (`binoc.plugins` + `binoc.native_plugins`).** Proposed to distinguish Python vs. native plugins at the packaging level. Rejected — it leaks implementation details and forces plugin authors to know their transport kind. One group with auto-detection is simpler.

**`can_handle` with IPC round-trip.** Keeping `can_handle` as a separate method the controller calls before `compare`. Rejected because every current implementation is a metadata check (`is_dir()`) that belongs in the descriptor, and a separate round-trip per candidate comparator is expensive over IPC/WASM. `CompareResult::Skip` handles the rare content-dependent case without a pre-check.

**In-memory cache for `store()`/`load()`.** The initial implementation kept cached data in a `HashMap` inside `LocalDataAccess`. This doesn't work across the C ABI — a native plugin gets its own `LocalDataAccess` instance and can't see the host's in-memory map. Making the cache filesystem-backed under a shared `data_root` solves this transparently.

**JSON-serialized tabular cache as cross-phase data channel.** The first version of cross-phase data sharing had the CSV comparator `store()` its parsed `TabularDataPair` as JSON, and transformers `load()` it. This was strictly worse than re-parsing the CSV: the JSON encoding was larger and no faster to deserialize. Replaced by `source_items` on `DiffNode` — transformers re-parse the source files directly. The `store()`/`load()` API is retained for cases where the cached format is genuinely more efficient than re-parsing (e.g., Arrow IPC for large datasets, or expensive parses like SQLite schema introspection).

**Separate `export_comparator!`, `export_transformer!`, `export_renderer!` macros.** Considered but rejected in favor of a unified `export_plugin!` that declares all types in one invocation. This better matches the reality that a single `.so` is one plugin pack that may contain any mix of plugin types, and avoids multiplying the number of `_binoc_plugin_describe` symbols.

**Removing `reopen()` from the Comparator trait.** During the initial SDK extraction, `reopen()` was removed under the assumption that `DataAccess` methods replaced its role. This was incorrect — `reopen()` serves a distinct purpose in the extract chain: on-demand reconstruction of physical access to nested items (e.g., extracting a file from a zip) without re-running the diff. It was restored with the same semantics, adapted to take `&dyn DataAccess` instead of the old `CompareContext`.