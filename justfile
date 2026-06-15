# Build the binoc Python package (primary distribution target) in dev mode.
build:
    cd binoc-python && uv sync --extra dev
    cd model-plugins/binoc-sqlite && uv sync --extra dev
    cd model-plugins/binoc-stat-binary && uv sync --extra dev
    cd model-plugins/binoc-html && uv sync --extra dev

# Build optimized release artifacts (Rust binaries + Python package).
build-release:
    cargo build --release
    cd binoc-python && MATURIN_PEP517_ARGS="--release" uv sync --extra dev

# Run binoc CLI with latest source (auto-rebuilds if needed).
binoc *ARGS:
    uv run --refresh-package binoc --with ./binoc-python --with ./model-plugins/binoc-sqlite --with ./model-plugins/binoc-stat-binary --with ./model-plugins/binoc-html binoc {{ARGS}}

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

# Run the mechanical lint tests with warnings visible. Lint errors already
# fail `just test`; this surfaces the advisory warnings that passing tests
# hide. See binoc_sdk::lints for the invariant/lint tier scheme and
# .agents/skills/lint-plugin/SKILL.md for the judgment-level checklist.
lint:
    cargo test -p binoc-core -p binoc-stdlib -p binoc-sqlite -p binoc-row-reorder -p binoc-excel -p binoc-parquet -p binoc-avro -p binoc-dbf -p binoc-binformats --test lints -- --nocapture

# Emit correspondence engine performance reports as JSONL. With no args, uses
# the deterministic synthetic fixture; pass `--left A --right B` for real
# snapshots. Timing fields are noisy; structural fields should be stable.
perf *ARGS:
    cargo run --release -q -p binoc-stdlib --bin perf_report -- {{ARGS}}

# Run all tests: Rust crates + Python binding tests.
# Note: no --all-features here. The test-vectors feature is already activated via
# dev-dependencies, and --all-features would enable binoc-sqlite's "python" feature,
# which builds a PyO3 cdylib that can only link via maturin (not bare cargo).
test:
    cargo test
    cd binoc-python && uv run --extra dev maturin develop && uv run --extra dev pytest
    cd model-plugins/binoc-sqlite && uv run --extra dev maturin develop && uv run --extra dev python -m pytest
    cd model-plugins/binoc-stat-binary && uv run --extra dev maturin develop && uv run --extra dev python -m pytest
    cd model-plugins/binoc-html && uv run --extra dev python -m pytest

# Aggregate docs generators. Each sub-recipe is cache-aware and skips work when
# inputs are unchanged. See docs/adr/2026-04-17-documentation_platform_and_info_design.md §6.
docs: docs-tutorial docs-cli docs-adr-index docs-plugin-catalog docs-schema docs-sdk docs-vectors docs-replays

# Regenerate docs/tutorial.md by re-running all embedded code blocks. Showboat
# runs in a uv tool env that includes ./binoc-python, so visible `binoc`
# commands use the local source tree. Depends on `just materialize` so the
# tutorial's `test-vectors-materialized/...` commands resolve.
docs-tutorial: materialize
    #!/usr/bin/env bash
    set -euo pipefail
    TARGET="docs/tutorial.md"
    if uvx --with ./binoc-python showboat verify "${TARGET}" --output "${TARGET}" > /dev/null 2>&1; then
        echo "${TARGET} is up to date."
    else
        echo "${TARGET} updated."
    fi

# Regenerate docs/users/reference/cli.md from the binoc-cli clap Command tree.
# Inputs: binoc-cli/** (every .rs file under binoc-cli/ contributes to the
# Command tree). The emitter only rewrites the region between the BEGIN/END
# markers in docs/users/reference/cli.md, leaving the authored framing intact.
# Cargo's incremental build makes the no-change case cheap; the emitter
# itself skips the write when the regenerated region already matches.
docs-cli:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo run --quiet -p binoc-cli --bin emit-cli-markdown -- docs/users/reference/cli.md

# Regenerate docs/adr/README.md from front matter of docs/adr/*.md.
docs-adr-index:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --quiet --script scripts/build_adr_index.py

# Create a new ADR from docs/adr/TEMPLATE.md with today's date in the filename,
# then refresh docs/adr/README.md. Usage: `just adr "Some title"`.
adr title:
    #!/usr/bin/env -S uv run --quiet python
    import datetime, pathlib, re, sys
    title = {{quote(title)}}
    slug = re.sub(r'[^a-z0-9]+', '_', title.lower()).strip('_')
    date = datetime.date.today().isoformat()
    adr_dir = pathlib.Path('docs/adr')
    template = adr_dir.joinpath('TEMPLATE.md').read_text()
    adr_file = adr_dir.joinpath(f'{date}-{slug}.md')
    suffix = 1
    while adr_file.exists():
        adr_file = adr_dir.joinpath(f'{date}-{slug}-{suffix}.md')
        suffix += 1
    adr_file.write_text(template.replace('TITLE', title).replace('DATE', date))
    print(f'Created {adr_file}')
    import subprocess
    subprocess.run(['just', 'docs-adr-index'], check=True)
    print('Next: edit the file. The ADR is already wired into the index and mkdocs nav.')

# Regenerate docs/users/explanation/test-vectors-gallery.md from the shared workspace
# vectors under test-vectors/. This first pass is manifest-only: it summarizes
# metadata, assertions, and committed snapshot layouts without materializing
# archives or running diffs.
docs-vectors:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --quiet --script scripts/build_test_vector_gallery.py

# Regenerate the interactive HTML replays under docs/users/explanation/replays/
# and the docs/users/explanation/replays.md index from a curated set of shared
# vectors. Materializes first (replays run `binoc diff --trace` over the built
# snapshots, then `binoc replay`). The generated HTML is gitignored; the index
# page is committed. See scripts/build_replays.py for the featured set.
docs-replays: materialize
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --quiet --script scripts/build_replays.py

# Regenerate docs/users/reference/third-party-plugins.md from third_party_plugins.json (repo root).
docs-plugin-catalog:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --quiet --script scripts/build_third_party_plugins_page.py

# Regenerate docs/users/reference/changeset-schema.{json,md} from the binoc-sdk IR
# types. Inputs: binoc-sdk/src/ir.rs, binoc-sdk/src/types.rs (the types that
# carry `#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]`), and
# scripts/build_changeset_schema_page.py. The Rust binary lives behind the
# `schema` feature on binoc-sdk so schemars stays out of the default
# dependency graph; Cargo's incremental build keeps reruns cheap, and the
# Markdown renderer skips the write when output is unchanged. See ADR
# `2026-04-17-documentation_platform_and_info_design.md` Open Question 1.
docs-schema:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo run --quiet -p binoc-sdk --features schema --bin gen-changeset-schema -- \
        docs/users/reference/changeset-schema.json
    uv run --quiet --script scripts/build_changeset_schema_page.py

# Regenerate docs/sdk/ by running `cargo doc` for binoc-sdk and copying the
# rendered rustdoc HTML into the docs tree so mkdocs serves it as a static
# subpath at /sdk/. Cargo's own incremental cache keeps reruns cheap. Output
# under docs/sdk/ is gitignored. See docs/plugin-developers/reference/sdk.md for the landing page.
docs-sdk:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo doc --no-deps --package binoc-sdk --quiet
    rm -rf docs/sdk
    mkdir -p docs/sdk
    cp -R target/doc/. docs/sdk/

mkdocs *ARGS:
    uvx --with mkdocs-material --with mkdocs-include-markdown-plugin --with pymdown-extensions \
        --with 'mkdocstrings[python]' --with ./binoc-python --with ruff \
        mkdocs {{ARGS}}

# Build the docs site (with --strict to fail on broken links / missing files).
# Runs `just docs` first to refresh generated inputs. `mkdocstrings[python]`
# imports the installed `binoc` package to render `docs/plugin-developers/reference/python.md`
# from its docstrings, so `./binoc-python` is installed into the docs env
# (requires a Rust toolchain via maturin).
docs-build: docs
    just mkdocs build --strict

# Live-preview the docs site at http://127.0.0.1:8000/. Uses watchexec to run
# `docs-build` then `mkdocs serve`, restarting whenever inputs to `just docs`
# or MkDocs config change. Requires watchexec on PATH (`brew install watchexec`).
docs-serve:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v watchexec >/dev/null 2>&1; then
        echo "error: docs-serve requires watchexec (e.g. brew install watchexec)" >&2
        exit 1
    fi
    exec watchexec \
        --restart \
        --debounce 750ms \
        -w docs \
        -w scripts \
        -w binoc-cli \
        -w binoc-sdk \
        -w binoc-python \
        -w binoc-stdlib \
        -w test-vectors \
        -w model-plugins/binoc-sqlite \
        -w model-plugins/binoc-stat-binary \
        -w mkdocs.yml \
        -w third_party_plugins.json \
        -w justfile \
        -w Cargo.toml \
        -i docs/tutorial.md \
        -i docs/adr/README.md \
        -i docs/users/explanation/test-vectors-gallery.md \
        -i docs/users/reference/third-party-plugins.md \
        -i docs/users/reference/changeset-schema.json \
        -i docs/users/reference/changeset-schema.md \
        -i 'docs/sdk/**' \
        -- bash -c 'just docs-build && exec just mkdocs serve'

# Review pending snapshot changes interactively.
snapshot-review:
    cargo insta test -p binoc-stdlib --test test_vectors --review

# Regenerate all expected-output snapshots (run after intentional IR/output changes).
snapshot-update:
    INSTA_UPDATE=always cargo test -p binoc-stdlib --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-sqlite --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-row-reorder --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-stat-binary --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-excel --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-parquet --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-avro --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-dbf --test test_vectors
    INSTA_UPDATE=always cargo test -p binoc-binformats --test test_vectors

# Materialize test-vectors/ into test-vectors-materialized/ for every workspace
# crate that ships vectors (same builders the test harness uses). Each plugin
# contributes its own VectorMaterializers; see docs/adr/2026-04-16-test_vector_materialization.md.
# Output: test-vectors-materialized/<vector-name>/ (gitignored) next to each vectors dir.
materialize:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf test-vectors-materialized model-plugins/binoc-sqlite/test-vectors-materialized model-plugins/binoc-stat-binary/test-vectors-materialized
    cargo run -q -p binoc-stdlib --features test-vectors --bin materialize-test-vectors -- \
        test-vectors-materialized test-vectors
    cargo run -q -p binoc-sqlite --features test-support --bin materialize-test-vectors -- \
        model-plugins/binoc-sqlite/test-vectors-materialized model-plugins/binoc-sqlite/test-vectors
    cargo run -q -p binoc-stat-binary --features test-support --bin materialize-stat-binary-test-vectors -- \
        model-plugins/binoc-stat-binary/test-vectors-materialized model-plugins/binoc-stat-binary/test-vectors
    echo "Materialized vectors under test-vectors-materialized/ and plugin test-vectors-materialized/ trees"

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
        relock_uv model-plugins/binoc-stat-binary
        relock_uv model-plugins/binoc-html
        ;;
      binoc-sqlite)
        set_manifest_version model-plugins/binoc-sqlite/pyproject.toml
        relock_uv model-plugins/binoc-sqlite
        ;;
      binoc-stat-binary)
        set_manifest_version model-plugins/binoc-stat-binary/pyproject.toml
        relock_uv model-plugins/binoc-stat-binary
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
        set_manifest_version model-plugins/binoc-stat-binary/pyproject.toml
        set_manifest_version model-plugins/binoc-html/pyproject.toml
        cargo update -w
        relock_uv binoc-python
        relock_uv model-plugins/binoc-sqlite
        relock_uv model-plugins/binoc-stat-binary
        relock_uv model-plugins/binoc-html
        ;;
      *)
        echo "Usage: just set-version [binoc|binoc-sqlite|binoc-stat-binary|binoc-html|binoc-sdk|all] <version>" >&2
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
        binoc-stat-binary)
          toml_string_from_origin_main model-plugins/binoc-stat-binary/pyproject.toml
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
      binoc|binoc-sqlite|binoc-stat-binary|binoc-sdk)
        packages=("$PACKAGE")
        ;;
      all)
        packages=("binoc" "binoc-sqlite" "binoc-stat-binary" "binoc-sdk")
        ;;
      *)
        echo "Usage: just release [binoc|binoc-sqlite|binoc-stat-binary|binoc-sdk|all]" >&2
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
