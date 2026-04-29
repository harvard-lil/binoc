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
    uvx ruff check --fix-only binoc-python/ model-plugins/

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
    cd binoc-python && uv run --extra dev maturin develop && uv run --extra dev pytest
    cd model-plugins/binoc-sqlite && uv run --extra dev maturin develop && uv run --extra dev python -m pytest
    cd model-plugins/binoc-html && uv run --extra dev python -m pytest

# Regenerate docs/tutorial.md by re-running all embedded code blocks. Depends on
# `just materialize` so the tutorial's `test-vectors-materialized/...` commands
# resolve.
docs: materialize
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

# Materialize test-vectors/ into test-vectors-materialized/ for every workspace
# crate that ships vectors (same builders the test harness uses). Each plugin
# contributes its own VectorMaterializers; see docs/adr/test_vector_materialization.md.
# Output: test-vectors-materialized/<vector-name>/ (gitignored) next to each vectors dir.
materialize:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf test-vectors-materialized model-plugins/binoc-sqlite/test-vectors-materialized
    cargo run -q -p binoc-stdlib --features test-vectors --bin materialize-test-vectors -- \
        test-vectors-materialized test-vectors
    cargo run -q -p binoc-sqlite --features test-support --bin materialize-test-vectors -- \
        model-plugins/binoc-sqlite/test-vectors-materialized model-plugins/binoc-sqlite/test-vectors
    echo "Materialized vectors under test-vectors-materialized/ and model-plugins/binoc-sqlite/test-vectors-materialized/"

# Set the shared published package version across Cargo + Python manifests.
set-version package version:
    #!/usr/bin/env bash
    set -euo pipefail

    PACKAGE="{{package}}"
    VERSION="{{version}}"

    if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
      echo "Version must look like semver (for example: 0.1.1 or 0.2.0-rc.1)." >&2
      exit 1
    fi

    set_manifest_version() {
      local path="$1"
      perl -0pi -e 's/^version = "[^"]*"/version = "'"$VERSION"'"/m' "$path"
    }

    relock_uv() {
      local path="$1"
      (
        cd "$path"
        uv lock
      )
    }

    case "$PACKAGE" in
      binoc)
        set_manifest_version binoc-python/pyproject.toml
        relock_uv binoc-python
        relock_uv model-plugins/binoc-sqlite
        relock_uv model-plugins/binoc-html
        ;;
      binoc-sqlite)
        set_manifest_version model-plugins/binoc-sqlite/pyproject.toml
        relock_uv model-plugins/binoc-sqlite
        ;;
      binoc-html)
        set_manifest_version model-plugins/binoc-html/pyproject.toml
        relock_uv model-plugins/binoc-html
        ;;
      binoc-sdk)
        set_manifest_version Cargo.toml
        cargo update -w
        ;;
      all)
        set_manifest_version Cargo.toml
        set_manifest_version binoc-python/pyproject.toml
        set_manifest_version model-plugins/binoc-sqlite/pyproject.toml
        set_manifest_version model-plugins/binoc-html/pyproject.toml
        cargo update -w
        relock_uv binoc-python
        relock_uv model-plugins/binoc-sqlite
        relock_uv model-plugins/binoc-html
        ;;
      *)
        echo "Usage: just set-version [binoc|binoc-sqlite|binoc-html|binoc-sdk|all] <version>" >&2
        exit 1
        ;;
    esac

    echo "Set ${PACKAGE} version to ${VERSION}."

# Tag the current origin/main commit for one published package, or `all`.
release package:
    #!/usr/bin/env bash
    set -euo pipefail

    PACKAGE="{{package}}"

    toml_string_from_origin_main() {
      local path="$1"
      local contents
      if ! contents="$(git show "origin/main:${path}" 2>/dev/null)"; then
        echo "Failed to read ${path} from origin/main." >&2
        exit 1
      fi
      printf '%s\n' "$contents" | sed -n 's/^version = "\(.*\)"/\1/p' | head -n 1
    }

    package_version_from_origin_main() {
      local package="$1"
      case "$package" in
        binoc)
          toml_string_from_origin_main binoc-python/pyproject.toml
          ;;
        binoc-sqlite)
          toml_string_from_origin_main model-plugins/binoc-sqlite/pyproject.toml
          ;;
        binoc-sdk)
          toml_string_from_origin_main Cargo.toml
          ;;
        *)
          echo "Unknown package: ${package}" >&2
          exit 1
          ;;
      esac
    }

    package_tag() {
      local package="$1"
      local version="$2"
      echo "${package}-v${version}"
    }

    git fetch origin main --tags
    REMOTE_MAIN="$(git rev-parse origin/main)"

    case "$PACKAGE" in
      binoc|binoc-sqlite|binoc-sdk)
        packages=("$PACKAGE")
        ;;
      all)
        packages=("binoc" "binoc-sqlite" "binoc-sdk")
        ;;
      *)
        echo "Usage: just release [binoc|binoc-sqlite|binoc-sdk|all]" >&2
        exit 1
        ;;
    esac

    tags=()
    versions=()

    for package in "${packages[@]}"; do
      version="$(package_version_from_origin_main "$package")"
      if [ -z "$version" ]; then
        echo "Failed to read ${package} version from origin/main." >&2
        exit 1
      fi

      tag="$(package_tag "$package" "$version")"

      if git show-ref --verify --quiet "refs/tags/${tag}"; then
        echo "Tag ${tag} already exists." >&2
        exit 1
      fi
      if git ls-remote origin "refs/tags/${tag}" 2>/dev/null | grep -q .; then
        echo "Tag ${tag} already exists on origin." >&2
        exit 1
      fi

      versions+=("$version")
      tags+=("$tag")
    done

    for i in "${!packages[@]}"; do
      echo "Tagging origin/main at ${REMOTE_MAIN} as ${tags[$i]} ..."
      git tag -a "${tags[$i]}" "${REMOTE_MAIN}" -m "Release ${packages[$i]} ${versions[$i]}"
    done

    push_refs=()
    for tag in "${tags[@]}"; do
      push_refs+=("refs/tags/${tag}")
    done

    git push origin "${push_refs[@]}"
    echo "Pushed ${tags[*]}. GitHub Actions should publish the selected package(s)."
    echo "Visit https://github.com/harvard-lil/binoc/actions/workflows/publish.yml to monitor the release."
