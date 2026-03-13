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
