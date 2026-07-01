# Fat-binoc Distribution and the ABI Canary

**Date:** 2026-06-30
**Status:** Accepted; implemented

## Context

After the correspondence-first engine overhaul, binoc ships ten first-party
format packs as separate workspace crates (SQLite, Excel, Parquet, Avro, DBF,
XML, Shapefile, binformats, stat-binary, row-reorder). The question for the
0.2.0 release was how to *distribute* them: one PyPI package per plugin, or one
"fat" `binoc` wheel that compiles them in.

Three facts about the current code decided it:

1. **Format packs are in-process rule packs, not ABI plugins.** Every plugin
   exposes `pub fn register_correspondence_rules(&mut CorrespondenceEngineConfig)`
   and registers `CoreRule`s directly. There is no wire format for rule packs.

2. **The C ABI covers renderers only.** `binoc_sdk::plugin_abi` defines
   `RenderRequest`/`RenderResponse` and states plainly that "renderers are the
   only graduated stable ABI family; rule families remain in-process until their
   trait shapes and vocabularies satisfy the graduation signal" (see
   [the tiered plugin surface ADR](2026-06-12-tiered_plugin_surface_pre_1_0.md)).
   `load_native_plugin_into_registry` in `binoc-python` confirms this: it
   registers only the `renderers` from a loaded `.so` — there is no path for a
   natively-loaded plugin to contribute correspondence rules.

   The consequence is decisive: **a separately-pip-installed format pack could
   not actually inject its rules across the package boundary today.** Publishing
   the format packs as their own wheels would ship non-functional packages. The
   only working mechanism for a format pack is the in-process
   `register_correspondence_rules` call — i.e. compiling it in.

3. **The anti-cheat ABI canary did not exist.** At decision time `export_plugin!`
   was defined but used nowhere; no in-tree renderer was built as a cdylib and
   loaded over the boundary in any test. The C-ABI seam was *specified* but not
   *exercised*. (This ADR adds it — see the Decision.)

The deeper worry with compiling plugins in-process is that it removes the
compiler's enforcement of the boundary. A hard ABI has the virtue that you
cannot accidentally couple across it — the borrow checker and linker reject
shared lifetimes, arena handles, generics, and reaching into host internals. In
one cargo build, those cheats compile fine, so the boundary can drift invisibly
until someone first tries to put a plugin behind a process boundary years later.

## Decision

**Ship one fat `binoc` wheel.** Compile the first-party format packs into the
`binoc` package, registered in-process through the same
`register_correspondence_rules` seam used everywhere else — no privileged
in-process shortcut that the eventual ABI path could not also express.

- Gate each pack behind a per-format cargo feature, default-on, aggregated by a
  `bundled` feature, so a slim wheel remains one maturin flag away. The standard
  library is itself a plugin pack and is already linked in this way; the format
  packs join it.
- The bundle is defined once (in `binoc-cli`, the shared assembly point both the
  standalone CLI and the Python host already depend on) and registered into both
  the library path (`binoc-python`'s controller construction) and the CLI path,
  so `binoc.diff(...)` and the `binoc` CLI are equally fat.
- **Defer multi-package PyPI publishing** until the rule-family ABI graduates per
  the tiered plugin surface ADR. Distribution (fat vs. split) is kept orthogonal
  to where the ABI seam sits, which is per-family and lives in the type system.
- **Pause separate publishing of the in-tree plugins** (removed from
  `publish.yml` and the `just release` set). `binoc-stat-binary` is in the fat
  bundle, so a separate wheel is redundant and re-introduces the ABI-coupling
  matrix the fat wheel exists to avoid. `binoc-sqlite` is paused for the same
  reason *and* to keep the build free of its bundled `rusqlite` C compilation
  during dev; it is deliberately excluded from the default `bundled` set. Both
  crates and their features remain for opt-in use and as reference plugins.
- **Add the ABI canary as the graduation gate.** The `binoc-abi-canary` crate is
  a real renderer exported via `export_plugin!` and built as a cdylib; its
  `tests/abi_crossing.rs` builds that cdylib, loads it over `libloading` (the
  same path `binoc-python` uses), and asserts description + render round-trip
  across the `extern "C"` + JSON boundary. It runs in CI via `cargo test` over
  the workspace. That crossing is the compiler/linker being forced to prove one
  honest crossing; if the renderer ABI line drifts, `export_plugin!` fails to
  compile or the crossing fails to load/round-trip. This restores, for the stable
  tier, the "can't cheat invisibly" property that the fat in-process build
  otherwise gives up — and it is the template each rule family must satisfy when
  it graduates into `plugin_abi`.

## Alternatives Considered

- **Many PyPI packages now ("bite the bullet" and automate publishing).**
  Rejected: the rule-family ABI does not exist, so these packages would not
  function; it would commit us to ABI stability we have explicitly deferred
  pre-1.0; and it imposes an N-package version-compatibility matrix that every
  core bump would break until the ABI is frozen.

- **`binoc[all]` extras now.** Rejected as a near-term mechanism: pip extras pull
  in *other Python packages*; they cannot toggle a Rust cargo feature. "Slim core
  + `binoc[all]`" is the many-package model under another name, and is deferred to
  the same graduation trigger.

- **Status quo (core only; format packs unavailable to users).** Rejected: it
  makes binoc markedly less useful for its actual audience, which was the point
  of building the packs.

- **A dedicated `binoc-bundle` aggregator crate.** Reasonable, but a new workspace
  member earns its keep only once a second consumer needs the bundle independently
  of `binoc-cli`. Until then the bundle lives with the existing optional-plugin
  seam in `binoc-cli`.

## Consequences

- The `binoc` wheel and its build pull in the format packs' dependencies (Arrow
  for Parquet/Avro, calamine for Excel, the geo stack for Shapefile). The
  per-format features and the slim build are the mitigation if any single
  dependency becomes too heavy to ship by default.
- Format packs remain in-process by design (per the tiered ADR); the fat build
  does not move that line. The canary is what keeps the *renderer* line
  compiler-enforced, and is the model for graduating rule families later.
