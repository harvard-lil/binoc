---
audience: anyone authoring a dataset config
---

# Dataset config

A *dataset config* is an optional YAML file that tells binoc which
plugins to run, in what order, and how a renderer should present
the resulting changes if you want grouped output. You do not need a config to run `binoc diff` —
the defaults handle the built-in comparators. A config becomes
useful when you want to:

- Restrict or reorder the comparator / transformer pipeline.
- Declare dataset semantics, such as logical file correspondence and
  row identity, for plugins that understand those fields.
- Teach the Markdown renderer how to group plugin-specific tags for
  your domain.
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
  - binoc.gzip
  - binoc.directory
  - binoc.csv
  - binoc.text
  - binoc.binary

transformers:
  - binoc.correlation_detector
  - binoc.folder_move_detector
  - binoc.tabular_analyzer
  - binoc.column_reorder_detector

dataset:
  files:
    correspondences:
      - name: quarterly-csvs
        left:
          path_regex: '^raw/(?P<table>[^/]+)/(?P<year>[0-9]{4})\.csv$'
        right:
          path_regex: '^normalized/(?P<year>[0-9]{4})/(?P<table>[^/]+)\.csv$'
        key: '${table}:${year}'
        logical_path: 'tables/${table}-${year}.csv'
        cardinality: one-to-one
        on_null_key: diagnostic
        on_duplicate_key: diagnostic

  tables:
    defaults:
      parse:
        header: true
        delimiter: ','
      row_identity:
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
    entries:
      products:
        match:
          logical_name: products
        row_identity:
          columns: ['BLA Number', 'Product Number']

output:
  markdown:
    groups:
      - heading: "Substantive changes"
        tags:
          - binoc.column-addition
          - binoc.column-removal
          - binoc.row-addition
          - binoc.content-changed
      - heading: "Clerical changes"
        tags:
          - binoc.column-reorder
          - binoc.whitespace-change
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
come before the directory comparator so that `.zip`, `.tar`, and
single-stream `.gz` extension matching happens before the extracted
contents are walked as ordinary files or directories; CSV comes before
text so `.csv` files get the column-aware comparator; binary is the
catch-all fallback.

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

Per-transformer knobs live under `transformer_config`, keyed by
transformer name. For example, `binoc.folder_move_detector` accepts a
`threshold` float (default `0.8`; set `1.0` for strict all-leaves
rollup-only behavior):

```yaml
transformer_config:
  binoc.folder_move_detector:
    threshold: 0.8
```

## `dataset`

The `dataset` block is a top-level semantic description of the dataset being
compared. Core carries this value through unchanged and exposes it to plugins
under the `dataset` key in their run config; core does not interpret paths,
tables, delimiters, or keys.

The SDK owns a shared v1 shape so independently authored plugins can agree on
common dataset semantics:

- `dataset.files.correspondences` declares that files with different snapshot
  paths are the same logical file.
- `dataset.tables.defaults` declares table-wide defaults, such as parse options
  and row identity failure policy.
- `dataset.tables.entries` declares per-table selectors, parse options, and row
  identity keys.

```yaml
dataset:
  files:
    correspondences:
      - name: state-records
        left:
          path_regex: '^data/state_(?P<state>[A-Z]{2})\.csv$'
        right:
          path_regex: '^by-state/(?P<state>[A-Z]{2})/records\.csv$'
        key: '${state}'
        logical_path: 'states/${state}.csv'
        cardinality: one-to-one
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
        report_path_change: false

  tables:
    defaults:
      row_identity:
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
    entries:
      products:
        match:
          logical_name: products
        parse:
          header: true
          delimiter: ','
        row_identity:
          columns: ['BLA Number', 'Product Number']
```

`cardinality` is currently `one-to-one`. `on_null_key` and
`on_duplicate_key` accept `diagnostic`, `error`, or `ignore`; plugins decide how
to apply those policies for the semantics they implement.

## `output.<renderer>`

Each renderer gets its own config section, keyed by the renderer's
short name. Unknown sections are ignored, and any renderer without a
section receives an empty object and applies its own defaults.

The Markdown renderer is the most interesting case today.

### `output.markdown.verbosity`

Controls how much renderer-visible evidence the Markdown changelog shows:

- `summary` renders only the main one-line bullet for each reportable node.
- `examples` renders the summary plus bounded inline examples from any
  `detail_blocks` attached to the node. This is the default.
- `full` renders all captured detail blocks and examples from the changeset,
  still subject to the renderer's hard safety budget.

The renderer never reopens source data. If a node advertises an extract aspect,
the changelog points you at `binoc extract` for the exhaustive content.

### `output.markdown.max_examples_per_block`

Only used at `verbosity: examples`. Caps how many examples the renderer shows
from each structured detail block before it switches to a "showing N of M"
message and an extract hint.

### `output.markdown.max_detail_blocks_per_node`

Only used at `verbosity: examples`. Caps how many structured detail blocks the
renderer shows under a single changelog bullet.

### `output.markdown.max_value_chars`

Caps how many characters of a single example value the renderer prints inline
before truncating it with `...`.

### `output.markdown.max_rendered_detail_bytes`

Hard safety budget for all rendered detail lines across the whole Markdown
output. When the renderer hits this budget it stops printing further inline
detail and leaves the summary bullets intact.

### `output.markdown.groups`

An ordered list of group definitions. Each group has a literal `heading`
string and a `tags` list. The renderer looks up each tagged node against this
list and places the change under the first matching heading.

```yaml
output:
  markdown:
    groups:
      - heading: "Review first"
        tags:
          - bio.cross-contamination
      - heading: "Substantive changes"
        tags:
          - binoc.column-addition
          - binoc.row-addition
          - bio.sequence-change      # custom tag from a plugin
      - heading: "Clerical changes"
        tags:
          - binoc.column-reorder
          - binoc.whitespace-change
          - bio.header-change        # custom tag from a plugin
```

A node with multiple tags goes to the first matching group; declared order is
both display order and priority order. Anything unmapped falls under
`Other Changes`, but only when at least one group is configured. If `groups`
is omitted or empty, the default Markdown output is a flat factual list with
no section headings. This is
intentionally a renderer concern, not an IR concern — a single
changeset can be rendered with different grouping policies for
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
