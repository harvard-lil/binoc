# binoc

`binoc` is the primary Python distribution for Binoc.

It provides:

- the `binoc` CLI
- Python bindings over the core diff engine
- native plugin discovery and loading
- the standard correspondence rule pack, Markdown renderer, and bundled
  first-party format packs

Install it with:

```bash
pip install binoc
```

Separate PyPI publishing for `binoc-sqlite` and `binoc-stat-binary` is paused.
`binoc-stat-binary` ships in the default fat wheel; SQLite remains an in-tree
opt-in rule pack excluded from the default bundled feature set.
