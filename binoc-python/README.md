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

Most first-party format packs are bundled into the `binoc` wheel. Separately
published plugins are discovered automatically through Python entry points;
native rule-pack publishing is paused until that plugin surface graduates.
