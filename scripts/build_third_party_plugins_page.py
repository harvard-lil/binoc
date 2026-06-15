#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Emit docs/users/reference/third-party-plugins.md from third_party_plugins.json (repo root).

Catalog entries include advertised file selectors (`extensions`, `media_types`,
and related fields) so tooling can match files to plugins without scraping
Markdown.
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


def _format_rule_pack_dispatch(d: dict[str, Any], path: str) -> list[tuple[str, str]]:
    exts = _optional_str_list(d.get("extensions"), f"{path}.extensions")
    mts = _optional_str_list(d.get("media_types"), f"{path}.media_types")
    scope = d.get("scope", "Files")
    if not isinstance(scope, str):
        _die(f"{path}.scope: expected string")
    rows: list[tuple[str, str]] = [
        ("`extensions`", ", ".join(f"`{e}`" for e in exts) if exts else "—"),
        ("`media_types`", ", ".join(f"`{m}`" for m in mts) if mts else "—"),
        ("`scope`", f"`{scope}`"),
    ]
    for key in d:
        if key not in ("extensions", "media_types", "scope"):
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

    rule_packs = p.get("rule_packs") or []
    if not isinstance(rule_packs, list):
        _die(f"{base}.rule_packs: expected array")

    for ci, c in enumerate(rule_packs):
        cp = f"{base}.rule_packs[{ci}]"
        if not isinstance(c, dict):
            _die(f"{cp}: expected object")
        cname = c.get("name")
        if not isinstance(cname, str):
            _die(f"{cp}.name: expected string")
        dispatch = c.get("dispatch")
        if not isinstance(dispatch, dict):
            _die(f"{cp}.dispatch: expected object")

        if len(rule_packs) == 1:
            lines.append("### When it handles your files")
        else:
            if ci == 0:
                lines.append("### When it handles your files")
                lines.append("")
            lines.append(f"#### `{cname}`")
        lines.append("")
        lines.append(
            "This rule pack is relevant when a file path matches one of the "
            "**extensions** or its detected **media type** matches. These selectors "
            "are advertised for discovery and documentation; current diff behavior "
            "is driven by the correspondence engine's rule configuration."
        )
        lines.append("")
        lines.append(_md_table(_format_rule_pack_dispatch(dispatch, f"{cp}.dispatch")))
        lines.append("")

        item_types = c.get("item_types")
        if item_types is not None:
            _ = _as_str_list(item_types, f"{cp}.item_types")
            lines.append(
                "*Labels you may see in a changeset (not used for routing):* "
                + ", ".join(f"`{t}`" for t in item_types)
            )
            lines.append("")

        rules = c.get("rules")
        if rules is not None:
            _ = _as_str_list(rules, f"{cp}.rules")
            lines.append("*Rule families supplied:* " + ", ".join(f"`{r}`" for r in rules))
            lines.append("")

        notes = c.get("notes")
        if notes is not None:
            if not isinstance(notes, str):
                _die(f"{cp}.notes: expected string")
            lines.append(notes)
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
        "rule_packs",
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
        "rule pack. Install one of the **add-on plugins** below when your "
        "snapshots include those kinds of files.",
        "",
        "To find a match, compare your filenames (suffixes) and, when available, "
        "detected media types to the tables under each plugin. Once you find one, "
        "install the package and configure any dataset semantics it documents.",
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
        "for unrecognized formats should read that file; dispatch fields describe "
        "the rule pack's advertised file selectors."
    )
    lines.append("")

    OUT_PATH.write_text("\n".join(lines), encoding="utf-8")
    print(f"Wrote {OUT_PATH.relative_to(ROOT)} ({len(plugins)} plugin(s)).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
