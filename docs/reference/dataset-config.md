---
audience: anyone authoring a dataset config
---

# Dataset config

A *dataset config* is an optional YAML file that tells binoc which
plugins to run, in what order, and how the renderer should classify
the resulting tags. You do not need a config to run `binoc diff` —
the defaults handle the built-in comparators. A config becomes
useful when you want to:

- Restrict or reorder the comparator / transformer pipeline.
- Teach the Markdown renderer that a plugin-specific tag is clerical
  or substantive for your domain.
- Configure a renderer's behavior (HTML theme, CI failure rules, …)
  without changing code.

!!! info "Work in progress"
    Config key coverage is currently partial and will expand as
    renderer-specific config grows. If a key you need is missing here,
    check the sources referenced from each section or
    [file an issue](https://github.com/harvard-lil/binoc/issues).

## Top-level shape

```yaml
comparators:
  - binoc.zip
  - binoc.tar
  - binoc.directory
  - binoc.csv
  - binoc.text
  - binoc.binary

transformers:
  - binoc.correlation_detector
  - binoc.folder_move_detector
  - binoc.tabular_analyzer
  - binoc.column_reorder_detector

output:
  markdown:
    significance:
      clerical:
        - binoc.column-reorder
        - binoc.whitespace-change
      substantive:
        - binoc.column-addition
        - binoc.column-removal
        - binoc.row-addition
        - binoc.content-changed
```

Passing this file via `binoc diff A B --config dataset.yaml` (or
through `binoc.Config.from_file(path)` in Python) applies it to the
run.

## `comparators`

A list of comparator names, in the order they should be tried. The
**first comparator to claim an item pair wins** — dispatch is
URL-routing-style, not fall-through-with-voting. See
[Dispatch model](../explanation/dispatch-model.md) for the full
story.

Names are opaque strings. Built-in names are namespaced `binoc.*`;
third-party plugins use their own namespace (for example
`biobinoc.fasta`, `binoc-sqlite.sqlite`). The defaults, in their
default order, are shown in the snippet above. Archive comparators
come before the directory comparator so that `.zip` / `.tar`
extension matching happens before the extracted contents are walked
as a directory; CSV comes before text so `.csv` files get the
column-aware comparator; binary is the catch-all fallback.

You can shorten the list to restrict what formats are recognized
(useful in test vectors that exercise a single comparator) or add
third-party plugin names after installing them — no "enable" step
required beyond listing them here.

## `transformers`

A list of transformer names, in the order they should run.
Transformers rewrite the already-built IR tree; later transformers
see the output of earlier ones.

The default order is shown above. `binoc.correlation_detector` and
`binoc.folder_move_detector` run first so that per-file moves and
folder renames collapse before the tabular pipeline adds cell-level
details. `binoc.tabular_analyzer` reads `tabular_v1` artifacts and
attaches tags and summaries; `binoc.column_reorder_detector`
downgrades pure column reorders to `action: "reorder"` after the
analyzer has labeled them.

See [Artifacts and composition](../explanation/artifacts-and-composition.md)
for why the order matters and how to slot a third-party transformer
into a sensible position.

## `output.<renderer>`

Each renderer gets its own config section, keyed by the renderer's
short name. Unknown sections are ignored, and any renderer without a
section receives an empty object and applies its own defaults.

The Markdown renderer is the most interesting case today.

### `output.markdown.significance`

A map from category names (`clerical`, `substantive`, …) to lists of
tag names. The renderer looks up each tagged node in this map and
buckets the change under the corresponding heading in the changelog.

```yaml
output:
  markdown:
    significance:
      clerical:
        - binoc.column-reorder
        - binoc.whitespace-change
        - bio.header-change        # custom tag from a plugin
      substantive:
        - binoc.column-addition
        - binoc.row-addition
        - bio.sequence-change      # custom tag from a plugin
```

A node with multiple tags is classified by the highest-priority
match; anything unmapped falls under `Other Changes`. This is
intentionally a renderer concern, not an IR concern — a single
changeset can be rendered with different significance mappings for
different audiences. See
[Significance classification](../explanation/significance-classification.md)
and
[Renderer config ADR](../adr/2026-03-09-renderer_config.md) for the rationale.

### Other renderer config

The `output` block can hold config for any registered renderer. For
the shape of an HTML renderer config, a CI-check renderer config,
etc., consult the renderer's documentation (for third-party
renderers) or source (for `binoc-stdlib`). Each renderer deserializes
its own section.

## Where to go next

- [Diff two snapshots](../howto/diff-two-snapshots.md) — the default
  pipeline in action.
- [Install and use plugins](../howto/install-and-use-plugins.md) —
  adding third-party plugin names to the config.
- [Plugin discovery](plugin-discovery.md) — how plugin names become
  running code.
- [Renderer config ADR](../adr/2026-03-09-renderer_config.md) — the decision
  record for per-renderer sections.
