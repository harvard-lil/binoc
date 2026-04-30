"""Plugin discovery via Python entry points.

Third-party packages register binoc plugins by declaring an entry point
in the ``"binoc.plugins"`` group.  Two forms are supported:

**Python plugins** use ``module:callable`` syntax.  The callable receives
a :class:`~binoc.PluginRegistry` and populates it::

    [project.entry-points."binoc.plugins"]
    biobinoc = "biobinoc:register"

**Native Rust plugins** use a bare module path pointing to the ``.so``
built by ``export_plugin!``.  The host loads it via libloading
automatically::

    [project.entry-points."binoc.plugins"]
    binoc-sqlite = "binoc_sqlite"
"""

import importlib.metadata
import logging

logger = logging.getLogger('binoc')


def discover_plugins(registry):
    """Scan installed packages for binoc plugin entry points.

    Each entry point in the ``"binoc.plugins"`` group is either a
    ``module:callable`` (Python plugin) or a bare module path (native
    Rust plugin).
    """
    eps = importlib.metadata.entry_points(group='binoc.plugins')
    for ep in eps:
        logger.debug('Loading plugin entry point: %s (from %s)', ep.name, ep.value)
        try:
            loaded = ep.load()
            if callable(loaded):
                loaded(registry)
            else:
                registry.load_native_plugin(ep.value)
        except Exception:
            logger.exception('Failed to load binoc plugin %r', ep.name)
