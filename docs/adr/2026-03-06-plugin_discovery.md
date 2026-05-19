# Plugin Discovery and the Rust/Python Boundary

**Date:** 2026-03-06
**Status:** Implemented

## Problem

Binoc's config files reference plugins by name (`comparators: [biobinoc.fasta]`), but there was no mechanism to resolve those names to implementations. The Rust CLI hardcoded `binoc_stdlib::default_registry()`. The Python bindings had a workaround (`Config.add_comparator()`) that bypassed name-based resolution entirely, appending plugin objects after the stdlib pipeline. Third-party plugins couldn't be used from the command line at all.

The core question: **how does a name in a YAML config file become a running plugin?** This is really two questions — where does discovery happen, and where does execution happen — which in a mixed Rust/Python codebase forces a decision about which language owns which responsibility.

## Decision

**Python owns discovery. Rust owns execution.** They meet at the `PluginRegistry`, which is a Rust struct exposed to Python via PyO3.

At startup, the Python CLI scans `importlib.metadata.entry_points(group="binoc.plugins")` and calls each discovered `register(registry)` function, populating the `PluginRegistry`. Then the Rust CLI engine takes over — all per-file dispatch goes through Rust trait object vtables, whether the plugin came from stdlib, a third-party Rust crate, or a Python class.

This means:

1. **Python is startup-only.** Entry point scanning happens once (~50ms). All per-file work is pure Rust.

2. **Rust plugins avoid the Python bridge at runtime.** A third-party Rust plugin (e.g. BioBinoc) is packaged as a PyO3 module. Its `register(registry)` function is called once from Python at startup, but it registers native `Arc<dyn Comparator>` trait objects. Per-file dispatch is a Rust vtable call — no GIL, no serialization, same speed as stdlib.

3. **Two distribution channels, one engine.** `pip install binoc` gives a CLI with plugin discovery. `cargo install binoc-cli` gives a standalone Rust binary with stdlib only. Both call `binoc_cli::run(registry, args)` — the only difference is how the registry was populated.

4. **`Config.add_comparator()` remains** for Jupyter/scripting use where you don't want to package anything.

## Why Python for discovery?

The target audience (archivists, data scientists) lives in the Python ecosystem. `pip install biobinoc` is the natural distribution gesture. Python entry points are the standard mechanism for pip-installed packages to register themselves with a host — pytest, tox, and [llm](https://github.com/simonw/llm/) all use this pattern. Trying to build a parallel Rust-native discovery system (dynamic libraries, a plugin directory, etc.) would mean fighting Rust's lack of a stable ABI and reimplementing packaging infrastructure that pip already provides.

The overhead is negligible: entry point scanning is one-time startup cost, and Rust plugins registered through Python entry points pay zero runtime cost per file.

## Implementation details

**Entry points, not pluggy.** [Pluggy](https://pluggy.readthedocs.io/en/stable/) (used by pytest and llm) adds hook specifications, call ordering, and wrappers on top of entry points. Binoc doesn't need any of these — ordering is a config concern, and the "hook" is just `register(registry)`. Raw `importlib.metadata.entry_points` keeps the mechanism to ~10 lines with no runtime dependency. Pluggy can be adopted later if the plugin ecosystem grows to need validation or call ordering at the registration layer.

**`binoc-cli` is a library.** The crate exposes `pub fn run(registry: PluginRegistry, args)` so both the Rust `main()` and the Python entry point share CLI logic. No duplication of argument parsing.

**`PluginRegistry` is exposed to Python** as `binoc.PluginRegistry` with methods for registering comparators/transformers and listing what's registered.

## Alternatives considered

- **Dynamic libraries (.so/.dylib):** Rust has no stable ABI; version skew between host and plugin causes UB. Cross-platform shared library distribution is painful.
- **Distro binaries only (BioBinoc as its own Rust binary):** Simple and zero-overhead, but not composable — combining two plugin packs requires yet another custom binary. Also moves CLI maintenance burden onto every plugin author.
- **Config-referenced import paths (`biobinoc.comparators:FastaComparator`):** Maximally explicit but ugly in YAML, only works in Python, breaks the "names are opaque identifiers" design.
- **WASM plugins:** Sandboxed and portable, but the ecosystem is immature for this pattern and adds significant implementation complexity.
