# H Landing Notes

## Merge

- `git merge main` reported `Already up to date`; this branch already contains
  the fat-binoc `main` history through `bc9a611` / PR #122:
  - `ad47b99` records the fat-binoc distribution ADR.
  - `6e02697` bundles first-party format packs into the fat wheel and adds the
    ABI canary.
  - `e6b32e4` pauses separate `binoc-sqlite` and `binoc-stat-binary` publishing.
  - `fabaf18` bumps binoc to `0.2.0`.

## Registry And Catalog

- `plugin_registry.json` is now the single source of truth for both generated
  plugin reference pages.
- `third_party_plugins.json` was removed.
- Registry entries now carry a `tier`:
  - `built-in` for `binoc-stdlib`.
  - `first-party-bundled` for format packs included in the fat `binoc` wheel.
  - `first-party-opt-in` for `binoc-sqlite`, which remains in-tree but excluded
    from the default `binoc-cli` `bundled` feature set.
  - `first-party-add-on` for in-repo packages distributed outside the default
    fat wheel.
- `binoc-sqlite` and `binoc-stat-binary` no longer advertise PyPI packages.
  `binoc-stat-binary` is described as bundled in the fat wheel; SQLite is
  described as opt-in/source-build only.
- Generated docs now label Rust packages as Rust crates rather than crates.io
  publications, because the in-tree model plugin crates are `publish = false`.

## Docs And Generated Output

- Regenerated docs with `UV_NO_CACHE=1 just docs`.
- `docs/users/reference/plugin-registry.md` and
  `docs/users/reference/third-party-plugins.md` moved because both now render
  from `plugin_registry.json` and include tier/distribution rows.
- `docs/users/explanation/test-vectors-gallery.md` moved because the uncached
  docs run picked up the current renderer output:
  - structured-document examples now show concrete value paths such as
    `$.replicas: 3 -> 5`;
  - the custom suppression sentinel vector now includes
    `Suppressed 2 cells in 'count'`.
- No committed insta snapshot files moved in this capstone.

## Deferred Review Findings Not Fixed Here

- Identity model fragmentation: precomputed maps vs. `DispatchResolver` vs.
  `ItemRef.tabular_parse`; `RowIdentity` policy fields should be `Option<T>`;
  `TabularParseConfig` on `ItemRef` still contradicts the per-path ADR's
  "schema owned entirely by stdlib" line.
- Suppression sentinels are still hardcoded in the writer's tagger while also
  configurable in the rule; the default remains defined in multiple places.
- Probe paths still swallow parse/walk errors with `let Ok(..) else continue`.
- Binary CDC still swallows IO errors via `.ok()?`.
- Core hardening remains: unvalidated `LinkProposal` indices can panic; no
  expansion depth backstop; `cost()` charges nothing for verb names;
  `merge_action` collapses unknown actions to `"identical"`.
- Plugin story remains partially deferred: `binoc[all]` still contradicts the
  fat-binoc ADR, and `diff(registry=...)` is still documented as a parameter
  that is never read. The stale `install-and-use-plugins.md` paused-wheel
  install example was fixed here to satisfy the landing gate.
