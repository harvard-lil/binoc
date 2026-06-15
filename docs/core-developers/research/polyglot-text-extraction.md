# Polyglot Text Extraction: A Survey for binoc's Fallback Projection

*Research / survey document — 2026-06-15*

> Status: **survey / research input, not a decision record.** This document presents
> tradeoffs and a recommended phased path to inform a later design decision. It does
> not commit binoc to any tool.

## 1. Why binoc needs this

binoc generates changelogs/diffs for datasets by comparing snapshots. Today, when a
file has a native model plugin (SQLite, Parquet, Excel, DBF, Avro, HTML, statistical
binaries, …) binoc produces a rich semantic diff. When a file has **no** native parser,
the diff degrades to an opaque "the bytes changed" signal, which is not human-readable.

We want a **polyglot fallback**: a two-tier degradation so that *every* file produces
*some* human-readable projection of its content for diffing.

1. **Text-extraction tier.** If the file is a recognizable document/format that isn't
   natively modeled, fall back to extracting its **text** (PDF → text, HTML → text,
   DOCX → text, spreadsheet → cell text, …) so the diff shows readable content changes.
2. **`strings(1)` tier.** For truly unreadable / unknown bytes, fall back to a
   `strings(1)`-style extraction of printable runs so that *something* shows up in the
   diff (embedded labels, paths, version strings, etc.).

This survey covers "dump the text from anything" libraries so we can choose what to
build on for each tier.

## 2. binoc's constraints (the lens for everything below)

These constraints are what make this a non-obvious choice; a tool that is great for
RAG ingestion can be a poor fit here.

- **Rust-first workspace.** binoc is a Rust workspace (`binoc-core`, `binoc-sdk`,
  `binoc-stdlib`, Rust model-plugins under `model-plugins/`). A pure-Rust crate has the
  lowest integration friction.
- **Python plugins are viable but cost packaging.** binoc ships a PyO3 layer
  (`binoc-python/`) and the public install path is `pip install binoc` / `uvx binoc`.
  So a Python extractor *can* live as a plugin — but it drags Python runtime + transitive
  dependency weight into a tool that is otherwise a self-contained CLI/library.
- **Determinism is mandatory.** binoc **diffs the extractor's output**. If the same input
  bytes can yield different text (reordered blocks, nondeterministic OCR, floating-point
  layout heuristics, model-version drift), the extractor will manufacture phantom diffs.
  This is the single most important axis below.
- **Version-pinnable / reproducible.** The extraction toolchain must pin to an exact
  version so a re-run months later yields byte-identical text. Tools that auto-download
  models at runtime (whose weights can change) are a reproducibility hazard.
- **Additive over a byte-hash equality oracle.** Extraction is **never** the source of
  truth for "did the file change" — a content hash already answers that. Extraction is
  only a **human-readable projection** layered on top. This *lowers* the bar (we don't
  need perfect extraction) but it does **not** excuse non-determinism: a projection that
  reshuffles itself run-to-run still produces noisy, untrustworthy diffs.
- **Low build/deploy friction preferred.** A JVM, a GraalVM build step, a Pandoc binary,
  a Tesseract install, or a multi-hundred-MB model download are each a real cost for a
  CLI/library that today installs with a single `pip`/`uvx`.
- **License must be MIT/Apache-2.0 compatible.** This rules out copyleft-for-the-extractor
  and source-available licenses (AGPL, SSPL, **Elastic License 2.0**) for anything we link
  or vendor.

## 3. Comparison table

| Tool | Lang / runtime | Formats | Native vs delegated (system deps) | Build / deploy cost | Determinism | Metadata | OCR | License | Maintenance (as of 2026-06) |
|---|---|---|---|---|---|---|---|---|---|
| **Apache Tika** | JVM (Java) | 1000+ | Delegates to POI, PDFBox, etc. Needs **JVM**; OCR needs **Tesseract** | High (JVM at runtime) | Mostly deterministic for text; OCR/auto-detect can vary | Yes (rich) | Yes (Tesseract) | Apache-2.0 | Very active; 3.3.x stable, 4.0 line in progress |
| **Extractous** | Rust core + FFI | ~most Tika formats | Compiles Tika to **native libs via GraalVM at build time** → no JVM at *runtime*; OCR via **Tesseract** | **High build** (GraalVM AOT), moderate deploy | Inherits Tika text behavior; OCR nondeterminism if used | Yes | Yes (Tesseract) | **Apache-2.0** | Slower cadence: latest **0.3.0 (Dec 2024)**, ~1.8k stars |
| **Kreuzberg** | **Rust core**, 14 lang bindings | **96** | Native Rust extractors; OCR optional (**Tesseract / PaddleOCR / EasyOCR / VLM**); ONNX for embeddings | Moderate (OCR/ONNX optional) | Text deterministic-ish; OCR/VLM paths nondeterministic | Yes | Yes (multiple) | **Elastic License 2.0** ⚠️ | Very active; v5 RC (Jun 2026), ~8.5k stars |
| **unstructured** | Python | Many (PDF, HTML, Office, …) | Heavy Python deps; layout models; OCR (Tesseract) | High (deps + models) | **Low** (ML layout/partition heuristics) | Yes (elements) | Yes | Apache-2.0 core, but **AGPL/LGPL transitive deps** ⚠️ | Active |
| **Docling (IBM)** | Python | PDF, Office, images, … | **Vision-language + layout models** (Granite-Docling 258M), auto-downloaded; OCR | **Very high** (model downloads, optional GPU) | **Low** (model inference) | Yes (rich structure) | Yes | **MIT** (lib) / Apache-2.0 (models) | Very active; ~30k+ stars |
| **textract (Py)** | Python | Many (shells out) | Delegates to **antiword, pdftotext, Tesseract, …** external binaries | Medium-high (many CLIs) | Depends on each sub-tool | Limited | Yes (via others) | MIT | **Largely stale**; `textract-py3` fork also frozen |
| **pdf-extract** (Rust) | Rust | PDF (text) | Native (`lopdf`) | Low | Generally deterministic (single-threaded text) | Minimal | No | MIT/Apache-2.0 | Active (0.10.x) |
| **lopdf / pdf** (Rust) | Rust | PDF (low-level) | Native | Low | Deterministic | Yes (objects) | No | MIT (lopdf) | Active |
| **html2text** (Rust) | Rust | HTML | Native (`html5ever`) | Low | Deterministic | No | No | MIT | Active (0.16.x, 2026) |
| **scraper** (Rust) | Rust | HTML | Native (`html5ever`) | Low | Deterministic | No | No | MIT/ISC | Active |
| **calamine** (Rust) | Rust | XLS/XLSX/ODS/… | Native | Low | Deterministic | Some | No | MIT/Apache-2.0 | Active |
| **quick-xml** (Rust) | Rust | XML | Native | Low | Deterministic | No | No | MIT | Active |
| **docx-rs** (Rust) | Rust | DOCX (writer-centric) | Native | Low | Deterministic | Some | No | MIT/Apache-2.0 | Active (2026) |
| **dotext** (Rust) | Rust | DOCX/ODT/… (read) | Native | Low | Deterministic | No | No | MIT | **Stale (~2017)** |
| **rust-strings** | Rust | any bytes | Native | **Very low** | **Fully deterministic** | No | No | MIT | Active (0.6.x) |

## 4. Per-tool notes

### Apache Tika — the reference
[Apache Tika](https://github.com/apache/tika) is the canonical "extract text + metadata
from over a thousand file types" toolkit. It is a JVM library that delegates to
format-specific parsers (Apache POI for Office, PDFBox for PDF, etc.) and can OCR via
Tesseract. Current stable is the 3.3.x line (3.2.3 released Sept 2025), with a 4.0 line
in progress; Tika 2.x and Java 8 reached EOL in April 2025
([formats list](https://tika.apache.org/3.0.0/formats.html),
[releases](https://github.com/apache/tika/releases),
[roadmap](https://cwiki.apache.org/confluence/display/TIKA/Tika+Roadmap+--+2.x,+3.x+and+Beyond)).
Apache-2.0 licensed and the gold standard for *coverage*. The cost for binoc is the
**JVM at runtime**, which is a heavy dependency for a `pip install` CLI. Tika's plain-text
extraction is largely deterministic, but its auto-detection and OCR paths introduce
variability. We are unlikely to embed Tika directly, but it matters because **Extractous
and (historically) much of this space is Tika under the hood.**

### Extractous — Tika without the JVM
[Extractous](https://github.com/yobix-ai/extractous) is a Rust crate (with Python
bindings) that gets Tika's coverage *without a runtime JVM* by compiling Tika to native
shared libraries via **GraalVM ahead-of-time compilation at build time**, then calling it
over FFI ([README](https://github.com/yobix-ai/extractous/blob/main/extractous-core/README.md),
[crates.io](https://crates.io/crates/extractous)). It does text + metadata and OCR via
Tesseract, and is **Apache-2.0** — license-clean for binoc. This is the most attractive
"broad coverage, Rust-native, permissive license" option on paper.

Caveats:
- **Build complexity.** The GraalVM AOT step is non-trivial; the build script installs a
  GraalVM JDK. This is a real CI/packaging cost and complicates reproducible, pinned builds.
- **Maintenance cadence.** Latest release is **0.3.0 (Dec 2024)** with ~1.8k stars
  ([lib.rs](https://lib.rs/crates/extractous)) — healthy download numbers but a slower
  release pace than the alternatives. Worth confirming the project is still actively
  maintained before betting on it.
- **Determinism** inherits Tika's behavior; plain text should be stable for a pinned
  Tika/GraalVM build, but OCR (if enabled) is not. For binoc we'd keep OCR **off**.

### Kreuzberg — survey carefully (license is the catch)
[Kreuzberg](https://github.com/kreuzberg-dev/kreuzberg) is, technically, the most exciting
option: a **Rust core** with SIMD + parallelism, bindings for ~14 languages, **96 formats**
across documents/office/images/web/email/archives/academic/code (306 languages via
tree-sitter), sync **and** async APIs, optional OCR via Tesseract / PaddleOCR / EasyOCR /
VLM, and claims of "10–100× faster than Python alternatives." It is very active —
**v5.0.0-rc (June 2026), ~8.5k stars**
([format support](https://docs.kreuzberg.dev/reference/formats/),
[lib.rs](https://lib.rs/crates/kreuzberg),
[v4 announcement](https://dev.to/t_ivanova/announcing-kreuzberg-v4-55ia)).

**However — the license is the catch.** The current Kreuzberg is licensed under the
**Elastic License 2.0 (ELv2)**, a *source-available* license with commercial-use
restrictions, **not** an OSI-approved open-source license
([LICENSE](https://github.com/kreuzberg-dev/kreuzberg/blob/main/LICENSE)). Search results
suggest earlier versions were MIT and the project moved to ELv2 in the v4/v5 era — a
license **regression** of the same kind that drove the Elasticsearch/OpenSearch fork
([context](https://socket.dev/blog/developers-burned-by-elasticsearchs-license-change-arent-going-back)).

For binoc this is close to disqualifying for anything we **link or vendor**, given the
MIT/Apache-2.0 requirement. ELv2 forbids providing the software "as a managed service"
and other uses; even if binoc's CLI use might be permissible, taking an ELv2 dependency
into an Apache/MIT project is a licensing-hygiene problem and a supply-chain risk (the
license can change again). **Recommendation: do not depend on Kreuzberg unless/until it
returns to a permissive license, or unless legal confirms ELv2 is acceptable for our use.**
We should re-check the license on each release because the older MIT lineage means the
situation is genuinely in flux. (If invoked purely as a separate CLI subprocess the
analysis differs, but that's a heavier integration than a crate and still inherits the
license-volatility risk.)

### unstructured.io — ML partitioning, AGPL transitive deps
[unstructured](https://github.com/Unstructured-IO/unstructured) is a Python ETL toolkit
that partitions documents into typed "elements" for LLM pipelines. The core is Apache-2.0,
but it pulls **layout/detection models** and transitive dependencies that include
**AGPLv3+ / LGPL** components (e.g. `ultralytics`)
([dependency-license issue](https://github.com/Unstructured-IO/unstructured/issues/3894)).
Its partitioning is **ML-heuristic**, so output ordering/segmentation is not guaranteed
stable across versions or even runs — a poor fit for a diff oracle. Heavy deps + model
downloads + license tangle make this a weak fit for binoc.

### Docling (IBM) — model-driven, heaviest cost, lowest determinism
[Docling](https://github.com/docling-project) is IBM Research's document-understanding
toolkit; it uses **vision-language + layout models** (Granite-Docling 258M, Idefics3-based)
that are **auto-downloaded** and run inference to reconstruct structure, tables, reading
order, etc. ([model card](https://huggingface.co/ibm-granite/granite-docling-258M),
[IBM Research blog](https://research.ibm.com/blog/docling-generative-AI)). The library is
**MIT** and the models **Apache-2.0**, so license is fine — but everything else is the
opposite of what binoc wants: large model downloads, optional GPU, and **model-inference
non-determinism** plus model-version drift. Excellent for rich ingestion; wrong tool for a
reproducible byte-diff projection.

### textract (Python) and equivalents
[textract](https://github.com/deanmalmgren/textract) is the classic "extract text from any
document, no muss no fuss" Python package, implemented by shelling out to external binaries
(antiword, pdftotext, Tesseract, …). It is **MIT** but **largely stale**; the
`textract-py3` fork that fixed dependency pinning is itself now frozen and points users
back upstream ([textract-py3](https://github.com/KyleKing/textract-py3)). Determinism
varies per sub-tool, and the external-binary sprawl is a deploy headache. Not recommended
as a foundation. The actively-maintained "extract from anything" successors today are
Extractous, Kreuzberg, Docling, unstructured, and Microsoft's MarkItDown
([2025 benchmark](https://dev.to/nhirschfeld/i-benchmarked-4-python-text-extraction-libraries-2025-4e7j)).

### Native Rust per-format crates — the deterministic, low-friction core
These are the building blocks for the text-extraction tier if we want maximum
determinism and minimal deps:

- **PDF:** [`pdf-extract`](https://docs.rs/pdf-extract) (built on
  [`lopdf`](https://github.com/J-F-Liu/lopdf)) extracts text from PDFs natively, MIT/Apache,
  active (0.10.x). For lower-level control, [`lopdf`](https://crates.io/crates/lopdf) and
  the [`pdf`](https://crates.io/crates/pdf) crate expose the object model. Newer entrants
  emphasize **deterministic page ordering** explicitly (e.g. `unpdf` documents an internal
  reorder buffer emitting pages in ascending order) — a useful pattern if we parallelize
  ([pdf-extract listing](https://lib.rs/crates/pdf-extract)).
- **HTML:** [`html2text`](https://github.com/jugglerchris/rust-html2text) (HTML → plain
  text via `html5ever`, active, MIT, 0.16.x in 2026) or [`scraper`](https://crates.io/crates/scraper)
  for DOM traversal. Both deterministic. *(Note: there is a same-named `html2text` Rust NIF
  for Elixir — the relevant crate is `jugglerchris/rust-html2text`.)*
- **Spreadsheets:** [`calamine`](https://crates.io/crates/calamine) reads XLS/XLSX/ODS and
  yields cell values as text — deterministic, MIT/Apache. (binoc already has a native Excel
  model plugin, so this is more relevant for spreadsheet-shaped formats outside that plugin.)
- **XML:** [`quick-xml`](https://crates.io/crates/quick-xml) — fast, deterministic, MIT.
- **DOCX/Office:** [`docx-rs`](https://crates.io/crates/docx-rs) is writer-centric but can
  read; [`dotext`](https://github.com/anvie/dotext) reads DOCX/ODT/… but is **stale (~2017)**
  and not recommended. DOCX/PPTX/XLSX are ZIP-of-XML, so `quick-xml` + the zip crate is a
  viable deterministic path we fully control.

### rust-strings — the `strings(1)` tier
[`rust-strings`](https://crates.io/crates/rust-strings) is a small, active, **MIT** Rust
library that extracts printable strings from arbitrary binary data, with configurable
minimum length, buffer size, and ASCII / UTF-16LE / UTF-16BE encodings — i.e. exactly the
`strings(1)`-equivalent for the unreadable-bytes tier ([lib.rs](https://lib.rs/crates/rust-strings)).
It is **fully deterministic** (pure byte scan), has near-zero build/deploy cost, and is
license-clean. This is the obvious, low-risk choice for tier 2.

## 5. Determinism — the key risk

Because binoc **diffs the extractor's output**, determinism is not a nice-to-have; a
nondeterministic extractor actively manufactures false changes. Three distinct failure
modes:

1. **Run-to-run nondeterminism (same binary, same input).** Parallel block extraction
   without a reorder buffer, hash-map iteration order, thread races, or ML sampling can
   reorder or alter output between runs. This is the worst case: binoc would report diffs
   where the bytes are identical. *Risk: high in unstructured, Docling, and any OCR/VLM
   path; mitigated in native Rust crates that emit in document order.*
2. **Version-drift nondeterminism (different tool/model version, same input).** Even if a
   single version is deterministic, an upgraded layout model, OCR engine, or Tika parser
   can change the text. binoc must therefore **pin the extractor version** and treat an
   extractor upgrade as a deliberate, reviewable event (potentially re-baselining
   snapshots). *Risk: severe for model-download tools (Docling, unstructured), where the
   "version" includes downloaded weights that can change out from under a pinned package.*
3. **Heuristic nondeterminism (reading-order / layout reconstruction).** Tools that infer
   reading order from geometry (multi-column PDFs, tables) can flip block order on
   near-ties. This is deterministic for a fixed input+version but **brittle**: a
   one-pixel layout change in the source can cascade into a large text reordering, inflating
   the diff. Native text-stream extraction (e.g. `pdf-extract` reading the content stream)
   tends to be more stable than vision-based layout inference.

**Implication.** The determinism ranking lines up almost perfectly *against* the coverage
ranking: the broadest tools (Docling, unstructured, OCR-heavy paths) are the least
deterministic, and the most deterministic tools (native Rust per-format crates,
`rust-strings`) are the narrowest. Since extraction is only an **additive projection over a
byte-hash oracle**, binoc can tolerate *imperfect* extraction — but it should **not**
tolerate *nondeterministic* extraction, because that defeats the projection's purpose.
Concretely: **keep OCR off** (it is the largest single source of nondeterminism), **pin
versions hard**, and **prefer text-stream extraction over layout/model inference.**

## 6. Recommendation (phased, as input to a later decision)

A staged path that front-loads determinism and low friction, deferring broad coverage
until it is justified:

**Phase 1 — `strings(1)` tier now (lowest risk, highest leverage).**
Adopt [`rust-strings`](https://crates.io/crates/rust-strings) for the unreadable-bytes
fallback. It is pure-Rust, fully deterministic, MIT, and trivially pinnable. This
immediately makes *every* file produce *some* readable projection and establishes the
"additive projection" plumbing in `binoc-core`/`binoc-stdlib`.

**Phase 2 — native Rust text extraction for the highest-value formats.**
Add deterministic, dependency-light extractors for the formats most worth reading:
- PDF via [`pdf-extract`](https://docs.rs/pdf-extract) / `lopdf` (watch page-ordering
  determinism; emit in page order).
- HTML via [`html2text`](https://github.com/jugglerchris/rust-html2text) / `scraper`.
- XML via [`quick-xml`](https://crates.io/crates/quick-xml); spreadsheet-shaped data via
  [`calamine`](https://crates.io/crates/calamine); Office (DOCX/PPTX/XLSX) via
  `quick-xml` + zip where a dedicated reader isn't trustworthy (avoid stale `dotext`).
These are MIT/Apache, deterministic, and add no system deps — the best fit for binoc's
constraints. Keep OCR out of scope.

**Phase 3 — broad coverage *if and when* the long tail justifies it.**
If demand for many exotic formats appears, evaluate a Tika-class engine:
- [**Extractous**](https://github.com/yobix-ai/extractous) is the leading candidate on
  license grounds (**Apache-2.0**) and gives Tika coverage with no runtime JVM. The price
  is the **GraalVM build-time AOT step** (a real reproducible-build / CI cost) and a
  **slower release cadence** to monitor. Run it with **OCR disabled** and **pin the exact
  Extractous + bundled-Tika version**.
- [**Kreuzberg**](https://github.com/kreuzberg-dev/kreuzberg) is technically the most
  capable Rust-core option but is currently **Elastic License 2.0**, which is *not*
  MIT/Apache-compatible. **Do not take it as a linked/vendored dependency under that
  license.** Re-check on each release: its MIT lineage means the license could move back,
  and if it does it becomes the strongest candidate.
- A Python-plugin route (Docling / unstructured via the PyO3 layer) is *possible* but
  fights nearly every constraint — model downloads, AGPL/LGPL transitive deps
  (unstructured), and **inference non-determinism** — so it should be a last resort, scoped
  to formats nothing else covers, and never on the determinism-critical path.

**Cross-cutting guardrails (any phase).** Pin every extractor version; treat extractor
upgrades as reviewable, potentially re-baselining events; keep OCR disabled by default;
prefer text-stream over layout/model inference; and remember the byte-hash oracle remains
the source of truth — text extraction is only a human-readable lens, so when in doubt,
favor the *more deterministic* tool over the *more complete* one.

## Sources

- Apache Tika — repo: <https://github.com/apache/tika> · formats: <https://tika.apache.org/3.0.0/formats.html> · releases: <https://github.com/apache/tika/releases> · roadmap: <https://cwiki.apache.org/confluence/display/TIKA/Tika+Roadmap+--+2.x,+3.x+and+Beyond>
- Extractous — repo: <https://github.com/yobix-ai/extractous> · core README: <https://github.com/yobix-ai/extractous/blob/main/extractous-core/README.md> · crate: <https://crates.io/crates/extractous> · lib.rs: <https://lib.rs/crates/extractous>
- Kreuzberg — repo: <https://github.com/kreuzberg-dev/kreuzberg> · formats: <https://docs.kreuzberg.dev/reference/formats/> · features: <https://docs.kreuzberg.dev/features/> · lib.rs: <https://lib.rs/crates/kreuzberg> · v4 announcement: <https://dev.to/t_ivanova/announcing-kreuzberg-v4-55ia> · LICENSE: <https://github.com/kreuzberg-dev/kreuzberg/blob/main/LICENSE>
- Elastic License context — <https://socket.dev/blog/developers-burned-by-elasticsearchs-license-change-arent-going-back> · <https://www.elastic.co/blog/licensing-change>
- unstructured.io — repo: <https://github.com/Unstructured-IO/unstructured> · dependency-license issue: <https://github.com/Unstructured-IO/unstructured/issues/3894>
- Docling (IBM) — project: <https://github.com/docling-project> · model card: <https://huggingface.co/ibm-granite/granite-docling-258M> · IBM Research blog: <https://research.ibm.com/blog/docling-generative-AI>
- textract — repo: <https://github.com/deanmalmgren/textract> · py3 fork: <https://github.com/KyleKing/textract-py3>
- 2025 Python extraction benchmark — <https://dev.to/nhirschfeld/i-benchmarked-4-python-text-extraction-libraries-2025-4e7j>
- pdf-extract — <https://docs.rs/pdf-extract> · <https://lib.rs/crates/pdf-extract> · lopdf: <https://github.com/J-F-Liu/lopdf>
- html2text — <https://github.com/jugglerchris/rust-html2text> · <https://lib.rs/crates/html2text>
- scraper — <https://crates.io/crates/scraper>
- calamine — <https://crates.io/crates/calamine>
- quick-xml — <https://crates.io/crates/quick-xml>
- docx-rs — <https://crates.io/crates/docx-rs> · dotext: <https://github.com/anvie/dotext>
- rust-strings — <https://crates.io/crates/rust-strings> · <https://lib.rs/crates/rust-strings>
