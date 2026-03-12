"""Auto-discovered test vectors for the binoc-sqlite plugin.

Each test vector directory under test-vectors/ with a manifest.toml,
snapshot-a/, and snapshot-b/ is run through the full Python stack:
Python wrapper -> PyO3 bridge -> Rust comparator.
"""

import shutil
import sqlite3
from pathlib import Path

import pytest

from binoc.testing import discover_vectors, run_vector

VECTORS_DIR = Path(__file__).resolve().parent.parent.parent / "test-vectors"
VECTORS = discover_vectors(VECTORS_DIR)


def _prepare_sqlite(snap_a: Path, snap_b: Path) -> None:
    """Build .sqlite/.db files from .sqlite.d/.db.d directories."""
    _build_sqlite_in_dir(snap_a)
    _build_sqlite_in_dir(snap_b)


def _build_sqlite_in_dir(dir_path: Path) -> None:
    for entry in sorted(dir_path.rglob("*")):
        if entry.is_dir() and (
            entry.name.endswith(".sqlite.d") or entry.name.endswith(".db.d")
        ):
            db_path = entry.parent / entry.name.removesuffix(".d")
            _create_sqlite(entry, db_path)
            shutil.rmtree(entry)


def _create_sqlite(source_dir: Path, db_path: Path) -> None:
    conn = sqlite3.connect(db_path)
    for sql_file in sorted(source_dir.glob("*.sql")):
        conn.executescript(sql_file.read_text())
    conn.close()


@pytest.mark.parametrize("vector_dir", VECTORS, ids=[v.name for v in VECTORS])
def test_vector(vector_dir, registry):
    run_vector(
        vector_dir,
        vectors_root=VECTORS_DIR,
        registry=registry,
        prepare=_prepare_sqlite,
    )
