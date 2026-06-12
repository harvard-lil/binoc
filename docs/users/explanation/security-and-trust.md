---
audience: core contributor, security reviewer, plugin author
---

# Security and trust

Binoc is primarily a **local** tool. It reads two snapshot directories (or
archives expanded under a session workspace), runs comparators and
transformers, and writes changelogs or changesets. The security posture
follows from that shape: the main risks are untrusted snapshot content,
the supply chain for Binoc itself, and plugins running in-process with
the host's privileges.

For the long-form record, see
[Security posture and auditing](../../adr/2026-04-10-security_posture_and_auditing.md).

## What Binoc trusts

| Input | Trust level | Why |
|---|---|---|
| Snapshot data (the two trees being diffed) | **Untrusted.** Treat as hostile. | A snapshot can be a download from anywhere — a public portal, a corporate partner, a research mirror. |
| Dataset config YAML | Trusted. | The user authored or curated it. Binoc checks for mistakes but does not defend against malicious edits. |
| Changeset JSON (as `extract` input) | Trusted. | The user chose to consume it. |
| Plugins (comparators, transformers, renderers) | **Trusted code, same as host.** | Plugins run in-process. A malicious plugin can do anything the host process can. Binoc does not sandbox plugins. |

The operator-facing guidance is simple: **install plugins and diff
snapshots only from sources you trust at least as much as you trust
running their code on your machine.** For hostile data-format ecosystems
or untrusted plugins, run binoc inside a dedicated VM or container with
minimal credentials.

## Threats in scope

- **Host integrity relative to the snapshots.** A malicious snapshot
  should not trivially read or write paths outside the snapshot trees
  and the session workspace. Path traversal in archives, symlink
  handling, and extraction targets are audit items.
- **Availability.** Pathological inputs — huge archives, deeply nested
  JSON/YAML, decompression bombs — can exhaust CPU, memory, or disk.
  Treating oversized or hostile inputs as a DoS risk is appropriate.
- **Supply chain.** Rust and Python dependencies, GitHub Actions
  (pinned vs. floating, token permissions, publishing workflows),
  lockfiles, and Dependabot policy are part of the audit surface.
- **Format handling.** Binoc shares the security risks of virus
  scanners and file converters: format agility is a feature and an
  attack surface. Format libraries need continuous review.

## Explicit non-goals

- **No plugin sandboxing.** A malicious plugin is out of scope for the
  current threat model. This may change if product scope grows (for
  example, WASM plugins were considered and rejected; see
  [Plugin discovery ADR](../../adr/2026-03-06-plugin_discovery.md)).
- **No defense against a user editing trusted inputs.** The config and
  saved changesets are the user's own artifacts.

## Auditing the core

A useful audit of this repository passes through these layers in
roughly this order:

1. **Entry points.** CLI argument handling, the Python extension
   surface, and any code that loads config or changesets from disk.
2. **Parsing and deserialization.** YAML dataset config, JSON changeset
   and ABI payloads, CSV and archive formats — distinguish untrusted
   from tool-generated inputs.
3. **Filesystem and archives.** Directory walking, zip and tar
   extraction (path sanitization, symlink policy), temp workspaces,
   artifact paths under `data_root/.artifacts/`.
4. **Controller session.** The path policy enforced by
   `LocalDataAccess::new_for_diff` must stay consistent with
   `register_local`, reads, and artifact access.
5. **Dependencies and automation.** `cargo audit`, Dependabot policy,
   minimum supported Rust version, CI `permissions`, and publish
   workflows (OIDC, environment protections).

Standard library comparators are part of the default attack surface:
every user who installs `binoc` runs them. Review them with the same
rigor as any third-party plugin.

## Auditing a plugin

Plugins are trusted code, but the code itself can be audited. A plugin
review focuses on:

- **Trust boundary.** Does the plugin treat snapshot-controlled data
  as hostile input to its format parser?
- **I/O behavior.** How does it use `read_bytes`, `open_read`,
  `local_path`, `workspace`, `register_local`, `provide`, and the
  artifact API? Does it avoid following attacker-controlled symlinks
  into sensitive locations unless that is intentional? Does it avoid
  shelling out with unsanitized paths?
- **Format readers.** Prefer widely trusted, memory-safe libraries. If
  the plugin pulls in a large unsafe dependency (ImageMagick, a native
  media stack) consider sandboxing the whole binoc run.
- **Downstream consumers.** What outputs depend on snapshot data, and
  what cleaning or escaping is (or isn't) done before rendering?

The SDK is designed to make safe behavior the easy path: prefer
`DataAccess` methods over `std::fs` in Rust plugins, prefer
binoc-provided path types over stringly-typed paths in Python plugins,
and treat the first few bytes of any file as untrusted.

## Where to go next

- [Security posture and auditing ADR](../../adr/2026-04-10-security_posture_and_auditing.md)
  — the long-form record of this posture.
- [Plugin model](../../plugin-developers/explanation/plugin-model.md) — how plugins are loaded and the
  trust implications of that loading model.
- [Plugin SDK and ABI ADR](../../adr/2026-03-12-plugin_sdk_and_abi.md) — the C ABI
  boundary between the host and native plugins.
