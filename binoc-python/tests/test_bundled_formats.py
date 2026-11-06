"""The installed wheel handles bundled format packs, through both the Python
API and the console script.

Format-pack correctness is covered by each pack's Rust tests, and the bundled
CLI runs every pack's vectors in binoc-cli/tests/bundled_formats.rs. This
checks the wheel itself: that the Python host registers the bundled packs.
Fixtures here need only the standard library, so this covers SQLite (built
with sqlite3) and XML; a wheel missing the bundle falls back to byte or line
comparison and loses these tags.
"""

import json
from pathlib import Path
import sqlite3
import subprocess
import sys

import binoc
import pytest

PLUGINS = Path(__file__).resolve().parent.parent.parent / 'model-plugins'


def _sqlite_pair(tmp_path):
    for side, rows in (('a', [(1, 'Alpha')]), ('b', [(1, 'Alpha'), (2, 'Beta')])):
        (tmp_path / side).mkdir()
        con = sqlite3.connect(tmp_path / side / 'data.sqlite')
        con.execute('create table t (id integer primary key, name text)')
        con.executemany('insert into t values (?, ?)', rows)
        con.commit()
        con.close()
    return str(tmp_path / 'a'), str(tmp_path / 'b'), 'binoc.row-addition'


def _vector_pair(plugin, vector, tag):
    def make(_tmp_path):
        base = PLUGINS / plugin / 'test-vectors' / vector
        return str(base / 'snapshot-a'), str(base / 'snapshot-b'), tag

    return make


CASES = {
    'sqlite': _sqlite_pair,
    'xml': _vector_pair(
        'binoc-xml', 'xml-socrata-rows-row-addition', 'binoc.document-value-change'
    ),
}


@pytest.mark.parametrize('pack', sorted(CASES))
def test_python_api_uses_bundled_pack(pack, tmp_path):
    a, b, tag = CASES[pack](tmp_path)
    changeset = binoc.diff(a, b)
    assert changeset.root is not None
    assert tag in changeset.root.all_tags()


@pytest.mark.parametrize('pack', sorted(CASES))
def test_console_script_uses_bundled_pack(pack, tmp_path):
    a, b, tag = CASES[pack](tmp_path)
    result = subprocess.run(
        [sys.executable, '-m', 'binoc', 'diff', a, b, '--format', 'json'],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    tags = set()

    def collect(node):
        tags.update(node.get('tags', []))
        for child in node.get('children', []):
            collect(child)

    collect(json.loads(result.stdout)['root'])
    assert tag in tags
