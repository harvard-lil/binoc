#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Emit docs/users/reference/third-party-plugins.md from third_party_plugins.json (repo root).

Catalog entries include declarative dispatch metadata (`extensions`, `media_types`,
and related fields) aligned with `ComparatorDescriptor` / `TransformerDescriptor`
in binoc-sdk so tooling can match files to plugins without scraping Markdown.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
DATA_PATH = ROOT / "third_party_plugins.json"
OUT_PATH = ROOT / "docs" / "users" / "reference" / "third-party-plugins.md"

PACKAGE_LABELS = {"pypi": "PyPI", "crate": "crates.io"}


def _die(msg: str) -> None:
    print(msg, file=sys.stderr)
    raise SystemExit(1)


def _as_str_list(val: Any, path: str) -> list[str]:
    if not isinstance(val, list):
        _die(f"{path}: expected a JSON array")
    out: list[str] = []
    for i, x in enumerate(val):
        if not isinstance(x, str):
            _die(f"{path}[{i}]: expected string")
        out.append(x)
    return out


def _optional_str_list(val: Any, path: str) -> list[str]:
    if val is None:
        return []
    return _as_str_list(val, path)


def _md_table(rows: list[tuple[str, str]]) -> str:
    lines = ["| Field | Value |", "|---|---|"]
    for k, v in rows:
        v_esc = v.replace("|", "\\|").replace("\n", "<br>")
        lines.append(f"| {k} | {v_esc} |")
    return "\n".join(lines)


def _format_comparator_dispatch(d: dict[str, Any], path: str) -> list[tuple[str, str]]:
    exts = _optional_str_list(d.get("extensions"), f"{path}.extensions")
    mts = _optional_str_list(d.get("media_types"), f"{path}.media_types")
    scope = d.get("scope", "Files")
    if not isinstance(scope, str):
        _die(f"{path}.scope: expected string")
    hi = d.get("handles_identical", False)
    if not isinstance(hi, bool):
        _die(f"{path}.handles_identical: expected boolean")
    rows: list[tuple[str, str]] = [
        ("`extensions`", ", ".join(f"`{e}`" for e in exts) if exts else "—"),
        ("`media_types`", ", ".join(f"`{m}`" for m in mts) if mts else "—"),
        ("`scope`", f"`{scope}`"),
        ("`handles_identical`", "`true`" if hi else "`false`"),
    ]
    for key in d:
        if key not in ("extensions", "media_types", "scope", "handles_identical"):
            _die(f"{path}: unknown dispatch key {key!r}")
    return rows


def _format_transformer_dispatch(d: dict[str, Any], path: str) -> list[tuple[str, str]]:
    rows: list[tuple[str, str]] = []
    match_types = _optional_str_list(d.get("match_types"), f"{path}.match_types")
    match_tags = _optional_str_list(d.get("match_tags"), f"{path}.match_tags")
    match_actions = _optional_str_list(d.get("match_actions"), f"{path}.match_actions")
    phase = d.get("suggested_phase", "default")
    if not isinstance(phase, str):
        _die(f"{path}.suggested_phase: expected string")
    node_shape = d.get("node_shape", "Any")
    if not isinstance(node_shape, str):
        _die(f"{path}.node_shape: expected string")

    arts_raw = d.get("match_artifacts") or []
    if not isinstance(arts_raw, list):
        _die(f"{path}.match_artifacts: expected array")
    arts_fmt: list[str] = []
    for i, a in enumerate(arts_raw):
        if not isinstance(a, dict):
            _die(f"{path}.match_artifacts[{i}]: expected object")
        for req in ("package", "name", "version"):
            if req not in a:
                _die(f"{path}.match_artifacts[{i}]: missing {req!r}")
        pkg, name, ver = a["package"], a["name"], a["version"]
        if not isinstance(pkg, str) or not isinstance(name, str):
            _die(f"{path}.match_artifacts[{i}]: package and name must be strings")
        if not isinstance(ver, int) or ver < 0:
            _die(f"{path}.match_artifacts[{i}]: version must be a non-negative int")
        arts_fmt.append(f"`{pkg}.{name}.v{ver}`")

    rows.append(("`match_types`", ", ".join(f"`{t}`" for t in match_types) if match_types else "—"))
    rows.append(("`match_tags`", ", ".join(f"`{t}`" for t in match_tags) if match_tags else "—"))
    rows.append(("`match_actions`", ", ".join(f"`{t}`" for t in match_actions) if match_actions else "—"))
    rows.append(("`suggested_phase`", f"`{phase}`"))
    rows.append(("`match_artifacts`", ", ".join(arts_fmt) if arts_fmt else "—"))
    rows.append(("`node_shape`", f"`{node_shape}`"))

    allowed = {
        "match_types",
        "match_tags",
        "match_actions",
        "suggested_phase",
        "match_artifacts",
        "node_shape",
    }
    for key in d:
        if key not in allowed:
            _die(f"{path}: unknown dispatch key {key!r}")
    return rows


def _plugin_section(p: dict[str, Any], idx: int) -> list[str]:
    base = f"plugins[{idx}]"
    pid = p.get("id")
    title = p.get("title")
    summary = p.get("summary")
    if not isinstance(pid, str) or not isinstance(title, str) or not isinstance(summary, str):
        _die(f"{base}: id, title, and summary must be strings")

    lines: list[str] = [f"## {title}", "", summary, ""]

    repo = p.get("repository")
    if repo is not None:
        if not isinstance(repo, str):
            _die(f"{base}.repository: expected string")
        lines.extend([f"- **Repository:** [{repo}]({repo})", ""])

    doc = p.get("documentation")
    if doc is not None:
        if not isinstance(doc, str):
            _die(f"{base}.documentation: expected string")
        lines.extend([f"- **More detail:** [{doc}]({doc})", ""])

    pkgs = p.get("packages") or {}
    if pkgs:
        if not isinstance(pkgs, dict):
            _die(f"{base}.packages: expected object")
        pkg_bits = []
        for k, v in pkgs.items():
            if not isinstance(k, str) or not isinstance(v, str):
                _die(f"{base}.packages: keys and values must be strings")
            label = PACKAGE_LABELS.get(k, k.replace("_", " ").title())
            pkg_bits.append(f"**{label}:** `{v}`")
        lines.append("- " + " · ".join(pkg_bits))
        lines.append("")

    ep = p.get("entry_point")
    if ep:
        if not isinstance(ep, dict):
            _die(f"{base}.entry_point: expected object")
        group = ep.get("group", "binoc.plugins")
        name = ep.get("name")
        target = ep.get("target")
        loader = ep.get("loader")
        if not isinstance(group, str) or not isinstance(name, str) or not isinstance(target, str):
            _die(f"{base}.entry_point: group, name, and target must be strings")
        lines.append("### Install")
        lines.append("")
        lines.append(
            "After you install the package (for example from PyPI), binoc picks it up "
            "via the standard entry-point group — see [Install and use plugins]"
            "(../howto/install-and-use-plugins.md) and [Plugin discovery]"
            "(../../plugin-developers/reference/plugin-discovery.md)."
        )
        lines.append("")
        lines.append("Published packages declare discovery metadata like this:")
        lines.append("")
        lines.append("```toml")
        lines.append(f'[project.entry-points."{group}"]')
        lines.append(f"{name} = \"{target}\"")
        lines.append("```")
        lines.append("")
        if loader is not None:
            if not isinstance(loader, str):
                _die(f"{base}.entry_point.loader: expected string")
            lines.append(
                f"This distribution is a **native Rust** plugin (`{loader}`): "
                "the target is a module name, not `module:function`."
            )
            lines.append("")

    comparators = p.get("comparators") or []
    if not isinstance(comparators, list):
        _die(f"{base}.comparators: expected array")

    for ci, c in enumerate(comparators):
        cp = f"{base}.comparators[{ci}]"
        if not isinstance(c, dict):
            _die(f"{cp}: expected object")
        cname = c.get("name")
        if not isinstance(cname, str):
            _die(f"{cp}.name: expected string")
        dispatch = c.get("dispatch")
        if not isinstance(dispatch, dict):
            _die(f"{cp}.dispatch: expected object")

        if len(comparators) == 1:
            lines.append("### When it handles your files")
        else:
            if ci == 0:
                lines.append("### When it handles your files")
                lines.append("")
            lines.append(f"#### `{cname}`")
        lines.append("")
        lines.append(
            "A file is routed to this plugin when **either** its path matches one of "
            "the **extensions** or its detected **media type** matches (same rules as "
            "the rest of the pipeline). Ordering relative to other plugins and the "
            "standard library is up to your "
            "[dataset config](dataset-config.md)."
        )
        lines.append("")
        lines.append(_md_table(_format_comparator_dispatch(dispatch, f"{cp}.dispatch")))
        lines.append("")

        ir_types = c.get("ir_item_types")
        if ir_types is not None:
            _ = _as_str_list(ir_types, f"{cp}.ir_item_types")
            lines.append(
                "*Labels you may see in a changeset (not used for routing):* "
                + ", ".join(f"`{t}`" for t in ir_types)
            )
            lines.append("")

        notes = c.get("notes")
        if notes is not None:
            if not isinstance(notes, str):
                _die(f"{cp}.notes: expected string")
            lines.append(notes)
            lines.append("")

    transformers = p.get("transformers") or []
    if not isinstance(transformers, list):
        _die(f"{base}.transformers: expected array")
    for ti, t in enumerate(transformers):
        tp = f"{base}.transformers[{ti}]"
        if not isinstance(t, dict):
            _die(f"{tp}: expected object")
        tname = t.get("name")
        if not isinstance(tname, str):
            _die(f"{tp}.name: expected string")
        dispatch = t.get("dispatch")
        if not isinstance(dispatch, dict):
            _die(f"{tp}.dispatch: expected object")
        lines.append(f"### Transformer: `{tname}`")
        lines.append("")
        lines.append(
            "Runs on diff nodes when the following transformer dispatch rules match "
            "(see [Dispatch model](../../plugin-developers/explanation/dispatch-model.md)):"
        )
        lines.append("")
        lines.append(_md_table(_format_transformer_dispatch(dispatch, f"{tp}.dispatch")))
        lines.append("")

    renderers = p.get("renderers") or []
    if not isinstance(renderers, list):
        _die(f"{base}.renderers: expected array")
    for ri, r in enumerate(renderers):
        rp = f"{base}.renderers[{ri}]"
        if not isinstance(r, dict):
            _die(f"{rp}: expected object")
        rname = r.get("name")
        ext = r.get("file_extension")
        if not isinstance(rname, str) or not isinstance(ext, str):
            _die(f"{rp}: name and file_extension must be strings")
        lines.append(f"### Renderer: `{rname}`")
        lines.append("")
        lines.append(f"- **Output extension:** `{ext}`")
        lines.append("")

    allowed_top = {
        "id",
        "title",
        "summary",
        "repository",
        "documentation",
        "source_path",
        "packages",
        "entry_point",
        "comparators",
        "transformers",
        "renderers",
    }
    for key in p:
        if key not in allowed_top:
            _die(f"{base}: unknown key {key!r}")

    return lines


def main() -> int:
    raw = json.loads(DATA_PATH.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        _die("root JSON value must be an object")
    plugins = raw.get("plugins")
    if not isinstance(plugins, list):
        _die(".plugins: expected array")
    for key in raw:
        if key != "plugins":
            _die(f"unknown root key {key!r}")

    rel_data = DATA_PATH.relative_to(ROOT)
    rel_script = Path(__file__).resolve().relative_to(ROOT)

    lines: list[str] = [
        "---",
        "audience: data steward, plugin consumer",
        "---",
        "",
        "# Third-party plugins",
        "",
        "Binoc ships a capable [standard library](../../plugin-developers/explanation/plugin-model.md) "
        "(`binoc-stdlib`), but some datasets use formats that need a dedicated "
        "comparator. Install one of the **add-on plugins** below when your "
        "snapshots include those kinds of files.",
        "",
        "To find a match, compare your filenames (suffixes) and, when available, "
        "detected media types to the tables under each plugin. Once you find one, "
        "install the package and add its comparator names to your "
        "[dataset config](dataset-config.md) where needed.",
        "",
        "!!! tip \"Publishing or listing a plugin\"",
        "",
        "    If you maintain a plugin and want it listed here, see "
        "[Publish a plugin](../../plugin-developers/howto/publish-a-plugin.md).",
        "",
        "!!! note \"Generated page\"",
        "",
        f"    Entries are maintained in `{rel_data}` at the repository root. "
        f"Maintainers regenerate this Markdown with `{rel_script}` "
        f"(`just docs-plugin-catalog`).",
        "",
    ]

    if not plugins:
        lines.append("_No plugins are listed in the catalog yet._")
        lines.append("")
    else:
        for i, p in enumerate(plugins):
            if not isinstance(p, dict):
                _die(f"plugins[{i}]: expected object")
            lines.extend(_plugin_section(p, i))

    lines.append("## Catalog file for tools")
    lines.append("")
    lines.append(
        f"The canonical data lives in `{rel_data}` (JSON). Hosts that suggest plugins "
        "for unrecognized formats should read that file; dispatch fields mirror "
        "[`ComparatorDescriptor`](https://docs.rs/binoc-sdk/latest/binoc_sdk/"
        "struct.ComparatorDescriptor.html) in binoc-sdk."
    )
    lines.append("")

    OUT_PATH.write_text("\n".join(lines), encoding="utf-8")
    print(f"Wrote {OUT_PATH.relative_to(ROOT)} ({len(plugins)} plugin(s)).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
