---
audience: Python plugin author, Python API consumer
---

# Python API

The public Python API lives in the top-level `binoc` package. Every symbol
on this page is reachable as `binoc.<name>` and is listed in
`binoc.__all__`; private names (anything starting with `_`) are
deliberately omitted. The page below is rendered directly from the
installed package's docstrings by
[`mkdocstrings[python]`](https://mkdocstrings.github.io/python/) — see the
[Documentation platform ADR](../adr/2026-04-17-documentation_platform_and_info_design.md#4-reference-is-generated-not-written).

## Limitations of the Python surface

Python comparators, transformers, and renderers receive a deliberately
simplified interface compared to Rust plugins:

- No `DataAccess`. Python comparators get physical file paths on
  `ItemPair`, not the trait object. They cannot
  publish artifacts or call `workspace()` for scratch space.
- No `content_hash` or `media_type` on `ItemPair`.
- No `source_items` on Python transformers — they operate on the
  `DiffNode` tree only, and cannot re-read the raw
  snapshot data.

For plugins that need those capabilities, write a Rust plugin. See
[Write a Rust comparator](../howto/write-a-rust-comparator.md) and the
[Rust SDK reference](sdk.md).

For worked Python examples, see:

- [Write a Python comparator](../howto/write-a-python-comparator.md)
- [Write a Python transformer](../howto/write-a-python-transformer.md)
- [Write a Python renderer](../howto/write-a-python-renderer.md)

## `binoc`

::: binoc
    options:
      show_root_heading: true
      show_root_full_path: false
      show_if_no_docstring: false
      members_order: source

## `binoc.testing`

Test-vector helpers for plugin authors. Separate submodule; import as
`from binoc.testing import discover_vectors, run_vector`.

::: binoc.testing
    options:
      show_root_heading: true
      show_root_full_path: false
      show_if_no_docstring: false
      members_order: source
