# Error Diagnostics Are Reportable Findings

**Date:** 2026-06-03
**Status:** Implemented

## Context

Dataset identity policies can encounter bad observed data: null row keys,
duplicate row keys, or ambiguous declared file correspondence keys. These
conditions are important enough that users may want automation to treat them as
errors.

At the same time, binoc's primary job is to compare whole snapshots. Aborting
the entire diff because one file has a duplicate key throws away useful results
from the rest of the dataset.

## Decision

`DiagnosticSeverity::Error` is a reportable finding in the completed changeset,
not a controller-level abort condition.

Identity policy `error` settings emit error-severity diagnostics and keep the
conservative matching behavior: unsafe ambiguous rows or files are left
unmatched, and the comparison continues. Callers can decide whether a completed
changeset containing error diagnostics should fail CI or another workflow.

Hard `BinocError` failures remain appropriate for failures that prevent the
comparison from being constructed at all, such as unreadable snapshots,
unresolvable comparator dispatch, or invalid API use.

## Alternatives Considered

The controller could stop on the first error diagnostic. This makes plugin
diagnostics too powerful: a local identity problem in one file prevents users
from seeing unrelated changes elsewhere in the snapshot.

Plugins could use warnings for all observed data problems and leave "error" to
the CLI. That loses the user's configured severity in library output and makes
embedded callers reimplement policy interpretation.
