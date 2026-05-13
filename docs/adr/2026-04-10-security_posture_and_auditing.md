# Security posture and how to audit Binoc (core and plugins)

**Date:** 2026-04-10
**Status:** Accepted

## Context

Binoc is primarily a **local** tool: it reads two snapshot directories (or archives expanded under a session workspace), runs comparators and transformers, and writes changelogs or changesets. Security expectations differ from a network service: the main risks are **untrusted snapshot content**, **untrusted configuration or serialized IR**, **supply chain** (dependencies and CI), and **plugins** (Rust, Python, or native shared libraries), which run **in-process with the same privileges as the host process**.

We did not previously document what “auditing Binoc for security” should mean. This ADR records a practical threat model and audit scope so maintainers, security reviewers, and plugin authors share the same assumptions.

## Decision

### 1. Documented threat model

**In scope**

- **Confidentiality and integrity of the host machine** relative to the data the user already chose to diff: a malicious snapshot should not trivially read or write paths outside the intended snapshot trees and session workspace (see `LocalDataAccess::new_for_diff` and path policy in `binoc-sdk`).
- **Availability**: pathological inputs (huge archives, deeply nested JSON/YAML, decompression bombs) can exhaust CPU, memory, or disk; treating oversized or hostile inputs as a DoS risk is appropriate.
- **Supply chain**: Rust and Python dependencies, GitHub Actions (pinned vs floating actions, token permissions, publishing workflows), and lockfiles are part of the audit surface for this repository.
- **Plugin ecosystem**: third-party plugins should be easy to write safely. The harness can assist by validating plugin input and output (for directory traversal, for example), and designing the SDK to help plugins protect against malicious inputs.
- **Format handling risks**: Binoc shares the security risks of virus scanners, file converters, and other tools that offer format agility as a feature. Binoc and its ecosystem need to be careful up front and continuously about security of underlying format libraries.

**Explicit non-goals (unless product scope changes)**

- Plugins are not sandboxed. We do not currently protect against malicious plugins.
- The user is free to edit the config file and IR files; unlike snapshots those are trusted inputs. We can check for mistakes, but we do not guarantee impossibility of malicious edits.

### 2. Auditing **this repository** (core + stdlib + CLI + bindings)

A useful audit passes through these layers:

1. **Entry points** — CLI argument handling, Python extension surface, and any code that loads config or changesets from disk.
2. **Parsing and deserialization** — YAML dataset config, JSON changeset / ABI payloads, CSV and archive formats; note **untrusted** vs **tool-generated** inputs.
3. **Filesystem and archives** — directory walking, zip extraction (e.g. path sanitization), tar extraction (keep `tar` crate patched; treat archive handling as a recurring audit item), temp workspaces, artifact paths under `data_root/.artifacts/`.
4. **Controller session** — `LocalDataAccess` mode used for real diffs (`new_for_diff`): path policy must remain consistent with `register_local`, reads, and artifact access.
5. **Dependencies and automation** — `cargo audit` / advisory databases, Dependabot policy, minimum supported Rust versions, CI `permissions`, and publish workflows (OIDC, environment protections).

Reviewers should treat **stdlib comparators** as part of the default attack surface for users who do not add third-party plugins, because they ship with the product.

### 3. Auditing a **plugin** (third-party or in-repo)

Plugins are **trusted code** with respect to the process: a comparator, transformer, or renderer can invoke `DataAccess` however the SDK allows, call native code, or (for Python) run arbitrary interpreter logic. Security review of a plugin therefore emphasizes:

1. **Trust boundary** — assume snapshot-controlled data is hostile.
2. **I/O behavior** — Uses of `read_bytes`, `open_read`, `local_path`, `workspace`, `register_local`, `provide`, `publish_artifact` / `get_artifact`: avoid following attacker-controlled symlinks into sensitive locations unless that is intentional; avoid shelling out with unsanitized paths. Use SDK methods to access data if possible.
3. **Format readers** — Plugins often provide readers for new data formats. Prefer widely trusted, memory-safe libraries for format handling. If format handling requires large unsafe dependencies (e.g. ImageMagick) consider sandboxing.
4. **Downstream consumers** — Document what outputs depend on snapshot data and what cleaning is or isn't done on that data.

### 4. What we tell operators and plugin authors

- **Default stance:** Only install plugins and only diff snapshots from sources you trust at least as much as you trust running their code on your machine.
- **Hardening:** Run Binoc in a dedicated VM or container with minimal credentials if handling many data formats in untrusted snapshots or if plugins are untrusted.
