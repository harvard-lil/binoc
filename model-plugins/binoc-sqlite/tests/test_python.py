"""Smoke test: verify the Python package imports.

Rule-pack correctness is covered by the Rust test vectors in
binoc-sqlite/tests/test_vectors.rs. Python does not expose parser-rule loading
yet; this test only verifies the package path.
"""


def test_native_plugin_loads():
    import binoc_sqlite  # noqa: F401
