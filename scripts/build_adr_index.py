#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Regenerate docs/adr/README.md from the front matter of docs/adr/*.md.

Each ADR begins with a level-1 heading and a `**Date:** YYYY-MM-DD` /
`**Status:** ...` block. We extract those, sort by date, and emit a
flat list grouped by status. See ADR
2026-04-17-documentation_platform_and_info_design.md §5.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ADR_DIR = ROOT / "docs" / "adr"
INDEX_PATH = ADR_DIR / "README.md"
MKDOCS_YML = ROOT / "mkdocs.yml"
SKIP_FILES = {"README.md", "TEMPLATE.md", "index.md"}
NAV_BEGIN = "# BEGIN-ADR-NAV"
NAV_END = "# END-ADR-NAV"

TITLE_RE = re.compile(r"^#\s+(.+?)\s*$", re.MULTILINE)
DATE_RE = re.compile(r"^\*\*Date:\*\*\s*([0-9]{4}-[0-9]{2}-[0-9]{2})", re.MULTILINE)
STATUS_RE = re.compile(r"^\*\*Status:\*\*\s*(.+?)\s*$", re.MULTILINE)


@dataclass
class Adr:
    path: Path
    title: str
    date: str
    status: str

    @property
    def filename(self) -> str:
        return self.path.name


def parse_adr(path: Path) -> Adr | None:
    text = path.read_text(encoding="utf-8")
    title_match = TITLE_RE.search(text)
    date_match = DATE_RE.search(text)
    status_match = STATUS_RE.search(text)
    if not (title_match and date_match):
        return None
    return Adr(
        path=path,
        title=title_match.group(1).strip(),
        date=date_match.group(1).strip(),
        status=(status_match.group(1).strip() if status_match else "Unknown"),
    )


def main() -> int:
    adrs: list[Adr] = []
    for path in sorted(ADR_DIR.glob("*.md")):
        if path.name in SKIP_FILES:
            continue
        adr = parse_adr(path)
        if adr is None:
            print(f"warning: could not parse front matter in {path}", file=sys.stderr)
            continue
        adrs.append(adr)

    adrs.sort(key=lambda a: (a.date, a.title))

    lines: list[str] = []
    lines.append("# Architectural Decisions")
    lines.append("")
    lines.append(
        "ADRs (Architecture Decision Records) capture the rationale behind binoc's "
        "design — including alternatives that were considered and rejected. "
        "They are the canonical long-form record of the project's reasoning."
    )
    lines.append("")
    lines.append(
        "Newer entries appear first. Each entry shows its date and current "
        "status. Create a new ADR with `just adr <title>`. See the "
        "[Documentation platform ADR](2026-04-17-documentation_platform_and_info_design.md) "
        "for how this index is produced and how ADRs fit into the docs site."
    )
    lines.append("")
    lines.append("| Date | Title | Status |")
    lines.append("|---|---|---|")
    for adr in reversed(adrs):
        lines.append(f"| {adr.date} | [{adr.title}]({adr.filename}) | {adr.status} |")
    lines.append("")

    output = "\n".join(lines)
    INDEX_PATH.write_text(output, encoding="utf-8")
    print(f"Wrote {INDEX_PATH.relative_to(ROOT)} ({len(adrs)} ADRs).")

    update_mkdocs_nav(adrs)
    return 0


def update_mkdocs_nav(adrs: list[Adr]) -> None:
    """Rewrite the ADR section of mkdocs.yml between BEGIN-ADR-NAV / END-ADR-NAV
    sentinels. Newest first, matching the README order."""
    text = MKDOCS_YML.read_text(encoding="utf-8")
    begin_match = re.search(rf"^(\s*){re.escape(NAV_BEGIN)}\s*$", text, re.MULTILINE)
    end_match = re.search(rf"^(\s*){re.escape(NAV_END)}\s*$", text, re.MULTILINE)
    if not (begin_match and end_match):
        print(
            f"warning: sentinels {NAV_BEGIN}/{NAV_END} not found in mkdocs.yml; "
            "skipping nav update",
            file=sys.stderr,
        )
        return

    indent = begin_match.group(1)
    nav_lines = [f"{indent}{NAV_BEGIN}"]
    nav_lines.append(f"{indent}- adr/README.md")
    for adr in reversed(adrs):
        # Quote titles defensively: many ADRs have colons in their H1 (e.g.
        # "Transformer dispatch: bottom-up..."), which is invalid unquoted YAML.
        title_quoted = "'" + adr.title.replace("'", "''") + "'"
        nav_lines.append(f"{indent}- {title_quoted}: adr/{adr.filename}")
    nav_lines.append(f"{indent}{NAV_END}")

    new_text = text[: begin_match.start()] + "\n".join(nav_lines) + text[end_match.end() :]
    if new_text != text:
        MKDOCS_YML.write_text(new_text, encoding="utf-8")
        print(f"Updated {MKDOCS_YML.relative_to(ROOT)} ADR nav block.")


if __name__ == "__main__":
    raise SystemExit(main())
