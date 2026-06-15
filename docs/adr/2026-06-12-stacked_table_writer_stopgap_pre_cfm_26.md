# Stacked Table Writer Stopgap Before CFM-26

**Date:** 2026-06-12
**Status:** Implemented

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

## Decision

Bless `write_stacked_table_edits` as a temporary CFM-26 deletion bridge. It is
allowed to become the only stacked-table implementation when the legacy
`table_splitter` and `table_collection_analyzer` path is deleted, but only as
a documented stopgap.

The replacement remains Phase 3 work: a real stacked-table parse rule should
produce `tabular_collection_v1` artifacts and per-table artifacts, after which
the generic tabular collection writer can own the comparison.

The legacy `binoc.table_splitter.ambiguous` suggestion behavior is intentionally
not preserved by the stopgap. That lost diagnostic must be restored or
explicitly retired when the real parse-rule replacement lands.

## Alternatives Considered

**Block CFM-26 until a full stacked-table parse rule exists.** Rejected because
the current full-corpus correspondence gate is green and the remaining issue is
localized product vocabulary, not core engine correctness.

**Delete stacked-table support until Phase 3.** Rejected because existing
manifests assert stacked-table behavior, and deleting it would make CFM-26 a
functional regression rather than an architecture cleanup.
