# Media Type Detection and Content-Aware Dispatch

**Date:** 2026-03-05
**Status:** Implemented (phase 1)

## Problem

Comparator dispatch was based entirely on file extension (`handles_extensions`) and an imperative fallback (`can_handle`). This breaks in real-world datasets where:

- Files have misleading extensions (a `.dat` file that's actually a zip archive).
- Files have no extension at all (`Makefile`, `LICENSE`, data dumps).
- Container formats share an extension but differ in content (`.xml` could be SVG, XHTML, ODF manifest, etc.).

The directory comparator already reads the full file into memory for BLAKE3 hashing. Running format detection on those same bytes is essentially free — a handful of magic-byte comparisons on data already in cache. The question was which detection library to use, what identifier system to standardize on, and how to wire it into dispatch without breaking the existing contract.

## Decision

**Phase 1 (implemented): `infer` for magic-byte sniffing, `mime_guess` for extension fallback, MIME media types as the identifier system.**

### Detection

The directory comparator's `read_and_identify` function reads a file once, computes the BLAKE3 hash, then:

1. Runs `infer::get(&data)` — checks ~100 magic-byte signatures. Zero dependencies, zero allocations. Covers images, audio, video, archives, fonts, WASM, common document formats.
2. If `infer` doesn't match (most text formats lack distinctive magic bytes), falls back to `mime_guess::from_path()` on the logical path. This was already a transitive dependency via the text comparator.
3. Stores the result as `media_type: Option<String>` on `Item`. Files with neither recognizable magic bytes nor a known extension carry `None`.

### Dispatch

A new trait method on `Comparator`:

```rust
fn handles_media_types(&self) -> &[&str] { &[] }
```

The controller checks each comparator in order: **extension → media type → `can_handle`**. All three stages are per-comparator (not separate passes), preserving the property that config ordering controls priority. Directories skip extension and media type matching entirely — they are claimed solely via `can_handle` — preventing extracted archive contents (a directory with `logical_path` like `"archive.zip"`) from being re-claimed by the archive comparator.

### Identifier system

MIME media types (`application/zip`, `text/csv`, `image/png`). Every detection library in the Rust ecosystem speaks MIME. PRONOM PUIDs are more precise for archival use but would require Siegfried; MIME is the pragmatic common currency that doesn't preclude adding PUIDs later.

## What this enables

- The zip comparator declares `handles_media_types: ["application/zip"]`. A `.dat` file with zip magic bytes now routes to the zip comparator automatically.
- Future comparators can match on media type alone, without maintaining extension lists.
- The media type is available to comparators via `pair.media_type()` for internal logic (e.g., a future "office document" comparator could claim all `application/vnd.openxmlformats-*` types).

## Phase 2 options (not yet decided)

### Upgrade detection backend

**`file-format` with reader features.** Adds container-aware detection: distinguishes DOCX from XLSX from PPTX by inspecting zip contents, identifies ODF variants, handles CFB (legacy Office) and XML-based formats. Pure Rust, no unsafe. The tradeoff is that its reader features duplicate some work the zip comparator already does (opening the archive), so it may make more sense to integrate at the comparator level rather than the detection level.

**`pure-magic` + `magic-db`.** A pure-Rust reimplementation of libmagic's rule engine with a precompiled database. Would bring libmagic-level coverage (thousands of formats) without the C dependency. Still maturing — not all libmagic test types are supported yet. Worth watching.

Either of these would be a drop-in replacement inside `read_and_identify`. The `media_type` field, the `handles_media_types` trait method, and all dispatch logic remain unchanged.

### Siegfried sidecar for PRONOM PUIDs

Siegfried is the gold standard for archival format identification: PRONOM PUIDs, Library of Congress FDD identifiers, basis reporting (byte offsets and match lengths). It's a Go CLI with JSON output and a built-in HTTP server mode.

If PRONOM-level identification becomes a requirement (likely for the archival audience), the pragmatic integration is:

- Shell out to `sf --json` or hit its HTTP server.
- Store the PUID in a new `pronom_id: Option<String>` field on `Item`, alongside `media_type`.
- Add `handles_pronom_ids() -> &[&str]` to the comparator trait, checked between media type and `can_handle`.
- Fall back to `infer`/`mime_guess` when Siegfried isn't available, so the tool degrades gracefully.

This is additive — no changes to the existing dispatch contract.

### Surface media type in the IR

Currently `media_type` lives only on `Item` (dispatch-time metadata) and doesn't appear in the IR tree. A future option is to attach the detected media type to `DiffNode.details` so renderers can report it (e.g., "file.dat was identified as application/zip"). This is a one-line change in the controller's `attach_content_hashes` method and has no architectural implications.

## Why `infer` over alternatives

| Crate | Why not (for phase 1) |
|---|---|
| `file-format` | Reader features are valuable but add complexity. Better suited as a phase 2 upgrade when container disambiguation is a real need. |
| `file_type` | Primarily a lookup/mapping crate (extension → media type). Doesn't do content-based magic-byte matching, which is the main gap we're filling. |
| `magic` (libmagic FFI) | C dependency, known CVEs, platform-specific build requirements. Unacceptable for a library targeting `pip install` distribution. |
| `pure-magic` | Not mature enough yet (missing some test types). Promising for future evaluation. |
| `tree_magic` | Development stalled; the maintained fork (`tree_magic_mini`) uses freedesktop.org's shared-mime-info which is Linux-centric. |
| Siegfried | Go binary, not embeddable. Right choice for PRONOM-level identification, wrong choice for a zero-cost detection layer. |

`infer` is ~100 pattern table entries, zero dependencies, `no_std`-compatible. It handles the "is this a ZIP, PNG, PDF, or something else?" question with a single function call on bytes already in memory. The extension-based `mime_guess` fallback covers text formats that lack magic bytes. Together they provide good coverage with no new I/O, no C dependencies, and a clear upgrade path.

## Risks

- **`infer` misidentifies a format.** Low risk — magic-byte signatures are conservative (they check specific byte sequences at specific offsets). A false positive would mean a comparator claims a file it can't actually process, which would surface as a comparator error, not silent corruption. Mitigation: comparators should validate content after claiming, not trust the media type blindly.
- **Extension-based fallback disagrees with content.** This is actually a feature — a `.csv` file that `infer` identifies as `application/zip` will be routed to the zip comparator, which is correct. The extension fallback only fires when `infer` returns nothing.
- **`infer` is abandoned.** The crate is mature and stable (17M+ downloads). If it stalls, `file-format` or `pure-magic` are drop-in replacements at the `read_and_identify` call site.
