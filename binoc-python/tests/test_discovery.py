"""Tests for plugin discovery and the PluginRegistry Python wrapper."""

import json
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

    def test_python_m_binoc_diff_json_stdout_is_complete(self, snapshot_pair):
        a, b = snapshot_pair('kitchen-sink')
        result = subprocess.run(
            [sys.executable, '-m', 'binoc', 'diff', a, b, '--format', 'json'],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        parsed = json.loads(result.stdout)
        assert parsed['from_snapshot'].endswith('snapshot-a')
        assert parsed['to_snapshot'].endswith('snapshot-b')

    def test_python_m_binoc_version_reports_distribution_version(self):
        from importlib.metadata import version

        result = subprocess.run(
            [sys.executable, '-m', 'binoc', '--version'],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert result.stdout.strip() == f'binoc {version("binoc")}'

    def test_python_m_binoc_exit_codes(self, snapshot_pair, tmp_path):
        def run(*args):
            return subprocess.run(
                [sys.executable, '-m', 'binoc', 'diff', *args, '-q'],
                capture_output=True,
                text=True,
            )

        assert run(*snapshot_pair('trivial-identical'), '--exit-code').returncode == 0
        assert run(*snapshot_pair('single-file-add'), '--exit-code').returncode == 1
        assert run(*snapshot_pair('single-file-add')).returncode == 0

        config = tmp_path / 'binoc.yaml'
        config.write_text('dataset:\n  tables: []\n')
        result = run(*snapshot_pair('single-file-add'), '--config', str(config))
        assert result.returncode == 2
        assert 'config error' in result.stderr

        a, _ = snapshot_pair('single-file-add')
        result = run(a, str(tmp_path / 'missing'))
        assert result.returncode == 2
        assert 'snapshot not found' in result.stderr
