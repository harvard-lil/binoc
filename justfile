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

# Tag v{version} from published package metadata, push branch + tag (triggers release workflow).
release:
    #!/usr/bin/env bash
    set -euo pipefail
    just test

    version_from() {
      sed -n 's/^version = "\(.*\)"/\1/p' "$1" | head -n 1
    }

    BINOC_PY_VERSION="$(version_from binoc-python/pyproject.toml)"
    BINOC_CARGO_VERSION="$(version_from binoc-python/Cargo.toml)"
    SQLITE_PY_VERSION="$(version_from model-plugins/binoc-sqlite/pyproject.toml)"
    SQLITE_CARGO_VERSION="$(version_from model-plugins/binoc-sqlite/Cargo.toml)"
    SDK_VERSION="$(version_from binoc-sdk/Cargo.toml)"

    if [ -z "$BINOC_PY_VERSION" ] || [ -z "$BINOC_CARGO_VERSION" ] || [ -z "$SQLITE_PY_VERSION" ] || [ -z "$SQLITE_CARGO_VERSION" ] || [ -z "$SDK_VERSION" ]; then
      echo "Failed to read one or more package versions." >&2
      exit 1
    fi

    if [ "$BINOC_PY_VERSION" != "$BINOC_CARGO_VERSION" ]; then
      echo "binoc Python/Cargo versions differ: $BINOC_PY_VERSION vs $BINOC_CARGO_VERSION" >&2
      exit 1
    fi
    if [ "$SQLITE_PY_VERSION" != "$SQLITE_CARGO_VERSION" ]; then
      echo "binoc-sqlite Python/Cargo versions differ: $SQLITE_PY_VERSION vs $SQLITE_CARGO_VERSION" >&2
      exit 1
    fi
    if [ "$BINOC_PY_VERSION" != "$SQLITE_PY_VERSION" ] || [ "$BINOC_PY_VERSION" != "$SDK_VERSION" ]; then
      echo "Published package versions must match:" >&2
      echo "  binoc=$BINOC_PY_VERSION" >&2
      echo "  binoc-sqlite=$SQLITE_PY_VERSION" >&2
      echo "  binoc-sdk=$SDK_VERSION" >&2
      exit 1
    fi

    VERSION="$BINOC_PY_VERSION"
    TAG="v${VERSION}"

    if ! git rev-parse --git-dir >/dev/null 2>&1; then
      echo "Not a git repository." >&2
      exit 1
    fi
    if ! git remote get-url origin >/dev/null 2>&1; then
      echo "No git remote named 'origin'. Add it before releasing." >&2
      exit 1
    fi
    if git show-ref --verify --quiet "refs/tags/${TAG}"; then
      echo "Tag ${TAG} already exists locally." >&2
      exit 1
    fi
    if git ls-remote origin "refs/tags/${TAG}" 2>/dev/null | grep -q .; then
      echo "Tag ${TAG} already exists on origin." >&2
      exit 1
    fi
    if [ -n "$(git status --porcelain)" ]; then
      echo "Working tree is not clean. Commit or stash before releasing." >&2
      exit 1
    fi

    echo "Pushing current branch, then tagging ${TAG} (version ${VERSION})..."
    git push
    git tag -a "${TAG}" -m "Release ${VERSION}"
    git push origin "${TAG}"
    echo "Pushed ${TAG}. GitHub Actions should publish binoc, binoc-sqlite, and binoc-sdk."
