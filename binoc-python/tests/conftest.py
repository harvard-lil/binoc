from pathlib import Path

import pytest


@pytest.fixture
def test_vectors_dir():
    """Path to the shared test-vectors directory at the workspace root."""
    here = Path(__file__).resolve().parent
    return here.parent.parent / 'test-vectors'


@pytest.fixture
def snapshot_pair(test_vectors_dir):
    """Helper to get (snapshot_a, snapshot_b) paths for a named test vector."""

    def _get(vector_name: str) -> tuple[str, str]:
        vector_dir = test_vectors_dir / vector_name
        a = str(vector_dir / 'snapshot-a')
        b = str(vector_dir / 'snapshot-b')
        return a, b

    return _get
