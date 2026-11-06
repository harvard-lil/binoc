# CLI Exit Status, Config Errors, and Path Exclusion

**Date:** 2026-11-05
**Status:** Implemented

## Context

Binoc is run unattended in monitoring jobs: capture a dataset, diff it against
the previous capture, and act on the result. Through 0.2.0 three things made
that harder than it needed to be:

- `binoc diff` exited 0 whether or not anything changed, so a job had to parse
  the changeset JSON to answer "did it change?".
- Dataset config keys the release did not know were ignored without an error.
  When the config format moved from `dataset.tables` to `dataset.paths`, 0.2.0
  configs stopped applying row keys and the only trace was a changed diff.
  The `tables` case was later caught as an error diagnostic, but only inside
  the changeset, and other unknown keys were still ignored.
- There was no way to leave a file out of a comparison. Capture tools write a
  manifest inside each snapshot, which then shows as modified on every run.

[Error Diagnostics Are Reportable Findings](2026-06-03-error_diagnostics_are_reportable_findings.md)
leaves it to callers to decide whether error diagnostics fail a workflow. The
CLI is such a caller.

## Decision

**Exit status.** `binoc diff` exits 0 after a completed diff, as before. With
`--exit-code` it exits 1 when any compared pair changed (a changeset root
other than `identical`) and 0 otherwise, following `git diff --exit-code`.
Errors exit 2, both hard failures (`BinocError`, missing snapshot, usage) and
dataset-config errors. Before this change, hard failures through the Python
console script exited 1. Other error diagnostics (identity policy errors,
parse failures) do not change the exit status; they are counted on stderr and
remain findings in the changeset.

**Config errors are loud.** Any top-level `dataset` key outside the SDK's
`DatasetSemanticsV1::KNOWN_FIELDS` is an error diagnostic
(`binoc.dataset_config.unknown_field`). The nested SDK config structs deny
unknown fields, so a typo there makes the block unreadable
(`binoc.dataset_config.invalid`, now error severity instead of warning). The
comparison still runs and writes its outputs, in line with the reportable
findings ADR, but `binoc diff` prints each `binoc.dataset_config.*` error to
stderr and exits 2: the run did not use the config the user wrote.

**Path exclusion.** `dataset.exclude` takes globs with `dataset.paths[].match`
syntax; `--exclude GLOB` appends to it for one run. The stdlib directory, zip,
and tar expand rules skip matching children before registering or hashing
them, so excluded items never enter either side tree. Exclusion lives in the
stdlib expand rules, not core, because core has no notion of paths.

**Version.** `binoc --version` prints the package version, and `binoc diff`
records it in each changeset's metadata under `binoc.version`, so a saved
changeset says which release produced it.

## Alternatives Considered

- **Exit 1 on change by default.** Matches `diff(1)`, but would break every
  existing script that treats nonzero as failure. Opt-in keeps the default.
- **Abort the diff on config errors.** Simpler for the CLI, but contradicts the
  reportable-findings ADR for library callers and discards a comparison the
  user may still want to inspect. Writing outputs and exiting 2 gives both.
- **`deny_unknown_fields` on `DatasetSemanticsV1` itself.** Produces serde's
  generic message and discards the whole block for one stray key; the explicit
  known-field check names the key and keeps the rest of the config working.
  Plugins that want their own top-level `dataset` keys will need an extension
  point; none do today.
- **Exclusion recorded in the changeset.** Would make exclusions visible to
  later readers without the config. Deferred: the changeset has no config
  provenance today, and adding it is a larger schema change.
