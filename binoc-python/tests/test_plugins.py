"""Tests for Python-authored comparators and transformers."""

import binoc


class TestPythonComparator:
    def test_leaf_comparator(self, snapshot_pair):
        """A Python comparator that returns a Leaf result."""

        class AlwaysModified(binoc.Comparator):
            name = "test.always_modified"
            extensions = [".txt"]

            def compare(self, pair):
                return binoc.Leaf(
                    binoc.DiffNode(
                        "modify",
                        "custom",
                        pair.logical_path,
                        tags=["test.custom-diff"],
                        details={"source": "python"},
                    )
                )

        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config(
            comparators=["binoc.directory"],
            transformers=[],
        )
        config.add_comparator(AlwaysModified())
        migration = binoc.diff(a, b, config=config)
        assert migration.root is not None
        all_tags = migration.root.all_tags()
        assert "test.custom-diff" in all_tags

    def test_identical_comparator(self, snapshot_pair):
        """A Python comparator that returns Identical."""

        class AlwaysIdentical(binoc.Comparator):
            name = "test.always_identical"
            extensions = [".txt"]

            def compare(self, pair):
                return binoc.Identical()

        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config(
            comparators=["binoc.directory", "binoc.binary"],
        )
        config.add_comparator(AlwaysIdentical())
        migration = binoc.diff(a, b, config=config)
        # The txt files should be seen as identical since our comparator says so.
        # The directory may still have a root node but no text-file children.
        if migration.root is not None:
            all_tags = migration.root.all_tags()
            assert "test.custom-diff" not in all_tags

    def test_can_handle_comparator(self, snapshot_pair):
        """A Python comparator that uses can_handle for dispatch."""

        class SpecialHandler(binoc.Comparator):
            name = "test.special"

            def can_handle(self, pair):
                return "story" in pair.logical_path

            def compare(self, pair):
                return binoc.Leaf(
                    binoc.DiffNode(
                        "modify",
                        "special",
                        pair.logical_path,
                        tags=["test.special-handled"],
                    )
                )

        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config(
            comparators=["binoc.directory"],
        )
        config.add_comparator(SpecialHandler())
        migration = binoc.diff(a, b, config=config)
        assert migration.root is not None
        all_tags = migration.root.all_tags()
        assert "test.special-handled" in all_tags


class TestPythonTransformer:
    def test_replace_transformer(self, snapshot_pair):
        """A Python transformer that tags matched nodes."""

        class Tagger(binoc.Transformer):
            name = "test.tagger"
            match_kinds = ["modify"]

            def transform(self, node):
                return binoc.Replace(node.with_tag("test.tagged-by-python"))

        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config.default()
        config.add_transformer(Tagger())
        migration = binoc.diff(a, b, config=config)
        assert migration.root is not None
        all_tags = migration.root.all_tags()
        assert "test.tagged-by-python" in all_tags

    def test_unchanged_transformer(self, snapshot_pair):
        """A transformer that returns Unchanged passes nodes through."""

        class NoOp(binoc.Transformer):
            name = "test.noop"
            match_kinds = ["modify"]

            def transform(self, node):
                return binoc.Unchanged()

        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config.default()
        config.add_transformer(NoOp())
        migration = binoc.diff(a, b, config=config)
        assert migration.root is not None
        all_tags = migration.root.all_tags()
        assert "test.tagged-by-python" not in all_tags

    def test_remove_transformer(self, snapshot_pair):
        """A transformer that removes matched nodes."""

        class Remover(binoc.Transformer):
            name = "test.remover"
            match_types = ["file"]

            def transform(self, node):
                return binoc.Remove()

        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config.default()
        config.add_transformer(Remover())
        binoc.diff(a, b, config=config)

    def test_match_by_tag_transformer(self, snapshot_pair):
        """A transformer that matches by tag."""

        class TagMatcher(binoc.Transformer):
            name = "test.tag_matcher"
            match_tags = ["binoc.cell-change"]

            def transform(self, node):
                return binoc.Replace(node.with_tag("test.saw-content-change"))

        a, b = snapshot_pair("csv-cell-changes")
        config = binoc.Config.default()
        config.add_transformer(TagMatcher())
        migration = binoc.diff(a, b, config=config)
        assert migration.root is not None
        all_tags = migration.root.all_tags()
        assert "test.saw-content-change" in all_tags

    def test_match_by_type_transformer(self, snapshot_pair):
        """A transformer that matches by item_type."""

        class TypeMatcher(binoc.Transformer):
            name = "test.type_matcher"
            match_types = ["directory"]

            def transform(self, node):
                return binoc.Replace(node.with_tag("test.saw-directory"))

        a, b = snapshot_pair("directory-nested")
        config = binoc.Config.default()
        config.add_transformer(TypeMatcher())
        migration = binoc.diff(a, b, config=config)
        assert migration.root is not None
        all_tags = migration.root.all_tags()
        assert "test.saw-directory" in all_tags


class TestItemPair:
    def test_both(self):
        pair = binoc.ItemPair.both("/tmp/a.csv", "/tmp/b.csv", "a.csv", "b.csv")
        assert pair.left_path == "/tmp/a.csv"
        assert pair.right_path == "/tmp/b.csv"
        assert pair.logical_path == "b.csv"
        assert pair.extension == ".csv"

    def test_added(self):
        pair = binoc.ItemPair.added("/tmp/new.txt", "new.txt")
        assert pair.left_path is None
        assert pair.right_path == "/tmp/new.txt"
        assert pair.logical_path == "new.txt"

    def test_removed(self):
        pair = binoc.ItemPair.removed("/tmp/old.txt", "old.txt")
        assert pair.right_path is None
        assert pair.left_path == "/tmp/old.txt"
        assert pair.logical_path == "old.txt"


class TestResultTypes:
    def test_identical(self):
        r = binoc.Identical()
        assert "Identical" in repr(r)

    def test_leaf(self):
        node = binoc.DiffNode("add", "file", "f.txt")
        r = binoc.Leaf(node)
        assert r.node.path == "f.txt"
        assert "Leaf" in repr(r)

    def test_expand(self):
        node = binoc.DiffNode("modify", "dir", "d")
        children = [binoc.ItemPair.both("/a", "/b", "a", "b")]
        r = binoc.Expand(node, children)
        assert r.node.path == "d"
        assert len(r.children) == 1
        assert "Expand" in repr(r)

    def test_unchanged(self):
        r = binoc.Unchanged()
        assert "Unchanged" in repr(r)

    def test_replace(self):
        node = binoc.DiffNode("add", "file", "f")
        r = binoc.Replace(node)
        assert r.node.path == "f"
        assert "Replace" in repr(r)

    def test_replace_many(self):
        nodes = [
            binoc.DiffNode("add", "file", "a"),
            binoc.DiffNode("add", "file", "b"),
        ]
        r = binoc.ReplaceMany(nodes)
        assert len(r.nodes) == 2
        assert "ReplaceMany" in repr(r)

    def test_remove(self):
        r = binoc.Remove()
        assert "Remove" in repr(r)
