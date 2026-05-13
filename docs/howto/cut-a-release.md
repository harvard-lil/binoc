---
audience: release manager, core contributor
---

# Cut a release

**Goal.** Ship a new version of one of binoc's published packages to
PyPI or crates.io.

**Prerequisites.**
- Write access to the `harvard-lil/binoc` repository.
- The one-time setup below is complete.

Binoc publishes three public artifacts today. Each has its own
version, its own tag namespace, and is released independently:

| Package | Registry | Tag format |
|---|---|---|
| `binoc` | PyPI | `binoc-vX.Y.Z` |
| `binoc-sqlite` | PyPI | `binoc-sqlite-vX.Y.Z` |
| `binoc-sdk` | crates.io | `binoc-sdk-vX.Y.Z` |

See
[Release surface and automated publishing ADR](../adr/2026-04-08-release_surface_and_automated_publishing.md)
and
[Independent release tags ADR](../adr/2026-04-10-independent_release_tags_and_published_version_policy.md).

## One-time setup

### PyPI trusted publishing

Configure a trusted publisher on PyPI for each project:

1. On PyPI, open `binoc` and add a GitHub trusted publisher:
   - Repository: `harvard-lil/binoc`
   - Workflow: `publish.yml`
   - Environment: `pypi-binoc`
2. Repeat for `binoc-sqlite` with environment `pypi-binoc-sqlite`.

If the projects do not exist on PyPI yet, create the trusted-publisher
setup there before the first automated release.

### crates.io trusted publishing

`binoc-sdk` must be published manually once before trusted publishing
can be enabled.

1. Publish the first `binoc-sdk` release manually from a maintainer
   machine (`cargo publish -p binoc-sdk`).
2. On crates.io, configure trusted publishing:
   - Repository: `harvard-lil/binoc`
   - Workflow: `publish.yml`
   - Environment: `crates-io-binoc-sdk`

The workflow uses `rust-lang/crates-io-auth-action` to exchange
GitHub OIDC credentials for a short-lived crates.io token.

### GitHub environments

Create these GitHub environments in the repository settings:

- `pypi-binoc`
- `pypi-binoc-sqlite`
- `crates-io-binoc-sdk`

If you restrict deployment refs, allow these tag patterns:

- `binoc-v*`
- `binoc-sqlite-v*`
- `binoc-sdk-v*`

## Versioning rules

- Bump **`binoc`** when the user-facing Python package changes.
- Bump **`binoc-sqlite`** when the SQLite plugin package changes.
- Bump **`binoc-sdk`** when the Rust plugin SDK or compatibility
  floor changes.
- A `binoc-sdk` release often implies a follow-on `binoc` release,
  because `binoc` is the host package that embeds the runtime loader
  and compatibility checks for native plugins.
- A `binoc-sdk` release only implies a `binoc-sqlite` release when
  the plugin itself changed or needs rebuilding against the new SDK
  for a fresh published wheel.

Per-package versions do not need to stay in lockstep.

## Cutting a release

### 1. Bump the version

```bash
just set-version binoc 0.1.1
```

`just set-version` updates the selected published manifest(s) plus
any tracked lockfiles the test suite would otherwise rewrite.
Examples:

```bash
just set-version binoc-sqlite 0.1.1
just set-version binoc-sdk 0.2.0
just set-version all 0.3.0
```

### 2. Open a PR and merge to `main`

Let CI pass. Merge the version bump.

### 3. Tag from `origin/main`

```bash
just release binoc
```

Examples:

```bash
just release binoc-sqlite
just release binoc-sdk
just release all
```

`just release` fetches `origin/main`, reads the selected package
version(s) from `origin/main`, creates annotated package-specific
tag(s) pointing at the current `origin/main` commit, and pushes
only those tag(s).

### 4. Watch the publish workflow

Tags trigger `.github/workflows/publish.yml`. It builds and publishes
only the package(s) named by the tags you pushed and leaves the
others untouched. Monitor at
[the Actions page](https://github.com/harvard-lil/binoc/actions/workflows/publish.yml).

## Manual fallbacks

If trusted publishing is unavailable, both registries support manual
uploads from a maintainer machine.

### PyPI

```bash
cd binoc-python && uv build
cd model-plugins/binoc-sqlite && uv build
# upload with an explicit token via twine / uv publish
```

### crates.io

```bash
cargo publish -p binoc-sdk
```

Use this for the initial crates.io release, before trusted publishing
is configured.

## Docs deployment

The docs site deploys automatically on push to `main` via
`.github/workflows/docs.yml`. It is independent of package releases —
doc updates ship whenever they merge, without waiting for a release.

## Where to go next

- [Release surface and automated publishing ADR](../adr/2026-04-08-release_surface_and_automated_publishing.md)
  — the full rationale for the three-package release surface.
- [Independent release tags ADR](../adr/2026-04-10-independent_release_tags_and_published_version_policy.md)
  — the decision record for per-package tag namespaces and the
  `just release` flow.
- [Contribute to binoc](contribute-to-binoc.md) — the everyday
  contribution loop that feeds into these releases.
