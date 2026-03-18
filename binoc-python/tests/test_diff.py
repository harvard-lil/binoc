"""Tests for the diff function using shared test vectors."""

import binoc


class TestDiffWithTestVectors:
    def test_trivial_identical(self, snapshot_pair):
        a, b = snapshot_pair("trivial-identical")
        changeset = binoc.diff(a, b)
        if changeset.root is not None:
            assert len(changeset.root.children) == 0

    def test_single_file_add(self, snapshot_pair):
        a, b = snapshot_pair("single-file-add")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        changeset.root.all_tags()
        kinds = _collect_kinds(changeset.root)
        assert "add" in kinds

    def test_single_file_remove(self, snapshot_pair):
        a, b = snapshot_pair("single-file-remove")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        kinds = _collect_kinds(changeset.root)
        assert "remove" in kinds

    def test_single_file_modify_text(self, snapshot_pair):
        a, b = snapshot_pair("single-file-modify-text")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        kinds = _collect_kinds(changeset.root)
        assert "modify" in kinds

    def test_single_file_modify_binary(self, snapshot_pair):
        a, b = snapshot_pair("single-file-modify-binary")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None

    def test_csv_column_reorder(self, snapshot_pair):
        a, b = snapshot_pair("csv-column-reorder")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        all_tags = changeset.root.all_tags()
        assert "binoc.column-reorder" in all_tags

    def test_csv_row_addition(self, snapshot_pair):
        a, b = snapshot_pair("csv-row-addition")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        all_tags = changeset.root.all_tags()
        assert "binoc.row-addition" in all_tags

    def test_csv_column_addition(self, snapshot_pair):
        a, b = snapshot_pair("csv-column-addition")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        all_tags = changeset.root.all_tags()
        assert "binoc.column-addition" in all_tags

    def test_csv_column_removal(self, snapshot_pair):
        a, b = snapshot_pair("csv-column-removal")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        all_tags = changeset.root.all_tags()
        assert "binoc.column-removal" in all_tags

    def test_csv_cell_changes(self, snapshot_pair):
        a, b = snapshot_pair("csv-cell-changes")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        all_tags = changeset.root.all_tags()
        assert "binoc.cell-change" in all_tags

    def test_csv_mixed_changes(self, snapshot_pair):
        a, b = snapshot_pair("csv-mixed-changes")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        assert changeset.node_count > 1

    def test_directory_file_move(self, snapshot_pair):
        a, b = snapshot_pair("directory-file-move")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        kinds = _collect_kinds(changeset.root)
        assert "move" in kinds or ("add" in kinds and "remove" in kinds)

    def test_directory_nested(self, snapshot_pair):
        a, b = snapshot_pair("directory-nested")
        changeset = binoc.diff(a, b)
        assert changeset.root is not None
        assert changeset.node_count > 1


class TestDiffOutput:
    def test_to_json(self, snapshot_pair):
        a, b = snapshot_pair("single-file-modify-text")
        changeset = binoc.diff(a, b)
        json_str = binoc.to_json(changeset)
        assert '"from_snapshot"' in json_str
        assert '"to_snapshot"' in json_str

    def test_to_markdown(self, snapshot_pair):
        a, b = snapshot_pair("csv-column-addition")
        changeset = binoc.diff(a, b)
        md = binoc.to_markdown([changeset])
        assert "Changelog" in md or "Changes" in md

    def test_to_markdown_with_config(self, snapshot_pair):
        a, b = snapshot_pair("csv-column-addition")
        changeset = binoc.diff(a, b)
        config = binoc.Config.default()
        md = binoc.to_markdown([changeset], config=config)
        assert len(md) > 0


class TestDiffConfig:
    def test_default_config(self, snapshot_pair):
        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config.default()
        changeset = binoc.diff(a, b, config=config)
        assert changeset.root is not None

    def test_custom_comparators(self, snapshot_pair):
        a, b = snapshot_pair("single-file-modify-text")
        config = binoc.Config(
            comparators=["binoc.directory", "binoc.text", "binoc.binary"],
            transformers=[],
        )
        changeset = binoc.diff(a, b, config=config)
        assert changeset.root is not None

    def test_config_repr(self):
        config = binoc.Config.default()
        r = repr(config)
        assert "Config" in r
        assert "binoc.csv" in r


def _collect_kinds(node):
    """Recursively collect all 'kind' values from a diff tree."""
    kinds = {node.kind}
    for child in node:
        kinds |= _collect_kinds(child)
    return kinds
