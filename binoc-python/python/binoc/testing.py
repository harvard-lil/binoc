"""Test vector helpers for binoc plugins.

Provides utilities to discover, prepare, and run test vectors against
the binoc Python API. Plugin authors use this to validate their
comparators end-to-end through the Python stack.

Typical usage in a plugin's pytest suite::

    import binoc
    from binoc.testing import discover_vectors, run_vector

    VECTORS = discover_vectors("test-vectors")

    @pytest.fixture
    def registry():
        r = binoc.PluginRegistry.default()
        r.register_comparator("my-plugin.foo", MyComparator())
        return r

    @pytest.mark.parametrize("vector_dir", VECTORS, ids=[v.name for v in VECTORS])
    def test_vector(vector_dir, registry):
        run_vector(vector_dir, registry=registry)
"""

from __future__ import annotations

import shutil
import tarfile
import tempfile
import zipfile
from pathlib import Path
from typing import TYPE_CHECKING, Callable

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
        and (p / "manifest.toml").exists()
        and (p / "snapshot-a").exists()
        and (p / "snapshot-b").exists()
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

    root_manifest = _load_toml(vectors_root / "manifest.toml")
    manifest = _load_toml(vector_dir / "manifest.toml")

    if "config" not in manifest and "config" in root_manifest:
        manifest["config"] = root_manifest["config"]
    if "expected" not in manifest and "expected" in root_manifest:
        manifest["expected"] = root_manifest["expected"]

    return manifest


def run_vector(
    vector_dir: str | Path,
    *,
    vectors_root: str | Path | None = None,
    registry: binoc.PluginRegistry | None = None,
    prepare: Callable[[Path, Path], None] | None = None,
) -> binoc.Changeset:
    """Run a single test vector and check its manifest assertions.

    Steps:
      1. Parse the manifest (with root-manifest defaults).
      2. Build a ``binoc.Config`` from the manifest's ``[config]`` section.
      3. Copy snapshots to a temp directory.
      4. Build ``.zip`` from ``.zip.d`` and ``.tar`` from ``.tar.d`` dirs.
      5. Call *prepare(snap_a, snap_b)* if provided (for plugin-specific
         artifact building, e.g. ``.sqlite`` from ``.sqlite.d``).
      6. Run ``binoc.diff()`` with the config and optional *registry*.
      7. Check ``[expected]`` assertions from the manifest.

    Returns the resulting :class:`binoc.Changeset`.
    """
    vector_dir = Path(vector_dir)
    vectors_root = Path(vectors_root) if vectors_root else vector_dir.parent

    manifest = load_manifest(vector_dir, vectors_root)
    config = _build_config(manifest)
    name = manifest["vector"]["name"]

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        snap_a = tmp_path / "snapshot-a"
        snap_b = tmp_path / "snapshot-b"
        shutil.copytree(vector_dir / "snapshot-a", snap_a)
        shutil.copytree(vector_dir / "snapshot-b", snap_b)

        _build_zips_in_dir(snap_a)
        _build_zips_in_dir(snap_b)
        _build_tars_in_dir(snap_a)
        _build_tars_in_dir(snap_b)

        if prepare:
            prepare(snap_a, snap_b)

        changeset = binoc.diff(
            str(snap_a), str(snap_b), config=config, registry=registry
        )

    expected = manifest.get("expected", {})
    if expected:
        check_assertions(name, changeset, expected)

    return changeset


def check_assertions(
    name: str,
    changeset: binoc.Changeset,
    expected: dict,
) -> None:
    """Verify a changeset against ``[expected]`` assertions from a manifest."""
    if "root_action" in expected:
        root_action = expected["root_action"]
        assert changeset.root is not None, (
            f"[{name}] Expected root with action '{root_action}' but changeset has no root"
        )
        root = changeset.root
        if root.item_type == "directory" and root.action != root_action:
            child_actions = [c.action for c in root]
            assert root.action == root_action or root_action in child_actions, (
                f"[{name}] Expected root_action '{root_action}', got root.action='{root.action}'"
                f" with child actions: {child_actions}"
            )

    if "child_count" in expected:
        child_count = expected["child_count"]
        assert changeset.root is not None, (
            f"[{name}] Expected child_count={child_count} but changeset has no root"
        )
        actual = len(list(changeset.root))
        assert actual == child_count, (
            f"[{name}] Expected child_count={child_count}, got {actual}"
        )

    if "has_tags" in expected:
        assert changeset.root is not None, (
            f"[{name}] Expected tags but changeset has no root"
        )
        all_tags = changeset.root.all_tags()
        for tag in expected["has_tags"]:
            assert tag in all_tags, (
                f"[{name}] Expected tag '{tag}' not found. All tags: {sorted(all_tags)}"
            )


# ── Internal helpers ──────────────────────────────────────────────────────


def _load_toml(path: Path) -> dict:
    if not path.exists():
        return {}
    return tomllib.loads(path.read_text())


def _build_config(manifest: dict) -> binoc.Config:
    mc = manifest.get("config", {})
    comparators = mc.get("comparators")
    transformers = mc.get("transformers")
    return binoc.Config(comparators=comparators, transformers=transformers)


def _build_zips_in_dir(dir_path: Path) -> None:
    """Recursively build .zip files from .zip.d directories, then remove the .d dirs."""
    for entry in sorted(dir_path.rglob("*")):
        if entry.is_dir() and entry.name.endswith(".zip.d"):
            zip_path = entry.parent / entry.name.removesuffix(".d")
            _create_zip(entry, zip_path)
            shutil.rmtree(entry)


def _create_zip(source_dir: Path, zip_path: Path) -> None:
    with zipfile.ZipFile(zip_path, "w") as zf:
        for file in sorted(source_dir.rglob("*")):
            if file.is_file():
                zf.write(file, file.relative_to(source_dir))


def _build_tars_in_dir(dir_path: Path) -> None:
    """Recursively build .tar/.tar.gz files from .tar.d/.tar.gz.d dirs."""
    for entry in sorted(dir_path.rglob("*")):
        if not entry.is_dir():
            continue
        name = entry.name
        if (
            name.endswith(".tar.d")
            or name.endswith(".tar.gz.d")
            or name.endswith(".tgz.d")
        ):
            tar_name = name.removesuffix(".d")
            tar_path = entry.parent / tar_name
            _create_tar(entry, tar_path)
            shutil.rmtree(entry)


def _create_tar(source_dir: Path, tar_path: Path) -> None:
    tar_name = tar_path.name
    is_gz = tar_name.endswith(".tar.gz") or tar_name.endswith(".tgz")
    mode = "w:gz" if is_gz else "w"
    with tarfile.open(tar_path, mode) as tf:
        for file in sorted(source_dir.rglob("*")):
            if file.is_file():
                tf.add(file, arcname=file.relative_to(source_dir))
