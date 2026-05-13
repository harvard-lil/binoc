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
either a `register(registry)` function (for Python plugins) or a
native module (for Rust plugins built with maturin). Either way, the
host learns about the plugin's comparators, transformers, and
renderers at startup.

You don't need to "enable" or "configure" the plugin to run it —
installing the package is enough. Plugins become available in the
default pipeline as soon as they are discovered.

See [Plugin discovery](../reference/plugin-discovery.md) for the
exact strings involved, and
[Plugin model](../explanation/plugin-model.md) for the conceptual
overview.

## Where do plugins come from?

The `binoc-*` namespace on PyPI is the shared ecosystem namespace
(similar to `pytest-*` or `llm-*`). Published reference plugins today
include:

- `binoc-sqlite` — SQLite schema and row-count diffing.
- (More plugins will land here as the ecosystem grows.)

For in-tree reference implementations see the `model-plugins/`
directory in the repository — they double as worked examples for
[Write a Python comparator](write-a-python-comparator.md),
[Write a Rust comparator](write-a-rust-comparator.md), and the
other authoring recipes.

## List what's registered

Once a plugin is installed, its comparators and transformers are
available to reference by name in a
[dataset config](../reference/dataset-config.md):

```yaml
comparators:
  - binoc.zip
  - binoc.directory
  - binoc-sqlite.sqlite   # provided by binoc-sqlite
  - binoc.csv
  - binoc.text
  - binoc.binary
```

To restrict the pipeline (for example to test just one comparator
against a specific file type) shorten the list. To reorder, rearrange
it — the first comparator to claim an item wins, per
[Dispatch model](../explanation/dispatch-model.md).

## Plugins in a one-shot script or notebook

If you are prototyping in a notebook or one-off script, attach a
plugin instance directly to a `Config`:

```python
import binoc
from my_plugin import FastaComparator

config = binoc.Config.default()
config.add_comparator(FastaComparator())
changeset = binoc.diff("snapshot-a", "snapshot-b", config=config)
```

This skips entry-point discovery entirely. Ad-hoc comparators run
before the built-in binary fallback, so they can claim a custom file
extension before binoc falls back to a byte-level diff. For anything
reusable, package the plugin and let entry-point discovery handle it.

## Trust

Plugins run in-process with the host's privileges. **Only install
plugins from sources you trust at least as much as you trust running
their code on your machine.** See
[Security and trust](../explanation/security-and-trust.md) for the
short version and
[Security posture and auditing ADR](../adr/2026-04-10-security_posture_and_auditing.md)
for the long version.

## Where to go next

- [Plugin discovery reference](../reference/plugin-discovery.md) —
  the exact entry-point strings and registry API.
- [Dataset config](../reference/dataset-config.md) — referencing
  plugin names from YAML.
- [Plugin model explanation](../explanation/plugin-model.md) — the
  three-axis plugin split and why it exists.
- [Publish a plugin](publish-a-plugin.md) — if you want to build your
  own.
