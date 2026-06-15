#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["tomli>=2.0.1; python_version < '3.11'"]
# ///
"""Render a user-facing examples gallery from the shared workspace vectors.

The page is generated from `test-vectors/*/manifest.toml`, the committed
snapshot source trees, and each vector's saved Markdown changelog snapshot.
Unlike the first manifest-inventory pass, this renderer is written for users:
each vector becomes a runnable example with setup notes, a command to try, and
the rendered changelog that binoc produces.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised on Python 3.10 only
    import tomli as tomllib

ROOT = Path(__file__).resolve().parent.parent
VECTORS_ROOT = ROOT / "test-vectors"
PAGE_PATH = ROOT / "docs" / "users" / "explanation" / "test-vectors-gallery.md"
REPO_BASE_URL = "https://github.com/harvard-lil/binoc/tree/main"

SNAPSHOT_SAMPLE_LIMIT = 4
INDEX_OUTPUT_LIMIT = 100
INDEX_SUMMARY_LIMIT = 120

# Shown only in manifest-derived text; omit the **Setup** bullet when this is the note.
DEFAULT_PIPELINE_SETUP_NOTE = (
    "No extra plugin or config is required. The default binoc pipeline is enough."
)

EXPECTED_KEY_ORDER = [
    "root_kind",
    "root_action",
    "child_count",
    "has_tags",
    "significance",
]

SNAP_HEADER_RE = re.compile(r"\A---\n.*?\n---\n", re.DOTALL)


@dataclass
class DocsMeta:
    summary: str | None = None
    setup: str | None = None


@dataclass
class ManifestConfig:
    dataset: dict[str, Any] | None = None
    output: dict[str, Any] | None = None

    @property
    def has_customization(self) -> bool:
        return bool(self.dataset or self.output)


@dataclass
class VectorDoc:
    name: str
    description: str
    summary: str
    setup_note: str
    tags: list[str]
    root_file: str | None
    config: ManifestConfig
    snapshot_a_summary: str
    snapshot_b_summary: str
    source_url: str
    command: str
    config_yaml: str | None
    changelog_markdown: str
    output_excerpt: str

    @property
    def anchor(self) -> str:
        return self.name


def _die(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def _read_toml(path: Path) -> dict[str, Any]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _read_changelog_snapshot(path: Path) -> str:
    text = path.read_text(encoding="utf-8").strip()
    text = SNAP_HEADER_RE.sub("", text, count=1).strip()
    if not text:
        _die(f"{path}: expected a non-empty changelog snapshot")
    return text


def _snapshot_summary(snapshot_dir: Path) -> str:
    files = sorted(
        path.relative_to(snapshot_dir).as_posix()
        for path in snapshot_dir.rglob("*")
        if path.is_file() and path.name != ".gitkeep"
    )
    count = len(files)
    if count == 0:
        return "0 files (empty snapshot)"

    sample = ", ".join(f"`{path}`" for path in files[:SNAPSHOT_SAMPLE_LIMIT])
    if count > SNAPSHOT_SAMPLE_LIMIT:
        sample += f", +{count - SNAPSHOT_SAMPLE_LIMIT} more"
    noun = "file" if count == 1 else "files"
    return f"{count} {noun} — {sample}"


def _compact(text: str) -> str:
    return " ".join(text.split())


def _truncate(text: str, limit: int) -> str:
    compact = _compact(text)
    if len(compact) <= limit:
        return compact
    return compact[: limit - 1].rstrip() + "…"


def _validate_str_list(value: Any, path: str) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        _die(f"{path}: expected a list of strings")
    return value


def _parse_docs(manifest_path: Path, raw: Any) -> DocsMeta:
    if raw is None:
        return DocsMeta()
    if not isinstance(raw, dict):
        _die(f"{manifest_path}: [docs] must be a table")

    summary = raw.get("summary")
    setup = raw.get("setup")
    if summary is not None and not isinstance(summary, str):
        _die(f"{manifest_path}: [docs].summary must be a string")
    if setup is not None and not isinstance(setup, str):
        _die(f"{manifest_path}: [docs].setup must be a string")
    return DocsMeta(summary=summary, setup=setup)


def _parse_config(manifest_path: Path, raw: Any) -> ManifestConfig:
    if raw is None:
        return ManifestConfig()
    if not isinstance(raw, dict):
        _die(f"{manifest_path}: [config] must be a table")

    dataset = raw.get("dataset")
    output = raw.get("output")

    if dataset is not None and not isinstance(dataset, dict):
        _die(f"{manifest_path}: [config].dataset must be a table")
    if output is not None and not isinstance(output, dict):
        _die(f"{manifest_path}: [config].output must be a table")
    for key in raw:
        if key not in {"dataset", "output"}:
            _die(f"{manifest_path}: [config] unknown key {key!r}")

    return ManifestConfig(
        dataset=dataset,
        output=output,
    )


def _yaml_scalar(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(value)
    if isinstance(value, str):
        if value == "" or any(ch in value for ch in ":#{}[]\n'\""):
            escaped = value.replace("\\", "\\\\").replace('"', '\\"')
            return f'"{escaped}"'
        return value
    if value is None:
        return "null"
    _die(f"Unsupported config value for YAML rendering: {value!r}")


def _yaml_lines(value: Any, indent: int = 0) -> list[str]:
    prefix = " " * indent
    if isinstance(value, dict):
        if not value:
            return [f"{prefix}{{}}"]
        lines: list[str] = []
        for key, child in value.items():
            if not isinstance(key, str):
                _die(f"Unsupported non-string config key: {key!r}")
            if isinstance(child, (dict, list)):
                lines.append(f"{prefix}{key}:")
                lines.extend(_yaml_lines(child, indent + 2))
            else:
                lines.append(f"{prefix}{key}: {_yaml_scalar(child)}")
        return lines
    if isinstance(value, list):
        if not value:
            return [f"{prefix}[]"]
        lines = []
        for child in value:
            if isinstance(child, (dict, list)):
                nested = _yaml_lines(child, indent + 2)
                first, *rest = nested
                lines.append(f"{prefix}- {first.strip()}")
                lines.extend(rest)
            else:
                lines.append(f"{prefix}- {_yaml_scalar(child)}")
        return lines
    return [f"{prefix}{_yaml_scalar(value)}"]


def _render_config_yaml(config: ManifestConfig) -> str | None:
    data: dict[str, Any] = {}
    if config.dataset:
        data["dataset"] = config.dataset
    if config.output:
        data["output"] = config.output
    if not data:
        return None
    return "\n".join(_yaml_lines(data))


def _vector_paths(name: str, root_file: str | None) -> tuple[str, str]:
    base = f"./test-vectors-materialized/{name}"
    if root_file:
        return (f"{base}/snapshot-a/{root_file}", f"{base}/snapshot-b/{root_file}")
    return (f"{base}/snapshot-a", f"{base}/snapshot-b")


def _run_command(name: str, root_file: str | None, uses_config: bool) -> str:
    left, right = _vector_paths(name, root_file)
    lines = [
        "binoc diff \\",
        f"  {left} \\",
        f"  {right}",
    ]
    if uses_config:
        lines[-1] += " \\"
        lines.append(f"  --config /tmp/{name}.yaml")
    return "\n".join(lines)


def _output_excerpt(changelog_markdown: str) -> str:
    lines = [line.strip() for line in changelog_markdown.splitlines() if line.strip()]
    for line in lines:
        if line == "No changes detected.":
            return line
        if line.startswith("- "):
            bullet = re.sub(r"\*\*(.*?)\*\*", r"\1", line[2:])
            return _truncate(bullet, INDEX_OUTPUT_LIMIT)
    return _truncate(lines[-1] if lines else "", INDEX_OUTPUT_LIMIT)


def _setup_note(config: ManifestConfig, docs: DocsMeta) -> str:
    if docs.setup:
        return docs.setup.strip()
    if config.has_customization:
        return (
            "This example uses a custom dataset config to make the relevant "
            "correspondence behavior obvious."
        )
    return DEFAULT_PIPELINE_SETUP_NOTE


def _load_vectors() -> list[VectorDoc]:
    vectors: list[VectorDoc] = []
    for vector_dir in sorted(path for path in VECTORS_ROOT.iterdir() if path.is_dir()):
        manifest_path = vector_dir / "manifest.toml"
        if not manifest_path.exists():
            continue

        manifest = _read_toml(manifest_path)
        vector = manifest.get("vector")
        if not isinstance(vector, dict):
            _die(f"{manifest_path}: missing [vector] table")

        name = vector.get("name")
        description = vector.get("description")
        tags = vector.get("tags", [])
        root_file = vector.get("root_file")

        if not isinstance(name, str) or not name:
            _die(f"{manifest_path}: [vector].name must be a non-empty string")
        if not isinstance(description, str) or not description.strip():
            _die(f"{manifest_path}: [vector].description must be a non-empty string")
        if not isinstance(tags, list) or not all(isinstance(tag, str) for tag in tags):
            _die(f"{manifest_path}: [vector].tags must be an array of strings")
        if root_file is not None and not isinstance(root_file, str):
            _die(f"{manifest_path}: [vector].root_file must be a string when present")

        docs = _parse_docs(manifest_path, manifest.get("docs"))
        config = _parse_config(manifest_path, manifest.get("config"))
        config_yaml = _render_config_yaml(config)
        changelog_markdown = _read_changelog_snapshot(
            vector_dir / "expected-output" / "changelog.snap"
        )

        summary = (
            docs.summary.strip()
            if docs.summary
            else _truncate(description, INDEX_SUMMARY_LIMIT)
        )

        vectors.append(
            VectorDoc(
                name=name,
                description=description.strip(),
                summary=summary,
                setup_note=_setup_note(config, docs),
                tags=tags,
                root_file=root_file,
                config=config,
                snapshot_a_summary=_snapshot_summary(vector_dir / "snapshot-a"),
                snapshot_b_summary=_snapshot_summary(vector_dir / "snapshot-b"),
                source_url=f"{REPO_BASE_URL}/test-vectors/{name}",
                command=_run_command(name, root_file, config.has_customization),
                config_yaml=config_yaml,
                changelog_markdown=changelog_markdown,
                output_excerpt=_output_excerpt(changelog_markdown),
            )
        )
    return vectors


def _render_page(vectors: list[VectorDoc]) -> str:
    lines: list[str] = [
        "---",
        "audience: new user, data steward, archivist",
        "---",
        "",
        "# Examples gallery",
        "",
        "<!--",
        "  GENERATED FILE — do not edit by hand.",
        "  Source of truth: test-vectors/*/manifest.toml, committed snapshot trees,",
        "  and expected-output/changelog.snap files.",
        "  Regenerate with `just docs-vectors`.",
        "-->",
        "",
        "These are runnable examples from binoc's test suite. "
        "Each example links to its source folder on GitHub, tells you whether it needs any "
        "extra setup, gives you the exact command to run, and shows the Markdown changelog "
        "binoc is expected to print.",
        "",
        f"Binoc currently ships **{len(vectors)} shared examples** in this gallery.",
        "",
        "## One-time setup",
        "",
        "Clone the repository and materialize the archive-based fixtures once:",
        "",
        "```bash",
        "git clone https://github.com/harvard-lil/binoc",
        "cd binoc",
        "just materialize",
        "```",
        "",
        "## At a glance",
        "",
        "| Example | What it shows | Example output | Setup |",
        "|---|---|---|---|",
    ]

    for vector in vectors:
        setup = (
            "Custom config" if vector.config.has_customization else "Default pipeline"
        )
        lines.append(
            f"| [`{vector.name}`](#{vector.anchor}) | {vector.summary.replace('|', '\\|')} | "
            f"{vector.output_excerpt.replace('|', '\\|')} | {setup} |"
        )

    for vector in vectors:
        detail_lines: list[str] = [
            "",
            f"## {vector.name}",
            "",
            vector.summary,
            "",
            f"- **Browse source:** [{vector.name}]({vector.source_url})",
            f"- **Tags:** {', '.join(f'`{tag}`' for tag in vector.tags) if vector.tags else 'none'}",
            f"- **Snapshots:** `snapshot-a` has {vector.snapshot_a_summary}; `snapshot-b` has {vector.snapshot_b_summary}",
        ]
        if vector.setup_note != DEFAULT_PIPELINE_SETUP_NOTE:
            detail_lines.append(f"- **Setup:** {vector.setup_note}")
        lines.extend(detail_lines)
        if vector.config_yaml:
            lines.extend(
                [
                    f"Save this dataset config as `/tmp/{vector.name}.yaml`:",
                    "",
                    "```yaml",
                    vector.config_yaml,
                    "```",
                    "",
                ]
            )
        lines.extend(
            [
                "",
                "Run it:",
                "```bash",
                vector.command,
                "```",
                "Result:",
                "```markdown",
                vector.changelog_markdown,
                "```",
            ]
        )

    lines.append("")
    return "\n".join(lines)


def main() -> int:
    vectors = _load_vectors()
    output = _render_page(vectors)
    existing = PAGE_PATH.read_text(encoding="utf-8") if PAGE_PATH.exists() else None
    if existing == output:
        print(f"{PAGE_PATH.relative_to(ROOT)} is up to date.")
        return 0
    PAGE_PATH.write_text(output, encoding="utf-8")
    print(f"Wrote {PAGE_PATH.relative_to(ROOT)} ({len(vectors)} vectors).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
