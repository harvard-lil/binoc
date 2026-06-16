---
audience: core developers and showcase curators surveying datasets binoc is most likely to be pointed at
---

# Revision-prone datasets: a cross-field catalog

!!! note "Research note, not normative documentation"
    This is a background survey of the datasets binoc is most likely to be
    pointed at, not a description of how binoc currently behaves or a
    commitment to support any of them. It is a curation aid for the
    [showcase pipeline](https://github.com/harvard-lil/binoc-showcase) and a
    coverage check for the rule families. Roughly 110 datasets across ten
    fields were web-verified against primary sources in June 2026;
    load-bearing claims carry a link to the source. Maintenance status,
    release URLs, and acquirability drift — re-verify before building a
    target on any single entry. Poke holes in it.

*binoc generates a changelog by diffing two snapshots of **the same logical
dataset**. So the datasets that matter are not the ones that change — almost
everything changes — but the ones that **silently overwrite already-published
records**: a value edited, a category reassigned, a column renamed, a
historical series restated, a label corrected, a boundary redrawn, with no
per-record changelog shipped. The single most important distinction in this
whole survey is **revision vs. vintage**. A revision changes a record that was
already there (BEA restates 2015 GDP; ClinVar flips a variant from pathogenic
to benign; the tz database corrects Mexico's 1921 offset). A vintage is a fresh
cohort wearing last year's name (NHANES samples new people each cycle; the ACS
1-year file is a different population). Diff a revision and binoc produces a
true changelog; diff a vintage and it produces a wall of meaningless churn. The
headline finding: in **every** field, authoritative reference data revises in
place and ships no changelog — and a surprising fraction of publishers ship a
**partial, machine-readable answer key** for those revisions (a `NEWS` file, a
`prev_symbol` column, a delisting table), which doubles as a ground-truth
oracle for evaluating binoc's output.*

## What counts: revision, not vintage

The showcase already encodes this distinction the hard way. The accepted
targets are all true revisions of a static-or-restated record:
[VA veterans by sex and age](https://www.data.va.gov/dataset/FY-2021-Total-Number-of-Veterans-Veteran-VA-Users-/6fsh-rj6s/about_data)
(a `Gender` column relabeled `Sex`, every number intact);
[CDC BRFSS pre-2011 prevalence](https://data.cdc.gov/Behavioral-Risk-Factors/Behavioral-Risk-Factor-Surveillance-System-BRFSS-P/y4ft-s73h/about_data)
(a frozen historical series silently word-edited in 2025); the
[FDA Purple Book](https://purplebooksearch.fda.gov/downloads) and
[USDA FoodData Central](https://fdc.nal.usda.gov/download-datasets) (the same
catalog re-released monthly / semi-annually with no notes). And the targets the
team had to *rescope* are exactly the vintage traps: the HMDA target's first
draft compared different reporting years (disjoint cohorts of loans) before
being rescoped to the **same year restated** across snapshot → one-year →
three-year vintages; the NSCH target is parked because year-over-year is a fresh
sample of children, so only the schema (questions appearing/disappearing) is
diffable, not the rows.

This catalog generalizes that lesson. Each entry is tagged with a **revision
character** — the vocabulary the rest of the document uses:

| Tag | Meaning | Canonical example |
|---|---|---|
| **silent value edit** | a published number/string is overwritten in place | BEA restates a prior quarter's GDP |
| **reclassification** | a record's category/status flips | ClinVar pathogenic → benign |
| **reformat / rename** | the value is stable but its label/shape changes | `Gender` → `Sex`; HGNC `BAI1` → `ADGRB1` |
| **restatement / backfill** | a whole historical span is re-stated at once | Our World in Data recomputes rolling metrics |
| **retroactive methodology re-release** | a new method re-derives the entire back-series | IHME GBD re-models 1990→present each release |
| **accession merge / deprecation** | identifiers are merged, retired, or remapped | dbSNP `RsMergeArch`; GO term obsoletion |
| **relabeling** | ground-truth labels are corrected on fixed inputs | ImageNet/MNIST label-error fixes |
| **continuous editing** | the same entities mutate constantly | Wikidata, OSM, OFAC SDN |
| **versioned silent update** | a fixed name points at moving content | Hugging Face `main`; Kaggle "latest" |

**Legend** used in every table below.
**Verdict** — ✅ true-revision · ◑ dual-axis (true-revision on one axis,
vintage on another; diff only the right artifacts) · 🚫 vintage trap (listed as
a contrast).
**Acquire** — 🟢 turnkey (publisher hosts dated/numbered snapshots, deltas, or
Git history) · 🟡 DIY (stable-schema file, but you must capture dated copies
yourself / Wayback) · 🔴 hard (gated, paywalled, deleted, or no snapshots).

## Public health, epidemiology & medicine

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [CDC BRFSS historical files](https://www.cdc.gov/brfss/annual_data/annual_data.htm) | silent value edit / rename — named in the [Lancet 2025 analysis](https://doi.org/10.1016/S0140-6736(25)01249-8) of 114 federal datasets silently edited Jan–Mar 2025 (106 swapped "gender"→"sex"), with [DataRescue mirrors](https://www.datarescueproject.org/cdc-brfss/) | SAS/XPT, CSV · annual + silent re-release | 🟢 | ◑ |
| [IHME Global Burden of Disease](https://www.healthdata.org/research-analysis/gbd) | retroactive methodology re-release — each edition re-models 1990→present; vitamin-A deaths went 233k→28k for the *same years* between GBD 2017 and 2019 ([PMC9991746](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC9991746/)) | CSV, API · per-release | 🟢 | ✅ |
| [ClinVar](https://www.ncbi.nlm.nih.gov/clinvar/) | reclassification — ~6% of variants reclassified; 40% of common pathogenic variants downgraded ([Sci Rep](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC6962394/)); stable variant IDs | VCF/XML · monthly archived | 🟢 | ✅ |
| [FDA Purple Book](https://purplebooksearch.fda.gov/downloads) | continuous editing — monthly full-DB re-release tags rows U/N/R but ships no narrative changelog | CSV · monthly | 🟢 | ✅ |
| [FDA Orange Book](https://www.fda.gov/drugs/drug-approvals-and-databases/approved-drug-products-therapeutic-equivalence-evaluations-orange-book) | reclassification — products silently moved to/from the Discontinued section; patent/exclusivity rows edited | `~`-delimited TXT · monthly | 🟢 | ✅ |
| [DailyMed](https://dailymed.nlm.nih.gov/) | continuous editing — labels superseded daily under a stable SetID; "as-of-date" archive | HL7 SPL XML · daily delta | 🟢 | ✅ |
| [RxNorm](https://www.nlm.nih.gov/research/umls/rxnorm/) | accession merge — RXCUIs go active→obsolete→remapped; a Terminology-Status API exists to chase them | RRF · monthly + weekly | 🟢 | ✅ |
| [ICD-10-CM](https://www.cms.gov/medicare/coding-billing/icd-10-codes) | reclassification / rename — FY2026 alone revised 38 existing codes; stable code keys | tabular · annual (Oct 1) | 🟢 | ✅ |
| [WHO ICD-11 (MMS)](https://icd.who.int/) | reclassification — continuous-maintenance model with annual versioned releases; titles/hierarchy edited | API, spreadsheet · annual versioned | 🟢 | ✅ |
| [NCI SEER](https://seer.cancer.gov/data/) | retroactive recoding — Apr 2021 dropped ~10k 1973–2000 cases from *all* DBs via a behavior recode ([change log](https://seer.cancer.gov/data/data-changes.html)) | SEER\*Stat, ASCII · annual | 🔴 | ◑ |
| [Our World in Data COVID-19](https://github.com/owid/covid-19-data) | restatement/backfill — source corrections back-fill history; rolling metrics recomputed; every edit is a Git commit | CSV/JSON · daily (Git) | 🟢 | ✅ |
| [JHU CSSE COVID-19](https://github.com/CSSEGISandData/COVID-19) | restatement/backfill — prior days revised continuously 2020–23; **frozen** since Mar 2023, full Git history preserved | CSV · frozen | 🟢 | ✅ (historical) |
| [CDC WONDER mortality](https://wonder.cdc.gov/) | restatement — provisional counts "continually revised"; cells <10 suppressed (a count can vanish between pulls) | query/TXT export · weekly→annual | 🟡 | ✅ |
| [CMS Care Compare](https://data.cms.gov/provider-data/) | silent value edit — star ratings/thresholds recomputed each period with no per-provider note; stable CCN | CSV · quarterly | 🟢 | ✅ |
| [NHANES](https://www.cdc.gov/nchs/nhanes/) | *(contrast)* — a new independent sample each 2-year cycle; positional diff is noise | XPT, CSV · biennial | — | 🚫 |

## Economics, macro, finance & trade

Statistical agencies revise published history as a matter of routine, and many
maintain a **real-time / vintage archive** precisely so the overwritten numbers
survive — which makes before/after snapshots unusually easy.

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [ALFRED (Archival FRED)](https://alfred.stlouisfed.org/) | silent value edit — FRED overwrites series in place; ALFRED keeps every vintage (>206k revisions tracked for Z.1 alone). A two-vintage download service by design | CSV/XLS/API · continuous | 🟢 | ✅ (gold standard) |
| [BEA GDP / NIPA](https://www.bea.gov/) | retroactive benchmark — the [2023 comprehensive update](https://www.bea.gov/information-updates-national-economic-accounts-2023) revised GDI back to 1979Q1 | CSV/XLS/API · quarterly + 5-yr | 🟢 | ✅ |
| [BLS CES payrolls](https://www.bls.gov/ces/) | benchmark restatement — the [preliminary 2025 benchmark](https://www.bls.gov/sae/notices/2025/preliminary-benchmark-announcement-2025.htm) was −911,000 jobs, restated across months | CSV/XLS/API · monthly + annual | 🟢 | ✅ |
| [BLS CPI (seasonally adjusted)](https://www.bls.gov/cpi/) | silent value edit — SA factors recomputed each January, restating ~5 years of SA history (NSA unchanged) | CSV/XLS/API · monthly | 🟢 | ✅ |
| [Fed Financial Accounts (Z.1)](https://www.federalreserve.gov/releases/z1/) | continuous editing — ["all data subject to revision on an ongoing basis"](https://www.federalreserve.gov/releases/z1/z1_technical_qa.htm); major revisions flagged each release | CSV (DDP)/API · quarterly | 🟢 | ✅ |
| [IMF World Economic Outlook DB](https://www.imf.org/en/publications/weo/weo-database/changes) | retroactive restatement — each Apr/Oct vintage rewrites 1980→ history; vintages archived to 2007 | XLS/SDMX/CSV · biannual | 🟢 | ✅ |
| [World Bank WDI](https://datahelpdesk.worldbank.org/knowledgebase/articles/906522) | restatement / rebasing — Nigeria's 2014 rebase made 2010–12 GDP 60–75% higher; PPP/ref-year rebased silently | CSV/XLS/API · quarterly + annual | 🟡 | ✅ |
| [Penn World Table](https://www.rug.nl/ggdc/productivity/pwt/) | retroactive methodology — v10.01 changed the investment deflator, altering 1950–2019 capital/TFP series; v11.0 current | XLSX/Stata · numbered versions | 🟢 | ✅ |
| [Maddison Project DB](https://www.rug.nl/ggdc/historicaldevelopment/maddison/) | retroactive methodology — the [2023 update](https://www.rug.nl/ggdc/blog/the-maddison-project-new-2023-update-illuminates-origins-of-modern-economic-growth) revised long-run GDP-pc for 169 countries | XLSX/Stata · versioned (2020, 2023) | 🟢 | ✅ |
| [HMDA national loan-level](https://ffiec.cfpb.gov/data-publication/snapshot-national-loan-level-dataset/) | restatement — the same year published as Snapshot → 1-Year → 3-Year as late filings/resubmissions arrive | CSV (pipe)/API · staged | 🟢 | ✅ |
| [OECD Main Economic Indicators](https://www.oecd.org/) | continuous editing — OECD ships a dedicated "Original Release Data and Revisions" (MEI-ORDR) DB to track first-release vs current | CSV/SDMX/XLS · monthly | 🟢 | ✅ |
| [USDA WASDE](https://www.usda.gov/oce/commodity-markets/wasde/historical-revisions) | continuous editing — prior-month balance-sheet estimates revised in place; USDA hosts a historical-revisions tool | PDF/XML/XLS · monthly | 🟢 | ✅ |
| [EIA Petroleum Supply (PSM/PSA)](https://www.eia.gov/petroleum/) | restatement — the annual PSA [revises up to 10 years](https://www.eia.gov/tools/faqs/faq.php?id=1398&t=6) of production history (use PSM/PSA, **not** the un-revised weekly WPSR) | CSV/XLS/API · monthly + annual | 🟡 | ✅ |
| [UN Comtrade](https://comtradeplus.un.org/) | silent value edit — reporters resubmit revised prior-period trade, overwriting earlier figures; no official vintage archive | CSV/JSON/API · continuous | 🔴 | ✅ |
| [Census ACS](https://www.census.gov/programs-surveys/acs) | *(mostly contrast)* — year-over-year is fresh sampling and 5-yr windows overlap; only the COVID-era 2020 reweighting is same-period revision | CSV/API · annual | 🟡 | 🚫 |

## Geospatial, Earth observation, climate & weather

Temperature records carry homogenization adjustments that change *past* months;
satellite archives are reprocessed into new "Collections" that overwrite the
science values for already-observed dates; boundary files are re-released with
silent geometry edits.

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [NOAA GHCN-Monthly v4](https://www.ncei.noaa.gov/products/land-based-station/global-historical-climatology-network-monthly) | silent value edit — each run re-applies the Pairwise Homogenization Algorithm over the *whole* record, changing past adjusted months | fixed-width text · monthly | 🟢 | ✅ |
| [NASA GISTEMP v4](https://data.giss.nasa.gov/gistemp/) | restatement — a dated [update log](https://data.giss.nasa.gov/gistemp/updates_v4/) records concrete corrections (e.g. a 2025-09 fix to a station off by ~12°C); a ready-made oracle | CSV/NetCDF · monthly | 🟢 | ✅ |
| [HadCRUT5 / CRUTEM5 / HadSST4](https://www.metoffice.gov.uk/hadobs/hadcrut5/) | methodology re-release — SST bias corrections restate the whole record between versions | NetCDF/CSV ensembles · ~monthly | 🟢 | ✅ |
| [Berkeley Earth](https://berkeleyearth.org/data/) | silent value edit — the entire record is re-estimated each monthly run | text/NetCDF · monthly | 🟢 | ✅ |
| [ERA5 / ERA5.1](https://cds.climate.copernicus.eu/) | reprocessing — ERA5.1 replaced 2000–2006 to fix a stratospheric cold bias; served as an explicit paired dataset | NetCDF/GRIB · continuous + corrections | 🟡 | ✅ |
| [MODIS Collections (C6→C6.1)](https://lpdaac.usgs.gov/) | collection re-release — recalibration re-derives values for already-observed tiles/dates | HDF-EOS/GeoTIFF · per-collection | 🟡 | ✅ |
| [Landsat Collections (C1→C2)](https://earthexplorer.usgs.gov/) | collection re-release — geometry + radiometry re-derived under the same scene IDs | GeoTIFF · per-collection | 🟡 | ✅ |
| [Sentinel-2 Collection-1](https://dataspace.copernicus.eu/news/2024-10-10-sentinel-2-old-baselines-products-deletion) | reprocessing — same scene/date, new values; **old baselines were deleted Oct–Nov 2024**, so the acquire window is closing | JPEG2000 (SAFE) · campaign | 🔴 | ✅ |
| [Satellite GMSL](https://sealevel.nasa.gov/) | reprocessing — altimeter retracking restates the 1993→ trend between versions | NetCDF/CSV · versioned | 🟡 | ✅ |
| [NOAA nClimGrid-Monthly](https://www.ncei.noaa.gov/products/land-based-station/nclimgrid-monthly) | restatement — preliminary replaced by final for the same period | NetCDF/text · monthly | 🟢 | ✅ |
| [USGS ComCat earthquakes](https://earthquake.usgs.gov/data/comcat/) | silent value edit — a stable event ID's preferred magnitude/location is revised from auto → reviewed → ISC reconciliation | GeoJSON/QuakeML/CSV · continuous | 🟡 | ✅ |
| [WDPA / Protected Planet](https://www.protectedplanet.net/) | continuous editing — the same WDPAID's boundary geometry is silently replaced; monthly snapshots back to 2017-07 | Shapefile/File GDB · monthly | 🟢 | ✅ |
| [OpenStreetMap (full-history)](https://planet.openstreetmap.org/pbf/full-history/) | continuous editing — the full-history PBF already contains every version of every node/way/relation | `.osm.pbf`/XML · weekly + full-history | 🟢 | ✅ |
| [GeoNames](https://download.geonames.org/export/dump/) | continuous editing — the same geonameId's coords/population change; daily `modifications-*` deltas shipped | TSV · daily | 🟢 | ✅ |
| [EPA AQS (AirData)](https://aqs.epa.gov/aqsweb/airdata/) | silent value edit — old samples altered on audit/reanalysis; a "was certified but data changed" status exists | CSV · continuous + annual cert | 🟡 | ✅ |
| [Natural Earth](https://www.naturalearthdata.com/) | continuous editing — same features re-shaped between versions, but a Git `CHANGELOG` already exists (weak motivation, good oracle) | Shapefile/GeoJSON · semver | 🟢 | ✅ (weak) |
| [GADM](https://gadm.org/data.html) | mixed — major versions re-shape geometries (Kashmir split) but also fold in genuinely new subdivisions | GeoPackage/Shapefile · major versions | 🟡 | ◑ |
| [Census TIGER/Line](https://www.census.gov/geographies/mapping-files/time-series/geo/tiger-line-file.html) | *(mostly contrast)* — annual vintages are dominated by legitimately-new boundaries; same-entity geometry shifts are a minority | Shapefile/GeoPackage · annual | 🟢 | 🚫 |
| [NOAA Storm Events](https://www.ncei.noaa.gov/stormevents/) | *(mostly contrast)* — NCEI reformats but states it does *not* change values; revisions are append-only late reports | CSV · monthly | 🟢 | 🚫 |

## Genomics & life-science reference

The richest field by acquirability: nearly all publish **dated/numbered
releases on open FTP**, and several ship their own diff artifact (a built-in
answer key). The differentiator is whether already-present entries change
(true-revision) or releases mostly bolt on new sequences (vintage).

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [ClinVar](https://www.ncbi.nlm.nih.gov/clinvar/docs/downloads/) | reclassification — clinical significance flips P↔VUS↔B on a stable variant ID; monthly VCFs archived by year | VCF/XML/TSV · monthly | 🟢 | ✅ (flagship) |
| [dbSNP](https://www.ncbi.nlm.nih.gov/snp/) | accession merge — rsIDs merge/deprecate; the `RsMergeArch` table is the publisher's own map | VCF/JSON/flat · per-build | 🟡 | ✅ |
| [GRCh38 + patches](https://www.ncbi.nlm.nih.gov/grc) | mixed — GRCh37→GRCh38 is a true coordinate revision; the p1–p14 patches mostly *add* fix-/alt-loci without changing main-chromosome bases | FASTA/AGP/BED · ~annual patch | 🟢 | ◑ |
| [RefSeq](https://www.ncbi.nlm.nih.gov/refseq/) | silent value edit — NM\_/NP\_ sequences revised with a version-suffix bump (NM_005656.1→.6) | FASTA/GenBank/GFF · bi-monthly | 🟢 | ✅ |
| [Ensembl / GENCODE](https://www.ensembl.org/) | re-annotation — gene/transcript models revised, stable-ID versions bump, IDs retired/merged | GTF/GFF3/FASTA · ~quarterly | 🟢 | ✅ |
| [UniProt / Swiss-Prot](https://www.uniprot.org/) | re-annotation + accession merge — sequences corrected; UniSave gives per-entry history | flat/FASTA/XML · 8-weekly | 🟢 | ✅ |
| [Pfam](https://www.ebi.ac.uk/interpro/) | accession deprecation — families "killed"/merged into clans; `dead_families` list shipped | Stockholm HMM · numbered | 🟢 | ✅ |
| [InterPro](https://www.ebi.ac.uk/interpro/) | restatement — member-DB signatures re-integrated; entries change | XML/TSV · 8-weekly | 🟢 | ✅ |
| [PDB (wwPDB)](https://www.wwpdb.org/) | re-refinement — entries re-versioned; the [2007 remediation](https://www.wwpdb.org/documentation/remediation) + a 2022–23 268-entry re-release transform coordinates | PDB/mmCIF · weekly + campaigns | 🟢 | ✅ |
| [NCBI Taxonomy](https://www.ncbi.nlm.nih.gov/taxonomy) | merge + rename — taxids merged to secondary; names/ranks change; the [taxid-changelog](https://github.com/shenwei356/taxid-changelog) tool is an oracle | taxdump (flat) · ~daily | 🟢 | ✅ |
| [GTDB](https://gtdb.ecogenomic.org/) | reclassification — organisms renamed/moved across releases (e.g. *Shigella* folded into *E. coli*) | TSV/FASTA/trees · numbered | 🟢 | ✅ |
| [Gene Ontology](http://geneontology.org/) | accession deprecation — ~4,173 terms obsoleted in 3 years; the `go-ontology-changes` file is a ready answer key | OBO/OWL/GAF · monthly | 🟢 | ✅ |
| [HGNC](https://www.genenames.org/) | rename — official gene-symbol changes (`BAI1`→`ADGRB1`); a `prev_symbol` field is built in | TSV/JSON · continuous | 🟢 | ✅ |
| [miRBase](https://www.mirbase.org/) | rename/renumber — miRNAs renamed (miR-422b→miR-378) and re-bounded; ships `miRNA.diff` + `miRNA.dead` | FASTA/EMBL/GFF · numbered | 🟢 | ✅ (notorious) |
| [gnomAD](https://gnomad.broadinstitute.org/) | *(mostly contrast)* — cross-version AF changes are driven by new samples, not reprocessing the same variants | VCF/Hail/TSV · major versions | 🟢 | ◑ |
| [OMIM](https://www.omim.org/) | continuous editing — entries/allelic-variant classifications edited nightly under stable MIM numbers; registration-gated, no clean FTP archive | flat/API · continuous | 🔴 | ✅ |

## Physical-science reference (chemistry, materials, physics, astronomy)

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [CODATA fundamental constants](https://physics.nist.gov/cuu/Constants/) | re-fit — the [2022 adjustment](https://physics.nist.gov/cuu/Reference/versioncon.shtml) moved α by 4.5× its 2018 uncertainty, shifting 15 dependent constants; archived ASCII per adjustment | ASCII/HTML · ~4-yearly | 🟢 | ✅ |
| [IUPAC standard atomic weights](https://ciaaw.org/atomic-weights.htm) | value edit + reclassification — argon went from 39.948±0.001 to the interval [39.792, 39.963] in 2021 | HTML/PDF · biennial-ish | 🟢 | ✅ |
| [Particle Data Group RPP](https://pdg.lbl.gov/) | re-fit — the neutron-lifetime world average drifted 885.7 s (≤2010) → 878.6 s (2026) on a stable node; machine-readable `mass_width` files per year | web/PDF/CSV · annual/biennial | 🟢 | ✅ |
| [HITRAN](https://hitran.org/) | methodology re-release — HITRAN2020 completely replaced the CO₂ line list for all 12 isotopologues vs 2016 | `.par` fixed-width · major editions | 🟡 | ✅ |
| [Materials Project](https://next-gen.materialsproject.org/) | recompute — [v2021.05.13](https://docs.materialsproject.org/changes/database-versions) silently changed formation energies for many existing `mp-id`s via a new correction scheme | JSON/API/dumps · dated versions | 🟡 | ✅ |
| [NASA Exoplanet Archive](https://exoplanetarchive.ipac.caltech.edu/) | reclassification — re-selecting a planet's "default parameter set" changes its headline mass/radius/period | CSV/VOTable/TAP · weekly | 🟡 | ◑ |
| [Gaia data releases](https://www.cosmos.esa.int/web/gaia) | re-derivation — the same source_id's astrometry/photometry is re-derived (EDR3→DR3 photometry correction folded in) | VOTable/FITS/TAP · major DR + errata | 🟡 | ◑ |
| [ChEMBL](https://www.ebi.ac.uk/chembl/) | re-curation — ChEMBL_33 re-annotated ~250k existing activities; full dumps kept indefinitely | DB dumps/RDF/SDF · numbered | 🟢 | ✅ |
| [DrugBank](https://go.drugbank.com/releases) | corrections — invalid structures/FASTA headers fixed under a stable DBID; academic license required | XML/CSV/SDF · semver | 🟡 | ✅ |
| [Crystallography Open Database](http://www.crystallography.net/cod/) | continuous editing — every CIF is under SVN; each correction is a new revision (already version-controlled; binoc adds the human summary) | CIF/MySQL/SVN · continuous | 🟢 | ✅ |
| [NIST Atomic Spectra DB](https://physics.nist.gov/asd) | re-compilation — energy levels/wavelengths revised across versions; but only the current version is served | web/ASCII export · numbered | 🔴 | ✅ |
| [Minor Planet Center MPCORB](https://minorplanetcenter.net/iau/MPCORB.html) | re-derivation — a designation's orbital elements re-fit daily as observations arrive; MPC hosts no archive of past dailies | fixed-width/JSON/SQLite · daily | 🟡 | ✅ |
| [PubChem Compound](https://pubchem.ncbi.nlm.nih.gov/) | re-standardization — existing CIDs re-canonicalized as the structure pipeline re-runs; dominated by appends | SDF/XML/ASN.1 · rolling + monthly dump | 🟡 | ◑ |
| [SIMBAD / VizieR (CDS)](https://simbad.cds.unistra.fr/) | continuous editing — SIMBAD revises an object's coords/cross-IDs as literature is folded in; no dated dumps | TAP/VOTable/ASCII · continuous | 🔴 | ◑ |
| [NIST-JANAF tables](https://janaf.nist.gov/) | *(contrast)* — historically revised across editions but **frozen since 1998**; no live before/after | HTML/PDF · frozen | — | 🚫 |

## Standards, identifier registries & knowledge bases

Chosen because these domains revise in place by construction — there is no
append-only trap here. Many ship a publisher-authored changelog that doubles as
ground truth.

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [IANA tz database](https://data.iana.org/time-zones/tzdb/NEWS) | retroactive correction — 2025a corrected Philippine offsets before 1900 & 1937–90; 2024b corrected Mexico 1921–1997. The `NEWS` file is a built-in answer key | text source · ~3–6/yr | 🟢 | ✅ (flagship) |
| [OFAC SDN list](https://ofac.treasury.gov/sanctions-list-service) | value edit + delisting — entities added, removed, and silently edited (aliases, passport numbers) with no per-record changelog | XML/CSV/PIP · ~daily | 🟢 | ✅ (flagship) |
| [GLEIF LEI](https://www.gleif.org/en/lei-data/gleif-concatenated-file/) | value edit + status — legal names/addresses revised; status ISSUED→LAPSED→RETIRED; **daily delta files** shipped | XML/CSV/JSON · daily ×3 | 🟢 | ✅ |
| [CVE / NVD](https://nvd.nist.gov/vuln/data-feeds) | rescore + reclassification — CVSS scores revised, descriptions edited, records flipped to REJECT; per-CVE change history exposed | JSON (CVE 5.0) · hourly | 🟢 | ✅ |
| [Unicode CLDR](https://github.com/unicode-org/cldr) | value edit — a locale's translations/number/date formats change between tagged releases | XML (LDML)/JSON · ~2/yr | 🟢 | ✅ |
| [MITRE ATT&CK](https://github.com/mitre-attack/attack-stix-data) | reclassification + revoke — techniques revoked/merged/renamed (T1574.002 → renamed T1574.001); official detailed changelog | STIX 2.1 JSON · ~2/yr | 🟢 | ✅ |
| [ISO 3166 country codes](https://www.iso.org/iso-3166-country-codes.html) | reassignment + rename — names change (Turkey→Türkiye, Macedonia→North Macedonia); official DB paywalled, GitHub mirrors carry history | DB/newsletters · ad hoc | 🟡 | ✅ |
| [ISO 4217 currency codes](https://www.six-group.com/en/products-services/financial-information/data-standards.html) | reassignment + delisting — numbered amendments retire/replace codes | XML/PDF · per amendment | 🟢 | ✅ |
| [OurAirports](https://github.com/davidmegginson/ourairports-data) | reassignment — a persistent integer ID survives an airport code change or a status→closed; full Git history | CSV · nightly (Git) | 🟢 | ✅ |
| [Wikidata](https://dumps.wikimedia.org/wikidatawiki/entities/) | continuous editing — the same entity's statements change (and get vandalized + reverted); weekly JSON dumps | JSON/RDF · weekly + live | 🟢 | ✅ |
| [MusicBrainz](https://metabrainz.org/datasets) | continuous editing + merges — MBID redirects record merges; twice-weekly dumps | PostgreSQL/JSON · ~2×/wk | 🟢 | ✅ |
| [Public Suffix List](https://github.com/publicsuffix/list) | edit + delisting — rules edited/removed under a single file; full Git history | `.dat` · a few/wk | 🟢 | ✅ |
| [IEEE MAC OUI registry](https://standards-oui.ieee.org/) | reassignment/rename — org names change on M&A under a fixed prefix | TXT/CSV · ~daily | 🟡 | ✅ |
| [IANA Root Zone DB](https://www.iana.org/domains/root/db) | reassignment + delisting — registry-operator changes, ccTLD retirements | HTML/`root.zone` · continuous | 🟡 | ✅ |
| [DBpedia](https://www.dbpedia.org/) | continuous editing — re-extracted from Wikipedia each release, so the same entity's facts shift | RDF/TTL · periodic | 🟢 | ✅ |

## Government, legal, civic & ML benchmarks

| Dataset (publisher) | Revises | Format · cadence | Acq. | Verdict |
|---|---|---|---|---|
| [eCFR](https://www.ecfr.gov/) | continuous editing — the same regulation text is amended in place; the live "current XML" overwrites with no section-level diff exposed | XML/JSON/PDF · daily | 🟢 | ✅ (flagship) |
| [US Code (OLRC)](https://uscode.house.gov/download/download.shtml) | continuous editing + reclassification — sections renumbered in place; OLRC ships [editorial-reclassification tables](https://uscode.house.gov/editorialreclassification/t34/index.html) as an answer key | USLM XML · release points | 🟢 | ✅ (flagship) |
| [SEC EDGAR Financial Statement Sets](https://www.sec.gov/data/financial-statements) | restatement — a fiscal period is refiled (10-K/A) with restated figures; EDGAR keeps the original and every amendment forever | TSV/ZIP (XBRL) · quarterly | 🟢 | ✅ (flagship) |
| [labelerrors.com corrected sets](https://labelerrors.com/) | relabeling — given vs. corrected labels keyed to original indices across 10 benchmarks; ≥6% of the ImageNet val set, 2,916 val errors ([arXiv](https://arxiv.org/abs/2103.14749)). A pre-built gold diff | JSON/CSV overlay · one-shot | 🟢 | ✅ (ML flagship) |
| [ImageNet + ReaL/ReLabel](https://github.com/naver-ai/relabel_imagenet) | relabeling — same images, single → corrected/multi-label; 30–34% of images have multiple valid labels | label files · multiple relabelings | 🟢 | ✅ |
| [MS COCO](https://cocodataset.org/) | relabeling — ~273k annotation errors found; [MJ-COCO-2025](https://medium.com/@jamie_34747/how-i-found-nearly-300-000-errors-in-ms-coco-79d382edf22b) is a corrected re-release sharing image IDs | JSON annotations · patched + forks | 🟢 | ✅ |
| [Hugging Face Hub datasets](https://huggingface.co/datasets) | versioned silent update — a dataset is a Git repo; `main` advances and `load_dataset` pulls new content unless `revision=` is pinned | Parquet/Arrow/CSV · per-commit | 🟢 | ✅ |
| [CourtListener / Free Law](https://www.courtlistener.com/help/api/bulk-data/) | silent value edit — opinions corrected/withdrawn/superseded; text re-OCR'd over time, under a stable cluster/opinion ID | JSON bulk/API · rolling | 🟢 | ✅ |
| [FEC filings](https://www.fec.gov/data/filings/) | restatement — amendments (F3 amend-1, -2…) supersede the original for the same committee/period | `.FEC`/CSV/JSON · nightly | 🟢 | ✅ |
| [MIT Election Lab returns](https://electionlab.mit.edu/data) | silent value edit — parallel "unofficial" and "official/certified" repos hold the same contest's revised totals | CSV · per cycle (Git) | 🟢 | ✅ |
| [USPTO Patent Assignment](https://data.uspto.gov/bulkdata/datasets) | reclassification — assignee/role disambiguation re-resolved across annual editions for the same patents | CSV bulk · annual | 🟡 | ✅ |
| [Congressional bill text](https://www.govinfo.gov/help/bills) | reformat/version progression — Introduced→Engrossed→Enrolled under one bill ID (but versions are *labeled*, so partly already changelogged) | USLM XML · per stage | 🟢 | ◑ |
| [LAION-5B → Re-LAION-5B](https://laion.ai/blog/relaion-5b/) | re-release with deletions — [Re-LAION](https://www.404media.co/massive-ai-dataset-back-online-after-being-cleaned-of-child-sexual-abuse-material/) removed 2,236 links (a safety scrub) under a refreshed identity | Parquet index · re-release | 🟡 | ✅ (deletion-only) |
| [Kaggle datasets](https://www.kaggle.com/datasets) | versioned silent update — immutable numbered versions under one slug; consumers pull "latest"; API resists fetching prior versions | any · per-version | 🟡 | ✅ |
| [USAspending.gov](https://www.usaspending.gov/data/) | *(mostly contrast)* — mods are reported as *new records* by design; only the "Correction Delete Indicator = D" path is true revision | CSV/ZIP/API · quarterly | 🟢 | 🚫 |
| [Common Crawl / C4](https://commoncrawl.org/) | *(mostly contrast)* — each monthly crawl is a fresh web cohort; only C4's changing cleaning heuristics are a minor revision angle | WARC/WET · monthly | 🟢 | 🚫 |

## Cross-cutting findings

### 1. Acquirability is the gating constraint, and it sorts cleanly

The revision behavior is nearly universal; the ability to get two comparable
snapshots is what separates a buildable target from a research curiosity. Three
tiers recur across every field:

- **🟢 Turnkey — the publisher hosts the history.** Either a purpose-built
  vintage store (ALFRED, IMF WEO, ALFRED-fed BEA/BLS/Fed), numbered/dated
  releases on open FTP (the entire genomics column; ChEMBL; CODATA; PDG), a
  monthly/daily full re-release (FDA Purple Book, GLEIF, OFAC, WDPA), or Git
  itself (OSM full-history, Wikidata dumps, Our World in Data, Hugging Face,
  OurAirports, US Code release points, SEC EDGAR). This is where showcase
  targets should come from — most of the catalog.
- **🟡 DIY — stable schema, but you must capture dated copies yourself.** The
  data is a clean keyed file but the publisher serves only "current" (NASA
  Exoplanet Archive TAP, MPCORB dailies, UN Comtrade, NIST ASD, SIMBAD,
  Materials Project old versions, World Bank old editions). Snapshots come from
  your own scheduled pulls or the Wayback Machine. binoc works fine; the
  collector carries the burden.
- **🔴 Hard — gated, paywalled, or deleted.** SEER (data-use agreement), OMIM
  (registration), ISO 3166 (paywalled official DB), Sentinel-2 old baselines
  (actively deleted Oct–Nov 2024), CSD/ICSD (commercial). Worth naming; not
  first targets.

### 2. A surprising number of publishers ship their own answer key

The most useful pattern for *testing* binoc: many of these datasets revise
silently in the payload but ship a separate, machine-readable record of what
changed. That artifact is a **ground-truth oracle** — run binoc on two
snapshots, then check its generated changelog against the publisher's:

| Dataset | Publisher-shipped answer key |
|---|---|
| IANA tz database | the `NEWS` file (per-release retroactive corrections) |
| Gene Ontology | `go-ontology-changes` |
| miRBase | `miRNA.diff` / `miRNA.dead` |
| HGNC | `prev_symbol` column |
| NCBI Taxonomy / dbSNP | merged-id lists / `RsMergeArch` |
| NVD | per-CVE change history + "Last Modified" |
| GeoNames | daily `modifications-YYYY-MM-DD` deltas |
| MITRE ATT&CK | the detailed version-to-version changelog |
| US Code (OLRC) | editorial-reclassification tables |
| labelerrors.com / MJ-COCO | the corrected-label overlay itself |
| GISTEMP | the dated "Updates to Analysis" log |
| NASA GISTEMP, Natural Earth, COD | Git/SVN history or update log |

These should be the **first** datasets used to build binoc's quality
regression: the desired output already exists in structured form.

### 3. The dual-axis trap is the recurring curation hazard

Several of the most cited datasets revise on one axis and vintage on another,
and a naive run hits the wrong one. The discipline, in every case, is to **diff
the right artifact and join on the stable key**:

- **BRFSS / SEER**: the annual new-cohort axis is a vintage trap; the silently
  re-released historical files (BRFSS) and the retroactively recoded full series
  (SEER) are the true-revision targets.
- **Gaia / NASA Exoplanet Archive / ChEMBL / PubChem / MPCORB**: all *also*
  append new objects heavily — diff on the stable key (`source_id`, `mp-id`,
  CID, designation) and ignore pure additions, which is exactly the
  add-vs-revise distinction binoc's correspondence engine must surface.
- **gnomAD / GRCh38 patches**: cross-version change is dominated by new samples
  / added alt-loci, not in-place revision of existing entries — scope tightly or
  skip.
- **ACS / TIGER / NHANES / USAspending / Common Crawl**: predominantly vintage;
  include only as contrast, or restrict to the narrow true-revision sliver
  (ACS 2020 reweighting; the USAspending "D" correction path).

### 4. Format coverage: what pointing binoc here actually demands

The catalog stress-tests the rule families well beyond the CSV/ZIP showcase. By
frequency, the formats a "top-100" run must handle: delimited tabular (CSV/TSV,
the bulk), then **bioinformatics flat formats** (VCF, GFF/GTF, FASTA,
GenBank, mmCIF/PDB, OBO/OWL, Stockholm) — a large, underserved cluster;
**XBRL-derived TSV** (EDGAR); **fixed-width scientific text** (GHCN, PDG
`mass_width`, MPCORB, HITRAN `.par`, CODATA ASCII); **gridded binary**
(NetCDF/HDF/GRIB for climate and reanalysis); **geospatial vector**
(shapefile, GeoJSON, GeoPackage, `.osm.pbf`); **structured documents**
(USLM/legislative XML, STIX 2.1 JSON, HL7 SPL XML, LDML); and **versioned
columnar** (Parquet/Arrow on the Hugging Face Hub). This intersects the
[data.gov format landscape](data-gov-inventory-analysis.md) findings: XML and
geospatial vector are the largest gaps; the genomics flat formats and gridded
scientific binary are the largest *new* demand surfaced by this revision lens
specifically. None is out of architectural scope; several (VCF, GTF, NetCDF,
mmCIF) would each unlock a whole field's worth of turnkey, answer-key-bearing
targets.

## The strongest targets, and how they extend the showcase

The existing showcase leads with US-government tabular CSVs. This catalog says
the highest-signal additions, ranked by *signal × acquirability × a documented
incident*, are:

1. **IANA tz database** — the platonic case: a tiny text dataset that
   retroactively rewrites the past, ships its own `NEWS` answer key, and has
   full Git history. The clearest possible "they edited history and you'd never
   know" story.
2. **ClinVar** — stable keys, monthly dated archives, a quantified
   reclassification rate, and life-or-death stakes. The flagship for a
   scientific audience.
3. **US Code release points / eCFR** — "the same law, silently amended, no
   diff shipped," in clean USLM XML, with OLRC's reclassification tables as
   ground truth. The flagship for a legal audience.
4. **SEC EDGAR restatements** — a 10-K vs its 10-K/A: same period, restated
   numbers, real financial stakes, everything retained forever.
5. **OFAC SDN / GLEIF** — daily-cadence registries that edit and delist
   entities with no per-record note; GLEIF even ships delta files.
6. **NOAA GHCN / NASA GISTEMP** — the politically loaded climate case: past
   months' temperatures change with each homogenization run, and GISTEMP's
   update log is the oracle.
7. **labelerrors.com / ImageNet relabeling** — the ML-audience hook: a
   pre-built before/after over the most famous benchmarks in the field.

Each is a turnkey acquire, each has a citable incident, and several ship the
answer key binoc's output can be graded against. They are the natural next wave
of [showcase targets](https://github.com/harvard-lil/binoc-showcase) once the
formats they need (text/tz source, VCF, USLM XML, XBRL TSV) are in reach — and
they extend the showcase's reach from "US open-data CSVs" to law, science,
finance, and machine learning, which is where the "I want changelogs like that
for *my* data" reaction is most likely to land.

## Next steps

- **Promote the turnkey, tabular-or-text true-revision targets into the
  showcase pipeline first** (tz, FDA already in; add OFAC, GLEIF, ClinVar once
  VCF lands, US Code/eCFR once USLM XML lands).
- **Build the quality regression on the answer-key datasets** (§2): they give a
  structured target to diff binoc's generated changelog against, which the
  current showcase (verbatim-output-only) lacks.
- **Treat the dual-axis datasets as correspondence-engine tests** (§3): they are
  the cleanest real-world exercises of add-vs-revise discrimination on a stable
  key.
- **Use the format demand (§4) to prioritize parse rules**: VCF, GFF/GTF,
  USLM/legislative XML, and NetCDF each convert a whole field from "interesting
  but unbuildable" to a stack of turnkey targets.
