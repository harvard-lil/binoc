"""Tests for the binoc-html outputter plugin."""

import binoc
from binoc_html import HtmlOutputter


def test_html_outputter_renders_basic_migration():
    root = binoc.DiffNode(
        "modify",
        "directory",
        "root",
        children=[
            binoc.DiffNode("add", "file", "root/new.txt", summary="New file added"),
            binoc.DiffNode("remove", "file", "root/old.txt"),
        ],
    )
    migration = binoc.Migration("v1", "v2", root)
    outputter = HtmlOutputter()
    html = outputter.render([migration], {})

    assert "<!DOCTYPE html>" in html
    assert "v1" in html
    assert "v2" in html
    assert "root/new.txt" in html
    assert "root/old.txt" in html
    assert "New file added" in html


def test_html_outputter_empty_migration():
    migration = binoc.Migration("v1", "v2")
    outputter = HtmlOutputter()
    html = outputter.render([migration], {})

    assert "No changes detected" in html


def test_html_outputter_custom_title():
    migration = binoc.Migration("v1", "v2")
    outputter = HtmlOutputter()
    html = outputter.render([migration], {"title": "My Dataset"})

    assert "My Dataset" in html


def test_register_adds_to_registry():
    registry = binoc.PluginRegistry.default()
    from binoc_html import register

    register(registry)
    assert "binoc.html" in registry.list_outputters()
