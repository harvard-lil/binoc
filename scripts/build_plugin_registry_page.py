#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Emit docs/users/reference/plugin-registry.md from plugin_registry.json."""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
DATA_PATH = ROOT / 'plugin_registry.json'
OUT_PATH = ROOT / 'docs' / 'users' / 'reference' / 'plugin-registry.md'
PACKAGE_LABELS = {'pypi': 'PyPI', 'crate': 'Rust crate'}
TIER_LABELS = {
    'built-in': 'Built in',
    'first-party-bundled': 'First-party bundled',
    'first-party-opt-in': 'First-party opt-in',
    'first-party-add-on': 'First-party add-on',
    'third-party': 'Third-party',
}
TIER_DISTRIBUTION = {
    'built-in': 'Included with every binoc build.',
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
        _die(f'{path}: expected array')
    out: list[str] = []
    for i, x in enumerate(val):
        if not isinstance(x, str):
            _die(f'{path}[{i}]: expected string')
        out.append(x)
    return out


def _md_table(rows: list[tuple[str, str]]) -> str:
    lines = ['| Field | Value |', '|---|---|']
    for k, v in rows:
        v_esc = v.replace('|', '\\|').replace('\n', '<br>')
        lines.append(f'| {k} | {v_esc} |')
    return '\n'.join(lines)


def _plugin_section(p: dict[str, Any], idx: int) -> list[str]:
    base = f'plugins[{idx}]'
    _ = _as_str(p.get('id'), f'{base}.id')
    title = _as_str(p.get('title'), f'{base}.title')
    tier = _as_str(p.get('tier'), f'{base}.tier')
    if tier not in TIER_LABELS:
        _die(f'{base}.tier: unknown tier {tier!r}')
    summary = _as_str(p.get('summary'), f'{base}.summary')
    handles = _as_str(p.get('handles'), f'{base}.handles')
    produces = _as_str(p.get('produces'), f'{base}.produces')

    lines = [
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

    for key in ('repository', 'documentation'):
        value = p.get(key)
        if value is not None:
            lines.append(f'- **{key.title()}:** [{value}]({value})')
    if p.get('source_path') is not None:
        lines.append(f'- **Source path:** `{_as_str(p.get("source_path"), f"{base}.source_path")}`')
    pkgs = p.get('packages')
    if pkgs:
        if not isinstance(pkgs, dict):
            _die(f'{base}.packages: expected object')
        pkg_bits = []
        for k, v in pkgs.items():
            label = PACKAGE_LABELS.get(k, k.replace('_', ' ').title())
            pkg_bits.append(f'**{label}:** `{_as_str(v, f"{base}.packages.{k}")}`')
        lines.append('- ' + ' · '.join(pkg_bits))
    lines.append('')

    rule_packs = p.get('rule_packs')
    if not isinstance(rule_packs, list):
        _die(f'{base}.rule_packs: expected array')
    lines.append('### Rule packs')
    lines.append('')
    for ri, rule_pack in enumerate(rule_packs):
        rp = f'{base}.rule_packs[{ri}]'
        if not isinstance(rule_pack, dict):
            _die(f'{rp}: expected object')
        name = _as_str(rule_pack.get('name'), f'{rp}.name')
        dispatch = rule_pack.get('dispatch')
        if not isinstance(dispatch, dict):
            _die(f'{rp}.dispatch: expected object')
        rules = _as_str_list(rule_pack.get('rules'), f'{rp}.rules')
        lines.append(f'#### `{name}`')
        lines.append('')
        rows: list[tuple[str, str]] = []
        exts = dispatch.get('extensions')
        mts = dispatch.get('media_types')
        rows.append(
            (
                '`extensions`',
                ', '.join(f'`{x}`' for x in _as_str_list(exts, f'{rp}.dispatch.extensions'))
                if exts
                else '—',
            )
        )
        rows.append(
            (
                '`media_types`',
                ', '.join(f'`{x}`' for x in _as_str_list(mts, f'{rp}.dispatch.media_types'))
                if mts
                else '—',
            )
        )
        rows.append(
            ('`scope`', f'`{_as_str(dispatch.get("scope", "files"), f"{rp}.dispatch.scope")}`')
        )
        lines.append(_md_table(rows))
        lines.append('')
        lines.append('*Rule families supplied:* ' + ', '.join(f'`{r}`' for r in rules))
        lines.append('')
    return lines


def main() -> int:
    raw = json.loads(DATA_PATH.read_text(encoding='utf-8'))
    if not isinstance(raw, dict):
        _die('root JSON value must be an object')
    plugins = raw.get('plugins')
    if not isinstance(plugins, list):
        _die('.plugins: expected array')

    rel_data = DATA_PATH.relative_to(ROOT)
    rel_script = Path(__file__).resolve().relative_to(ROOT)
    lines: list[str] = [
        '---',
        'audience: data steward, plugin consumer',
        '---',
        '',
        '# Plugin registry',
        '',
        'This registry covers the plugins readers are most likely to encounter in changelog output: the built-in `binoc-stdlib` pack plus the in-tree `model-plugins/` packs.',
        '',
        'Use the package ids as stable anchors when linking from rendered changesets. The generated headings below match those ids one-for-one.',
        '',
        '!!! note "Generated page"',
        '',
        f'    Entries are maintained in `{rel_data}` at the repository root. Maintainers regenerate this Markdown with `{rel_script}`.',
        '',
    ]

    for i, p in enumerate(plugins):
        if not isinstance(p, dict):
            _die(f'plugins[{i}]: expected object')
        lines.extend(_plugin_section(p, i))

    OUT_PATH.write_text('\n'.join(lines), encoding='utf-8')
    print(f'Wrote {OUT_PATH.relative_to(ROOT)} ({len(plugins)} plugin(s)).')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
