"""Entry point for ``python -m binoc`` and the ``binoc`` console script.

Builds a plugin registry (stdlib + any entry-point-discovered plugins),
then delegates to the Rust CLI.
"""

from importlib.metadata import PackageNotFoundError, version
import sys

from binoc._binoc import PluginRegistry, run_cli
from binoc._discovery import discover_plugins

# Matches binoc_cli::EXIT_ERROR: the command failed or the config had errors.
EXIT_ERROR = 2


def main():
    registry = PluginRegistry.default()
    discover_plugins(registry)
    args = ['binoc'] + sys.argv[1:]
    try:
        package_version = version('binoc')
    except PackageNotFoundError:
        package_version = None
    try:
        status = run_cli(registry, args, package_version)
    except RuntimeError as e:
        print(str(e), file=sys.stderr)
        sys.exit(EXIT_ERROR)
    sys.exit(status)


if __name__ == '__main__':
    main()
