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
    Subclass :class:`Comparator` to parse a new file format into the IR, or
    subclass :class:`Transformer` to rewrite the diff tree. Register them on
    a :class:`Config` with :meth:`Config.add_comparator` /
    :meth:`Config.add_transformer`, or on a :class:`PluginRegistry` for
    reuse across multiple diffs and for distribution as an entry point.

Test-vector helpers for plugin authors live in :mod:`binoc.testing`.
"""

from binoc._binoc import (
    Changeset,
    Config,
    DiffNode,
    Expand,
    Identical,
    ItemPair,
    Leaf,
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

    A comparator is the parser layer of binoc: it takes an :class:`ItemPair`
    and decides whether the two sides are semantically identical, whether
    they differ (and how), and — for container formats — what child items
    the controller should recursively diff next.

    Subclass this and set the class attributes listed below, then implement
    :meth:`compare`. Override :meth:`can_handle` only if declarative
    dispatch by ``extensions`` is not enough.

    Attributes:
        name: Dispatch name / registry key for this comparator, e.g.
            ``"bio.fasta"``. Plugins should namespace by package.
        extensions: File extensions (with leading ``.``) this comparator
            claims. Declarative dispatch: first comparator to claim an
            item wins. Ordering is a :class:`Config` concern.

    Example::

        class FastaComparator(binoc.Comparator):
            name = "bio.fasta"
            extensions = [".fasta", ".fa"]

            def compare(self, pair):
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
        """Return ``True`` if this comparator can handle *pair*.

        Declarative dispatch by ``extensions`` is the normal path; this is
        the imperative escape hatch. For most comparators, setting
        :attr:`extensions` is sufficient and this method can be left alone.
        """
        return False

    def compare(self, pair: ItemPair) -> "Identical | Leaf | Expand":
        """Compare an :class:`ItemPair` and return a result variant.

        Must return one of:

        - :class:`Identical` — items are semantically the same; produce no
          diff node.
        - :class:`Leaf` — terminal diff node; the controller will not
          recurse into it.
        - :class:`Expand` — container diff node plus the child
          :class:`ItemPair` s to recurse into.

        Raises :class:`NotImplementedError` if a subclass forgets to
        implement it.
        """
        raise NotImplementedError


class Transformer:
    """Base class for Python-authored transformers.

    A transformer is an optimization / normalization pass over the diff
    tree: it rewrites :class:`DiffNode` s after all comparators have run
    but before rendering. Transformers operate only on the IR — they do
    not have access to the raw snapshot data.

    Subclass this, set the dispatch filters, and implement :meth:`transform`.

    Attributes:
        name: Dispatch name / registry key for this transformer.
        match_types: If non-empty, only call :meth:`transform` on nodes
            whose :attr:`~DiffNode.item_type` is in this list.
        match_tags: If non-empty, only call :meth:`transform` on nodes
            carrying at least one of these tags.
        match_actions: If non-empty, only call :meth:`transform` on nodes
            whose :attr:`~DiffNode.action` is in this list.
        node_shape: Dispatch filter on node shape — one of ``"any"``
            (default), ``"container"`` (only nodes with children), or
            ``"leaf"`` (only childless nodes).

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
    node_shape: str = "any"

    def can_handle(self, node: DiffNode) -> bool:
        """Return ``True`` if this transformer should process *node*.

        Imperative escape hatch for cases where the declarative filters
        (``match_types`` / ``match_tags`` / ``match_actions`` /
        ``node_shape``) cannot express the match.
        """
        return False

    def transform(self, node: DiffNode) -> "Unchanged | Replace | ReplaceMany | Remove":
        """Rewrite a matched :class:`DiffNode` and return a result variant.

        Must return one of:

        - :class:`Unchanged` — leave the node alone.
        - :class:`Replace` — replace the node with one new node.
        - :class:`ReplaceMany` — replace the node with zero or more nodes.
        - :class:`Remove` — drop the node from the tree entirely.
        """
        raise NotImplementedError


__all__ = [
    "Changeset",
    "Comparator",
    "Config",
    "DiffNode",
    "Expand",
    "Identical",
    "ItemPair",
    "Leaf",
    "PluginRegistry",
    "Remove",
    "Replace",
    "ReplaceMany",
    "Transformer",
    "Unchanged",
    "diff",
    "to_json",
    "to_markdown",
]
