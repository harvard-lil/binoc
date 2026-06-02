# Example Verbosity and Plugin-Supplied Details

**Date:** 2026-06-01
**Status:** Decided

## Context

Binoc's human-readable changelog has to sit between two bad extremes:

- "1 cell changed" is too terse for a steward trying to decide whether a
  dataset revision matters.
- Rendering every changed value, row, hunk, or schema element inline makes the
  changelog unreadable and can create huge output.

The existing IR already has `summary`, `tags`, and plugin-specific `details`.
The Markdown renderer uses those fields for one-line bullets and significance
classification. It does not have a stable way to ask, "show a few useful
examples for this change," without scraping plugin-private payloads or gaining
domain knowledge about CSV, SQLite, text, or future plugin formats.

This decision must preserve the existing architecture:

- Significance and prose layout remain renderer concerns.
- Comparators and transformers are the only phases that understand source
  formats or semantic domains.
- The controller remains type-ignorant.
- `binoc extract` remains the way to retrieve exhaustive changed content on
  demand.

## Decision

Binoc will support three standard renderer verbosity levels:

| Level | Meaning |
|---|---|
| `summary` | Render reportable nodes and their one-line summaries only. Do not render examples. |
| `examples` | Render summaries plus bounded, structured examples where nodes provide them. This is the default for human-readable renderers. |
| `full` | Render every structured detail block and captured example present in the changeset, subject to hard renderer safety caps. This does not re-open source data or replace `binoc extract`. |

Verbosity is renderer config, not transformer config:

```yaml
output:
  markdown:
    verbosity: examples
    max_examples_per_block: 3
    max_detail_blocks_per_node: 4
    max_value_chars: 160
    max_rendered_detail_bytes: 200000
```

Each renderer owns its config schema under `output.<renderer>`, following the
per-renderer config decision. Standard renderers should use the names above for
portable behavior, but third-party renderers may add renderer-specific controls.

Example content is not renderer config. Comparators and transformers may attach
structured detail blocks to `DiffNode`s while they still have domain knowledge
and access to source-derived artifacts. Renderers choose how much of those
blocks to display based on verbosity and caps.

### Changeset Shape

`DiffNode` will gain a first-class structured detail field separate from the
existing plugin-private `details` map. The shape is intentionally generic and
openly typed:

```rust
pub struct DiffNode {
    pub summary: Option<String>,
    pub tags: BTreeSet<String>,
    pub details: BTreeMap<String, serde_json::Value>, // plugin-private metrics
    pub detail_blocks: Vec<DetailBlock>,              // renderer-visible evidence
    // ...
}

pub struct DetailBlock {
    /// Stable within this node, for anchors and extract selection.
    pub id: String,

    /// Open, namespaced kind such as "binoc.tabular.cell_changes.v1".
    pub kind: String,

    /// Short renderer-facing label, not a sentence of prose.
    pub label: Option<String>,

    /// Total matching items if known, including omitted examples.
    pub total_count: Option<u64>,

    /// Examples captured for inline rendering.
    pub examples: Vec<DetailExample>,

    /// Optional named extract aspects that can return exhaustive content.
    pub extract: Vec<ExtractHint>,

    /// Whether the producer truncated capture before exhausting candidates.
    pub truncated: bool,
}

pub struct DetailExample {
    /// Structured locator, for example row/column, line range, key path,
    /// table name, or schema object. Renderers can display known keys and
    /// fall back to compact JSON for unknown keys.
    pub locator: BTreeMap<String, serde_json::Value>,

    /// Values before and after the change. Missing sides represent add/remove.
    pub before: Option<ValuePreview>,
    pub after: Option<ValuePreview>,

    /// Extra structured fields for domain-specific context.
    pub fields: BTreeMap<String, serde_json::Value>,
}

pub struct ValuePreview {
    pub value: serde_json::Value,
    pub media_type: Option<String>,
    pub truncated: bool,
}

pub struct ExtractHint {
    /// Aspect name accepted by `binoc extract`, such as "cell-changes",
    /// "rows-added", or "text-hunks".
    pub aspect: String,
    pub label: Option<String>,
}
```

The field is serialized into changeset JSON. It carries bounded evidence and
extract hooks, not full source data by default. It is distinct from artifacts,
which are transient session data, and from `details`, which remains available
for plugin-private metrics and machine checks.

Detail blocks must not contain pre-rendered prose paragraphs. They may contain
short labels and scalar values, but sentence construction, table layout,
ordering, and significance grouping are renderer responsibilities.

Example JSON for a tabular cell change:

```json
{
  "id": "cells_changed",
  "kind": "binoc.tabular.cell_changes.v1",
  "label": "Changed cells",
  "total_count": 127,
  "truncated": true,
  "examples": [
    {
      "locator": {"row": 42, "column": "status"},
      "before": {"value": "draft", "media_type": "text/plain", "truncated": false},
      "after": {"value": "published", "media_type": "text/plain", "truncated": false},
      "fields": {}
    }
  ],
  "extract": [{"aspect": "cell-changes", "label": "All changed cells"}]
}
```

### Plugin Participation Contract

Plugins participate by emitting data, not by owning renderer behavior.

Comparators and transformers may populate `detail_blocks` on nodes they create
or rewrite. The plugin that last changes a node is responsible for keeping the
node's summaries, tags, details, detail blocks, and extract aspects coherent.
This follows the existing "last toucher owns extraction" rule.

No new renderer callback is introduced. The contract is the data shape above
plus the existing `extract` methods:

```rust
pub trait Comparator {
    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<DiffNode>;

    // If a DetailBlock advertises an ExtractHint, the owning comparator or
    // last transformer must accept that aspect here.
    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult>;
}

pub trait Transformer {
    fn transform(
        &self,
        node: DiffNode,
        data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult;

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult>;
}
```

SDK helper methods may make this ergonomic, but there is no separate
`ExampleProvider` or `DetailProvider` plugin registry. A renderer can render
unknown `kind`s generically because the block structure is stable. A specialized
renderer may recognize known namespaced kinds for richer layout.

### Output Caps

There are two layers of caps:

1. **Capture caps** limit how much evidence a comparator or transformer records
   in the changeset. These keep saved changesets bounded. Standard plugins
   should capture a small, deterministic sample by default. Future
   comparator/transformer config can expose plugin-specific capture budgets,
   but those budgets must not live under `output.<renderer>`.
2. **Render caps** live in renderer config and limit how much of the captured
   detail a renderer displays in a particular output.

Renderers must always make truncation visible when it affects displayed detail:
for example, "showing 3 of 127 changed cells." If a block includes an extract
hint, Markdown should point to the aspect that returns the exhaustive data.

`full` means "render all captured structured detail" and is still bounded by
hard safety caps such as `max_rendered_detail_bytes`. If the changeset captured
only examples, `full` renders only those examples and directs the user to
`binoc extract` for exhaustive content.

### Interaction With `binoc extract`

Inline examples are evidence for scan-and-triage workflows. `binoc extract` is
the exhaustive retrieval path.

Detail blocks may advertise extract aspects. Renderers may display those aspects
as follow-up commands, links, or machine-readable references. The aspect string
is still resolved through the existing provenance chain: the comparator or last
transformer named on the node handles extraction.

This keeps saved changelogs compact while giving users a clear path from
"interesting example" to "all underlying changed data."

### Interaction With Diagnostics

Diagnostics are for process health: warnings, parse fallbacks, unavailable
artifacts, capture failures, and other conditions that affect trust in the run.
Detail blocks are for user-facing evidence about actual dataset changes.

Expected omission caused by caps belongs in the detail block (`total_count`,
`truncated`) and in renderer text such as "showing 3 of 127." Unexpected
problems capturing examples should be emitted through the diagnostics channel
and may also result in no detail block or a block with fewer examples. Renderers
should not treat diagnostics as examples, and plugins should not hide process
warnings inside `detail_blocks`.

## Consequences

- Human-readable renderers get a stable way to show useful examples without
  learning plugin-private schemas.
- Plugins can provide domain-specific evidence for generic renderers by filling
  a structured field.
- Saved changesets remain bounded because examples are captured samples, not
  arbitrary raw data dumps.
- `binoc extract` remains necessary and clearly separated from changelog
  rendering.
- The existing `details` map remains useful for metrics and plugin-private
  machine data, but renderers should prefer `detail_blocks` for inline examples.
- The implementation is a breaking changeset schema change, acceptable during
  greenfield development.

## Alternatives Considered

**Renderer scrapes `details`.** Rejected because `details` is plugin-private
and already contains ad hoc metrics. A renderer would either become domain-aware
or produce unstable output based on undocumented keys.

**Examples are transformer output only.** Rejected because comparators may be
the only component that understands a source format, and some changes never pass
through a semantic transformer. Both comparators and transformers need to be
able to attach detail blocks.

**Renderer calls plugins to generate examples at render time.** Rejected because
rendering should work from saved changeset JSON without source access. On-demand
source reconstruction is already the job of `binoc extract`.

**Store every changed value in the changeset and let renderers hide most of it.**
Rejected because it makes changeset size scale with the raw diff and creates
privacy/performance surprises. Capture must be bounded before serialization.

**Add a separate `DetailProvider` plugin trait.** Rejected because it adds
another dispatch axis and repeats information already known to the comparator or
transformer that created the node. The existing provenance and extract contract
is enough.
