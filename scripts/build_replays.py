#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["tomli>=2.0.1; python_version < '3.11'"]
# ///
"""Generate interactive HTML replays for a curated set of test vectors.

For each featured vector this runs `binoc diff --trace` over its materialized
`snapshot-a` / `snapshot-b` trees and then `binoc replay` to produce a
self-contained HTML page that animates the correspondence run (trees, links,
edit-list writing and compaction, and the final changelog). It also writes the
`docs/users/explanation/replays.md` index page that links to each one.

The vectors are the same shared workspace vectors exercised by `just test`, so
the replays always reflect real binoc behaviour. The generated HTML lives under
a gitignored directory (like the rustdoc under `docs/sdk/`); the index page is
committed and regenerated.

Run via `just docs-replays` (which materializes vectors first).
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib

ROOT = Path(__file__).resolve().parent.parent
VECTORS_ROOT = ROOT / "test-vectors"
MATERIALIZED_ROOT = ROOT / "test-vectors-materialized"
OUT_DIR = ROOT / "docs" / "users" / "explanation" / "replays"
INDEX_PATH = ROOT / "docs" / "users" / "explanation" / "replays.md"
REPO_BASE_URL = "https://github.com/harvard-lil/binoc/tree/main/test-vectors"

# Curated, ordered for teaching: a rich end-to-end run first, then vectors that
# isolate one interesting mechanism. `note` is an editorial hook for the index;
# the one-line description is pulled from each vector's manifest.
FEATURED: list[dict[str, str]] = [
    {
        "vector": "kitchen-sink",
        "note": "A full run end to end: directories, a zip and a tar.gz expanding "
        "into children, CSV and text edits, a binary file, and move/copy detection — "
        "then edit-list writing and compaction, building up to the final changelog.",
    },
    {
        "vector": "csv-column-reorder",
        "note": "Watch compaction at work: open the Compaction step to see the edit "
        "stack shrink and the cost drop as the row-alignment rewrite is kept.",
    },
    {
        "vector": "folder-move-nested",
        "note": "A whole folder moves. Links form on content and a container is inferred "
        "from its children — hover the links to see the evidence behind a move.",
    },
    {
        "vector": "directory-file-copy",
        "note": "One source file corresponds to two outputs: copy detection links the "
        "original and its duplicate.",
    },
    {
        "vector": "csv-rename-modify",
        "note": "A renamed file that also changed: the link is established despite the "
        "new name, and the edit list explains the content change.",
    },
]


def binoc_binary() -> Path:
    """Build the CLI once and return the debug binary path."""
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "binoc-cli", "--bin", "binoc-cli"],
        cwd=ROOT,
        check=True,
    )
    return ROOT / "target" / "debug" / "binoc-cli"


def manifest_description(vector: str) -> str:
    manifest = VECTORS_ROOT / vector / "manifest.toml"
    if not manifest.exists():
        return ""
    data = tomllib.loads(manifest.read_text())
    return str(data.get("vector", {}).get("description", "")).strip()


def build_replay(binoc: Path, vector: str) -> bool:
    """Produce <OUT_DIR>/<vector>.html from the materialized snapshots.

    Returns False (with a warning) if the materialized snapshots are missing,
    so a partial `just materialize` does not abort the whole docs build.
    """
    snap_a = MATERIALIZED_ROOT / vector / "snapshot-a"
    snap_b = MATERIALIZED_ROOT / vector / "snapshot-b"
    if not snap_a.is_dir() or not snap_b.is_dir():
        print(f"  ! skipping {vector}: materialized snapshots not found "
              f"(run `just materialize`)", file=sys.stderr)
        return False
    out_html = OUT_DIR / f"{vector}.html"
    with tempfile.TemporaryDirectory() as tmp:
        trace_json = Path(tmp) / f"{vector}.json"
        subprocess.run(
            [binoc, "diff", str(snap_a), str(snap_b), "--trace", str(trace_json), "-q"],
            cwd=ROOT,
            check=True,
        )
        # Relabel the absolute materialized temp paths with stable, readable
        # snapshot names so the replay header reads `<vector>/snapshot-a → …`.
        trace = json.loads(trace_json.read_text())
        trace["from_snapshot"] = f"{vector}/snapshot-a"
        trace["to_snapshot"] = f"{vector}/snapshot-b"
        trace_json.write_text(json.dumps(trace))
        subprocess.run(
            [binoc, "replay", str(trace_json), "-o", str(out_html)],
            cwd=ROOT,
            check=True,
        )
    return True


def render_index(built: list[dict[str, str]]) -> str:
    lines = [
        "# Visual replays",
        "",
        "Binoc turns two snapshots into a changeset by running a correspondence",
        "engine: a saturation loop that grows two trees and links matching items,",
        "then writes and compacts the edits that explain each link. These",
        "interactive replays animate that process step by step — useful for",
        "understanding how a result was reached, and for debugging rules.",
        "",
        "Each replay is generated from a real "
        "[test vector](test-vectors-gallery.md) by `just docs-replays`, so it",
        "always reflects current binoc behaviour. Open one and press **play**, or",
        "step through with the arrow keys; the **?** button explains what you are",
        "looking at.",
        "",
    ]
    for entry in built:
        vector = entry["vector"]
        desc = manifest_description(vector)
        lines.append(f"## [{vector}](replays/{vector}.html)")
        lines.append("")
        if desc:
            lines.append(f"*{desc}*")
            lines.append("")
        lines.append(entry["note"])
        lines.append("")
        lines.append(
            f"[Open the replay →](replays/{vector}.html){{target=_blank}} · "
            f"[vector source]({REPO_BASE_URL}/{vector})"
        )
        lines.append("")
    lines.append("<!-- Generated by scripts/build_replays.py via `just docs-replays`. -->")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    binoc = binoc_binary()
    built: list[dict[str, str]] = []
    for entry in FEATURED:
        if build_replay(binoc, entry["vector"]):
            built.append(entry)
            print(f"  replay: {entry['vector']}.html")
    if not built:
        print("No replays generated (no materialized vectors found).", file=sys.stderr)
    INDEX_PATH.write_text(render_index(built))
    print(f"Wrote {INDEX_PATH.relative_to(ROOT)} ({len(built)} replays).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
