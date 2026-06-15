---
audience: data steward, plugin consumer
---

# Install and use plugins

**Goal.** Extend binoc with domain-specific format support by
installing a plugin package.

**Prerequisites.** `binoc` working at the command line (see
[Diff two snapshots](diff-two-snapshots.md)).

## The one-liner

Plugins are regular Python packages distributed on PyPI. Install one
and it becomes available automatically — no configuration required:

```bash
pip install binoc-sqlite
binoc diff snapshots/v1 snapshots/v2
```

With `uvx` you can run binoc plus a plugin without installing
anything permanently:

```bash
uvx --with binoc-sqlite binoc diff snapshots/v1 snapshots/v2
```

Either way, `.sqlite` / `.db` files in the snapshots now get semantic
schema and row-count diffs instead of "content changed".

## How it works

At startup, binoc scans Python entry points in the group
`binoc.plugins` and loads everything it finds. An installed plugin
package declares an entry point in its `pyproject.toml` and exposes
either a `register(registry)` function (for Python plugins) or a native module
(for Rust plugins built with maturin). Either way, the host learns about the
plugin's available surfaces at startup.

You don't need to "enable" the plugin to load it; installing the package is
enough. Dataset-specific semantics still belong in dataset config when a plugin
documents them.

See [Plugin discovery](../../plugin-developers/reference/plugin-discovery.md) for the
exact strings involved, and
[Plugin model](../../plugin-developers/explanation/plugin-model.md) for the conceptual
overview.

## Where do plugins come from?

The `binoc-*` namespace on PyPI is the shared ecosystem namespace
(similar to `pytest-*` or `llm-*`). Published reference plugins today
include:

- `binoc-sqlite` — SQLite schema and row-count diffing.
- (More plugins will land here as the ecosystem grows.)

For in-tree reference implementations see the `model-plugins/`
directory in the repository. They double as worked examples for the current
[Plugin model](../../plugin-developers/explanation/plugin-model.md).

## List what's registered

Once a plugin is installed, any stable plugin surfaces it exposes are available
through the host. Current diff behavior uses correspondence rule packs and
renderer plugins.

For current dataset semantics, use [dataset config](../reference/dataset-config.md).
For the rule-family dispatch model, see
[Dispatch model](../../plugin-developers/explanation/dispatch-model.md).

## Trust

Plugins run in-process with the host's privileges. **Only install
plugins from sources you trust at least as much as you trust running
their code on your machine.** See
[Security and trust](../explanation/security-and-trust.md) for the
short version and
[Security posture and auditing ADR](../../adr/2026-04-10-security_posture_and_auditing.md)
for the long version.

## Where to go next

- [Plugin discovery reference](../../plugin-developers/reference/plugin-discovery.md) —
  the exact entry-point strings and registry API.
- [Dataset config](../reference/dataset-config.md) — dataset semantics and
  renderer config.
- [Plugin model explanation](../../plugin-developers/explanation/plugin-model.md) — the
  current rule-family plugin split and why it exists.
- [Publish a plugin](../../plugin-developers/howto/publish-a-plugin.md) — if you want to build your
  own.
