# binoc-sdk

`binoc-sdk` is the stable Rust surface for writing Binoc plugins.

It contains:

- plugin traits
- IR and result types
- `DataAccess`
- artifact descriptors and codec-facing types
- the native plugin ABI
- the `export_plugin!` macro

If you are writing a Rust rule pack or renderer for Binoc, depend on `binoc-sdk`, not `binoc-core`.
