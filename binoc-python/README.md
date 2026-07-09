# binoc

`binoc` is the primary Python distribution for Binoc.

It provides:

- the `binoc` CLI
- Python bindings over the core diff engine
- native plugin discovery and loading
- the standard correspondence rule pack and Markdown renderer

Install it with:

```bash
pip install binoc
```

Optional format-specific plugins, such as `binoc-sqlite`, install separately and are discovered automatically through Python entry points.
