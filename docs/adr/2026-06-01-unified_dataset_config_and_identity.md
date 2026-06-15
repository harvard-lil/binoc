# Unified Dataset Config and Identity Policy

**Date:** 2026-06-01
**Status:** Accepted; implementation notes superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md)

Note: the Markdown-renderer grouping example below is superseded by
[2026-06-02-renderer_groups.md](2026-06-02-renderer_groups.md). Current config
uses `output.markdown.groups`, not `output.markdown.significance`.

Note: the dataset-semantics shape remains current, but implementation passages
that mention comparator-to-transformer orchestration or `pending_recompare` are
superseded by the correspondence engine. Declared file identity is now pair-rule
evidence over side items.

## Context

Several tabular features need the same concept but have been phrased as separate
requests:

- keyed row identity for tables, such as FDA product rows keyed by
  `["BLA Number", "Product Number"]`
- declared file correspondence across snapshots, where two different paths
  should be treated as the same logical file
- tabular parse options, especially delimiter selection
- different table keys and parse options within one multi-table dataset
- clear behavior when keys are null, duplicated, one-to-many, or many-to-one

Treating each feature as plugin-specific configuration would make real datasets
awkward to describe. A workbook, a SQLite database, and a directory full of CSVs
can all be "the same dataset" from the user's perspective. The configuration
surface should let users describe dataset semantics once, while plugins consume
the parts they understand.

At the same time, Binoc's core rules still apply:

- the controller must remain type-ignorant
- parse rules publish source data as artifacts
- pair, writer, compaction, and projection rules optimize the correspondence
  result before changeset projection
- significance remains a renderer concern
- configuration is passed into the run, not read from global state

## Decision

Binoc will add a unified, SDK-owned dataset semantics section to the dataset
config. It describes file identity, table identity, row identity, and parse
options in one place. Core may carry this config and pass it to plugins, but it
does not interpret paths, tables, delimiters, or keys.

The existing orchestration sections stay:

```yaml
comparators:
  - binoc.zip
  - binoc.tar
  - binoc.directory
  - binoc.csv
  - binoc.text
  - binoc.binary

transformers:
  - binoc.declared_correspondence
  - binoc.correlation_detector
  - binoc.fuzzy_correlation_detector
  - binoc.folder_move_detector
  - binoc.tabular_analyzer
  - binoc.column_reorder_detector

output:
  markdown:
    groups:
      - heading: "Substantive changes"
        tags: [binoc.schema-change, binoc.row-addition]
      - heading: "Clerical follow-up"
        tags: [binoc.column-reorder]
```

The new semantic section is separate from plugin order:

```yaml
dataset:
  files:
    correspondences:
      - name: fda-quarterly-csvs
        left:
          path_regex: '^raw/(?P<table>[^/]+)/(?P<year>[0-9]{4})\.csv$'
        right:
          path_regex: '^normalized/(?P<year>[0-9]{4})/(?P<table>[^/]+)\.csv$'
        key: '${table}:${year}'
        logical_path: 'tables/${table}-${year}.csv'
        cardinality: one-to-one
        on_null_key: diagnostic
        on_duplicate_key: diagnostic
        report_path_change: false

  tables:
    defaults:
      parse:
        header: true
        delimiter: ','
      row_identity:
        on_null_key: diagnostic
        on_duplicate_key: diagnostic

    entries:
      applications:
        match:
          logical_name: applications
        parse:
          delimiter: ','
        row_identity:
          columns: ['ApplNo']

      products:
        match:
          logical_name: products
        row_identity:
          columns: ['BLA Number', 'Product Number']

      adverse_events:
        match:
          source:
            path_regex: '^events/.*\.tsv$'
        parse:
          delimiter: '\t'
        row_identity:
          columns: ['case_id', 'event_seq']
```

### Type sketch

The SDK owns the schema and helper types. The exact Rust names can move during
implementation, but the shape is:

```rust
pub struct DatasetSemanticsV1 {
    pub files: FileIdentityConfig,
    pub tables: TableConfig,
}

pub struct FileIdentityConfig {
    pub correspondences: Vec<FileCorrespondenceRule>,
}

pub struct FileCorrespondenceRule {
    pub name: String,
    pub left: FileSelector,
    pub right: FileSelector,
    pub key: Template,
    pub logical_path: Option<Template>,
    pub cardinality: Cardinality,
    pub on_null_key: IdentityFailurePolicy,
    pub on_duplicate_key: IdentityFailurePolicy,
    pub report_path_change: bool,
}

pub struct TableConfig {
    pub defaults: TableDefaults,
    pub entries: BTreeMap<String, TableEntry>,
}

pub struct TableEntry {
    pub match_: TableSelector,
    pub parse: TabularParseOptions,
    pub row_identity: RowIdentity,
}

pub struct RowIdentity {
    pub columns: Vec<String>,
    pub cardinality: Cardinality,
    pub on_null_key: IdentityFailurePolicy,
    pub on_duplicate_key: IdentityFailurePolicy,
}

pub enum Cardinality {
    OneToOne,
}

pub enum IdentityFailurePolicy {
    Diagnostic,
    Error,
    Ignore,
}
```

`Cardinality` is intentionally narrow in v1. User language can name the hazard
as "one-to-many" or "many-to-one", but Binoc will not auto-match those shapes
until a concrete aggregate matching design exists.

Plugin-specific config remains available for plugin knobs that are not dataset
semantics. The current `transformer_config` pattern should be mirrored for
comparators as `comparator_config`; renderers keep using `output.<renderer>`.
The semantic `dataset` section is the preferred surface for keys, parse options,
and correspondence rules. The lower-level per-plugin sections remain escape
hatches, not the main UX.

### Comparators and transformers receive run config

Comparators need configuration for parse options, and transformers need the same
semantic config for keyed row analysis and declared file correspondence. The
follow-on implementation should pass a run-config view to plugins, analogous to
the current renderer and transformer config values:

```rust
pub struct PluginRunConfig<'a> {
    /// The comparator_config / transformer_config / output value selected for
    /// this plugin, depending on plugin kind.
    pub plugin: &'a serde_json::Value,

    /// The top-level dataset semantics value, passed through unchanged by core.
    pub dataset: &'a serde_json::Value,
}
```

Core only selects the plugin's own config value and passes the dataset semantics
value through. It does not deserialize or act on table/file fields. SDK/stdlib
helpers deserialize `dataset` into `DatasetSemanticsV1` for plugins that opt in.

This is a trait/config plumbing change, not a comparator-to-transformer pipeline
rework. The existing pipeline remains:

1. comparators build the initial tree and publish artifacts
2. root-scope correlation transformers rewrite or inflate the tree
3. data-shape transformers analyze artifacts
4. renderers classify and format tags

### File correspondence is an explicit correlation pass

Declared file correspondence belongs before heuristic move/copy detection. It is
implemented as a root-scope transformer, tentatively
`binoc.declared_correspondence`, ordered before `binoc.correlation_detector` and
`binoc.fuzzy_correlation_detector`.

The transformer:

1. walks residual add/remove leaves
2. applies user-declared correspondence rules
3. builds a key index from the left and right selectors
4. pairs only unambiguous one-to-one matches
5. creates a node at the declared `logical_path` or the right path
6. sets `pending_recompare` so the normal comparator pipeline parses the pair

This uses the existing transformer-initiated re-dispatch mechanism from the
rename-and-modify ADR. No new controller phase is required.

Declared correspondence is not a replacement for the move detectors:

- declared correspondence is user-supplied identity
- exact/fuzzy correlation is inferred content similarity
- `folder_move_detector` is a reporting rollup over already-detected file moves

Declared matches are removed from the residual add/remove pool before heuristic
detectors run, so the two layers do not double count. If
`report_path_change: false`, a path difference is treated as snapshot layout and
is not rendered as a move. If `report_path_change: true`, the node gets a factual
path-change tag and source/destination details; renderers may surface that
without treating it as a heuristic `binoc.move`.

### Row identity is table-local

Row keys live on table entries, not globally. A dataset can have different keys
per logical table:

```yaml
dataset:
  tables:
    entries:
      applications:
        row_identity:
          columns: ['ApplNo']
      products:
        row_identity:
          columns: ['BLA Number', 'Product Number']
```

The tabular analyzer resolves the current table identity from the table
collection artifact or from the single-table source location. It then applies
the matching table entry and compares rows by key instead of position.

No configured key means the current positional behavior is retained. That is
important for simple CSVs without stable IDs.

### Null, duplicate, and many-to-many policy

File keys and row keys use the same identity-index rule:

| Left count | Right count | Result |
|---|---|---|
| 1 | 1 | match |
| 1 | 0 | removal |
| 0 | 1 | addition |
| 0 | N | additions, unless null/duplicate policy says otherwise |
| N | 0 | removals, unless null/duplicate policy says otherwise |
| 1 | N | ambiguous one-to-many |
| N | 1 | ambiguous many-to-one |
| N | M | ambiguous many-to-many |

`N` means more than one item for the same normalized key. The default
`diagnostic` policy does not guess. It emits an identity diagnostic tag and
details, leaves the ambiguous members unmatched, and lets normal add/remove
reporting continue. `error` fails the run with a config/data quality error.
`ignore` suppresses the diagnostic and treats the members as if no key existed
for them.

Standard tags:

- `binoc.identity-diagnostic`
- `binoc.null-key`
- `binoc.duplicate-key`
- `binoc.ambiguous-key`
- `binoc.file-correspondence-ambiguous`
- `binoc.row-identity-ambiguous`

The renderer may group these as warnings or review-first changes through normal
significance config. The IR still carries factual tags only.

## Consequences

- Users get one place to describe dataset semantics, even when those semantics
  affect multiple plugins.
- CSV delimiter and other parse options now have a proper config path; the CSV
  comparator will need comparator config plumbing.
- Keyed row diffs and file correspondence share the same conservative identity
  failure policy.
- Declared correspondence runs before heuristic move/copy detectors but does
  not replace them.
- The controller remains type-ignorant: it passes JSON config through and
  handles `pending_recompare`, but it does not understand tables or paths.
- Follow-on implementation can proceed without a new pipeline architecture.

## Alternatives Considered

**Put everything in per-plugin config.** This fits the current
`transformer_config` model but creates a scattered UX: CSV delimiter in one
place, row keys in another, file correspondence in a third. It also makes
multi-table datasets awkward because the same logical table metadata would need
to be repeated for each plugin that cares.

**Teach the controller about file correspondence before directory comparison.**
This would pair files earlier, but it makes the controller understand paths and
dataset identity. A root-scope declared-correspondence transformer preserves the
existing type-ignorant controller and reuses `pending_recompare`.

**Treat declared correspondence as a generalization that replaces move
detectors.** Rejected because declared identity and inferred similarity answer
different questions. A user declaration can say two changing paths are the same
logical file even when their contents differ. A heuristic detector can still
find moves that users did not configure.

**Auto-match duplicate keys greedily.** Rejected for both rows and files.
Greedy pairing can fabricate precise-looking cell or file changes from
ambiguous identity. Binoc should report the ambiguity and require either better
keys or a future explicit aggregate matching mode.

**Promote key failures directly to significance levels.** Rejected for the same
reason significance is not in the IR. Key failures are factual tags; renderers
decide whether they are warnings, clerical issues, or substantive review items.
