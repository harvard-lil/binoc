# `just` as the Canonical Task Runner

**Date:** 2026-03-06
**Status:** Implemented

## Problem

The workspace has two build systems: `cargo` for Rust and `maturin`/`uv` for the Python bindings. `cargo build` can't build `binoc-python` because PyO3 cdylib crates need maturin to set up the correct linker flags — the linker fails on unresolved CPython symbols that are normally provided by the host interpreter at load time. This meant the docs had to explain a split workflow: `cargo build --release -p binoc-cli -p binoc-core -p binoc-stdlib` for Rust, then a separate `cd binoc-python && uv sync` for Python. `cargo test` had the same gap — it couldn't run the 61 Python tests.

Contributors shouldn't need to know which build system owns which crate.

## Decision

**`just build` and `just test` are the canonical commands.** Each recipe runs both the Rust and Python steps in sequence:

- `just build` → `cargo build --release` + `cd binoc-python && uv sync --extra dev`
- `just test` → `cargo test` + `cd binoc-python && uv run pytest`

`Cargo.toml` sets `default-members` to exclude `binoc-python`, so bare `cargo build` / `cargo test` still work for fast Rust-only iteration without hitting the PyO3 linker issue.

## Why `just`?

It was already a required dependency (for `just docs`), adds no new tool to install, and its recipes are plain shell commands — no DSL to learn. Make would work too, but `just` is already in the prerequisites.

## Alternatives considered

- **`cargo xtask`:** Keeps everything in Rust, but building the xtask binary still can't invoke maturin without shelling out, so it just adds a layer of indirection.
- **A Makefile:** Equivalent, but `just` was already established in the project.
- **`uv` as the sole entry point:** Could wrap cargo via `uv run`, but that inverts the dependency — Rust is the primary codebase, not Python.
