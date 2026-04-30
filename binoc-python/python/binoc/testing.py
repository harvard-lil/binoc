"""Test vector helpers for binoc plugins.

Provides utilities to discover test vectors and run them against the binoc
Python API. Plugin authors use this to validate their comparators end-to-end
through the Python stack.

Snapshots are assumed to be **already materialized** — that is, any ``.zip.d``
/ ``.tar.gz.d`` / plugin-specific staging dirs in ``test-vectors/`` have been
built into real artifacts by ``just materialize`` (or the equivalent
``cargo run -p <crate> --bin materialize-test-vectors`` invocations). See
``docs/adr/test_vector_materialization.md`` for the design; pytest sessions
typically materialize once in a session-scoped fixture::

    @pytest.fixture(scope="session")
    def vectors_dir(tmp_path_factory):
        import subprocess
        dest = tmp_path_factory.mktemp("vectors")
        subprocess.check_call([
            "cargo", "run", "-q", "-p", "my_plugin",
            "--features", "test-support",
            "--bin", "materialize-test-vectors", "--",
            str(dest), "my-plugin/test-vectors",
        ])
        return dest

Typical usage in a plugin's pytest suite::

    import binoc
    from binoc.testing import discover_vectors, run_vector

    @pytest.fixture
    def registry():
        r = binoc.PluginRegistry.default()
        r.register_comparator("my-plugin.foo", MyComparator())
        return r

    @pytest.mark.parametrize(
        "vector_dir",
        discover_vectors(vectors_dir()),
        ids=lambda v: v.name,
    )
    def test_vector(vector_dir, registry):
        run_vector(vector_dir, registry=registry)
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import binoc

if TYPE_CHECKING:
    pass

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore[no-redef]


def discover_vectors(vectors_dir: str | Path) -> list[Path]:
    """Find test vector directories under *vectors_dir*.

    A valid vector directory contains ``manifest.toml``, ``snapshot-a/``,
    and ``snapshot-b/``.  Returns a sorted list of Paths.
    """
    vectors_dir = Path(vectors_dir)
    if not vectors_dir.exists():
        return []
    return sorted(
        p
        for p in vectors_dir.iterdir()
        if p.is_dir()
        and (p / 'manifest.toml').exists()
        and (p / 'snapshot-a').exists()
        and (p / 'snapshot-b').exists()
    )


def load_manifest(
    vector_dir: str | Path,
    vectors_root: str | Path | None = None,
) -> dict:
    """Load a vector's manifest, merging defaults from the root manifest.

    Returns a dict with keys ``vector``, ``config`` (optional),
    ``expected`` (optional).
    """
    vector_dir = Path(vector_dir)
    vectors_root = Path(vectors_root) if vectors_root else vector_dir.parent

    root_manifest = _load_toml(vectors_root / 'manifest.toml')
    manifest = _load_toml(vector_dir / 'manifest.toml')

    if 'config' not in manifest and 'config' in root_manifest:
        manifest['config'] = root_manifest['config']
    if 'expected' not in manifest and 'expected' in root_manifest:
        manifest['expected'] = root_manifest['expected']

    return manifest


def run_vector(
    vector_dir: str | Path,
    *,
    vectors_root: str | Path | None = None,
    registry: binoc.PluginRegistry | None = None,
) -> binoc.Changeset:
    """Run a single test vector and check its manifest assertions.

    *vector_dir* must be a **materialized** vector — any ``.zip.d`` /
    ``.tar.gz.d`` / plugin-specific staging directories should already have
    been built into real artifacts. See module docstring for how to run
    materialization once per session.

    Steps:
      1. Parse the manifest (with root-manifest defaults).
      2. Build a ``binoc.Config`` from the manifest's ``[config]`` section.
      3. Run ``binoc.diff()`` against the snapshots with the config and
         optional *registry*.
      4. Check ``[expected]`` assertions from the manifest.

    Returns the resulting :class:`binoc.Changeset`.
    """
    vector_dir = Path(vector_dir)
    vectors_root = Path(vectors_root) if vectors_root else vector_dir.parent

    manifest = load_manifest(vector_dir, vectors_root)
    config = _build_config(manifest)
    name = manifest['vector']['name']

    snap_a = vector_dir / 'snapshot-a'
    snap_b = vector_dir / 'snapshot-b'

    changeset = binoc.diff(str(snap_a), str(snap_b), config=config, registry=registry)

    expected = manifest.get('expected', {})
    if expected:
        check_assertions(name, changeset, expected)

    return changeset


def check_assertions(
    name: str,
    changeset: binoc.Changeset,
    expected: dict,
) -> None:
    """Verify a changeset against ``[expected]`` assertions from a manifest."""
    if 'root_action' in expected:
        root_action = expected['root_action']
        assert changeset.root is not None, (
            f"[{name}] Expected root with action '{root_action}' but changeset has no root"
        )
        root = changeset.root
        if root.item_type == 'directory' and root.action != root_action:
            child_actions = [c.action for c in root]
            assert root.action == root_action or root_action in child_actions, (
                f"[{name}] Expected root_action '{root_action}', got root.action='{root.action}'"
                f' with child actions: {child_actions}'
            )

    if 'child_count' in expected:
        child_count = expected['child_count']
        assert changeset.root is not None, (
            f'[{name}] Expected child_count={child_count} but changeset has no root'
        )
        actual = len(list(changeset.root))
        assert actual == child_count, f'[{name}] Expected child_count={child_count}, got {actual}'

    if 'has_tags' in expected:
        assert changeset.root is not None, f'[{name}] Expected tags but changeset has no root'
        all_tags = changeset.root.all_tags()
        for tag in expected['has_tags']:
            assert tag in all_tags, (
                f"[{name}] Expected tag '{tag}' not found. All tags: {sorted(all_tags)}"
            )


def _load_toml(path: Path) -> dict:
    if not path.exists():
        return {}
    return tomllib.loads(path.read_text())


def _build_config(manifest: dict) -> binoc.Config:
    mc = manifest.get('config', {})
    comparators = mc.get('comparators')
    transformers = mc.get('transformers')
    return binoc.Config(comparators=comparators, transformers=transformers)
