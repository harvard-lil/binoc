---
audience: plugin author
---

# Publish a plugin

**Goal.** Ship a binoc plugin so users can
`pip install your-package` and have the `binoc` CLI discover it
automatically.

**Prerequisites.** A working plugin (see
[Write a Python comparator](write-a-python-comparator.md) or
[Write a Rust comparator](write-a-rust-comparator.md)).

## Naming

Use the shared ecosystem namespace on PyPI:

- **PyPI package name**: `binoc-<your-name>` (analogous to
  `pytest-*`, `llm-*`).
- **Plugin names inside your package**: `<your-name>.<plugin-name>`
  (for example `biobinoc.fasta`). Never use the reserved `binoc.*`
  prefix; that's for the standard library.

See [Plugin discovery](../reference/plugin-discovery.md) for the
full convention table (plugin names, tags, item types, actions).

## The entry point

Every binoc plugin registers under the `binoc.plugins` entry point
group in `pyproject.toml`.

### Python plugin

```toml
[project]
name = "biobinoc"
version = "0.1.0"
dependencies = ["binoc>=0.1"]

[project.entry-points."binoc.plugins"]
biobinoc = "biobinoc:register"
```

```python
# biobinoc/__init__.py
def register(registry):
    from biobinoc.fasta import FastaComparator
    from biobinoc.normalizer import SequenceNormalizer
    registry.register_comparator("biobinoc.fasta", FastaComparator())
    registry.register_transformer("biobinoc.sequence_normalizer", SequenceNormalizer())
```

### Rust (native) plugin

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

Note the entry point value is just the module name, no
`module:function`. The discovery code detects that it's a native
module and loads it via the C ABI. The registration happens inside
the `export_plugin!` macro you already wrote.

## Versioning

Each published binoc package versions independently; your plugin
should too.

**Python plugins.** `binoc` is a real Python API dependency.

- **Lower bound** `binoc>=X.Y` for the Python APIs you use.
- **No upper bound** unless you know of a specific incompatibility.

**Rust plugins.** The Rust compatibility boundary is `binoc-sdk`,
not the `binoc` host.

- **Tight dependency on `binoc-sdk`** in `Cargo.toml` — depend on
  the minor line you built against. Native plugin compatibility is
  checked at runtime via the plugin's `sdk_version`.
- **Loose dependency on `binoc`** in `pyproject.toml` — depend with
  a lower bound for the loader features you need. Do not cap
  `binoc` just to mirror the SDK minor.

See [Release surface and automated publishing ADR](../adr/2026-04-08-release_surface_and_automated_publishing.md)
for why.

## Build and publish

### Python-only plugin

```bash
uv build                              # or: python -m build
uv publish                            # or: twine upload dist/*
```

### Rust (maturin) plugin

```bash
uv run --extra dev maturin develop    # local install for testing
uv run --extra dev maturin build --release
uv publish dist/*.whl
```

For production, set up **trusted publishing** from a GitHub Actions
workflow (OIDC, no PyPI token to manage). The binoc project's own
release setup is documented in
[Cut a release](cut-a-release.md); mirror it for your plugin.

## Test before publishing

From the consumer's perspective:

```bash
pip install ./your-plugin        # editable also fine
binoc diff tests/snap-a tests/snap-b
```

If the plugin shows up in the output (or its comparator claims the
right files), entry-point discovery is working.

For a reproducible test, see
[Test a plugin with vectors](test-a-plugin-with-vectors.md) — the
same harness the stdlib uses.

## Publish alongside the `binoc` project

The in-tree reference plugins (`model-plugins/binoc-sqlite`,
`model-plugins/binoc-row-reorder`, `model-plugins/binoc-html`) are
published independently of the host `binoc` package; your plugin
should be too. Independent releases mean a bugfix to your plugin
doesn't require a new binoc host release, and vice versa.

## Where to go next

- [Plugin discovery reference](../reference/plugin-discovery.md) —
  exhaustive entry-point spec.
- [Install and use plugins](install-and-use-plugins.md) — the
  consumer-facing side of what you just shipped.
- [Cut a release](cut-a-release.md) — the binoc project's own release
  workflow, adaptable for plugin packages.
