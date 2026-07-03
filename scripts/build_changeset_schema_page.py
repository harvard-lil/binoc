#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Render docs/users/reference/changeset-schema.md from the generated JSON Schema.

The authoritative schema lives in `docs/users/reference/changeset-schema.json`,
emitted by the `gen-changeset-schema` Rust binary from the IR types (see
`binoc-sdk/src/bin/gen_changeset_schema.rs` and ADR
`2026-04-17-documentation_platform_and_info_design.md` Open Question 1).

This renderer turns that schema into a human-readable Markdown reference
page with one table per defined type, plus a short preamble. It stays
intentionally format-specific: it targets the subset of JSON Schema that
`schemars` actually emits for our IR types, not arbitrary schemas.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA_PATH = ROOT / "docs" / "users" / "reference" / "changeset-schema.json"
PAGE_PATH = ROOT / "docs" / "users" / "reference" / "changeset-schema.md"

# Order the defs in a reading-friendly sequence. Any type present in the
# schema but not listed here gets appended alphabetically at the end so the
# page stays correct if new types are added but a human hasn't curated the
# order yet.
TYPE_ORDER = [
    "Changeset",
    "DiffNode",
    "ItemPair",
    "ItemRef",
    "ArtifactDescriptor",
    "ArtifactFormat",
    "ArtifactSubject",
]

PREAMBLE = """---
audience: pipeline integrator
---

# Changeset JSON schema

<!--
  GENERATED FILE — do not edit by hand.
  Source of truth: binoc-sdk IR types (binoc-sdk/src/ir.rs, src/types.rs).
  Regenerate with `just docs-schema`.
-->

A changeset JSON document is a tree of [`DiffNode`](#diffnode) values wrapped
in a [`Changeset`](#changeset) envelope. The shape is deliberately open:
`action`, `item_type`, and `tags` are unbounded strings that plugins extend.
Consumers should treat unknown values as opaque and fall through to generic
handling.

The machine-readable schema (JSON Schema draft 2020-12) lives alongside this
page at [`changeset-schema.json`](changeset-schema.json) and is generated
from the Rust IR types. The tables below are a rendering of that schema.

## What is *not* in the changeset

- **Significance classification.** Changelog grouping is a renderer
  concern, applied at render time from configured headings and tag lists.
  The IR is judgment-free. See
  [Significance classification](../explanation/significance-classification.md).
- **Transient session data.** `source_items` and `artifacts` are wire-visible
  because the plugin ABI carries them across (potentially process-isolated)
  boundaries, but they are stripped at the output boundary via
  `DiffNode::strip_transient` before changeset JSON is written for users.
  They appear in the schema below, but callers writing changeset files
  should not expect to see populated values. See the
  [Transient fields on wire ADR](../../adr/2026-04-16-transient_fields_on_wire.md).

## Stability

The IR is still evolving. Once a first stable version is cut, the schema
will be versioned and this page will document compatibility guarantees.
Until then, treat the shape as informative and pin your downstream pipeline
to known plugin versions.

## Where to go next

- [IR and changesets](../../plugin-developers/explanation/ir-and-changesets.md) — the conceptual
  model behind the shape documented here.
- [Save and render changesets](../howto/save-and-render-changesets.md) —
  producing and combining changeset JSON from the CLI.
- [Extract changed data](../howto/extract-changed-data.md) — using the
  provenance fields to pull actual changed content out of a changeset.

"""


@dataclass
class Field:
    name: str
    type_display: str
    required: bool
    description: str


def slug(name: str) -> str:
    return name.lower()


def clean_description(text: str | None) -> str:
    """Flatten a multi-line doc comment into a single table-cell string.

    Strips leading/trailing whitespace, collapses newlines to spaces, and
    removes Markdown list artifacts that don't render inside a table cell.
    Doc-comment references like `[`Foo::bar`]` are left intact — rustdoc
    syntax rendered as backticks is acceptable in prose.
    """
    if not text:
        return ""
    cleaned = text.replace("\r\n", "\n").strip()
    # Collapse paragraph breaks, then any remaining newlines.
    cleaned = cleaned.replace("\n\n", " ").replace("\n", " ")
    # Table cells can't contain raw pipes; escape any that slip through.
    cleaned = cleaned.replace("|", "\\|")
    # Collapse runs of whitespace.
    return " ".join(cleaned.split())


def type_display(prop: dict | bool) -> str:
    """Render a JSON-Schema type fragment as short human prose.

    Handles the shapes schemars actually emits: `type` scalars, `type`
    arrays (for nullable primitives), `$ref`, `anyOf` (nullable refs),
    `array` with `items`, `object` with `additionalProperties`, and
    `enum` strings.
    """
    if prop is True:
        return "any"
    if prop is False:
        return "never"

    # Nullable $ref: anyOf: [{$ref}, {type: null}]
    if "anyOf" in prop:
        branches = prop["anyOf"]
        non_null = [b for b in branches if b.get("type") != "null"]
        has_null = any(b.get("type") == "null" for b in branches)
        if len(non_null) == 1:
            inner = type_display(non_null[0])
            return f"{inner} \\| null" if has_null else inner
        # Rare: untagged union. Render each branch.
        return " \\| ".join(type_display(b) for b in branches)

    if "$ref" in prop:
        ref = prop["$ref"].rsplit("/", 1)[-1]
        return f"[`{ref}`](#{slug(ref)})"

    t = prop.get("type")
    if isinstance(t, list):
        # e.g. ["string", "null"] — nullable primitive
        non_null = [x for x in t if x != "null"]
        base = non_null[0] if non_null else "null"
        return f"{base} \\| null" if "null" in t and non_null else base

    if t == "array":
        items = prop.get("items", {})
        return f"array of {type_display(items)}"

    if t == "object":
        extra = prop.get("additionalProperties")
        if isinstance(extra, dict):
            return f"object (map of string -> {type_display(extra)})"
        if extra is True:
            return "object (free-form)"
        return "object"

    if t == "integer":
        fmt = prop.get("format")
        return f"integer ({fmt})" if fmt else "integer"

    if t == "string" and "enum" in prop:
        choices = ", ".join(f"`{c}`" for c in prop["enum"])
        return f"string (one of {choices})"

    if t is None:
        # Fallback for completely unconstrained values.
        return "any"

    return t  # string, boolean, number, null


def fields_from_object(def_schema: dict) -> list[Field]:
    required = set(def_schema.get("required", []))
    props = def_schema.get("properties", {})
    out = []
    for name, prop in sorted(props.items()):
        description = clean_description(prop.get("description")) if isinstance(prop, dict) else ""
        out.append(
            Field(
                name=name,
                type_display=type_display(prop),
                required=name in required,
                description=description,
            )
        )
    return out


def render_object(name: str, def_schema: dict) -> str:
    lines = [f"### `{name}`", ""]
    desc = clean_description(def_schema.get("description"))
    if desc:
        lines.extend([desc, ""])

    fields = fields_from_object(def_schema)
    if not fields:
        lines.append("*(no fields)*")
        lines.append("")
        return "\n".join(lines)

    lines.append("| Field | Type | Required | Description |")
    lines.append("|---|---|---|---|")
    for f in fields:
        req = "yes" if f.required else "no"
        desc = f.description or ""
        lines.append(f"| `{f.name}` | {f.type_display} | {req} | {desc} |")
    lines.append("")
    return "\n".join(lines)


def render_enum(name: str, def_schema: dict) -> str:
    lines = [f"### `{name}`", ""]
    desc = clean_description(def_schema.get("description"))
    if desc:
        lines.extend([desc, ""])
    lines.append("String enum. One of:")
    lines.append("")
    for choice in def_schema.get("enum", []):
        lines.append(f"- `{choice}`")
    lines.append("")
    return "\n".join(lines)


def render_def(name: str, def_schema: dict) -> str:
    if def_schema.get("type") == "object" or "properties" in def_schema:
        return render_object(name, def_schema)
    if "enum" in def_schema:
        return render_enum(name, def_schema)
    # Shouldn't happen with our current types; surface it rather than silently drop.
    return f"### `{name}`\n\n*(unrecognised schema shape)*\n"


def ordered_def_names(defs: dict) -> list[str]:
    seen = set()
    ordered: list[str] = []
    for name in TYPE_ORDER:
        if name in defs:
            ordered.append(name)
            seen.add(name)
    for name in sorted(defs):
        if name not in seen:
            ordered.append(name)
    return ordered


def render(schema: dict) -> str:
    parts = [PREAMBLE, "## Types", ""]

    # The root schema itself is the Changeset type; merge it into `$defs` for
    # uniform rendering so Changeset gets the same table treatment as other types.
    defs = dict(schema.get("$defs", {}))
    root_name = schema.get("title") or "Changeset"
    defs.setdefault(
        root_name,
        {
            "description": schema.get("description"),
            "type": schema.get("type", "object"),
            "properties": schema.get("properties", {}),
            "required": schema.get("required", []),
        },
    )

    for name in ordered_def_names(defs):
        parts.append(render_def(name, defs[name]))
    return "\n".join(parts).rstrip() + "\n"


def main() -> int:
    if not SCHEMA_PATH.exists():
        print(
            f"error: {SCHEMA_PATH.relative_to(ROOT)} does not exist; "
            "run `cargo run -p binoc-sdk --features schema --bin gen-changeset-schema` first "
            "(or `just docs-schema`).",
            file=sys.stderr,
        )
        return 1

    schema = json.loads(SCHEMA_PATH.read_text())
    rendered = render(schema)

    if PAGE_PATH.exists() and PAGE_PATH.read_text() == rendered:
        print(f"{PAGE_PATH.relative_to(ROOT)} is up to date.")
        return 0

    PAGE_PATH.write_text(rendered)
    print(f"{PAGE_PATH.relative_to(ROOT)} updated.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
