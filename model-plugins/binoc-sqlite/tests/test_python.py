"""Smoke test: verify the native plugin loads and registers via the C ABI bridge.

Comparator correctness is covered by the Rust test vectors in
binoc-sqlite/tests/test_vectors.rs. This test only verifies the
Python packaging and native loading path.
"""

import binoc


def test_native_plugin_loads():
    r = binoc.PluginRegistry.default()
    r.load_native_plugin('binoc_sqlite')
    assert 'binoc-sqlite.sqlite' in r.list_comparators()
