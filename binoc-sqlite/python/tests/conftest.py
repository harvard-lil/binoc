from pathlib import Path

import pytest

import binoc
import binoc_sqlite

VECTORS_DIR = Path(__file__).resolve().parent.parent.parent / "test-vectors"


@pytest.fixture
def registry():
    """PluginRegistry with standard plugins + the SQLite comparator."""
    r = binoc.PluginRegistry.default()
    r.register_comparator("binoc-sqlite.sqlite", binoc_sqlite.SqliteComparator())
    return r
