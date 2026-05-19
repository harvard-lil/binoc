---
audience: data steward, plugin consumer
---

# Third-party plugins

Binoc ships a capable [standard library](../explanation/plugin-model.md) (`binoc-stdlib`), but some datasets use formats that need a dedicated comparator. Install one of the **add-on plugins** below when your snapshots include those kinds of files.

To find a match, compare your filenames (suffixes) and, when available, detected media types to the tables under each plugin. Once you find one, install the package and add its comparator names to your [dataset config](dataset-config.md) where needed.

!!! tip "Publishing or listing a plugin"

    If you maintain a plugin and want it listed here, see [Publish a plugin](../howto/publish-a-plugin.md).

!!! note "Generated page"

    Entries are maintained in `third_party_plugins.json` at the repository root. Maintainers regenerate this Markdown with `scripts/build_third_party_plugins_page.py` (`just docs-plugin-catalog`).

## binoc-sqlite

Compares two SQLite database files: table/column layout, column types, primary keys, and per-table row counts. Useful when a dataset ships as `.db` / `.sqlite` snapshots instead of flat files.

- **Repository:** [https://github.com/harvard-lil/binoc](https://github.com/harvard-lil/binoc)

- **More detail:** [https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite](https://github.com/harvard-lil/binoc/tree/main/model-plugins/binoc-sqlite)

- **PyPI:** `binoc-sqlite` · **crates.io:** `binoc-sqlite`

### Install

After you install the package (for example from PyPI), binoc picks it up via the standard entry-point group — see [Install and use plugins](../howto/install-and-use-plugins.md) and [Plugin discovery](plugin-discovery.md).

Published packages declare discovery metadata like this:

```toml
[project.entry-points."binoc.plugins"]
binoc-sqlite = "binoc_sqlite"
```

This distribution is a **native Rust** plugin (`native_rust_module`): the target is a module name, not `module:function`.

### When it handles your files

A file is routed to this plugin when **either** its path matches one of the **extensions** or its detected **media type** matches (same rules as the rest of the pipeline). Ordering relative to other plugins and the standard library is up to your [dataset config](dataset-config.md).

| Field | Value |
|---|---|
| `extensions` | `.sqlite`, `.sqlite3`, `.db` |
| `media_types` | `application/vnd.sqlite3`, `application/x-sqlite3` |
| `scope` | `Files` |
| `handles_identical` | `false` |

*Labels you may see in a changeset (not used for routing):* `sqlite_database`, `sqlite_table`

## Catalog file for tools

The canonical data lives in `third_party_plugins.json` (JSON). Hosts that suggest plugins for unrecognized formats should read that file; dispatch fields mirror [`ComparatorDescriptor`](https://docs.rs/binoc-sdk/latest/binoc_sdk/struct.ComparatorDescriptor.html) in binoc-sdk.
