#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Emit docs/users/reference/third-party-plugins.md from plugin_registry.json.

The page is a format-pack catalog for users and hosts that want to match file
selectors to available rule packs. Keep the metadata in one place: the registry
page and this catalog both read plugin_registry.json.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
DATA_PATH = ROOT / 'plugin_registry.json'
OUT_PATH = ROOT / 'docs' / 'users' / 'reference' / 'third-party-plugins.md'

PACKAGE_LABELS = {'pypi': 'PyPI', 'crate': 'Rust crate'}
TIER_LABELS = {
    'first-party-bundled': 'First-party bundled',
    'first-party-opt-in': 'First-party opt-in',
    'first-party-add-on': 'First-party add-on',
    'third-party': 'Third-party',
}
TIER_DISTRIBUTION = {
    'first-party-bundled': 'Bundled into the default `binoc` wheel through the `binoc-cli` `bundled` feature.',
    'first-party-opt-in': 'Not published as a separate PyPI wheel. The pack remains in-tree and can be enabled explicitly; SQLite is excluded from the default `bundled` feature set.',
    'first-party-add-on': 'Maintained in this repository but distributed outside the default fat `binoc` wheel.',
    'third-party': 'Maintained outside the core binoc distribution.',
}


def _die(msg: str) -> None:
    print(msg, file=sys.stderr)
    raise SystemExit(1)


def _as_str(val: Any, path: str) -> str:
    if not isinstance(val, str):
        _die(f'{path}: expected string')
    return val


def _as_str_list(val: Any, path: str) -> list[str]:
    if not isinstance(val, list):
        _die(f'{path}: expected a JSON array')
    out: list[str] = []
    for i, x in enumerate(val):
        if not isinstance(x, str):
            _die(f'{path}[{i}]: expected string')
        out.append(x)
    return out


def _optional_str_list(val: Any, path: str) -> list[str]:
    if val is None:
        return []
    return _as_str_list(val, path)


def _md_table(rows: list[tuple[str, str]]) -> str:
    lines = ['| Field | Value |', '|---|---|']
    for k, v in rows:
        v_esc = v.replace('|', '\\|').replace('\n', '<br>')
        lines.append(f'| {k} | {v_esc} |')
    return '\n'.join(lines)


def _format_rule_pack_dispatch(d: dict[str, Any], path: str) -> list[tuple[str, str]]:
    exts = _optional_str_list(d.get('extensions'), f'{path}.extensions')
    mts = _optional_str_list(d.get('media_types'), f'{path}.media_types')
    scope = d.get('scope', 'Files')
    if not isinstance(scope, str):
        _die(f'{path}.scope: expected string')
    rows: list[tuple[str, str]] = [
        ('`extensions`', ', '.join(f'`{e}`' for e in exts) if exts else '-'),
        ('`media_types`', ', '.join(f'`{m}`' for m in mts) if mts else '-'),
        ('`scope`', f'`{scope}`'),
    ]
    for key in d:
        if key not in ('extensions', 'media_types', 'scope'):
            _die(f'{path}: unknown dispatch key {key!r}')
    return rows


def _plugin_section(p: dict[str, Any], idx: int) -> list[str]:
    base = f'plugins[{idx}]'
    title = _as_str(p.get('title'), f'{base}.title')
    tier = _as_str(p.get('tier'), f'{base}.tier')
    if tier not in TIER_LABELS:
        _die(f'{base}.tier: unsupported catalog tier {tier!r}')
    summary = _as_str(p.get('summary'), f'{base}.summary')
    handles = _as_str(p.get('handles'), f'{base}.handles')
    produces = _as_str(p.get('produces'), f'{base}.produces')

    lines: list[str] = [
        f'## {title}',
        '',
        summary,
        '',
        _md_table(
            [
                ('Tier', TIER_LABELS[tier]),
                ('Distribution', TIER_DISTRIBUTION[tier]),
                ('Handles', handles),
                ('Produces', produces),
            ]
        ),
        '',
    ]

    repo = p.get('repository')
    if repo is not None:
        lines.append(f'- **Repository:** [{_as_str(repo, f"{base}.repository")}]({repo})')

    doc = p.get('documentation')
    if doc is not None:
        lines.append(f'- **More detail:** [{_as_str(doc, f"{base}.documentation")}]({doc})')

    source_path = p.get('source_path')
    if source_path is not None:
        lines.append(f'- **Source path:** `{_as_str(source_path, f"{base}.source_path")}`')

    pkgs = p.get('packages') or {}
    if pkgs:
        if not isinstance(pkgs, dict):
            _die(f'{base}.packages: expected object')
        pkg_bits = []
        for k, v in pkgs.items():
            if not isinstance(k, str) or not isinstance(v, str):
                _die(f'{base}.packages: keys and values must be strings')
            label = PACKAGE_LABELS.get(k, k.replace('_', ' ').title())
            pkg_bits.append(f'**{label}:** `{v}`')
        lines.append('- ' + ' · '.join(pkg_bits))
    lines.append('')

    rule_packs = p.get('rule_packs') or []
    if not isinstance(rule_packs, list):
        _die(f'{base}.rule_packs: expected array')

    for ci, rule_pack in enumerate(rule_packs):
        rp = f'{base}.rule_packs[{ci}]'
        if not isinstance(rule_pack, dict):
            _die(f'{rp}: expected object')
        name = _as_str(rule_pack.get('name'), f'{rp}.name')
        dispatch = rule_pack.get('dispatch')
        if not isinstance(dispatch, dict):
            _die(f'{rp}.dispatch: expected object')

        if len(rule_packs) == 1:
            lines.append('### When it handles your files')
        else:
            if ci == 0:
                lines.append('### When it handles your files')
                lines.append('')
            lines.append(f'#### `{name}`')
        lines.append('')
        lines.append(
            'This rule pack is relevant when a file path matches one of the '
            '**extensions** or its detected **media type** matches.'
        )
        lines.append('')
        lines.append(_md_table(_format_rule_pack_dispatch(dispatch, f'{rp}.dispatch')))
        lines.append('')

        rules = rule_pack.get('rules')
        if rules is not None:
            _ = _as_str_list(rules, f'{rp}.rules')
            lines.append('*Rule families supplied:* ' + ', '.join(f'`{r}`' for r in rules))
            lines.append('')

    allowed_top = {
        'id',
        'title',
        'tier',
        'summary',
        'repository',
        'documentation',
        'source_path',
        'packages',
        'handles',
        'produces',
        'rule_packs',
    }
    for key in p:
        if key not in allowed_top:
            _die(f'{base}: unknown key {key!r}')

    return lines


def main() -> int:
    raw = json.loads(DATA_PATH.read_text(encoding='utf-8'))
    if not isinstance(raw, dict):
        _die('root JSON value must be an object')
    plugins = raw.get('plugins')
    if not isinstance(plugins, list):
        _die('.plugins: expected array')

    catalog_plugins = []
    for i, p in enumerate(plugins):
        if not isinstance(p, dict):
            _die(f'plugins[{i}]: expected object')
        tier = p.get('tier')
        if tier == 'built-in':
            continue
        catalog_plugins.append((i, p))

    rel_data = DATA_PATH.relative_to(ROOT)
    rel_script = Path(__file__).resolve().relative_to(ROOT)

    lines: list[str] = [
        '---',
        'audience: data steward, plugin consumer',
        '---',
        '',
        '# Plugin catalog',
        '',
        'Binoc ships a capable [standard library](../../plugin-developers/explanation/plugin-model.md) '
        '(`binoc-stdlib`) plus first-party format packs. Most format packs are '
        'compiled into the fat `binoc` wheel; SQLite remains an explicit opt-in '
        'pack and is not published as a separate PyPI wheel.',
        '',
        'To find a match, compare your filenames (suffixes) and, when available, '
        'detected media types to the tables under each plugin.',
        '',
        'For package ids that may appear in changelog output, see the '
        '[plugin registry](plugin-registry.md).',
        '',
        '!!! tip "Publishing or listing a plugin"',
        '',
        '    If you maintain a plugin and want it listed here, see '
        '[Publish a plugin](../../plugin-developers/howto/publish-a-plugin.md).',
        '',
        '!!! note "Generated page"',
        '',
        f'    Entries are maintained in `{rel_data}` at the repository root. '
        f'Maintainers regenerate this Markdown with `{rel_script}` '
        f'(`just docs-plugin-catalog`).',
        '',
    ]

    if not catalog_plugins:
        lines.append('_No plugins are listed in the catalog yet._')
        lines.append('')
    else:
        for original_idx, p in catalog_plugins:
            lines.extend(_plugin_section(p, original_idx))

    lines.append('## Catalog file for tools')
    lines.append('')
    lines.append(
        f'The canonical data lives in `{rel_data}` (JSON). Hosts that suggest plugins '
        'for unrecognized formats should read that file; dispatch fields describe '
        "the rule pack's advertised file selectors."
    )
    lines.append('')

    OUT_PATH.write_text('\n'.join(lines), encoding='utf-8')
    print(f'Wrote {OUT_PATH.relative_to(ROOT)} ({len(catalog_plugins)} plugin(s)).')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
