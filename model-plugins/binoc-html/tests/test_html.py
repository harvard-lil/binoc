"""Tests for the binoc-html renderer plugin."""

import binoc
from binoc_html import HtmlRenderer


def test_html_renderer_renders_basic_changeset():
    root = binoc.DiffNode(
        "modify",
        "directory",
        "root",
        children=[
            binoc.DiffNode("add", "file", "root/new.txt", summary="New file added"),
            binoc.DiffNode("remove", "file", "root/old.txt"),
        ],
    )
    changeset = binoc.Changeset("v1", "v2", root)
    renderer = HtmlRenderer()
    html = renderer.render([changeset], {})

    assert "<!DOCTYPE html>" in html
    assert "v1" in html
    assert "v2" in html
    assert "root/new.txt" in html
    assert "root/old.txt" in html
    assert "New file added" in html


def test_html_renderer_empty_changeset():
    changeset = binoc.Changeset("v1", "v2")
    renderer = HtmlRenderer()
    html = renderer.render([changeset], {})

    assert "No changes detected" in html


def test_html_renderer_custom_title():
    changeset = binoc.Changeset("v1", "v2")
    renderer = HtmlRenderer()
    html = renderer.render([changeset], {"title": "My Dataset"})

    assert "My Dataset" in html


def test_register_adds_to_registry():
    registry = binoc.PluginRegistry.default()
    from binoc_html import register

    register(registry)
    assert "binoc.html" in registry.list_renderers()
