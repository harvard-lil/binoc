---
audience: anyone choosing where to begin
---

# Start here

Pick the path closest to what you are trying to do.

## Users

### Data steward / archivist

You want to diff dataset snapshots, understand what changed, and produce a
usable changelog.

- Start with [Diff two snapshots](howto/diff-two-snapshots.md).
- If you need files you can keep or share, continue to
  [Save and render changesets](howto/save-and-render-changesets.md).
- If your dataset includes formats binoc does not understand yet, see
  [Install and use plugins](howto/install-and-use-plugins.md) and
  [Third-party plugins](reference/third-party-plugins.md).
- If you want the framing before the mechanics, read
  [Why binoc exists](explanation/why-binoc-exists.md).

### Pipeline integrator

You care about contracts, automation, and stable machine-readable output.

- Start with [Changeset JSON schema](reference/changeset-schema.md).
- Read [Dataset config](reference/dataset-config.md) for the YAML surface.
- Use [Save and render changesets](howto/save-and-render-changesets.md) for
  output routing and [Extract changed data](howto/extract-changed-data.md)
  for extracting the underlying changed records.
- Read [Significance classification](explanation/significance-classification.md)
  if you need to understand how semantic tags become user-facing changelog
  sections.

## Plugin developers

### Python plugin author

You want to extend binoc with a comparator, transformer, or renderer in Python.

- Start with [Plugin model](explanation/plugin-model.md).
- Then follow [Write a Python comparator](howto/write-a-python-comparator.md),
  [Write a Python transformer](howto/write-a-python-transformer.md), or
  [Write a Python renderer](howto/write-a-python-renderer.md).
- Keep [Python API](reference/python.md) and
  [Plugin discovery](reference/plugin-discovery.md) open while implementing.
- Before publishing, read [Test a plugin with vectors](howto/test-a-plugin-with-vectors.md)
  and [Publish a plugin](howto/publish-a-plugin.md).

### Rust plugin author

You want the same extension points, but implemented in Rust.

- Start with [Plugin model](explanation/plugin-model.md).
- Then follow [Write a Rust comparator](howto/write-a-rust-comparator.md) or
  [Write a Rust transformer](howto/write-a-rust-transformer.md).
- Keep [Rust SDK](reference/sdk.md) and
  [Plugin discovery](reference/plugin-discovery.md) open while implementing.
- Before publishing, read [Test a plugin with vectors](howto/test-a-plugin-with-vectors.md)
  and [Publish a plugin](howto/publish-a-plugin.md).

## Core developers

### Core contributor

You are changing binoc itself rather than just using or extending it.

- Start with [Contribute to binoc](howto/contribute-to-binoc.md).
- Read [Architecture overview](explanation/architecture.md) next.
- Use [Test vectors](explanation/test-vectors.md) and
  [Security and trust](explanation/security-and-trust.md) when touching those areas.
- Treat the [ADR index](adr/README.md) as the long-form record of design
  decisions and rejected alternatives.

For contributor rules that live in the repository root rather than the docs
site, see [AGENTS.md](https://github.com/harvard-lil/binoc/blob/main/AGENTS.md).

### Release manager

You are preparing a release rather than doing feature work.

- Start with [Cut a release](howto/cut-a-release.md).
- Keep [CLI](reference/cli.md), [Python API](reference/python.md), and
  [Rust SDK](reference/sdk.md) nearby if you need to sanity-check a public surface.
- Use the [ADR index](adr/README.md) when a packaging or compatibility question
  turns out to be a design question.
