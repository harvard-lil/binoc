"""Tests for DiffNode and Changeset construction, traversal, and serialization."""

import json

import binoc


class TestDiffNode:
    def test_basic_construction(self):
        node = binoc.DiffNode('modify', 'file', 'data.csv')
        assert node.action == 'modify'
        assert node.item_type == 'file'
        assert node.path == 'data.csv'
        assert node.source_path is None
        assert node.tags == []
        assert node.children == []
        assert len(node) == 0

    def test_construction_with_all_fields(self):
        child = binoc.DiffNode('add', 'file', 'child.txt')
        node = binoc.DiffNode(
            'modify',
            'directory',
            'root',
            source_path='old/root',
            tags=['binoc.test', 'binoc.other'],
            details={'count': 42, 'name': 'test'},
            children=[child],
        )
        assert node.source_path == 'old/root'
        assert set(node.tags) == {'binoc.test', 'binoc.other'}
        assert node.details['count'] == 42
        assert node.details['name'] == 'test'
        assert len(node) == 1
        assert node.children[0].path == 'child.txt'

    def test_tags_from_set(self):
        node = binoc.DiffNode('add', 'file', 'f', tags={'tag-a', 'tag-b'})
        assert set(node.tags) == {'tag-a', 'tag-b'}

    def test_with_tag(self):
        node = binoc.DiffNode('modify', 'file', 'f')
        tagged = node.with_tag('binoc.test')
        assert 'binoc.test' in tagged.tags
        assert 'binoc.test' not in node.tags

    def test_with_children(self):
        child = binoc.DiffNode('add', 'file', 'a.txt')
        node = binoc.DiffNode('modify', 'directory', 'dir')
        with_children = node.with_children([child])
        assert len(with_children) == 1
        assert len(node) == 0

    def test_with_detail(self):
        node = binoc.DiffNode('modify', 'file', 'f')
        updated = node.with_detail('key', 'value')
        assert updated.details['key'] == 'value'

    def test_with_source_path(self):
        node = binoc.DiffNode('move', 'file', 'new.txt')
        moved = node.with_source_path('old.txt')
        assert moved.source_path == 'old.txt'

    def test_node_count(self):
        tree = binoc.DiffNode(
            'modify',
            'directory',
            'root',
            children=[
                binoc.DiffNode('add', 'file', 'a.txt'),
                binoc.DiffNode(
                    'modify',
                    'directory',
                    'sub',
                    children=[binoc.DiffNode('remove', 'file', 'b.txt')],
                ),
            ],
        )
        assert tree.node_count() == 4

    def test_all_tags(self):
        tree = binoc.DiffNode(
            'modify',
            'dir',
            'root',
            tags=['root-tag'],
            children=[
                binoc.DiffNode('add', 'file', 'a', tags=['child-tag']),
            ],
        )
        all_tags = set(tree.all_tags())
        assert all_tags == {'root-tag', 'child-tag'}

    def test_find_node(self):
        tree = binoc.DiffNode(
            'modify',
            'dir',
            'root',
            children=[
                binoc.DiffNode('add', 'file', 'root/a.txt'),
                binoc.DiffNode(
                    'modify',
                    'dir',
                    'root/sub',
                    children=[binoc.DiffNode('remove', 'file', 'root/sub/b.txt')],
                ),
            ],
        )
        found = tree.find_node('root/sub/b.txt')
        assert found is not None
        assert found.action == 'remove'
        assert tree.find_node('nonexistent') is None

    def test_to_dict(self):
        node = binoc.DiffNode(
            'modify',
            'file',
            'data.csv',
            tags=['binoc.test'],
            details={'lines': 10},
        )
        d = node.to_dict()
        assert d['action'] == 'modify'
        assert d['item_type'] == 'file'
        assert d['path'] == 'data.csv'
        assert 'binoc.test' in d['tags']
        assert d['details']['lines'] == 10
        assert isinstance(d['children'], list)

    def test_to_json(self):
        node = binoc.DiffNode('add', 'file', 'new.txt')
        j = node.to_json()
        parsed = json.loads(j)
        assert parsed['action'] == 'add'
        assert parsed['path'] == 'new.txt'

    def test_indexing(self):
        tree = binoc.DiffNode(
            'modify',
            'dir',
            'root',
            children=[
                binoc.DiffNode('add', 'file', 'a.txt'),
                binoc.DiffNode('remove', 'file', 'b.txt'),
            ],
        )
        assert tree[0].path == 'a.txt'
        assert tree[1].path == 'b.txt'
        assert tree[-1].path == 'b.txt'

    def test_indexing_out_of_range(self):
        node = binoc.DiffNode('add', 'file', 'f')
        try:
            node[0]
            assert False, 'should have raised IndexError'
        except IndexError:
            pass

    def test_iteration(self):
        tree = binoc.DiffNode(
            'modify',
            'dir',
            'root',
            children=[
                binoc.DiffNode('add', 'file', 'a.txt'),
                binoc.DiffNode('remove', 'file', 'b.txt'),
            ],
        )
        paths = [child.path for child in tree]
        assert paths == ['a.txt', 'b.txt']

    def test_repr_and_str(self):
        node = binoc.DiffNode('modify', 'file', 'data.csv')
        assert 'modify' in repr(node)
        assert 'data.csv' in repr(node)
        assert 'modify' in str(node)

    def test_bool(self):
        node = binoc.DiffNode('add', 'file', 'f')
        assert bool(node) is True


class TestChangeset:
    def test_construction_no_root(self):
        m = binoc.Changeset('v1', 'v2')
        assert m.from_snapshot == 'v1'
        assert m.to_snapshot == 'v2'
        assert m.root is None
        assert m.node_count == 0
        assert bool(m) is False

    def test_construction_with_root(self):
        root = binoc.DiffNode('modify', 'dir', 'root')
        m = binoc.Changeset('v1', 'v2', root)
        assert m.root is not None
        assert m.root.action == 'modify'
        assert m.node_count == 1
        assert bool(m) is True

    def test_find_node(self):
        root = binoc.DiffNode(
            'modify',
            'dir',
            'root',
            children=[binoc.DiffNode('add', 'file', 'root/a.txt')],
        )
        m = binoc.Changeset('v1', 'v2', root)
        found = m.find_node('root/a.txt')
        assert found is not None
        assert found.action == 'add'

    def test_json_round_trip(self):
        root = binoc.DiffNode(
            'modify',
            'file',
            'data.csv',
            tags=['binoc.content-changed'],
            details={'lines': 42},
        )
        m = binoc.Changeset('v1', 'v2', root)
        j = m.to_json()
        restored = binoc.Changeset.from_json(j)
        assert restored.from_snapshot == 'v1'
        assert restored.to_snapshot == 'v2'
        assert restored.root.path == 'data.csv'
        assert 'binoc.content-changed' in restored.root.tags

    def test_save_and_load(self, tmp_path):
        root = binoc.DiffNode('modify', 'file', 'data.csv')
        m = binoc.Changeset('v1', 'v2', root)
        path = str(tmp_path / 'changeset.json')
        m.save(path)
        loaded = binoc.Changeset.from_file(path)
        assert loaded.from_snapshot == 'v1'
        assert loaded.root.path == 'data.csv'

    def test_to_dict(self):
        root = binoc.DiffNode('add', 'file', 'f.txt')
        m = binoc.Changeset('a', 'b', root)
        d = m.to_dict()
        assert d['from_snapshot'] == 'a'
        assert d['to_snapshot'] == 'b'
        assert d['root']['action'] == 'add'

    def test_repr_and_str(self):
        m = binoc.Changeset('v1', 'v2')
        assert 'v1' in repr(m)
        assert 'no changes' in str(m)
