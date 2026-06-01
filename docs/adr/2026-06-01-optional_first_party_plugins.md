# Optional First-Party Plugins and `binoc[all]`

**Date:** 2026-06-01
**Status:** Accepted

## Context

The [standard library boundary ADR](2026-03-09-stdlib_boundary.md) keeps
`binoc-stdlib` small: structural containers, universal fallbacks, and common
lightweight formats belong there; heavier or more domain-specific readers belong
in plugins. `binoc-sqlite` is the existing precedent. It is maintained by the
Binoc project, published as a separate Python package, discovered through the
normal `binoc.plugins` entry point, and kept out of the base `binoc` install
because it brings a bundled SQLite dependency through `rusqlite`.

The next format request is a reader for statistical binary datasets: Stata
`.dta`, SAS `.sas7bdat`, and SAS transport `.xpt`. These formats are important
to archivists and data stewards, but they are not universal container formats or
fallback formats. They also have enough parser complexity and dependency churn
that putting them in `binoc-stdlib` would make every user pay for a specialized
capability.

Surveyed Rust candidates on 2026-06-01. Versions below are the latest
non-yanked releases reported by the crates.io API and the docs.rs `latest`
pages at the time of the survey; stale search snippets or cached search indexes
are not a sufficient source for this decision.

| Crate | Coverage | Dependency weight | Fit |
|---|---|---|---|
| [`dta 0.6.0`](https://crates.io/crates/dta/0.6.0), published 2026-05-15 | Stata `.dta` versions 102-119, plus Stata dictionary files | Small. Direct runtime dependency on `encoding_rs`; optional `chrono` and `tokio`. | Good candidate for an optional Stata reader. |
| [`sas_xport 0.4.1`](https://crates.io/crates/sas_xport/0.4.1), published 2026-05-14 | SAS XPORT/XPT v5 and v8 | Small. Direct runtime dependencies on `encoding_rs` and `ibm_hfp`; optional `chrono` and `tokio`. | Good candidate for an optional XPT reader. |
| [`sas7bdat 0.2.0`](https://crates.io/crates/sas7bdat/0.2.0), published 2026-01-27 | SAS `.sas7bdat` | Medium. Pulls parser and CLI-oriented crates such as `clap`, `rayon`, `serde_json`, `time`, `walkdir`, hash-map variants, and numeric/string helpers; optional CSV/Parquet output can remain disabled. | Acceptable only in an optional plugin, not stdlib. |
| [`xportrs 0.0.8`](https://crates.io/crates/xportrs/0.0.8), published 2026-01-13 | SAS XPT with CDISC emphasis | Default dependencies are moderate, with optional `polars`, but the crate declares Rust 1.92. | Not usable without raising Binoc's Rust 1.88 MSRV. |
| [`polars-readstat-rs 0.20.0`](https://crates.io/crates/polars-readstat-rs/0.20.0), published 2026-05-30 | Broad Stata, SAS, XPT, SPSS support | Heavy. Directly depends on Polars, Polars Arrow/Core, `flate2`, `rayon`, `num_cpus`, and related parser dependencies. It declares Rust 1.93. | Reject for now unless Binoc deliberately accepts a Polars-scale dependency and an MSRV bump. |
| [`xpttools 0.2.2`](https://crates.io/crates/xpttools/0.2.2), published 2025-11-10 | XPT to CSV tooling | Heavy and unsuitable for Binoc: GPL-3.0 and a direct `tauri` dependency. | Reject. |

## Decision

### 1. First-party optional plugins are separate packages, not stdlib features

A format reader stays out of `binoc-stdlib` when any of these are true:

- it is domain-specific rather than structurally necessary or a universal
  fallback;
- it introduces medium or heavy parser dependencies, native dependencies, large
  transitive trees, or an MSRV pressure point;
- it needs a release cadence or maintenance review that should not block the
  base `binoc` package.

Such a reader may still be first-party. "Optional" means optional for the base
install, not necessarily third-party ownership.

First-party optional plugins use the same architecture as third-party native
plugins:

- Rust crate and Python wheel package named `binoc-<domain>`;
- Python module name `binoc_<domain>`;
- `binoc_sdk::export_plugin!` for native plugin exports;
- `pyproject.toml` entry point in the existing `binoc.plugins` group;
- plugin-specific test vectors in the plugin package;
- no special registration path in `binoc-core`, `binoc-stdlib`, or `binoc`.

While the Binoc project owns the plugin, it may live under `model-plugins/` in
this repository, following `model-plugins/binoc-sqlite/`. If the plugin later
needs separate governance, a separate repository, or a release cadence that no
longer fits the main repository, it can move out without changing the public
packaging or discovery contract.

### 2. Stata/SAS belongs in a first-party optional plugin

The `.dta` / `.sas7bdat` / `.xpt` reader should live in:

```text
model-plugins/binoc-stat-binary/
```

with these public names:

```text
PyPI package:     binoc-stat-binary
Rust crate:       binoc-stat-binary
Python module:    binoc_stat_binary
Entry point:      binoc-stat-binary = "binoc_stat_binary"
Comparator names: binoc-stat-binary.stata
                  binoc-stat-binary.sas7bdat
                  binoc-stat-binary.xpt
```

It must not be added to `binoc-stdlib`, and it must not be a Cargo feature on
`binoc-stdlib` or `binoc-python`.

The implementation should prefer format-specific, MSRV-compatible pure-Rust
crates:

- `dta` for `.dta`;
- `sas_xport` for `.xpt`;
- `sas7bdat` for `.sas7bdat`, with output-oriented optional features disabled.

Using `polars-readstat-rs`, enabling Polars/Arrow output paths, or raising
Binoc's MSRV is a separate design decision. The follow-on reader work may
proceed without additional sign-off only if it stays inside the optional plugin
package and preserves the current Rust 1.88 MSRV.

### 3. `binoc[all]` is a Python extra for first-party optional plugins

The primary install remains:

```bash
pip install binoc
```

It installs the host, Python bindings, CLI, stdlib comparators, and stdlib
renderers only.

The kitchen-sink install is:

```bash
pip install "binoc[all]"
```

`binoc[all]` is a dependency convenience in `binoc-python/pyproject.toml`. It
lists the first-party optional plugin packages:

```toml
[project.optional-dependencies]
all = [
  "binoc-sqlite>=0.1.1",
  "binoc-stat-binary>=0.1.0",
]
```

The extra does not register plugins itself. Pip installs the plugin wheels; the
existing Python entry-point discovery loads them through `binoc.plugins`. This
keeps one discovery model for base plugins, first-party optional plugins, and
third-party plugins.

`binoc[all]` should use minimum compatible version floors, not lockstep exact
pins and not routine `<next-minor` caps. Runtime native-plugin compatibility is
still governed by the SDK version check described in the
[plugin SDK and ABI ADR](2026-03-12-plugin_sdk_and_abi.md). The `binoc` package
needs a release when the membership of `all` changes or when the minimum
compatible version floor for an included plugin changes.

### 4. Release and catalog policy

Each first-party optional plugin has an independent version and package-specific
release tag, following the
[independent release tags ADR](2026-04-10-independent_release_tags_and_published_version_policy.md):

```text
binoc-stat-binary-vX.Y.Z
```

The plugin's `pyproject.toml` should declare a minimum `binoc` version for the
host-side loader/runtime features it needs. It should not cap `binoc` merely to
mirror the SDK minor version.

The plugin should be added to the plugin catalog currently stored in
`third_party_plugins.json`. The filename is historical; the catalog records
separately installable plugin packages, including first-party optional plugins.

## Consequences

- The base `binoc` wheel stays small and keeps its current dependency posture.
- Users who want all first-party format coverage get a simple pip extra instead
  of remembering every plugin package.
- Optional plugins remain testable and releasable independently.
- The follow-on statistical binary reader can move ahead as an optional native
  plugin. It does not require blocking for human sign-off unless the
  implementation proposes Polars, a new MSRV, a native C/C++ dependency, or
  another dependency beyond the optional plugin policy above.

## Alternatives Considered

### Add Stata/SAS to `binoc-stdlib`

Rejected. The formats are valuable but not structural, universal, or lightweight
enough for every Binoc user. `.sas7bdat` in particular pulls enough parser and
support code that it belongs behind an explicit install choice.

### Add Cargo features to `binoc-stdlib` or `binoc-python`

Rejected. Feature flags would make the base workspace conditional and blur the
plugin boundary. They also do not solve the Python packaging problem for normal
users, who install Binoc with pip rather than selecting Cargo features.

### Use `polars-readstat-rs` as the one broad reader

Rejected for now. It is attractive because it covers Stata, SAS, XPT, and SPSS,
but current releases require Rust 1.93 and bring a Polars/Arrow-sized dependency
tree. That may be reasonable for a future dedicated plugin, but it is too large
for the first Stata/SAS implementation and incompatible with the current MSRV.

### Make `binoc[all]` install every known plugin

Rejected. `all` means first-party optional plugins maintained by the Binoc
project. Third-party plugin selection should remain explicit because those
packages have independent ownership, trust, licensing, dependency, and release
policies.

### Create a new discovery path for first-party optional plugins

Rejected. The existing `binoc.plugins` entry point already handles native Rust
plugins and pure Python plugins. A special first-party mechanism would add
global state and make optional plugins less like the third-party packages they
are meant to model.
