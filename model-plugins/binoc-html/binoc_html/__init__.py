"""Simple HTML renderer plugin for binoc.

Renders changesets as a self-contained HTML changelog.
"""

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
            "<html><head>",
            f"<title>{escape(title)}</title>",
            "<style>",
            "body { font-family: system-ui, sans-serif; max-width: 48em; margin: 2em auto; color: #333; }",
            "h1 { border-bottom: 2px solid #e0e0e0; padding-bottom: .3em; }",
            "h2 { color: #555; }",
            ".node { margin: .5em 0 .5em 1.5em; padding: .3em .5em; border-left: 3px solid #ccc; }",
            ".add { border-color: #2a2; }",
            ".remove { border-color: #c22; }",
            ".modify { border-color: #da0; }",
            ".path { font-family: monospace; font-size: .9em; }",
            ".summary { color: #666; }",
            ".tag { display: inline-block; background: #e8e8e8; border-radius: 3px; padding: 0 .4em; font-size: .8em; margin-left: .3em; }",
            "</style>",
            "</head><body>",
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


def _render_node(node, parts, depth=0):
    kind = node.kind
    css_class = "node"
    if kind in ("add",):
        css_class += " add"
    elif kind in ("remove",):
        css_class += " remove"
    elif kind in ("modify",):
        css_class += " modify"

    parts.append(f'<div class="{css_class}">')
    parts.append(f'<span class="path">{escape(node.path)}</span>')
    parts.append(f" <strong>{escape(kind)}</strong>")

    for tag in node.tags:
        parts.append(f'<span class="tag">{escape(tag)}</span>')

    if node.summary:
        parts.append(f'<div class="summary">{escape(node.summary)}</div>')

    for child in node.children:
        _render_node(child, parts, depth + 1)

    parts.append("</div>")


def register(registry):
    """Entry point called by binoc plugin discovery."""
    registry.register_renderer("binoc.html", HtmlRenderer())
