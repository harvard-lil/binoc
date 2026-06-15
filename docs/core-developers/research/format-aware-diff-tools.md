---
audience: core developers surveying the format-aware diff landscape
---

# Format-aware diff tools: a field survey

!!! note "Research note, not normative documentation"
    This is a background research survey, not a detailed description of how
    binoc behaves. It catalogs the tools that diff specific formats more
    meaningfully than line-of-text `diff`, records each tool's maintenance
    status (verified against repos and registries, June 2026), and notes for
    each format whether binoc currently handles it. It is the format-by-format
    companion to [Prior art and architecture precedents](precedents.md), which
    frames the same field around binoc's build-vs-buy argument and architecture
    claims. Poke holes in it; maintenance status drifts, and the "binoc today"
    column reflects the tree as of this writing.

*Plain `diff` (Myers, 1976) compares **lines of text**. That is the wrong unit
for most structured data: a reordered CSV column, a re-serialized JSON
document, a recompressed archive, or a reflowed function body all read as a
wall of changed lines even when little or nothing meaningful changed. For each
such format, tools exist that understand the structure and diff at the
appropriate unit. This page surveys those tools and records, per format,
whether binoc currently diffs it.*

*A recurring technique runs through the field: **normalize opaque bytes into a
canonical, semantically aligned representation, then diff that.** binoc's
expand/parse rules apply the same approach across formats and nesting levels.*

## Tools by format

The tool a practitioner most plausibly reaches for, per format, with the honest
caveat where there isn't a clear winner — and whether binoc currently diffs
that format. Verified June 2026.

Legend for **binoc today**: 🟢 = handled by a shipping stdlib rule or model
plugin today; 🟠 = not yet implemented, though within reach of the same
architecture. (Nothing here is out of scope by design, so there is no "never"
state.)

| Format | Go-to tool(s) | binoc today |
|---|---|---|
| **CSV / TSV** | **daff** (semantic, column-aware); **qsv diff** / **csvdiff** (fast, keyed) | 🟢 |
| **Columnar data files** (Parquet, Arrow/Feather, Avro, Stata, SAS) | **datacompy** (for the in-memory dataframe equivalent) | 🟢 |
| **Spreadsheets** (xlsx, ods) | **Beyond Compare** (commercial GUI) | 🟢 |
| **JSON** | **jd** (CLI); **deepdiff** / **jsondiffpatch** (libraries) | 🟢 |
| **YAML / TOML / INI** | **dyff** (YAML) | 🟢 |
| **XML** | **xmldiff** (FOSS); DeltaXML (commercial) | 🟢 |
| **Multi-format structured** | **graphtage** (one engine, many formats) | 🟢 |
| **Source code** (structural / AST) | **difftastic** (humans); **GumTree** (machines) | 🟠 — line-level text only |
| **Jupyter notebooks** | **nbdime** | 🟠 |
| **Rich-text documents** | **MS Word Compare**; **Draftable** (API/cross-format) | 🟠 |
| **Plain prose** (word-level) | **wdiff** / **dwdiff** | 🟠 — line-level text only |
| **Binary delta** (update patches) | **xdelta3**; **bsdiff** (standard format) | 🟠 — binary fallback (hash + strings) |
| **Binary inspection** (executables) | **radiff2** / **rz-diff** | 🟠 — binary fallback (hash + strings) |
| **Images** | **ImageMagick `compare`**; **pixelmatch** (JS) | 🟠 — binary fallback only |
| **PDF** | **diff-pdf** (visual); **pdf-diff** (text) | 🟠 |
| **Fonts** | **fontTools** (`ttx` / `fonttools diff`) | 🟠 |
| **SQLite files** | **sqldiff** | 🟢 |
| **SQL schema / migrations** | **Atlas** (modern); **Liquibase** (enterprise) | 🟠 — SQLite schema diff is 🟢 |
| **Versioned datasets** | **Dolt** (tabular); **lakeFS** (files) | 🟠 — different model (see below) |
| **Geospatial — vector** | **Kart**; **geodiff** | 🟢 — shapefile fusion |
| **Geospatial — raster** | (fragmented) | 🟠 |
| **HDF5 / NetCDF** | **h5diff**; **nccmp**; **CDO** | 🟠 |
| **Containers / archives** | **diffoscope** | 🟢 — zip/tar/gzip, recursive |
| **Move / rename / copy detection** | **git diffcore-rename** | 🟢 |

binoc reaches the 🟢 rows through stdlib rules plus first-party model plugins
(`binoc-sqlite`, `binoc-excel`, `binoc-xml`, `binoc-shapefile`,
`binoc-parquet`, `binoc-avro`, `binoc-stat-binary`, `binoc-binformats`,
`binoc-dbf`, `binoc-row-reorder`), not stdlib alone. Two limits worth flagging
against the specialists below: binoc detects column add/remove and cell changes
but **not** column rename/reorder (it does detect *row* reorder, via
`binoc-row-reorder`); and the versioned-dataset tools occupy a different model
(they version data inside their own store, whereas binoc compares datasets as
published — the case for data that ships no changelog).

A distinction the table compresses: for JSON, *jd* is the go-to **command-line**
tool, while *deepdiff* and *jsondiffpatch* are the most-installed diff
**libraries** (tens of millions of downloads/month) — they simply aren't what
one types at a shell.

## Tabular leaves — CSV, dataframes, spreadsheets

- **[daff](https://github.com/paulfitz/daff)** (MIT, Haxe→JS/Python/Java/…;
  v1.4.2 May 2025, slow-burn but alive). Keyed row matching and
  inserted/deleted/**reordered/renamed columns**; produces an *aligned* diff
  that is itself a table, round-trippable as a patch (CSV/JSON/HTML). Its
  **[Tabular Diff Format](https://specs.frictionlessdata.io/tabular-diff/)** is
  the only standardized tabular-diff representation — though the spec text is
  frozen at v0.8.0 (last touched 2020); the *implementation* is more current
  than the *standard*. The natural reference point for any tabular differ, and
  the source of the column-rename detection binoc does not yet implement.
- **[qsv](https://github.com/dathere/qsv)** `diff` (Unlicense, Rust; app v21.x,
  very active — releases weekly). Key-based matching, column sorting, ~600ms on
  1M×9 rows. The fastest *actively maintained* CSV differ in 2026; diff is one
  of 50+ subcommands.
- **[csvdiff](https://github.com/aswinkarthik/csvdiff)** (MIT, Go; v1.4.0 Feb
  2020, **stale**). PK-keyed, ~2s on 1M rows, JSON + five other output formats.
  The historically most-cited "fast CSV diff," but functionally frozen.
- **[csv-diff](https://github.com/simonw/csv-diff)** (Simon Willison; Apache-2.0,
  Python; v1.2 Sep 2024, near-dormant). Keyed CSV/TSV/JSON diff with human
  summaries plus `--json`; popular in the Datasette/git-scraping niche. No
  column rename/reorder.
- **[datacompy](https://github.com/capitalone/datacompy)** (Capital One;
  Apache-2.0, Python; v1.0.2 June 2026, **very active, ~3M downloads/mo**).
  Join-key DataFrame comparison across pandas/Polars/Spark/Snowpark with numeric
  tolerances and extractable mismatch frames. The most widely used tool for
  dataframe comparison.
- **[reladiff](https://github.com/erezsh/reladiff)** (MIT, Python; v0.6.0 Mar
  2025) is the maintained successor to **data-diff** (Datafold;
  **archived May 2024**) — hash-segmented cross-database row comparison at scale,
  not a local file differ.
- **Spreadsheets** have no open-source CLI champion. **Beyond Compare 5**
  (commercial, v5.2.0 Feb 2026) Table Compare is the practical interactive
  option; **Microsoft Spreadsheet Compare** is enterprise-SKU-locked and
  declining (its sibling Database Compare retires June 2026);
  **[git-xl](https://github.com/xltrail/git-xl)** (MIT, stale 2023) diffs VBA
  *code* inside workbooks, not cells.
- **Parquet** has no canonical differ. The common 2026 pattern is **DuckDB** as
  the engine (`EXCEPT`/anti-joins on two files), or datacompy via pandas/Polars.

Adjacent but *not* differs, worth knowing: **VisiData** (interactive tabular
explorer), **Miller**/`mlr` and **csvkit** (CSV wrangling) — a comparison is
hand-rolled from their join/filter primitives.

## Structured config and markup — JSON, YAML, XML

- **[jd](https://github.com/josephburnett/jd)** (MIT, Go; v2.5.0 Feb 2026, very
  active). Structural JSON/YAML diff with **minimal array diffs via LCS** and
  context to keep patches safe. Emits its own format plus **RFC 6902 JSON Patch**
  and **RFC 7386 Merge Patch**, and can apply/translate between them.
- **[deepdiff](https://github.com/qlustered/deepdiff)** (Python; v9.1.0 May 2026,
  **~76M downloads/mo** — highest adoption in this survey). Deep diff of
  arbitrary objects/dicts/JSON with `ignore_order`, tolerances, and a
  serializable `Delta` for reconstruction. The most-used diff library in Python.
  (Repo moved from `seperman` to the `qlustered` org in early 2026; note the
  2026 Delta-deserialization CVE if consuming untrusted deltas.)
- **[jsondiffpatch](https://github.com/benjamine/jsondiffpatch)** (MIT, TS;
  v0.7.6 May 2026, ~10M downloads/mo). Array **move detection via LCS +
  object-hash matching**, a compact delta format, reverse/patch, and an HTML
  visualizer. **JSON Patch (RFC 6902)** is the standards-track change format
  (`fast-json-patch` applies it at ~30M downloads/mo); **gron** flattens JSON to
  greppable lines for plain `diff` (a workaround, not a structural differ).
- **[dyff](https://github.com/homeport/dyff)** (MIT, Go; v1.12.0 Apr 2026, very
  active). Purpose-built YAML/JSON differ that **matches named-entry list items
  by an identifier key** (auto-detects `name`/`id`) so reordered lists aren't
  false positives, with `--ignore-order-changes` and Kubernetes entity
  detection. The de-facto `kubectl diff` external differ
  (`KUBECTL_EXTERNAL_DIFF`). Display/CI-oriented (weak patch output).
- **[xmldiff](https://github.com/Shoobx/xmldiff)** (MIT, Python; v3.0 June 2026).
  Tree-structural XML diff via an edit-script algorithm that detects node
  **moves**, emitting an applicable XML patch. The most common open-source XML
  differ — but XML diffing is fragmented: **DeltaXML** (commercial) leads
  enterprise, and Microsoft's **XML Diff and Patch** is effectively abandoned.
  There is no dominant FOSS winner for XML.
- **[graphtage](https://github.com/trailofbits/graphtage)** (Trail of Bits;
  LGPL-3.0, Python; no release since v0.3.1 Jan 2024, slow-burn). One engine
  diffs **JSON, JSON5, XML, HTML, YAML, TOML, plist, CSS** via a shared
  intermediate tree, round-tripping edits back into any supported format.
  Conceptually notable — one tool, many formats — but low adoption.

## Source code, notebooks, and prose

A three-layer distinction worth stating, because it is the most common
confusion in this space:

1. **Line diff, plain** — `diff` / `git diff`. No language awareness.
2. **Line diff, prettified** — **delta/git-delta** (~31k stars, the
   most-starred "diff" tool on GitHub), **bat**, **ydiff**. These syntax- and
   word-highlight an ordinary line diff. **They never parse a tree** — a
   reformatted-but-identical file still shows as fully changed. Presentation,
   not structure.
3. **Structural / tree diff** — parse to an AST, diff the tree:

- **[difftastic](https://github.com/Wilfred/difftastic)** (MIT, Rust; v0.69.0
  Apr 2026, very active, ~25k stars). tree-sitter AST diff across 30+ languages,
  ignores insignificant whitespace, falls back to line diff on parse errors.
  Human-facing (JSON output exists but is explicitly unstable).
- **[GumTree](https://github.com/GumTreeDiff/gumtree)** (LGPL, Java; v4.0.0-beta6
  Dec 2025). The canonical fine-grained AST differencer (Falleri et al. 2014)
  with first-class **move** detection, emitting JSON/XML edit scripts — the tool
  to reach for when a machine-consumable AST edit script is needed.
  **[diffsitter](https://github.com/afnanenayet/diffsitter)** (MIT, Rust;
  cooling) is a lighter tree-sitter cousin; GitHub's **semantic** is
  **archived (Apr 2025)**; **srcML/srcDiff** take an XML-serialization approach.
- **[nbdime](https://github.com/jupyter/nbdime)** (BSD, Jupyter; v4.0.4 Feb 2026).
  Content-aware diff/merge of notebook JSON, per cell type, with structured diff
  objects and 3-way merge. The established tool for `.ipynb`.
- **Documents:** **Microsoft Word "Compare Documents"** is the de-facto baseline
  (word-level, flags formatting changes, ubiquitous); **LibreOffice Compare** is
  the free-desktop analogue. **[Draftable](https://draftable.com)** (commercial)
  is the API/cross-format option (PDF/Word/PPT/Excel).
  **[pandiff](https://github.com/davidar/pandiff)** does prose diffs through
  Pandoc (CriticMarkup/HTML/tracked-changes output);
  **[python-redlines](https://github.com/JSv4/Python-Redlines)** generates Word
  tracked changes from code. **GNU wdiff** / **dwdiff** are the word-level
  plain-text CLIs (no formatting awareness — they will mangle markup, which is
  what pandiff fixes).

## Binary, images, PDF, fonts

Two distinct purposes hide under "binary diff":

- **Binary *delta* (reconstruct B from A — software updates):**
  **[bsdiff](https://www.daemonology.net/bsdiff/)** (BSD, C) is the de-facto
  *standard format*, embedded everywhere (Android, Chromium, FreeBSD) — but the
  reference code is frozen (~2005). **[xdelta3](https://github.com/jmacd/xdelta)**
  (Apache-2.0, C; "v3.1.0 2016" by releases but genuinely active — security
  commits June 2026) is the maintained general-purpose choice.
  **Google Courgette** disassembles executables and normalizes pointers before
  bsdiffing (~9× smaller Chrome patches) — Chromium-internal.
  **[HDiffPatch](https://github.com/sisong/HDiffPatch)** is a modern,
  actively-used alternative compatible with bsdiff/xdelta formats.
- **Binary *inspection* (show what changed — reverse engineering):**
  **radiff2** (radare2; LGPL, v6.x, very active) and **rz-diff** (rizin; the
  cleaner modern fork) do byte/delta/function/graph-level executable diffs with
  JSON output. **vbindiff** is an interactive hex differ (stale).

Images split **pixel/exact** vs **perceptual**:

- **[ImageMagick `compare`](https://imagemagick.org/script/compare.php)** (C,
  very active) is the universal CLI — many metrics (AE/MSE/RMSE/PSNR/PHASH/SSIM),
  a visual diff image, and exit codes. Installed almost everywhere.
- **[pixelmatch](https://github.com/mapbox/pixelmatch)** (ISC, JS; v7.2.0 Apr
  2026) is the visual-regression-testing default (Jest/Playwright);
  **[odiff](https://github.com/dmtrKovalenko/odiff)** is the speed pick with JSON
  output; **[dssim](https://github.com/kornelski/dssim)** (AGPL, Rust) is the
  perceptual (multi-scale SSIM) CLI. **perceptualdiff** and **Resemble.js** are
  largely superseded.

PDF splits **rendered-visual** vs **text**:

- **[diff-pdf](https://github.com/vslavik/diff-pdf)** (GPL, C++; v0.5.3 Mar 2026,
  most-starred OSS PDF differ) rasterizes pages and highlights pixel differences
  (exit code for CI). **[pdf-diff](https://github.com/JoshData/pdf-diff)** (CC0,
  Python) is the text-based option. **qpdf**'s QDF mode normalizes a PDF into a
  diff-able text form — a useful *pre-processor*, not a differ. (Watch the naming
  collision: hyphenated **diff-pdf** is OSS/active; **DiffPDF** from Qtrac is
  commercial and EOL.)

Fonts are the cleanest illustration of the field's core technique:
**[fontTools](https://github.com/fonttools/fonttools)** (`ttx` / `fonttools diff`)
serializes a binary OTF/TTF into a canonical XML representation and diffs *that*
— the same "normalize to canonical form, then diff" pattern that Courgette
(executables) and qpdf-QDF (PDFs) also use.

## Databases, schemas, and versioned datasets

- **[sqldiff](https://sqlite.org/sqldiff.html)** (public domain, C; ships with
  SQLite). PK-paired row diff producing a transforming SQL script, plus a binary
  `--changeset`. The standard tool for SQLite file diff.
- **Schema diff / migration generation** splits into *declarative-state* tools
  (**[Atlas](https://github.com/ariga/atlas)** — Apache-2.0, Go, v0.37 Apr 2026,
  very active, 15+ engines, JSON/HCL/ERD output; **skeema** for MySQL/MariaDB)
  vs *diff-two-live-DBs* tools (**Liquibase** — enterprise incumbent,
  diff→changelog; **Redgate SQL Compare** / **dbForge** — commercial). Atlas
  straddles both and is the momentum leader. **migra is deprecated** (last
  release 2022); **apgdiff** is unmaintained; **alembic autogenerate** operates
  *within* the SQLAlchemy ecosystem (and candidly documents that its output is a
  draft needing review).
- **Versioned-dataset tools** version data *inside* their own store, the inverse
  of binoc's case (generating a changelog for data that ships none):
  **[Dolt](https://github.com/dolthub/dolt)** (Apache-2.0, Go; v2.1.x June 2026,
  releases every 1–3 days) is "Git for data" — true row+cell+schema diff via
  `dolt_diff` system tables. **[lakeFS](https://github.com/treeverse/lakeFS)**
  covers object-storage/file datasets and, since **Nov 2025, also stewards DVC**.
  **[Oxen](https://github.com/Oxen-AI/Oxen)** (Rust) is a fast rising challenger
  with key-targeted tabular diffs; **DataLad**, **Quilt**, **TerminusDB**, and
  **Pachyderm** (now HPE) hold research/graph/pipeline niches. All require
  importing data into their repo formats first.

## Geospatial and scientific

- **Geospatial:** **[Kart](https://github.com/koordinates/kart)** (GPL, Python;
  v0.17.1 June 2024, slow-burn) does Git-style feature- and cell-level diffs
  **across physical formats** (GeoPackage/PostGIS/SQL Server/MySQL) — the
  precedent for "same logical dataset, different physical formats."
  **[geodiff](https://github.com/MerginMaps/geodiff)** (MIT, C++; v1.0+) is the
  lower-level changeset engine powering it. **GDAL/OGR** is the ubiquitous
  conversion substrate (no first-class diff command); QGIS has several
  fragmented layer-compare plugins. binoc currently handles shapefiles (geometry
  summary + attribute table + CRS/encoding) via `binoc-shapefile`; raster
  formats are not yet covered.
- **Scientific / array formats** are defined by **tolerant numeric compare**:
  **[h5diff](https://github.com/HDFGroup/hdf5)** (BSD, C; ships with HDF5)
  recursively compares datasets with absolute/relative/epsilon tolerances — the
  standard for HDF5. **[nccmp](https://gitlab.com/remikz/nccmp)** and **NCO**
  (`ncdiff`) handle NetCDF; **CDO** (`diffn`) is the climate-data operator suite;
  **ncompare** (NASA) does structural NetCDF/HDF5 diffs; **xarray**'s
  `assert_allclose` is the in-code/test option. Their `rtol/atol` tolerance
  contrasts with the exact row/byte identity used by sqldiff and Dolt.

## Containers and archives

Covered in depth by [precedents.md](precedents.md); briefly:
**[diffoscope](https://diffoscope.org/)** (Reproducible Builds; GPL-3, Python;
v318 May 2026, very active) is the strongest generalist — recursive container
unpacking (depth 50), pluggable dispatch-by-type, 100+ formats including
zip/tar/gzip and SQLite, fuzzy matching to pair renamed files inside containers,
and text/HTML/JSON presenters. Its leaves are unified byte/line diffs with no
transformable IR, so a column reorder inside a zipped CSV still appears as
changed lines. **Beyond Compare** handles archives interactively.
**git diffcore-rename** (exact-hash pass first, bounded similarity matrix second,
hard cap on the quadratic stage) is the standard structure for move/rename
detection, which binoc's pairing follows.

## Cross-cutting patterns

Four observations that hold across the field and inform binoc's design:

- **The canonical-form technique is widespread.** The "meaningful" binary
  differs work by normalizing opaque bytes into a canonical, semantically
  aligned representation first — Courgette (executables → normalized pointers),
  fontTools (OTF → TTX/XML), qpdf (PDF → QDF text), gron (JSON → flat lines) —
  *then* diffing that. It is the same move binoc makes when an expand/parse rule
  turns raw data into side-tree items.
- **Machine-readable vs display is a real fork.** Some tools emit an applicable
  patch (jd → RFC 6902, daff's round-trippable table, deepdiff's `Delta`,
  jsondiffpatch's delta, geodiff's changeset, sqldiff's SQL); others only render
  for humans (dyff, difftastic, diffoscope, VisiData). For a changelog
  generator, the patch-emitting tools are the closer prior art.
- **Tolerant compare is its own dimension.** The scientific cluster's `rtol/atol`
  epsilon matching has no analogue in the tabular/structured tools, which assume
  exact identity — a gap to note if binoc ever diffs float-heavy arrays.
- **Most tools cover one layer.** Each surveyed tool addresses a single format
  or layer. The generalists that recurse through containers (diffoscope) keep
  byte-diff leaves; the semantic leaf differs (daff, jd, sqldiff) do not recurse
  through containers or compose with one another. binoc currently combines
  container recursion (directories, zip/tar/gzip), semantic leaves (CSV, JSON,
  XML, spreadsheets, Parquet, SQLite, shapefiles, and more), and move/rename
  detection in one changeset. For any individual leaf, a more specialized point
  tool generally exists — daff detects column renames binoc does not, h5diff
  applies numeric tolerances binoc does not — so the survey is also a map of
  where binoc's per-format depth could grow.
