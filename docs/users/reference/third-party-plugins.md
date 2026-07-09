---
audience: data steward, plugin consumer
---

# Third-party plugins

Binoc ships a capable [standard library](../../plugin-developers/explanation/plugin-model.md) (`binoc-stdlib`) and first-party format packs in the `binoc` wheel. Community plugins listed below extend binoc for formats outside that first-party bundle.

To find a match, compare your filenames (suffixes) and, when available, detected media types to the tables under each plugin. Once you find one, install the package and configure any dataset semantics it documents.

For built-in and in-tree plugins that may also appear in changelog output, including the first-party bundled packs, see the [plugin registry](plugin-registry.md).

!!! tip "Publishing or listing a plugin"

    If you maintain a plugin and want it listed here, see [Publish a plugin](../../plugin-developers/howto/publish-a-plugin.md).

!!! note "Generated page"

    Entries are filtered from third-party entries in `plugin_registry.json` at the repository root. Maintainers regenerate this Markdown with `scripts/build_third_party_plugins_page.py` (`just docs-plugin-catalog`).

_No third-party plugins are listed in the catalog yet._

## Catalog file for tools

The canonical data lives in `plugin_registry.json` (JSON). Hosts that suggest plugins for unrecognized formats should read entries with `tier: third-party`; dispatch fields describe each rule pack's advertised file selectors.
