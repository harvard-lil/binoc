# Build the binoc Python package (primary distribution target) in dev mode.
build:
    cd binoc-python && uv sync --extra dev
    cd model-plugins/binoc-sqlite && uv sync --extra dev
    cd model-plugins/binoc-html && uv sync --extra dev

# Build optimized release artifacts (Rust binaries + Python package).
build-release:
    cargo build --release
    cd binoc-python && MATURIN_PEP517_ARGS="--release" uv sync --extra dev

# Run binoc CLI with latest source (auto-rebuilds if needed).
binoc *ARGS:
    uv run --with ./binoc-python --with ./model-plugins/binoc-sqlite --with ./model-plugins/binoc-html binoc {{ARGS}}

# Auto-format Rust and Python code.
fmt:
    cargo fmt
    uvx ruff format binoc-python/ model-plugins/

# Run formatting and lint checks (mirrors CI).
check:
    cargo fmt --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    uvx ruff check binoc-python/ model-plugins/
    uvx ruff format --check binoc-python/ model-plugins/

# Run all tests: Rust crates + Python binding tests.
# Note: no --all-features here. The test-vectors feature is already activated via
# dev-dependencies, and --all-features would enable binoc-sqlite's "python" feature,
# which builds a PyO3 cdylib that can only link via maturin (not bare cargo).
test:
    cargo test
    cd binoc-python && uv run --extra dev pytest
    cd model-plugins/binoc-sqlite && uv run --extra dev maturin develop && uv run --extra dev python -m pytest
    cd model-plugins/binoc-html && uv run --extra dev python -m pytest

# Regenerate docs/tutorial.md by re-running all embedded code blocks.
docs:
    #!/usr/bin/env bash
    set -euo pipefail
    if uvx showboat verify docs/tutorial.md --output docs/tutorial.md > /dev/null 2>&1; then
        echo "docs/tutorial.md is up to date."
    else
        echo "docs/tutorial.md updated."
    fi

# Review pending snapshot changes interactively.
snapshot-review:
    cargo insta test -p binoc-stdlib --test test_vectors --review

# Regenerate all expected-output snapshots (run after intentional IR/output changes).
snapshot-update:
    INSTA_UPDATE=always cargo test -p binoc-stdlib --test test_vectors

# Set the shared published package version across Cargo + Python manifests.
set-version version:
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION="{{version}}"

    if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
      echo "Version must look like semver (for example: 0.1.1 or 0.2.0-rc.1)." >&2
      exit 1
    fi

    perl -0pi -e 's/^version = "[^"]*"/version = "'"$VERSION"'"/m' Cargo.toml
    perl -0pi -e 's/^version = "[^"]*"/version = "'"$VERSION"'"/m' binoc-python/pyproject.toml
    perl -0pi -e 's/^version = "[^"]*"/version = "'"$VERSION"'"/m' model-plugins/binoc-sqlite/pyproject.toml

    # update the lock files
    cargo update -w
    cd binoc-python && uv lock
    cd ../model-plugins/binoc-sqlite && uv lock
    cd ../binoc-html && uv lock

    echo "Set published package version to ${VERSION}."

# Tag the current origin/main commit using published package metadata from origin/main.
release:
    #!/usr/bin/env bash
    set -euo pipefail

    toml_string_from_origin_main() {
      local path="$1"
      local contents
      if ! contents="$(git show "origin/main:${path}" 2>/dev/null)"; then
        echo "Failed to read ${path} from origin/main." >&2
        exit 1
      fi
      printf '%s\n' "$contents" | sed -n 's/^version = "\(.*\)"/\1/p' | head -n 1
    }

    git fetch origin main --tags
    REMOTE_MAIN="$(git rev-parse origin/main)"

    WORKSPACE_VERSION="$(toml_string_from_origin_main Cargo.toml)"
    BINOC_PY_VERSION="$(toml_string_from_origin_main binoc-python/pyproject.toml)"
    SQLITE_PY_VERSION="$(toml_string_from_origin_main model-plugins/binoc-sqlite/pyproject.toml)"

    if [ -z "$WORKSPACE_VERSION" ] || [ -z "$BINOC_PY_VERSION" ] || [ -z "$SQLITE_PY_VERSION" ]; then
      echo "Failed to read one or more package versions from origin/main." >&2
      exit 1
    fi

    if [ "$BINOC_PY_VERSION" != "$SQLITE_PY_VERSION" ] || [ "$BINOC_PY_VERSION" != "$WORKSPACE_VERSION" ]; then
      echo "Published package versions on origin/main must match:" >&2
      echo "  workspace=$WORKSPACE_VERSION" >&2
      echo "  binoc=$BINOC_PY_VERSION" >&2
      echo "  binoc-sqlite=$SQLITE_PY_VERSION" >&2
      echo "Merge a version-bump commit before releasing." >&2
      exit 1
    fi

    TAG="v${WORKSPACE_VERSION}"

    if git show-ref --verify --quiet "refs/tags/${TAG}"; then
      echo "Tag ${TAG} already exists." >&2
      exit 1
    fi
    if git ls-remote origin "refs/tags/${TAG}" 2>/dev/null | grep -q .; then
      echo "Tag ${TAG} already exists on origin." >&2
      exit 1
    fi

    # push the tag
    echo "Tagging origin/main at ${REMOTE_MAIN} as ${TAG} ..."
    git tag -a "${TAG}" "${REMOTE_MAIN}" -m "Release ${WORKSPACE_VERSION}"
    git push origin "refs/tags/${TAG}"
    echo "Pushed ${TAG}. GitHub Actions should publish binoc, binoc-sqlite, and binoc-sdk."
    echo "Visit https://github.com/harvard-lil/binoc/actions/workflows/publish.yml to monitor the release."
