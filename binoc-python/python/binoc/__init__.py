"""Binoc: the missing changelog for datasets.

Binoc generates changelogs for datasets that don't ship with them. Given
snapshots of a dataset downloaded at different times, Binoc detects what
changed, expresses changes as a minimal structured diff (the :class:`Changeset`
/ :class:`DiffNode` tree), and renders changes as JSON or Markdown.

This module is the top-level Python API. Every symbol listed in
``binoc.__all__`` is considered public and is documented on this page.

Quick start::

    import binoc

    changeset = binoc.diff("snapshots/2024-03", "snapshots/2024-06")
    print(changeset)

    # Inspect the diff tree
    for child in changeset.root:
        print(f"{child.path}: {child.action}")

    # Serialize
    json_str = changeset.to_json()
    markdown = binoc.to_markdown([changeset])

Writing plugins:
    Python supports embedding, rendering, and dataset configuration. Parser
    and rewrite rule authoring is Rust-only until the correspondence rule ABI
    lands.

Test-vector helpers for plugin authors live in :mod:`binoc.testing`.
"""

from binoc._binoc import (
    Changeset,
    Config,
    DiffNode,
    ItemPair,
    PluginRegistry,
    diff,
    to_json,
    to_markdown,
)

__all__ = [
    'Changeset',
    'Config',
    'DiffNode',
    'ItemPair',
    'PluginRegistry',
    'diff',
    'to_json',
    'to_markdown',
]
