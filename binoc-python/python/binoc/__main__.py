"""Entry point for ``python -m binoc`` and the ``binoc`` console script.

Builds a plugin registry (stdlib + any entry-point-discovered plugins),
then delegates to the Rust CLI.
"""

import sys

from binoc._binoc import PluginRegistry, run_cli
from binoc._discovery import discover_plugins


def main():
    registry = PluginRegistry.default()
    discover_plugins(registry)
    args = ['binoc'] + sys.argv[1:]
    try:
        run_cli(registry, args)
    except RuntimeError as e:
        print(str(e), file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    main()
