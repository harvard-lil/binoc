---
audience: core developers surveying prior art and architecture precedents
---

# Prior art and architecture precedents

!!! note "Research note, not normative documentation"
    This is a background research survey, not a description of how binoc
    currently behaves. It records the build-vs-buy evidence and the
    architecture precedents that informed binoc's design — the companion (Part
    VII) to [Binoc: the architecture, told as a story](../../adr/2026-06-12-historical_single_tree_architecture_story.md),
    where the `Claim N`, `Move N`, and `Cn` labels used below are defined.
    Treat those labels as pointers into that reasoning rather than guarantees
    about the code. Poke holes in it.

*Two questions, two surveys (web research verified against registries and
repos, June 2026). First: what existing tools are the best argument for not
building binoc from scratch? Second: assuming it is built from scratch, which
systems have the most to teach its architecture? The headline finding for the
first: **no maintained tool occupies binoc's intersection** — container
recursion × tabular semantics × compaction passes × extractable saved diff.
The serious "don't reimplement" arguments are all per-layer.*

## The field — overlapping tools (the build-vs-buy evidence)

!!! tip "Companion survey"
    For a format-by-format catalog of the tools below (CSV, JSON, XML, YAML,
    code, images, PDF, fonts, databases, geospatial, scientific…) with verified
    maintenance status, a go-to tool per format, and a column noting which
    formats binoc currently handles, see
    [Format-aware diff tools: a field survey](format-aware-diff-tools.md). This
    section keeps the build-vs-buy framing tied to binoc's claims; that page is
    the broader landscape.


**Container recursion (Moves 0–2).**

- **[diffoscope](https://diffoscope.org/)** (Reproducible Builds; GPL-3,
  Python; v318 May 2026, very active) is the strongest single prior-art
  claim. It *is* binoc's compare phase: recursive container unpacking
  (depth 50), pluggable dispatch-by-type handlers, 100+ formats
  including zip/tar/gzip and SQLite, ssdeep fuzzy matching to pair renamed
  files inside containers, and text/HTML/JSON/markdown presenters. What it
  lacks is everything from Move 3 on: leaves are unified byte/line diffs (a
  column reorder is a wall of changed lines), there is no IR that passes
  could rewrite, no facts/judgments split, no extract. Its goal is
  exhaustive *display* for build debugging, not minimal description. It
  proves the container layer is a solved problem; binoc's bet was never
  about that layer — but "why not diffoscope?" is the question reviewers
  will ask first, and the answer (C1/C2 performance and ABI constraints;
  leaf output that isn't a transformable IR) should be ready.
- **Beyond Compare** (commercial GUI; BC5, 2024): keyed Table Compare
  sessions, format converters, archive handling — interactive, not a batch
  semantic-summary tool.
- **Nested-path notation precedent.** Java
  [`JarURLConnection`](https://docs.oracle.com/en/java/javase/11/docs/api/java.base/java/net/JarURLConnection.html)
  uses `!/` to separate a JAR URL from the path of an entry inside it; that is
  the strongest prior art for a compact archive-boundary glyph. GDAL's
  [virtual file systems](https://gdal.org/en/stable/user/virtual_file_systems.html)
  go further, composing handlers such as `/vsizip/` and `/vsicurl/` and using
  brace forms for nested archive paths. A 2017
  [gdal-dev thread](https://lists.osgeo.org/pipermail/gdal-dev/2017-October/047361.html)
  explicitly treats braces as an ambiguity improvement over older nested-path
  spelling. Transferable lesson for binoc: a compact left-to-right separator is
  readable, but portable canonical paths need an escape or bracketing rule.

**Tabular leaves (Move 4, Claim 1).**

- **[daff](https://github.com/paulfitz/daff)** (MIT, Haxe→JS/Java/Python;
  1.4.2 May 2025, slow-burn but alive; predecessor coopy dormant since
  2020): aligned table diffs handling inserted/deleted/reordered/**renamed
  columns** and keyed or heuristic row matching — a real chunk of
  `TabularAnalyzer` + `ColumnReorderDetector`. More important than the code:
  its **[Tabular Diff Format](https://specs.frictionlessdata.io/tabular-diff/)**,
  adopted into the Frictionless specs, is the only standardized
  tabular-diff representation in existence. Even keeping our engine, binoc
  should consider emitting or converting to it rather than inventing a new
  intra-table change vocabulary — this bears directly on Claim 1's
  nodes-vs-tags line.
- **[csv-diff](https://github.com/simonw/csv-diff)** (Simon Willison;
  Apache-2.0, Python; last release Sep 2024): keyed CSV/TSV/JSON diff with
  exactly binoc-style human summaries plus `--json` — single-file, no
  column rename/reorder, low maintenance. **csvdiff** (Go, MIT; stale
  2024): fast keyed CSV diff, additions/modifications only.
- **[datacompy](https://github.com/capitalone/datacompy)** (Capital One;
  Apache-2.0, Python; v1.0.1 June 2026, very active): keyed DataFrame
  comparison (pandas/Polars/Spark/Snowpark) with tolerances and extractable
  mismatch frames — an in-memory library with text reports, not a file
  tool.
- **[data-diff](https://github.com/datafold/data-diff)** (Datafold) —
  **confirmed archived** May 2024; the in-database checksum-segmentation
  differ. Community fork **reladiff** barely moving (last release Mar
  2025). The warehouse branch of this space consolidated into commercial
  products (Datafold Cloud, Recce), not file-based tools.
- **[sqldiff](https://sqlite.org/sqldiff.html)** (SQLite project; public
  domain, C, maintained): compares two SQLite files, outputs transforming
  SQL — covers SQLite table parsing for nearly free.
- **Scientific formats**: **h5diff** (HDF5, tolerant object-by-object
  compare) and **nccmp**/NCO for NetCDF — single-format leaf-diff
  prior art only.
- **Spreadsheets**: Microsoft **Spreadsheet Compare** (Office Pro
  Plus/M365; older, interactive only, no machine output);
  **xltrail** (commercial Excel versioning) and its open-source **git-xl**
  (dormant; diffs VBA only, not cells).

**Data version-control systems (Move 8's extract story, file-level only
otherwise).**

- **[Dolt](https://github.com/dolthub/dolt)** (Apache-2.0, Go; very
  active): versioned SQL database; `dolt diff` does schema + PK-keyed row
  diffs with JSON/SQL output, and `dolt_diff_<table>` system tables make
  "give me the added rows, later, as a query" a solved problem — *inside
  Dolt's storage*. The best existing answer to C8, as a design teacher.
- **[Kart](https://github.com/koordinates/kart)** (GPL-2.0, Python;
  v0.17.1 June 2026): git-based versioning of geospatial/tabular data with
  feature-level PK-keyed diffs **across physical formats**
  (GeoPackage/PostGIS/etc.) — strong conceptual precedent for "same logical
  dataset, different physical formats." **[Oxen](https://github.com/Oxen-AI/Oxen)**
  (Apache-2.0, Rust; active): ML-dataset VCS whose `oxen diff` is
  format-aware for CSV/parquet with key columns ("92,630 rows added").
  Both require importing data into their repo formats.
- **DVC** (active; acquired by lakeFS/Treeverse Nov 2025), **lakeFS**,
  **DataLad**, **Quilt**: diff at file/object granularity only
  (add/delete/modify/rename by hash) — no content semantics.

**Structured-document and object diff (IR prior art).**

- **[nbdime](https://github.com/jupyter/nbdime)** (BSD, active): structured
  diff/merge of notebook JSON, content-aware per cell type, with CLI/web
  renderers — a good small-scale "format-specific semantic model +
  renderers" precedent, and a test case for Claim 1's deep-interior
  formats.
- **deepdiff** (MIT, Python, active): deep object diff whose serializable,
  applicable, reversible `Delta` objects are conceptually close to
  "extract later from saved JSON." **jsondiffpatch** (TypeScript, active):
  compact delta format with LCS array-move detection and an HTML renderer;
  **JSON Patch (RFC 6902)** is the standards-track change-IR, machine-
  applyable but not human-compact and weak at moves.
- **[difftastic](https://github.com/Wilfred/difftastic)** (MIT, Rust, very
  active): tree-sitter structural diff of source code; excellent
  tree-alignment prior art, explicitly code-oriented. **GumTree** (LGPL,
  Java; research-active): the canonical AST move-detection engine — see
  the teachers section. GitHub's **semantic**: confirmed archived.

**New entrants (2024–2026).**

- **[Recce](https://github.com/DataRecce/recce)** (Apache-2.0; very
  active): "AI data review agent" for dbt PRs — lineage/schema/row-count/
  profile diffs with LLM-assisted review summaries and an MCP server.
  Warehouse-scoped, but the clearest existing example of "semantically
  compact, human-readable description of data change." Its predecessor
  PipeRider is deprecated.
- **snapdiff** (Rust; small): two directory snapshots in, concise file-level
  summary out — binoc's UX shape with zero content awareness. **sdiff-rs**,
  **semdiff** (Rust; small/young): semantic diffs of JSON/YAML/TOML and
  text/JSON/binary respectively.
- An explicit search for anything combining archive recursion + tabular
  semantics + structured summarization returned nothing. That intersection
  appears genuinely unoccupied.

**Bottom line.** The strongest form of the "don't reimplement" argument is
not "use tool X" — it's that Moves 0–2 reproduce diffoscope and the tabular
leaf reproduces daff, so the project must be able to say why those
reimplementations were necessary. The defensible novelty is the later
rule-family architecture: artifacts, link-driven correspondence, extract, and
the facts/judgments wall. No incumbent has the full combination binoc needs.

## The teachers — architecture precedents, by claim

**MLIR / LLVM — Claims 3, 4, 5; the most instructive single precedent.**

- *Pass ordering:* MLIR deliberately has **no inter-pass dependency
  system** — pipelines are explicit ordered lists and ordering correctness
  is the pipeline author's job. LLVM's older pass manager *had* declared
  dependencies plus a scheduler; the new pass manager abandoned that for
  explicit pipelines + cached analyses. The 15-year arc is strong evidence
  that "config order is semantics, no solver" is where this design space
  converges, not a missing feature.
- *Fixpoint quarantine:* MLIR's `canonicalize` runs greedy rewriting to
  fixpoint *inside one pass*, with iteration caps — never in the driver.
  That is the precedent-backed answer to Claim 4: if cascading compaction
  is ever needed, it becomes one bounded compaction rule with a convergence
  cap, and the single recompare bounce stays a driver-level principle.
- *Analyses vs passes:* expensive shared computations (similarity
  matrices, hash indexes) live in a cached, invalidation-aware analysis
  layer (`AnalysisManager`, `markAnalysesPreserved`), not inside transform
  passes — relevant the moment two correlation detectors recompute the
  same pairwise comparison.
- *Open vocabularies:* MLIR splits attributes into **inherent** (verified
  by the owning op; tooling may rely on them) and **discardable**
  (namespaced `dialect.name`, externally defined, *legally droppable by
  any pass*). Binoc tags are facts + dispatch keys + renderer keys with no
  statement of which a rule may drop on rewrite; a 2025 MLIR
  workshop paper documents the exact failure mode (discardable attrs
  silently lost across rewrites). Adopting the inherent/discardable
  distinction — plus a `dependentDialects` analogue where rule packs
  declare at pipeline-build time which vocabularies they may *emit* — is
  the cheapest available big win for Claim 3. MLIR also allows
  unregistered ops/dialects (unknown vocabulary flows through unharmed),
  the posture binoc wants for unknown tags.

**Pandoc — the m + n hub itself (Moves 1, 5, 7; C5/C7).** Readers → fixed
typed AST → writers; Lua filters are user-ordered in-process passes and
JSON filters the out-of-process twin (literally binoc's JSON-wire shape).
No dependency system, no fixpoint; inter-filter ordering bugs are a known,
accepted cost. Its open-vocabulary escape hatch — `Div`/`Span` `Attr`
(id, classes, key-values) — grew whole ecosystems (Quarto, pandoc-crossref)
*and* exhibits exactly the silent-collision failure binoc's tags risk. The
**tables cautionary tale**: pandoc 2.10 (2020) replaced its too-weak
`Table` type and broke every table-touching filter in the ecosystem; the
survival kit was an API version embedded in the wire format *plus
conversion shims* (`to_simple_table`/`from_simple_table`). Lesson for
C5/C7: version checks alone don't preserve an ecosystem — plan migration
shims. Pandoc's manual is candid that the hub IR is lossy by design, a
floor not a union; binoc's typed artifact side-channel is the
pressure-relief valve pandoc never built (a point *for* the architecture).

**Apache Arrow C Data Interface — Claims 2 and 6.** Designed for exactly
binoc's situation: two tiny C structs, copied into your own header,
**frozen forever**, with a `release` callback solving cross-allocator
ownership across a C ABI between separately compiled components — plus a
stream variant for lazy/chunked production and an mmap-able IPC file
format. arrow-rs is healthy (monthly releases) but its *Rust* API breaks
quarterly: one more reason a plugin contract should be the C structs / IPC
bytes, never crate types. The precedent-backed move for `tabular_v1`: keep
eager JSON as v1, define `tabular_v2` as Arrow IPC bytes, and let the
existing artifact versioning carry the migration — converting Claim 2's
"interface ceiling" into a planned upgrade rather than a locked contract.
(Columnar layout also makes column-reorder detection pointer shuffling, and
DuckDB/polars/ADBC/DataFusion all use this interface as a plugin boundary
in production.)

**Plugin ABI — C1–C5.** No 2026 consensus, but a settled tier list, and
binoc's C-ABI + JSON-wire hybrid sits in the mainstream:

- **Nushell's plugin protocol** is the closest production analogue:
  out-of-process executables, a `Hello` handshake carrying protocol
  version + **feature flags**, msgpack as a faster twin to JSON, plugins
  persisted in a registry and spun up on demand. The handshake design is
  directly copyable.
- **GStreamer** is the elder proof that "C ABI + string-keyed structured
  capability declarations (caps) + integer rank" scales to thousands of
  plugins; caps-intersection autoplugging is a more principled version of
  binoc's three-stage dispatch. **GDAL/OGR** (~80 vector formats behind one
  feature model) adds driver capability flags (`TestCapability()`) and
  registry probes (`Identify()`) — fixed hub model, open negotiated
  capability vocabulary. **Apache Tika** (>1000 formats → one parse
  signature) shows the hub representation can be a *streaming event
  sequence* rather than a materialized tree — relevant to the eager-
  artifact question. **LSP** solves m × n as protocol, with capability
  negotiation as the transferable mechanism.
- **abi_stable** (mature, high-ceremony, slowing) and **stabby** (newer,
  smaller adoption, has chased rustc internals) are the Rust↔Rust dylib
  options — both validate binoc's choice *not* to share Rust types across
  the ABI. **Extism** (Wasm) and the **WebAssembly Component Model**
  (WASI 0.3 shipped native async Feb 2026; 1.0 targeted late 2026/2027)
  are the long-term alternative buying sandboxing + typed, versioned WIT
  signatures — out of scope per C3, but the known successor if the threat
  model changes; worth a line in the ADRs.

**Correlation and process inference — Moves 3 and 6, Claim 7.**

- **git diffcore-rename**: exact content-hash pass first (free), bounded
  similarity-matrix pass second, hard cap on the quadratic stage
  (`diff.renameLimit`). Validates binoc's exact-before-fuzzy ordering and
  the 400 cap as the standard structure, not an ad-hoc bound.
- **GumTree**: greedy top-down matching of identical subtrees, then
  bottom-up container matching by descendant-overlap (dice ≥ 0.5), then
  derive the edit script. The framing worth stealing: **the mapping
  between trees is the artifact; the edit script is derived from it** —
  which maps cleanly onto facts-in-IR / judgments-in-renderer and may be
  the principled answer to Claim 1's nodes-vs-tags tension.
- **The schema-diff family is the real "process inference" precedent**:
  migra (deprecated; succeeded in spirit by Stripe's pg-schema-diff, which
  adds dependency-ordered, online-safe plans), **Atlas** (explicitly
  compiler-shaped: loaders → core schema model → per-dialect planners,
  plus lint/policy over the inferred plan — precedent for renderer-level
  judgments over inferred operations), **skeema** (deliberately *refuses*
  to guess renames — "don't infer processes you can't verify," worth
  making configurable per-judgment), and **alembic autogenerate** (docs
  say output "cannot be relied upon" without review — the honest framing:
  inferred process descriptions are drafts).
- **Terraform plan**: state diff classified into action sets via provider
  schema metadata, and the **`moved` block** — when inference is
  impossible, *the user declares the process and the engine consumes the
  declaration*. Binoc's declared-correspondence config is the same move;
  the precedent says to extend it to every inference (column renames, key
  changes), not just file pairing.
- **Darcs/Pijul patch theory**: the maximalist position — changes as
  first-class objects whose commutation determines reorderability. The
  transferable question: "do these two inferred operations commute?" as a
  test for when rule order *can't* matter.
- **OpenRefine** records operations as replayable JSON recipes — the
  inverse of inference, and the missing correctness test for Claim 7: if
  binoc's inferred processes ("find-replace sneakers→shoes") serialize to
  a replayable operation list, then *apply to A, compare with B* becomes
  mechanical verification that the inference is right.

**Diagnostics — Move 9.** **SARIF 2.1.0** (OASIS standard; 2.2 still in
committee) is the established findings format: first-class plugin
attribution (`tool.extensions`), namespaced rules, fingerprint dedup,
argumented message strings, free GitHub code-scanning ingestion. Its
weakness for a diff tool: locations are single-artifact-version, while diff
findings are two-sided (workable via `relatedLocations`, but lossy). The
consonant shape: keep the native diagnostics channel as truth and ship a
SARIF exporter as one more renderer config — diagnostics become just
another *o*.

**The cross-cutting pattern.** Every mature hub examined — MLIR
discardable attrs, pandoc Attr, Tika's metadata bag, GStreamer caps, LSP
capabilities, GDAL capability flags — converges on the same triple: a
small closed core + a namespaced open vocabulary + **capability
negotiation**, so components declare what they understand rather than
silently ignoring what they don't. Binoc has the first two. Negotiation —
renderers and rule packs advertising which tag and artifact vocabularies
they speak — is the recurring third leg it hasn't built, and the standard
answer to the "unknown tag silently lands in Other Changes" class of
failure.
