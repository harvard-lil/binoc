"""Tests for plugin discovery and the PluginRegistry Python wrapper."""

import subprocess
import sys
from unittest.mock import MagicMock, patch

import binoc
from binoc._discovery import discover_plugins


class TestPluginRegistry:
    def test_default_registry_has_stdlib_plugins(self):
        registry = binoc.PluginRegistry.default()
        comparators = registry.list_comparators()
        assert "binoc.csv" in comparators
        assert "binoc.text" in comparators
        assert "binoc.binary" in comparators
        assert "binoc.directory" in comparators
        assert "binoc.zip" in comparators

    def test_default_registry_has_stdlib_transformers(self):
        registry = binoc.PluginRegistry.default()
        transformers = registry.list_transformers()
        assert "binoc.move_detector" in transformers
        assert "binoc.copy_detector" in transformers
        assert "binoc.column_reorder_detector" in transformers

    def test_default_registry_has_stdlib_outputters(self):
        registry = binoc.PluginRegistry.default()
        outputters = registry.list_outputters()
        assert "binoc.markdown" in outputters

    def test_register_python_comparator(self):
        registry = binoc.PluginRegistry.default()

        class MyComparator(binoc.Comparator):
            name = "test.my_comparator"
            extensions = [".xyz"]

            def compare(self, pair):
                return binoc.Identical()

        registry.register_comparator("test.my_comparator", MyComparator())
        assert "test.my_comparator" in registry.list_comparators()

    def test_register_python_transformer(self):
        registry = binoc.PluginRegistry.default()

        class MyTransformer(binoc.Transformer):
            name = "test.my_transformer"

            def transform(self, node):
                return binoc.Unchanged()

        registry.register_transformer("test.my_transformer", MyTransformer())
        assert "test.my_transformer" in registry.list_transformers()


class TestDiscoverPlugins:
    def test_discover_calls_register_functions(self):
        registry = binoc.PluginRegistry.default()
        mock_register = MagicMock()

        mock_ep = MagicMock()
        mock_ep.name = "test_plugin"
        mock_ep.value = "test_plugin:register"
        mock_ep.load.return_value = mock_register

        with patch(
            "binoc._discovery.importlib.metadata.entry_points", return_value=[mock_ep]
        ):
            discover_plugins(registry)

        mock_ep.load.assert_called_once()
        mock_register.assert_called_once_with(registry)

    def test_discover_handles_missing_plugins_gracefully(self):
        """A broken entry point should log an error, not crash."""
        registry = binoc.PluginRegistry.default()

        mock_ep = MagicMock()
        mock_ep.name = "broken_plugin"
        mock_ep.value = "broken:register"
        mock_ep.load.side_effect = ImportError("no such module")

        with patch(
            "binoc._discovery.importlib.metadata.entry_points", return_value=[mock_ep]
        ):
            discover_plugins(registry)

    def test_discover_with_no_plugins(self):
        """When no entry points exist, discovery is a no-op."""
        registry = binoc.PluginRegistry.default()
        before = registry.list_comparators()

        with patch("binoc._discovery.importlib.metadata.entry_points", return_value=[]):
            discover_plugins(registry)

        assert registry.list_comparators() == before


class TestPythonCLI:
    def test_python_m_binoc_help(self):
        result = subprocess.run(
            [sys.executable, "-m", "binoc", "--help"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "binoc" in result.stdout
        assert "diff" in result.stdout

    def test_python_m_binoc_diff(self, snapshot_pair):
        a, b = snapshot_pair("single-file-add")
        result = subprocess.run(
            [sys.executable, "-m", "binoc", "diff", a, b, "--format", "json"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert '"kind": "add"' in result.stdout
