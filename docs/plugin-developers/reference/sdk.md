---
audience: Rust plugin author
---

# Rust SDK (`binoc-sdk`)

The canonical Rust API reference is the rustdoc output for the
`binoc-sdk` crate, served on this site as a static subpath:

[**Browse the `binoc_sdk` rustdoc →**](../../sdk/binoc_sdk/index.html)

It is regenerated from source by `just docs-sdk` (which wraps
`cargo doc --no-deps --package binoc-sdk`) and rebuilt in CI on every
docs deploy, so it tracks `main`. For a published-version view, see
[`binoc-sdk` on docs.rs](https://docs.rs/binoc-sdk); for a local
checkout, `cargo doc --no-deps --package binoc-sdk --open` opens the
same content in your browser.

## The published Rust surface

`binoc-sdk` is the only Rust crate published to crates.io. It contains
everything a third-party Rust plugin needs:

- Plugin traits: `Comparator`, `Transformer`, `Renderer`.
- IR types: `DiffNode`, `ItemRef`, `ItemPair`, `CompareResult`,
  `TransformResult`.
- Data access: the `DataAccess` trait.
- Descriptors: `ComparatorDescriptor`, `TransformerDescriptor`.
- Artifact types: `ArtifactFormat`, `ArtifactSubject`,
  `ArtifactDescriptor`, `tabular_v1`, and helpers for publishing /
  consuming artifacts.
- The `export_plugin!` macro that generates all C ABI glue and the
  PyO3 stub required by maturin.

Internal crates (`binoc-core`, `binoc-stdlib`, `binoc-cli`,
`binoc-python`) are not published and do not have hosted reference
docs. This is deliberate — see
[Release surface and automated publishing ADR](../../adr/2026-04-08-release_surface_and_automated_publishing.md)
and [Plugin SDK and ABI ADR](../../adr/2026-03-12-plugin_sdk_and_abi.md).

## Depending on it

```toml
[dependencies]
binoc-sdk = "0.1"
```

or

```bash
cargo add binoc-sdk
```

For a worked end-to-end example, see
[Write a Rust comparator](../howto/write-a-rust-comparator.md) and the
reference implementation at
[`model-plugins/binoc-sqlite`](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite).
