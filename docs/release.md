# Releasing Binoc

Binoc publishes three public artifacts today:

- PyPI: `binoc`
- PyPI: `binoc-sqlite`
- crates.io: `binoc-sdk`

Releases are published from GitHub Actions when a `v*` tag is pushed. The publish workflow builds platform wheels for the Python packages and uses trusted publishing for both PyPI and crates.io.

(The single `v*` tag is a short term solution. Under active development, we'll use separate tags for each package, or split out repositories.)

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

If you restrict deployment refs, allow tag pattern `v*` for each environment.

## Versioning

The release tag version must match all published package versions:

- `binoc-python/pyproject.toml`
- `binoc-python/Cargo.toml`
- `model-plugins/binoc-sqlite/pyproject.toml`
- `model-plugins/binoc-sqlite/Cargo.toml`
- `binoc-sdk/Cargo.toml`

`just release` verifies that these stay in lockstep before it pushes a tag.

## Cutting a release

1. Update the versions in the published manifests.
2. Commit the release changes and make sure the working tree is clean.
3. Run:

```bash
just release
```

This runs the full test suite, verifies the published versions match, pushes the current branch, creates an annotated `vX.Y.Z` tag, and pushes the tag.

The publish workflow then:

- builds abi3 wheels plus sdists for `binoc`
- builds abi3 wheels plus sdists for `binoc-sqlite`
- publishes both Python packages to PyPI via trusted publishing
- publishes `binoc-sdk` to crates.io via trusted publishing

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
