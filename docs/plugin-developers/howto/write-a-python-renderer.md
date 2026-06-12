---
audience: Python plugin author
---

# Write a Python renderer

**Goal.** Build a renderer in Python that turns one or more
changesets into a new output format (HTML, a CI-check summary, a
custom JSON shape, …), ending with a working plugin you can drive
from `binoc diff -o ...`.

**Prerequisites.**
- `pip install binoc`.
- Familiarity with the
  [changeset JSON schema](../../users/reference/changeset-schema.md).

## The minimal shape

A renderer is a small class with `name`, optional `file_extension`,
and `render(changesets, config)`:

```python
from html import escape


class HtmlRenderer:
    name = "binoc.html"
    file_extension = "html"

    def render(self, changesets, config):
        title = (
            config.get("title", "Changelog")
            if isinstance(config, dict)
            else "Changelog"
        )
        parts = [
            "<!DOCTYPE html>",
            f"<html><head><title>{escape(title)}</title></head><body>",
            f"<h1>{escape(title)}</h1>",
        ]
        for changeset in changesets:
            parts.append(
                f"<h2>{escape(str(changeset.from_snapshot))} &rarr; "
                f"{escape(str(changeset.to_snapshot))}</h2>"
            )
            root = changeset.root
            if root is None:
                parts.append("<p>No changes detected.</p>")
            else:
                _render_node(root, parts)
        parts.append("</body></html>")
        return "\n".join(parts)


def _render_node(node, parts):
    parts.append(f"<div><code>{escape(node.path)}</code> "
                 f"<strong>{escape(node.action)}</strong>")
    for tag in node.tags:
        parts.append(f" <span>{escape(tag)}</span>")
    if node.summary:
        parts.append(f"<div>{escape(node.summary)}</div>")
    for child in node.children:
        _render_node(child, parts)
    parts.append("</div>")


def register(registry):
    registry.register_renderer("binoc.html", HtmlRenderer())
```

Key points:

- **`name`** is the renderer name referenced from the CLI
  (`binoc diff A B --format binoc.html`) and from dataset config
  (`output.binoc-html.*` for any renderer-specific settings).
- **`file_extension`** (optional) lets `binoc diff -o changelog.html`
  infer the renderer from the extension. Without it, users must
  pass `binoc.html:path`.
- **`render(changesets, config)`** receives a list of `Changeset`
  objects and the renderer's own config section (a plain dict).
  Return a string — the rendered output. The CLI writes it to
  stdout or the designated file.

The `binoc-html` model plugin is the full runnable example; see
[`model-plugins/binoc-html/`](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-html).

## Use it without packaging

```python
import binoc

config = binoc.Config.default()
config.add_renderer(HtmlRenderer())

changeset = binoc.diff("snapshot-a", "snapshot-b", config=config)
print(binoc.render(changesets=[changeset], renderer="binoc.html", config={}))
```

For distribution, see [Publish a plugin](publish-a-plugin.md).

## Renderer config

Each renderer gets its own section of the dataset config (per
[Renderer config ADR](../../adr/2026-03-09-renderer_config.md)). Access it via the
`config` argument:

```yaml
# dataset.yaml
output:
  binoc-html:
    title: "Q4 data release"
```

```python
def render(self, changesets, config):
    title = config.get("title", "Changelog") if isinstance(config, dict) else "Changelog"
    ...
```

Renderers deserialize their own config — no coordination with core
required for new knobs.

## Apply renderer-side grouping

If your renderer wants grouped sections, read an ordered `groups` list from
config and match each node's tags against it. Preserve declared order and let
the first matching group win. The Markdown renderer is the reference; see its source
and [Significance classification](../../users/explanation/significance-classification.md)
for the pattern.

## Testing

Call `render()` directly with synthetic changesets, or round-trip a
real diff:

```python
import binoc

changeset = binoc.diff("tests/snap-a", "tests/snap-b")
html = HtmlRenderer().render([changeset], {"title": "Test"})
assert "<h1>Test</h1>" in html
```

## Where to go next

- [Publish a plugin](publish-a-plugin.md) — package the renderer so
  users get `binoc diff A B -o CHANGES.html` for free.
- [Save and render changesets](../../users/howto/save-and-render-changesets.md) — how
  users will drive your renderer from the CLI.
- [Output routing and CLI UX ADR](../../adr/2026-03-09-output_routing_and_cli_ux.md)
  — the decision record for how renderers are selected.
