---
audience: data steward, archivist, data scientist
---

# Diff SEC monthly Financial Statement and Notes bundles

This showcase compares the SEC Financial Statement and Notes monthly ZIPs for
February 2026 and March 2026 as tabular datasets, not opaque archives. It is the
large-dataset benchmark for Binoc's current standard-library path: ZIP expansion
is streamed, and large keyed TSV tables use bounded summary output instead of a
full in-memory `tabular_v1` artifact.

The SEC publishes these bundles monthly. The February 2026 ZIP is 314.12 MB and
the March 2026 ZIP is 293.29 MB on the SEC download page. The downloaded ZIPs
contain 10 members each: eight TSV tables (`sub`, `tag`, `dim`, `ren`, `cal`,
`pre`, `num`, `txt`), `readme.htm`, and `notes-metadata.json`. Their
decompressed member payloads total 2,153,288,328 bytes for February and
1,920,248,123 bytes for March.

The SEC documentation states that the bundle contains tab-delimited files with
lowercase headers and defines the unique keys used by the config below. The 2026
monthly notes `dim.tsv` header names its key column `dimhash`, so the checked-in
config follows the actual bundle header for that table.

## Config

Use the checked-in config:

```bash
docs/examples/sec-monthly-notes/sec-notes-monthly.yaml
```

It declares every SEC `.tsv` member as tab-delimited and gives Binoc the SEC
documentation's row keys:

| File | Row identity |
|---|---|
| `sub.tsv` | `adsh` |
| `tag.tsv` | `tag`, `version` |
| `dim.tsv` | `dimhash` |
| `num.tsv` | `adsh`, `tag`, `version`, `ddate`, `qtrs`, `uom`, `dimh`, `iprx` |
| `txt.tsv` | `adsh`, `tag`, `version`, `ddate`, `qtrs`, `dimh`, `iprx` |
| `ren.tsv` | `adsh`, `report` |
| `pre.tsv` | `adsh`, `report`, `line` |
| `cal.tsv` | `adsh`, `grp`, `arc` |

## Download the inputs

```bash
mkdir -p target/sec-showcase
curl -L -A "binoc showcase benchmark you@example.org" \
  -o target/sec-showcase/2026_02_notes.zip \
  https://www.sec.gov/files/dera/data/financial-statement-notes-data-sets/2026_02_notes.zip
curl -L -A "binoc showcase benchmark you@example.org" \
  -o target/sec-showcase/2026_03_notes.zip \
  https://www.sec.gov/files/dera/data/financial-statement-notes-data-sets/2026_03_notes.zip
```

## Run the benchmark report

```bash
just perf --left target/sec-showcase/2026_02_notes.zip \
  --right target/sec-showcase/2026_03_notes.zip \
  --config docs/examples/sec-monthly-notes/sec-notes-monthly.yaml \
  --mode parallel_parse
```

The perf report emits one JSON object. This measurement was captured on an
Apple M4 Pro Mac (`Mac16,7`) with 48 GiB RAM:

| Contract field | Required by ADR | Measured |
|---|---:|---:|
| Wall-clock | <= 10 minutes | 24.660 s |
| Peak RSS | < 1 GiB | 1,006,000 KiB (about 982 MiB) |
| Input ZIPs | real SEC 2026-02 -> 2026-03 monthly notes bundles | 314.12 MB + 293.29 MB |
| Decompressed payload | real monthly bundle members | 2.15 GB + 1.92 GB |

The structural report confirms that the run expanded both ZIPs and used the
bounded streaming writer for the large TSV members:

```json
"fires": {"binoc.expand.zip": 2, "binoc.pair.name": 10, "binoc.pair.root": 1, "binoc.parse.csv": 2, "binoc.parse.json": 2}
"writer_used": {
  "cal.tsv": "binoc.write.tabular_stream",
  "dim.tsv": "binoc.write.tabular_stream",
  "num.tsv": "binoc.write.tabular_stream",
  "pre.tsv": "binoc.write.tabular_stream",
  "ren.tsv": "binoc.write.tabular_stream",
  "tag.tsv": "binoc.write.tabular_stream",
  "txt.tsv": "binoc.write.tabular_stream"
}
"changeset_json_hash": "d056a5078dbf508a5b3cd4878dc1f8409301324aa1401af67fd012177df0b0cc"
```

## Read the result

The Markdown run uses the same config:

```bash
just binoc diff target/sec-showcase/2026_02_notes.zip \
  target/sec-showcase/2026_03_notes.zip \
  --config docs/examples/sec-monthly-notes/sec-notes-monthly.yaml
```

For the largest tables, Binoc reports bounded keyed-row summaries. A changed
`num.tsv` node reads as row additions, row removals, and rows modified by key,
with the key columns preserved in JSON output for examples. That is the intended
large-data reading: month-over-month disclosure churn by SEC table, not a dump
of millions of row-level edits.

In the measured February -> March run, the high-level changelog reads:

```markdown
## Submission and disclosure rows

- **2026_03_notes.zip/>cal.tsv**: 413737 rows added; 487826 rows removed
- **2026_03_notes.zip/>dim.tsv**: 319326 rows added; 373191 rows removed
- **2026_03_notes.zip/>num.tsv**: 4092008 rows added; 4990819 rows removed
- **2026_03_notes.zip/>pre.tsv**: 2896487 rows added; 3159656 rows removed
- **2026_03_notes.zip/>ren.tsv**: 249409 rows added; 261956 rows removed
- **2026_03_notes.zip/>sub.tsv**: 9679 rows added; 8756 rows removed
- **2026_03_notes.zip/>tag.tsv**: 478026 rows added; 479902 rows removed
- **2026_03_notes.zip/>txt.tsv**: 743043 rows added; 688003 rows removed
```

The result is a real month-over-month SEC reading: March contains a fresh filing
cohort, so the large keyed tables mostly turn over by row identity rather than
showing in-place row edits. The smaller `sub.tsv` table still renders bounded
examples of added and removed submissions, while `readme.htm` and
`notes-metadata.json` surface separately as documentation/metadata changes.

Sources:

- SEC Financial Statement and Notes Data Sets download page:
  <https://www.sec.gov/data-research/sec-markets-data/financial-statement-notes-data-sets>
- SEC Financial Statement and Notes Data PDF:
  <https://www.sec.gov/files/aqfsn_1.pdf>
