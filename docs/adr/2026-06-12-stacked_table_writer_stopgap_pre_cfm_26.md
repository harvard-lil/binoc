# Retired Stacked Table Writer Stopgap

**Date:** 2026-06-12
**Status:** Retired by CFM-45

## Context

The correspondence-first engine replaces transformer tree surgery with parse
rules, edit-list writers, compaction rules, and projection. Legacy stacked CSV
handling uses `binoc.table_splitter` plus `binoc.table_collection_analyzer` to
turn table-like regions in one CSV into child table nodes.

During the CFM-25 port, `binoc.write.tabular` gained a bounded
`write_stacked_table_edits` heuristic. It recognizes clear stacked table
regions in a parsed `tabular_v1` artifact and emits `tabular_collection.*`
edits. This preserves the existing `csv-stacked-tables` manifest behavior
through the correspondence engine without adding a separate
`tabular_collection_v1` parse rule for CSV regions yet.

CFM-45 retired that heuristic. Stacked CSV detection now lives in
`binoc.parse.csv_stacked_tables`, which publishes a `tabular_collection_v1`
manifest for unambiguous CSV regions and creates synthetic child table nodes
with per-table `tabular_v1` artifacts. `binoc.write.tabular_collection` owns
the table-by-name comparison through the same SDK helper used by the SQLite
collection writer, while ordinary tabular writers own per-table content
details. Ambiguous stacked layouts again emit `binoc.table_splitter.ambiguous`
as a suggestion diagnostic and fall through to ordinary tabular comparison.

## Decision

Keep this ADR as a historical record of the short-lived bridge, but do not
preserve the bridge in code. The writer must not detect stacked table sections
from a `tabular_v1` artifact; section detection belongs to parse rules.

## Alternatives Considered

**Block CFM-26 until a full stacked-table parse rule exists.** Rejected because
the current full-corpus correspondence gate is green and the remaining issue is
localized product vocabulary, not core engine correctness.

**Delete stacked-table support until Phase 3.** Rejected because existing
manifests assert stacked-table behavior, and deleting it would make CFM-26 a
functional regression rather than an architecture cleanup.
