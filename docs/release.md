# Releasing Binoc

Binoc publishes three public artifacts today:

- PyPI: `binoc`
- PyPI: `binoc-sqlite`
- crates.io: `binoc-sdk`

Releases are published from GitHub Actions when a package-specific tag is pushed. The publish workflow builds platform wheels for the Python packages and uses trusted publishing for both PyPI and crates.io.

## One-time setup

### PyPI trusted publishing

Configure trusted publishers for both PyPI projects:

1. On PyPI, open `binoc` and add a GitHub trusted publisher for:
   - Repository: `harvard-lil/binoc`
   - Workflow: `publish.yml`
   - Environment: `pypi-binoc`
2. Repeat for `binoc-sqlite` with environment `pypi-binoc-sqlite`.

If the projects do not exist on PyPI yet, create the trusted-publisher setup there before the first automated release.

### crates.io trusted publishing

`binoc-sdk` must be published manually once before trusted publishing can be enabled on crates.io.

1. Publish the first `binoc-sdk` release manually from a maintainer machine.
2. On crates.io, configure trusted publishing for:
   - Repository: `harvard-lil/binoc`
   - Workflow: `publish.yml`
   - Environment: `crates-io-binoc-sdk`

The workflow uses `rust-lang/crates-io-auth-action` to exchange GitHub OIDC credentials for a short-lived crates.io token.

### GitHub environments

Create these GitHub environments:

- `pypi-binoc`
- `pypi-binoc-sqlite`
- `crates-io-binoc-sdk`

If you restrict deployment refs, allow these tag patterns:

- `binoc-v*`
- `binoc-sqlite-v*`
- `binoc-sdk-v*`

## Versioning

Each published artifact has its own version and its own tag namespace:

- `binoc-vX.Y.Z` publishes the version in `binoc-python/pyproject.toml`
- `binoc-sqlite-vX.Y.Z` publishes the version in `model-plugins/binoc-sqlite/pyproject.toml`
- `binoc-sdk-vX.Y.Z` publishes the workspace version in `Cargo.toml`

These versions no longer need to stay in lockstep.

Versioning rules:

- Bump `binoc` when the user-facing Python package changes.
- Bump `binoc-sqlite` when the SQLite plugin package changes.
- Bump `binoc-sdk` when the Rust plugin SDK or compatibility floor changes.
- A `binoc-sdk` release often implies a follow-on `binoc` release, because `binoc` is the host package that embeds the runtime loader and compatibility checks for native plugins.
- A `binoc-sdk` release only implies a `binoc-sqlite` release when the plugin itself changed or needs rebuilding against the new SDK for a fresh published wheel.

## Cutting a release

1. Update the version for the package you are releasing:

```bash
just set-version binoc 0.1.1
```

Examples:

```bash
just set-version binoc-sqlite 0.1.1
just set-version binoc-sdk 0.2.0
just set-version all 0.3.0
```

`just set-version` updates only the selected published manifest(s) plus any tracked lockfiles that would otherwise be rewritten by the test suite.

2. Open a PR, let CI pass, and merge the version bump to `main`.
3. Run:

```bash
just release binoc
```

Examples:

```bash
just release binoc-sqlite
just release binoc-sdk
just release all
```

This fetches `origin/main`, reads the selected package version(s) from `origin/main`, creates annotated package-specific tag(s) pointing at the current `origin/main` commit, and pushes only those tag(s).

The publish workflow then:

- builds and publishes only the package(s) named by the tag(s) you pushed
- leaves unrelated packages untouched

## Manual fallbacks

### PyPI

Build locally:

```bash
cd binoc-python && uv build
cd model-plugins/binoc-sqlite && uv build
```

Upload with an explicit token only if trusted publishing is unavailable.

### crates.io

Publish locally:

```bash
cargo publish -p binoc-sdk
```

Use this for the initial crates.io release before trusted publishing is configured.
