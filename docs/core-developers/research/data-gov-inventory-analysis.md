# data.gov Inventory Analysis — Format Landscape for binoc

**Date:** 2026-06-15
**Status:** Research / exploratory
**Analysis script:** `scripts/analyze_data_gov_inventory.py` (in the repo)

## What this is

We obtained the [harvard-lil/gov-data](https://source.coop/harvard-lil/gov-data)
inventory — a `metadata.jsonl` dump covering **every file linked from
data.gov** at capture time. This document analyzes that inventory to understand
which file formats and structures matter most for binoc, so we can prioritize
correspondence-rule coverage. A later phase will sample resources, fetch current
versions, and test binoc's diff output against real before/after pairs.

The dump is **5.5 GB / 311,820 datasets / 1,925,167 resource links**. Each
JSONL record is one archived dataset and carries: the full data.gov (CKAN)
metadata, the dataset's declared `resources[]` (the catalog's file links), and a
`zip_entries[]` listing of what was actually captured into the dataset's nabit
bag (real payloads live under `data/files/`).

To reproduce:

```bash
scripts/analyze_data_gov_inventory.py path/to/metadata.jsonl --json report.json
```

The script streams the file (constant memory, ~22 s for the full dump),
classifies every resource into a **binoc bucket** aligned with our rule families
(`tabular`, `structured_document`, `archive`, `database`, `text`, `document`,
`image`, `web_page`, `web_service`, `executable`, `other`), and reports both the
declared-resource view and the actually-captured-payload view.

## Two views, two stories

There are two ways to count formats, and they disagree — usefully.

1. **Declared resources** — the catalog's links. `resources[].format` is often
   blank (47% empty), so the script falls back to a file extension parsed from
   the resource URL.
2. **Captured payloads** — what the harvester actually fetched into each bag
   (`data/files/*`). This is closer to "what bytes exist," but it is **heavily
   contaminated with incidental context** (see Caveats).

### Declared resources by binoc bucket (1.93M resources)

| Bucket | Count | Share |
|---|---:|---:|
| `other` (mostly XML — see below) | 882,476 | 45.8% |
| `structured_document` (JSON, RDF, GeoJSON, YAML…) | 261,016 | 13.6% |
| `web_page` (HTML) | 229,490 | 11.9% |
| `archive` (zip/tar/gz) | 140,850 | 7.3% |
| `document` (PDF, DOC) | 85,759 | 4.5% |
| `text` | 77,834 | 4.0% |
| `tabular` (CSV/TSV/XLS/XLSX) | 67,596 | 3.5% |
| `web_service` (ESRI REST, WMS, WFS…) | 66,851 | 3.5% |
| `image` (TIFF, JPEG, PNG, MrSID) | 60,003 | 3.1% |
| `executable` (EXE, BIN) | 51,260 | 2.7% |
| `database` (SQLite…) | 115 | 0.0% |

The headline: **XML is the single largest format and currently lands entirely in
`other`** (binoc has no XML parser; `xml` is not in any bucket). XML alone is
**203,523 resources** — adding XML support would roughly *double* binoc's
structured-document footprint by resource count. This is the largest single
coverage gain available.

### Captured payloads by binoc bucket (2.3M files under `data/files/`)

| Bucket | Count | Share |
|---|---:|---:|
| `web_page` (HTML) | 994,561 | 43.3% |
| `structured_document` (JSON/JSON-LD/XML/RDF) | 563,057 | 24.5% |
| `image` (PNG/JPG/TIFF) | 332,663 | 14.5% |
| `archive` | 161,212 | 7.0% |
| `document` (PDF) | 81,749 | 3.6% |
| `tabular` | 56,191 | 2.4% |
| `text` | 40,104 | 1.7% |

Top captured extensions: `html` 43.3%, `png` 12.3%, `json` 10.8%, `xml` 6.9%,
`jsonld` 4.6%, `zip` 4.6%, `pdf` 3.4%, `gz` 2.4%, `jpg` 1.8%, `txt` 1.7%,
`csv` 1.5%.

### Structural signal: nested archives are everywhere

**107,908 of 311,778 bags (~35%) contain at least one nested archive** (a
zip/tar/gz inside the captured payload). Archive expansion and recursion is not
an edge case for this corpus — it is a third of it. binoc already handles
zip/tar/gzip expansion; this confirms that path must stay robust.

## Is the XML data or artifact? (And RDF, HTML?)

A key question: much of data.gov is *metadata about data*, not data. We
classified the 203,523 XML resources:

| XML kind | Count | Share |
|---|---:|---:|
| **Geospatial metadata records** (ISO 19115/19139, FGDC sidecars) | 151,768 | **74.6%** |
| Genuine data (Socrata `rows.xml` / API exports) | 13,974 | 6.9% |
| Unclassified (mix) | 37,781 | 18.6% |

So **~75% of data.gov XML is geospatial *metadata*** — `.iso.xml` /
`.fgdc.xml` sidecars describing other files (e.g.
`tl_2023_roads.shp.ea.iso.xml`, NGDC `H04659.xml`, USGS "Original Metadata").
This is **not a reason to skip XML** — detecting when a dataset's described
temporal extent, contact, lineage, or bounding box changed is exactly the
archival-provenance signal binoc exists to surface. But it reframes the value:
the dominant XML payload is *records about datasets*, and our diff output should
read well for those.

- **RDF** (14,022) is almost entirely Socrata open-data portals
  (`data.cityofnewyork.us`, `data.austintexas.gov`, `data.ny.gov`,
  `data.cdc.gov`) — DCAT/dataset exports. Niche; a generic XML parser covers
  RDF/XML structurally.
- **HTML** (209,850) is 82% plain web pages + viewer/service pages, ~5% data.gov
  landing pages. This is context, not data — binoc's render-only stance toward
  HTML (a renderer plugin, no data parser) is correct.

## Caveats — this collection was hurried and broad

The capture pulled in substantial incidental noise. Evidence in the captured
payloads:

- **43% of captured files are HTML** — landing pages and documentation grabbed
  as context, not dataset data.
- **12% are PNG**, dominated by agency logos (`noaa_logo.png`,
  `CensusLogo-white.png`) and map tiles.
- Much of the captured **JSON / JSON-LD is DCAT catalog metadata**
  (`catalog.json`, `data.json`, `dcat-us`), not dataset content.

**Implication:** the *captured-payload* view overcounts metadata and context.
The *declared-resource* view (filtered to real data formats/extensions) is the
more honest basis for "what data does a dataset actually publish."

Other notes:
- The `resources[].size` field is **null throughout this dump** — no size
  distribution is available here. (The script computes one if a future dump
  carries sizes.)
- **Agency concentration is extreme:** NOAA 33.9%, Census 24.7%, DOI 12.1%,
  NASA 7.2% — the top 4 agencies are ~78% of all datasets. Any sampling for
  testing must be stratified across agencies, or it will just measure NOAA's
  and Census's house styles.

## Implications for binoc feature priorities

Cross-referencing the corpus against current binoc coverage (Excel incl. `.xls`,
JSON/JSON-LD, CSV/TSV, SQLite, DBF, Avro, Parquet/Arrow, Stata/SAS, and the tree
formats YAML/TOML/INI/CBOR/MsgPack/BSON/Plist/Ion):

1. **XML → `structured_document` (highest leverage).** Largest single format,
   currently unsupported. One parser also covers RDF/KML/GML/Atom structurally.
   Mostly ISO/FGDC metadata, which is still valuable to diff. *In progress:
   `model-plugins/binoc-xml`, tagged `format: "xml"` for later XML-specific
   rewrite rules.*
2. **Shapefile / geospatial vector.** Census (25% of datasets) is overwhelmingly
   TIGER shapefiles. We already parse `.dbf` attributes; the gaps are `.shp`
   geometry and treating the `.shp/.shx/.dbf/.prj` sibling set as one logical
   dataset (a **multi-input parsing** question binoc may not yet support).
   *Under investigation: `model-plugins/binoc-shapefile` + a multi-input design
   proposal.*
3. **JSON-LD extension dispatch.** JSON-LD was only matched by media type, not
   the `.jsonld` extension. *In progress: add the extension and tag it
   `format: "jsonld"` distinctly.*
4. **Polyglot text fallback (PDF/HTML/opaque binary).** PDF is 79k captured
   files; "everything else" is large. A `strings`-style additive fallback over
   the byte-hash truth keeps binoc from going silent on unparseable bytes.
   *In progress: `rust-strings` extension to the binary comparison; broader
   text-extraction options surveyed in
   [`research/polyglot-text-extraction.md`](polyglot-text-extraction.md).*
5. **Deferred:** NetCDF/HDF (~5k, n-dimensional binary, specialized) and native
   RDF semantics (niche; XML parser covers RDF/XML structurally).

## Next steps

- **Stratified sampling for fetch-and-test.** Add a `--sample-urls` mode to the
  analysis script that emits a representative, agency-stratified list of *real
  data* resource URLs (filtered by format/extension, excluding web pages and
  service endpoints). Fetch current versions and build before/after test vectors
  to measure binoc's actual diff quality — not against captured logos and
  landing pages, but against the data formats that matter.
- **Re-run the bucket analysis after XML lands** to confirm the
  `structured_document` share roughly doubles as predicted.
