# Output Routing and CLI UX

**Date:** 2026-03-09
**Status:** Implemented

## Problem

The original CLI defaulted to printing raw changeset JSON to stdout (`binoc diff a b` -> JSON), with human-readable Markdown only appearing as an automatic sidecar file when `--output` was specified. This was backwards for the common case: a human at a terminal wants to see what changed, not parse JSON. Meanwhile, the single `--output <path>` flag conflated two concerns — choosing where to write and choosing what format to write — and couldn't express "save JSON to a file *and* save Markdown to another file" in one invocation.

The sidecar model (write `changeset.json`, automatically get `changeset.md` alongside it) was also surprising: it created files the user didn't explicitly ask for, and there was no way to control which sidecar formats were produced without editing the dataset config's `renderers` list.

## Options Considered

### A: Keep JSON-to-stdout, add `--format markdown` (rejected)

Least disruptive change. Add `--format` to switch stdout output. Keep `--output` writing JSON with sidecars.

Rejected because it preserves the unintuitive default. The tool's value proposition is human-readable changelogs; the default experience should reflect that. Machine consumers can opt in to JSON.

### B: Separate `--save` for JSON, `--output` for formatted (rejected)

Two flags: `--save changeset.json` writes the raw changeset, `-o changelog.md` writes formatted output.

Rejected for adding an unnecessary concept split. JSON is just another output format — the distinction between "raw IR" and "formatted output" matters internally but shouldn't require the user to learn two flags.

### C: Markdown to stdout by default, repeatable `-o [format:]path` (chosen)

Stdout prints the human-readable format by default. `--format` switches what goes to stdout (e.g. `--format json` for piping). Repeatable `-o` writes to files with format inferred from extension or set explicitly via `format:path` prefix. `-q` suppresses stdout for CI use.

## Decision

Option C. The implementation:

- **Stdout** defaults to the first configured renderer (Markdown). `--format json` switches to raw changeset JSON. `--format <name>` accepts any registered renderer name, with `binoc.` prefix optional (so `markdown` and `binoc.markdown` both work). `-q`/`--quiet` suppresses stdout entirely.

- **`-o`/`--output` is repeatable.** Each value is parsed as an `OutputSpec` — either `format:path` (explicit) or a bare path (inferred). The split happens on the first colon, but only if the prefix contains no path separators (so `/tmp/file.json` isn't misread as format `"/tmp/file"`, path `"json"`).

- **Extension inference** checks two sources: the special `json` format (for `.json` files, meaning raw changeset IR), and the renderer registry's `file_extension()` values for everything else. If no renderer claims the extension, the CLI errors with a message suggesting `format:path` syntax. If multiple renderers claim the same extension, it errors listing the conflicting names.

- **`json` is not a renderer.** It's a reserved format name handled by the CLI via `output::to_json()`. This keeps the `Renderer` trait focused on human-readable rendering — JSON serialization doesn't use `OutputConfig`, doesn't do significance bucketing, and is conceptually different (it's the IR, not a view of it).

- **Both `diff` and `changelog` share the same output routing**, extracted into a `write_outputs()` function. The `changelog` command accepts the same flags.

## Examples

```
binoc diff a b                                  # markdown to stdout
binoc diff a b --format json                    # raw JSON to stdout
binoc diff a b -o changeset.json                # JSON to file, markdown to stdout
binoc diff a b -o changeset.json -o CHANGES.md  # both to files, markdown to stdout
binoc diff a b -o changeset.json -q             # JSON to file, no stdout
binoc diff a b -o json:output.dat               # explicit format for non-standard extension
binoc changelog changesets/*.json -o CHANGES.md # render saved changesets to file
```

## Consequences

- The default `binoc diff` experience is now immediately useful to humans. Machine consumers use `--format json` or `-o file.json`.
- The sidecar model is gone. Every file output is explicitly requested. This is more predictable but means migrating from the old `--output` flag to `-o file.json -o file.md` if both were wanted.
- Custom renderers that produce JSON (e.g. a structured changelog in JSON format) can use the explicit prefix to avoid the `.json` -> raw inference: `-o my-changelog:output.json`.
- `ResolvedPlugins` gained `renderer_for_extension()` and `renderer_by_name()` methods to support the lookup. These are generally useful for any code that needs to find renderers by something other than their full registered name.
