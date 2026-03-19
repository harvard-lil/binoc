"""Binoc: The missing changelog for datasets.

Generate changelogs for datasets that don't have them. Given snapshots of a
dataset downloaded at different times, Binoc detects what changed, expresses
changes as a minimal structured diff, and produces human-readable summaries.

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
"""

from binoc._binoc import (
    Config,
    DiffNode,
    Expand,
    Identical,
    ItemPair,
    Leaf,
    Changeset,
    PluginRegistry,
    Remove,
    Replace,
    ReplaceMany,
    Unchanged,
    diff,
    to_json,
    to_markdown,
)


class Comparator:
    """Base class for Python-authored comparators.

    Subclass this to create a custom comparator that integrates with the
    binoc pipeline. At minimum, set ``name`` and implement ``compare()``.

    Example::

        class FastaComparator(binoc.Comparator):
            name = "bio.fasta"
            extensions = [".fasta", ".fa"]

            def compare(self, pair):
                # Your comparison logic here
                return binoc.Leaf(binoc.DiffNode(
                    action="modify",
                    item_type="fasta",
                    path=pair.logical_path,
                ))

        config = binoc.Config.default()
        config.add_comparator(FastaComparator())
        changeset = binoc.diff("a", "b", config=config)
    """

    name: str = ""
    extensions: list[str] = []

    def can_handle(self, pair: ItemPair) -> bool:
        """Return True if this comparator can handle the given item pair.

        Override for imperative dispatch. For most comparators, setting
        ``extensions`` is sufficient and this method can be left as-is.
        """
        return False

    def compare(self, pair: ItemPair) -> "Identical | Leaf | Expand":
        """Compare an item pair and return a result.

        Must return one of:
        - ``Identical()`` — items are the same
        - ``Leaf(node)`` — terminal diff
        - ``Expand(node, children)`` — container with children to recurse into
        """
        raise NotImplementedError


class Transformer:
    """Base class for Python-authored transformers.

    Subclass this to create a custom transformer that rewrites the diff tree.

    Example::

        class Normalizer(binoc.Transformer):
            name = "myproject.normalizer"
            match_tags = ["myproject.raw"]

            def transform(self, node):
                return binoc.Replace(node.with_tag("myproject.normalized"))

        config = binoc.Config.default()
        config.add_transformer(Normalizer())
    """

    name: str = ""
    match_types: list[str] = []
    match_tags: list[str] = []
    match_actions: list[str] = []

    def can_handle(self, node: DiffNode) -> bool:
        """Return True if this transformer should process the given node.

        Override for imperative matching. For most transformers, setting
        ``match_types``, ``match_tags``, or ``match_actions`` is sufficient.
        """
        return False

    def transform(self, node: DiffNode) -> "Unchanged | Replace | ReplaceMany | Remove":
        """Rewrite a matched node.

        Must return one of:
        - ``Unchanged()`` — no change
        - ``Replace(node)`` — replace with new node
        - ``ReplaceMany(nodes)`` — replace with multiple nodes
        - ``Remove()`` — delete this node
        """
        raise NotImplementedError


__all__ = [
    "diff",
    "to_json",
    "to_markdown",
    "DiffNode",
    "Changeset",
    "Config",
    "PluginRegistry",
    "ItemPair",
    "Identical",
    "Leaf",
    "Expand",
    "Unchanged",
    "Replace",
    "ReplaceMany",
    "Remove",
    "Comparator",
    "Transformer",
]
