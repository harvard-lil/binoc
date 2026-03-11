# Build the binoc Python package (primary distribution target) in dev mode.
build:
    cd binoc-python && uv sync --extra dev

# Build optimized release artifacts (Rust binaries + Python package).
build-release:
    cargo build --release
    cd binoc-python && MATURIN_PEP517_ARGS="--release" uv sync --extra dev

# Run binoc CLI with latest source (auto-rebuilds if needed).
binoc *ARGS:
    uv run --with ./binoc-python --with ./binoc-sqlite binoc {{ARGS}}

# Auto-format Rust and Python code.
fmt:
    cargo fmt
    uvx ruff format binoc-python/ binoc-sqlite/python/

# Run formatting and lint checks (mirrors CI).
check:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    uvx ruff check binoc-python/ binoc-sqlite/python/
    uvx ruff format --check binoc-python/ binoc-sqlite/python/

# Run all tests: Rust crates + Python binding tests.
test:
    cargo test
    cd binoc-python && uv run pytest

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
