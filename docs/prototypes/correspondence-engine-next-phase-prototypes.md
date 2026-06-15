# Correspondence Engine Next-Phase Prototypes

**Date:** 2026-06-14
**Status:** Prototype notes, not implemented API

This records concrete shapes for the post-CFM-67 work in
`CORRESPONDENCE_ENGINE_TRACKER.md`. These are deliberately not ADR decisions;
they are testable sketches for CFM-68 through CFM-76.

## CFM-68: JSON Records

Prototype artifact: `binoc.record_collection_v1`.

```json
{
  "format": "binoc.record_collection_v1",
  "records": [
    {
      "id": {"id": "A123"},
      "path": "$[0]",
      "fields": {"id": "A123", "status": "active"}
    }
  ],
  "identity": {
    "kind": "field_key",
    "fields": ["id"],
    "source": "detected"
  },
  "source_format": "json"
}
```

Use `tabular_v1` only when the JSON record collection is rectangular and all
record fields are scalar enough that existing tabular extraction is honest.
Otherwise use the record artifact and a sibling writer that reuses edit verbs
where they are genuinely shared (`record.add`, `record.remove`,
`record.edit_field`), not tabular verbs.

## CFM-69: Parsed Children

Prototype child locator convention:

```text
<parent logical path>#<kind>/<stable logical name>
```

Examples:

- `data.csv#table/table_1`
- `data.sqlite#table/customers`
- `report.txt#section/introduction`

Rules that emit parsed children should:

- make child `ItemRef.logical_path` equal to the locator;
- set `ItemRef.handle` to a producer-scoped cache handle or source locator;
- attach the child-owned semantic artifact (`tabular_v1`, future
  `record_collection_v1`, etc.) to the child;
- leave a parent manifest artifact only for collection summaries and parent
  reconciliation.

Invariant tests to add before broad adoption:

- child logical paths are deterministic across runs;
- children can be pair-rule endpoints;
- parent residual edits do not duplicate child edits.

## CFM-70/71: SQLite And Container Reshape

SQLite should move from "one database node with only a collection artifact" to
"one database node plus parsed table children." The database node still carries
a manifest artifact, but table content changes belong to `data.sqlite#table/T`.

Container type changes should project from plugin-supplied facts:

```json
{
  "action": "container_representation_change",
  "from_item_type": "directory",
  "to_item_type": "sqlite_database",
  "member_links": ["customers", "orders"]
}
```

Core remains type-ignorant: it only sees open actions, item types, tags, and
links. Stdlib/plugins decide the wording and evidence.

## CFM-72/73: Split And Merge

Prototype link shape should not be forced into one-to-one `LinkProposal`.
Represent split/merge as a claim over several ordinary links:

```json
{
  "verb": "binoc.tabular_split",
  "scope": "table",
  "from": ["observations.csv"],
  "to": ["observations_2024.csv", "observations_2025.csv"],
  "evidence": {
    "partition_field": "year",
    "covered_rows": 18320,
    "residual_rows": 0
  }
}
```

Fuzzy one-to-one moves should be suppressed only when the split/merge evidence
strictly covers more content at lower description cost. Near misses should emit
a diagnostic, not a false split claim.

## CFM-74/75: Replayable Claims

Prototype claim payload:

```json
{
  "verb": "binoc.unit_conversion",
  "scope": {"node": "measurements.csv", "column": "length_cm"},
  "params": {"from_unit": "cm", "to_unit": "m", "scale": 0.01},
  "covered": {"cells": 4812},
  "residuals": [],
  "evidence": {"max_error": 0.0}
}
```

A claim may reduce local edit detail only when replay verification succeeds:
applying the transformation plus residual edits to the left values reproduces
the right values within the declared tolerance.
