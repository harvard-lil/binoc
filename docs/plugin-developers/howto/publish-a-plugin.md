---
audience: plugin author
---

# Publish a plugin

**Goal.** Ship a binoc plugin so users can
`pip install your-package` and have the `binoc` CLI discover it
automatically.

**Prerequisites.** A working plugin (see
[Write a Python renderer](write-a-python-renderer.md) or
[Write a Rust rule pack](write-a-rust-rule-pack.md)).

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
    from biobinoc.renderer import FastaSummaryRenderer
    registry.register_renderer("biobinoc.fasta_summary", FastaSummaryRenderer())
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

Native loading currently applies to stable-tier renderer plugins. Rust
correspondence rule packs are registered in process by Rust hosts while the rule
ABI is still settling.

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

See [Release surface and automated publishing ADR](../../adr/2026-04-08-release_surface_and_automated_publishing.md)
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
[Cut a release](../../core-developers/howto/cut-a-release.md); mirror it for your plugin.

## Test before publishing

From the consumer's perspective:

```bash
pip install ./your-plugin        # editable also fine
binoc diff tests/snap-a tests/snap-b
```

If the plugin's renderer or rule-pack behavior shows up in the output,
entry-point discovery and registration are working.

For a reproducible test, see
[Test a plugin with vectors](test-a-plugin-with-vectors.md) — the
same harness the stdlib uses.

## Publish alongside the `binoc` project

The in-tree reference plugins under `model-plugins/` are first-party examples;
some are bundled into the host `binoc` package, some are opt-in, and some may
publish separately. Third-party plugins should still publish independently of
the host `binoc` package so a plugin bugfix does not require a new host release.

## Where to go next

- [Plugin discovery reference](../reference/plugin-discovery.md) —
  exhaustive entry-point spec.
- [Install and use plugins](../../users/howto/install-and-use-plugins.md) — the
  consumer-facing side of what you just shipped.
- [Cut a release](../../core-developers/howto/cut-a-release.md) — the binoc project's own release
  workflow, adaptable for plugin packages.
