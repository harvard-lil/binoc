"""Tests for plugin discovery and the PluginRegistry Python wrapper."""

import subprocess
import sys
from unittest.mock import MagicMock, patch

import binoc
from binoc._discovery import discover_plugins


class TestPluginRegistry:
    def test_default_registry_has_stdlib_renderers(self):
        registry = binoc.PluginRegistry.default()
        renderers = registry.list_renderers()
        assert 'binoc.markdown' in renderers


class TestDiscoverPlugins:
    def test_discover_calls_register_functions(self):
        registry = binoc.PluginRegistry.default()
        mock_register = MagicMock()

        mock_ep = MagicMock()
        mock_ep.name = 'test_plugin'
        mock_ep.value = 'test_plugin:register'
        mock_ep.load.return_value = mock_register

        with patch('binoc._discovery.importlib.metadata.entry_points', return_value=[mock_ep]):
            discover_plugins(registry)

        mock_ep.load.assert_called_once()
        mock_register.assert_called_once_with(registry)

    def test_discover_handles_missing_plugins_gracefully(self):
        """A broken entry point should log an error, not crash."""
        registry = binoc.PluginRegistry.default()

        mock_ep = MagicMock()
        mock_ep.name = 'broken_plugin'
        mock_ep.value = 'broken:register'
        mock_ep.load.side_effect = ImportError('no such module')

        with patch('binoc._discovery.importlib.metadata.entry_points', return_value=[mock_ep]):
            discover_plugins(registry)

    def test_discover_with_no_plugins(self):
        """When no entry points exist, discovery is a no-op."""
        registry = binoc.PluginRegistry.default()
        before = registry.list_renderers()

        with patch('binoc._discovery.importlib.metadata.entry_points', return_value=[]):
            discover_plugins(registry)

        assert registry.list_renderers() == before


class TestPythonCLI:
    def test_python_m_binoc_help(self):
        result = subprocess.run(
            [sys.executable, '-m', 'binoc', '--help'],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert 'binoc' in result.stdout
        assert 'diff' in result.stdout

    def test_python_m_binoc_diff(self, snapshot_pair):
        a, b = snapshot_pair('single-file-add')
        result = subprocess.run(
            [sys.executable, '-m', 'binoc', 'diff', a, b, '--format', 'json'],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert '"action": "add"' in result.stdout
