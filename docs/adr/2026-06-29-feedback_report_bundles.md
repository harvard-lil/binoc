# Feedback Report Bundles for Imperfect Results

**Date:** 2026-06-29
**Status:** Implemented

## Context

Binoc is still in beta. When a result is wrong, the most useful bug report is a
reproducible test case: both snapshots, the config that shaped dispatch and
rendering, the produced changeset/changelog, and enough version metadata to run
the same comparison again. We already have one important substrate:
`binoc diff --trace` captures a full serialized correspondence run, and
`binoc replay` can render it. The feedback feature should lean on that instead
of inventing a second debug format.

The hard part is not serialization; it is the operational policy around user
data:

- Snapshots may be multi-GB directories or archives.
- Snapshots may contain sensitive data.
- A renderer-level “was this right?” nudge could increase reporting, but Binoc’s
  default posture is quiet-by-default, and changelog output should not turn into
  solicitation copy.

## Decision

Add a local-only `binoc report` subcommand that prepares a bug-report bundle
directory. The command:

- reruns a two-snapshot diff with trace capture;
- writes `changeset.json`, `changelog.md`, `run.trace.json`,
  `dataset-config.yaml`, `metadata.json`, and a bundle `README.md`;
- copies the snapshot payload into `snapshots/` by default, making the bundle
  self-contained;
- refuses by default when the copied snapshot payload would exceed 512 MiB,
  unless the caller raises `--max-snapshot-bytes` or switches to
  `--snapshot-mode reference`.

The bundle is a filesystem artifact, not an issue submission flow. `binoc
report` never uploads data, opens a browser, or mutates rendered output. Users
decide whether to inspect, redact externally, compress, or share the bundle.

The bundle’s reproducibility substrate is the existing run trace. The report
command adds packaging and metadata around that trace; it does not define a
parallel replay format.

## Privacy, Size, and UX Stance

### Exact copies, not automatic redaction

The first implementation copies exact snapshot bytes. It does not attempt
automatic redaction or down-sampling. A bug-report bundle is supposed to be a
repro case; silent mutation of inputs would make the report easier to share but
less trustworthy for debugging. If a user cannot share the exact bundle, the
fallback is `--snapshot-mode reference`, which still preserves config, output,
trace, and source paths without copying the data.

### Local directory, not issue-template integration

The first destination is a normal directory on disk. This is the lowest-risk
surface:

- it works offline;
- it avoids locking Binoc to any issue tracker;
- it keeps the privacy boundary explicit;
- it composes with whatever sharing workflow the project adopts later.

If the project later wants a GitHub issue template or an uploader, that should
consume the bundle artifact rather than replace it.

### No in-output nudge

Do not add a “was this right?” message to changelog output for now. The output
surface is for the dataset delta itself. A feedback prompt there would be
intrusive in normal use, especially when Markdown output is piped into files,
docs, or downstream publishing flows. Users who want to report a problem can
invoke `binoc report` explicitly.

## Alternatives Considered

### Trace-only export as the primary mechanism

Rejected as the main path. `--trace` is the right substrate, but trace alone is
not enough for a user-friendly bug report: it does not package the config,
version metadata, or the actual snapshot payload, and it requires the reporter
to know which extra files matter.

### Automatic redaction or down-sampling in the first version

Rejected. The tool cannot know which bytes are safe to remove without format-
specific policy, and the result would stop being a faithful repro. Redaction may
be added later as an explicit, format-aware transformation pipeline, but it is
not a safe default for the initial feature.

### Issue-template or network submission flow

Rejected for the first step. It crosses the privacy boundary too early and
forces transport/product decisions before the local reproducibility artifact is
settled.

### Feedback nudge embedded in renderer output

Rejected. It conflicts with the quiet-by-default CLI posture and would pollute
routine changelog output with product messaging.

## Consequences

- Users get one command that packages the context needed for a high-quality bug
  report.
- The default bundle is reproducible for small and medium cases, because it
  copies snapshots exactly.
- Large or sensitive datasets still require judgment from the user. The command
  makes that explicit through the size cap and the `reference` mode rather than
  pretending the trade-off does not exist.
- Future work can add compression, checksum manifests, richer build metadata, or
  issue-tracker integration without changing the core bundle shape.
