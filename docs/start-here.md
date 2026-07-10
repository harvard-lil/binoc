---
audience: anyone choosing where to begin
---

# Start here

Pick the path closest to what you are trying to do.

## Users

### Data steward / archivist

You want to diff dataset snapshots, understand what changed, and produce a
usable changelog.

- Start with [Diff two snapshots](users/howto/diff-two-snapshots.md).
- If you need files you can keep or share, continue to
  [Save and render changesets](users/howto/save-and-render-changesets.md).
- If your dataset includes formats binoc does not understand yet, see
  [Install and use plugins](users/howto/install-and-use-plugins.md) and
  [Plugin catalog](users/reference/third-party-plugins.md).
- If you want the framing before the mechanics, read
  [Why binoc exists](users/explanation/why-binoc-exists.md).

### Pipeline integrator

You care about contracts, automation, and stable machine-readable output.

- Start with [Changeset JSON schema](users/reference/changeset-schema.md).
- Read [Dataset config](users/reference/dataset-config.md) for the YAML surface.
- Use [Save and render changesets](users/howto/save-and-render-changesets.md) for
  output routing and [Extract changed data](users/howto/extract-changed-data.md)
  for extracting the underlying changed records.
- Read [Significance classification](users/explanation/significance-classification.md)
  if you need to understand how semantic tags become user-facing changelog
  sections.

## Plugin developers

### Python plugin author

You want to extend binoc from Python.

- Start with [Plugin model](plugin-developers/explanation/plugin-model.md).
- Then follow [Write a Python renderer](plugin-developers/howto/write-a-python-renderer.md)
  for stable renderer work. Python rule authoring for the correspondence
  engine is deferred until the stable ABI tier lands.
- Keep [Python API](plugin-developers/reference/python.md) and
  [Plugin discovery](plugin-developers/reference/plugin-discovery.md) open while implementing.
- Before publishing, read [Test a plugin with vectors](plugin-developers/howto/test-a-plugin-with-vectors.md)
  and [Publish a plugin](plugin-developers/howto/publish-a-plugin.md).

### Rust plugin author

You want correspondence rule packs or renderers implemented in Rust.

- Start with [Plugin model](plugin-developers/explanation/plugin-model.md).
- Then read [Dispatch model](plugin-developers/explanation/dispatch-model.md)
  and [Write a Rust rule pack](plugin-developers/howto/write-a-rust-rule-pack.md).
- Keep [Rust SDK](plugin-developers/reference/sdk.md) and
  [Plugin discovery](plugin-developers/reference/plugin-discovery.md) open while implementing.
- Before publishing, read [Test a plugin with vectors](plugin-developers/howto/test-a-plugin-with-vectors.md)
  and [Publish a plugin](plugin-developers/howto/publish-a-plugin.md).

## Core developers

### Core contributor

You are changing binoc itself rather than just using or extending it.

- Start with [Contribute to binoc](core-developers/howto/contribute-to-binoc.md).
- Read [Architecture overview](plugin-developers/explanation/architecture.md) next.
- Use [Test vectors](plugin-developers/explanation/test-vectors.md) and
  [Security and trust](users/explanation/security-and-trust.md) when touching those areas.
- Treat the [ADR index](adr/README.md) as the long-form record of design
  decisions and rejected alternatives.

For contributor rules that live in the repository root rather than the docs
site, see [AGENTS.md](https://github.com/harvard-lil/binoc/blob/main/AGENTS.md).

### Research

Background analysis and prior-art surveys that inform binoc's design without
being normative documentation. This is where research notes are collected.

- [Prior art and architecture precedents](core-developers/research/precedents.md) — which
  comparable tools exist (the build-vs-buy evidence) and which systems have
  the most to teach binoc's architecture.

### Release manager

You are preparing a release rather than doing feature work.

- Start with [Cut a release](core-developers/howto/cut-a-release.md).
- Keep [CLI](users/reference/cli.md), [Python API](plugin-developers/reference/python.md), and
  [Rust SDK](plugin-developers/reference/sdk.md) nearby if you need to sanity-check a public surface.
- Use the [ADR index](adr/README.md) when a packaging or compatibility question
  turns out to be a design question.
