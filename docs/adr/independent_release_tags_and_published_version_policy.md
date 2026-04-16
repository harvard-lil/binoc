# Independent release tags and published version policy

**Date:** 2026-04-10
**Status:** Implemented

## Context

Binoc currently publishes three public artifacts from one repository:

- `binoc` on PyPI
- `binoc-sqlite` on PyPI
- `binoc-sdk` on crates.io

The first release workflow used a single `vX.Y.Z` tag and required all published manifests to share one version number. That made the release mechanics simple, but it coupled unrelated deploys:

- a failed publish for one package could not be retried by retagging just that package
- maintainers had to think in terms of one repo-wide release even when only one published artifact changed
- version numbers implied lockstep across packages with different audiences and compatibility boundaries

This coupling is especially misleading for native plugins. Their compatibility boundary is the Rust SDK protocol version, not the `binoc` Python package minor version.

## Decision

Binoc now uses package-specific release tags:

- `binoc-vX.Y.Z`
- `binoc-sqlite-vX.Y.Z`
- `binoc-sdk-vX.Y.Z`

The publish workflow triggers jobs only for the package named by the tag.

The `just` release helpers are package-scoped:

- `just set-version binoc <version>`
- `just set-version binoc-sqlite <version>`
- `just set-version binoc-sdk <version>`
- `just release binoc`
- `just release binoc-sqlite`
- `just release binoc-sdk`

`all` remains available as a convenience when maintainers intentionally want a coordinated multi-package release, but it is no longer the default release model.

Versioning policy:

- `binoc` versions the user-facing Python distribution: CLI, Python bindings, host-side packaging, and bundled runtime behavior.
- `binoc-sqlite` versions the SQLite plugin package.
- `binoc-sdk` versions the Rust plugin SDK and compatibility floor.
- These version numbers do not need to match.

Native plugin compatibility policy:

- Rust native plugins should key compatibility to `binoc-sdk`, not to the `binoc` package minor version.
- A native plugin's `pyproject.toml` should usually declare a minimum `binoc` version floor for host-side loader/runtime features, but should not add a `binoc<next-minor` cap merely to mirror the SDK line.
- Runtime acceptance remains governed by the plugin `sdk_version` check performed by the host.

Release coordination policy:

- A `binoc-sdk` release often implies a follow-on `binoc` release, because `binoc` ships the native-plugin host runtime that loads plugins and enforces SDK compatibility.
- A `binoc-sdk` release does not automatically require a `binoc-sqlite` release. Release the plugin when the plugin changed or when a fresh wheel built against the new SDK is needed.

## Alternatives Considered

### Keep one shared `vX.Y.Z` tag

Rejected because a shared tag turns three independent publications into one coupled deploy surface. It makes retries coarse-grained and keeps maintainers thinking in terms of repo-wide releases rather than package-specific releases.

### Keep independent versions but trigger all publish jobs from every tag

Rejected because it preserves the operational problem. The repository would still rebuild and attempt to publish unrelated packages on every release tag, so a failed publish would remain entangled with successful ones.

### Tie native plugin compatibility to `binoc` package minor versions

Rejected because the host package version and the SDK compatibility version are not the same thing. `binoc` may ship CLI or packaging changes without changing the native plugin protocol, and a `binoc<next-minor` cap would block otherwise compatible plugin installs when a plugin author is no longer around to cut a no-op release.
