---
audience: anyone authoring a dataset config
---

# Dataset config

A *dataset config* is an optional YAML file that tells binoc dataset
semantics and how a renderer should present the resulting changes if
you want grouped output. You do not need a config to run `binoc diff` —
the defaults handle built-in formats through the correspondence engine.
A config becomes useful when you want to:

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
dataset:
  files:
    correspondences:
      - name: running-list
        left:
          path_regex: '^(?P<list>running_list)_as_of_[0-9]{4}\.csv$'
        right:
          path_regex: '^(?P<list>running_list)_as_of_[0-9]{4}\.csv$'
        key: '${list}'
        logical_path: '${list}.csv'
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
  tables:
    - logical_name: products
      columns: ['BLA Number', 'Product Number']
  correspondence:
    expand_renamed_unchanged_collections: true

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

## Rejected pipeline keys

The CLI no longer uses `comparators`, `transformers`, or
`transformer_config` to build the diff pipeline. Those keys are rejected if
they appear in config files; correspondence rules now own expansion, parsing,
pairing, edit writing, compaction, and projection.

## `dataset`

The `dataset` block is a top-level semantic description of the dataset being
compared. Core carries this value through unchanged and exposes it to plugins
under the `dataset` key in their run config; core does not interpret paths,
tables, delimiters, or keys.

The SDK owns a shared v1 shape so independently authored plugins can agree on
common dataset semantics:

- `dataset.files.correspondences` declares that files with different snapshot
  paths are the same logical file.
- `dataset.tables` declares table row keys. It can be a list for the common
  case, or an object with `defaults` and `entries` when shared policy is useful.
- `dataset.correspondence.expand_renamed_unchanged_collections` controls a
  correspondence-engine performance tradeoff for renamed unchanged containers.

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
    - logical_name: products
      columns: ['BLA Number', 'Product Number']
    - path: data.csv
      columns: ['id']

  correspondence:
    expand_renamed_unchanged_collections: true
```

`cardinality` is currently `one-to-one`. `on_null_key` and
`on_duplicate_key` accept `diagnostic`, `error`, or `ignore`; plugins decide how
to apply those policies for the semantics they implement.

### `dataset.files.correspondences`

Declared correspondence rules tell binoc that an unmatched removed file and an
unmatched added file are the same logical file even though their paths differ.
When a rule produces one unambiguous left match and one unambiguous right match
for the same key, binoc links those side items in the correspondence engine and
lets normal writers explain the linked content. If `report_path_change` is
false and the content is identical, the projected node is pruned like any other
identical change. If `report_path_change` is true, binoc keeps a move-style path
change node even when the content is identical.

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
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
        report_path_change: false
```

`path_regex` uses named capture groups. `key` and `logical_path` are templates
that substitute captures as `${name}`. If `logical_path` is omitted, the right
path is used. Null keys, duplicate keys, one-to-many, and many-to-one matches
are skipped by default with warning diagnostics; use `error` to emit an
error-severity diagnostic or `ignore` to silence those diagnostics. Error
diagnostics do not stop the snapshot comparison.

### `dataset.correspondence`

The correspondence engine defaults
`expand_renamed_unchanged_collections: true`. This is the correct-by-default
setting: if a folder or archive is renamed but its own contents are unchanged,
binoc still expands inside it so it can detect facts such as a file copied out
of that renamed collection.

Set it to `false` for the faster short-circuit posture. In that mode, renamed
unchanged collections can be settled without looking beneath them, so copy or
move provenance involving their children may be reported less specifically.

### `dataset.tables`

Tabular row identity is optional. Without it, `binoc.tabular_analyzer` keeps
the positional fallback: row additions/removals are counted by row count
differences, and changed cells are compared at the same row offset. With
`columns`, the analyzer builds a table-local key and matches rows by that key
before reporting row additions, removals, and modified cells.

```yaml
dataset:
  tables:
    - path: data.csv
      columns: ['id']
    - logical_name: products
      columns: ['BLA Number', 'Product Number']
    - path: workbook.xlsx
      logical_name: Products
      columns: ['id']
```

Use `logical_name` for logical table children produced by multi-table parsers
or collection parse rules. Use `path` or `path_regex` for ordinary
single-file CSVs. When an entry includes both a source selector and
`logical_name`, both must match; for example, the `workbook.xlsx` entry above
targets the `Products` logical table inside that source item.

When several entries should share the same identity failure policy, expand
`tables` to an object with `defaults` and `entries`:

```yaml
dataset:
  tables:
    defaults:
      row_identity:
        on_null_key: diagnostic
        on_duplicate_key: error
    entries:
      - logical_name: applications
        columns: ['ApplNo']
      - path_regex: '^data/products\.csv$'
        columns: ['BLA Number', 'Product Number']
```

For unusual cases, entries also accept explicit `match` and `row_identity`
blocks, but the flat selector and `columns` form above is preferred.

Rows with blank key components are null-key rows. Keys that appear more
than once on either side are duplicate-key rows. The default
`diagnostic` policy emits warnings and leaves those rows unmatched so
normal add/remove counts still reflect them. `error` emits an error-severity
diagnostic without stopping the comparison. `ignore` suppresses the diagnostic
while keeping the same conservative matching behavior.

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
[Renderer config ADR](../../adr/2026-03-09-renderer_config.md) for the rationale.

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
- [Plugin discovery](../../plugin-developers/reference/plugin-discovery.md) — how plugin names become
  running code.
- [Renderer config ADR](../../adr/2026-03-09-renderer_config.md) — the decision
  record for per-renderer sections.
