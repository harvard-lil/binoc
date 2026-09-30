---
audience: core developers surveying techniques for extracting tables from messy plain-text / CSV files
---

# Extracting tables from messy plain text: ExtracTable and Pytheas

!!! note "Research note, not normative documentation"
    This is a background survey of two academic table-extraction systems,
    written so an implementer could re-build their techniques from scratch, potentially in rust as part of binoc. Where a technique originates
    with earlier work the two systems build on, that is credited too. Verified
    against the primary sources, June 2026.

*Two systems attack the same problem from opposite methodological poles, and
they share one load-bearing intuition: **a real data table keeps each column
internally coherent — values in a column look alike — and surrounding junk
does not.** Everything else is machinery for exploiting that signal under the
fog of an undocumented format. [ExtracTable](#part-a-extractable) treats it as
a **combinatorial search + scoring** problem: enumerate every way each line
could be parsed (CSV dialects and fixed-width ASCII column boundaries), score
each candidate table by a closed-form column-consistency measure, and pick the
non-overlapping set of tables that maximizes total consistency via a
shortest-path solve. [Pytheas](#part-b-pytheas) treats it as a **fuzzy,
learned classification** problem: a catalog of human-readable rules votes on
whether each line (and each cell) is data or not-data, the votes are fused with
a noisy-OR, and table boundaries fall out of the line-label sequence. The two
are complementary, and the [synthesis section](#part-c-synthesis-for-an-implementer)
argues an implementer should merge ideas from the front half of ExtracTable and the back
half of Pytheas.*

## Why this is in binoc's research folder

binoc diffs datasets. A large fraction of open data is published as plain-text
"CSV" that does not obey RFC 4180 — custom delimiters, preamble metadata,
multiple stacked tables in one file, footnotes, fixed-width ASCII layout. To
diff two snapshots of such a file *as tables* (column reorder, row add/remove,
cell edits) rather than as opaque lines, a parse/expand rule first has to
**find the table(s)** and recover the right parsing instructions. That is
exactly the problem these two systems solve. This is research input for a
future robust tabular expand rule; it is not a commitment to either approach.
See also [Format-aware diff tools](format-aware-diff-tools.md) and the
[data.gov inventory analysis](data-gov-inventory-analysis.md) for why tabular
coverage matters.

## Provenance and credit

- **ExtracTable** — Leonardo Hübscher, *ExtracTable: Extracting Tables From
  Plain Text*, Master's thesis, Hasso Plattner Institute / University of
  Potsdam (supervisor Prof. Dr. Felix Naumann; advisor Lan Jiang), May 2021.
  Repo: `HPI-Information-Systems/ExtracTable`. The thesis is the authoritative
  description; the `table-extraction/` Python package is the reference
  implementation.
- **Pytheas** — Christina Christodoulakis, Eric B. Munson, Moshe Gabel, Angela
  Demke Brown, and Renée J. Miller, *Pytheas: Pattern-based Table Discovery in
  CSV Files*, PVLDB 13(11):2075–2089, 2020
  ([DOI 10.14778/3407790.3407810](https://doi.org/10.14778/3407790.3407810)).
  Repo: `cchristodoulaki/Pytheas`.
- **Techniques the two build on, credited where used below:** the column
  *homogeneity* (Herfindahl/Simpson) measure ExtracTable scores with is
  attributed in the thesis to **Guo et al.**; ExtracTable positions its
  pattern-based dialect detection relative to **Burg et al.** ("Wrangling messy
  CSV files", the `CleverCSV` line of work). Pytheas's symbol-abstraction
  lineage traces to pattern/regex-summary work on data profiling.

## The shared intuition, stated precisely

Both systems reduce a cell value to an **abstract shape** and ask whether a
column's shapes agree:

- ExtracTable abstracts a value into a short sequence of **typed components** —
  `Number` (N), `String` (S), `Other` (O, runs of punctuation), `Known` (K, a
  value matching a curated regex such as a date or email), `Empty` (E). The
  string `"Gina Carano"` becomes `S,O,S`; `"1553725"` becomes `N`;
  `"/a/b.txt"` becomes `O,S,O,S,O,S` (or collapses to a single `Known`
  file-path component).
- Pytheas abstracts a value into a **symbol sequence** ("train") of run-length
  pairs over the alphabet `D` (digit), `A` (letter), `S` (space), and literal
  punctuation: `"-12.5"` -> `[[D,2],[.,1],[D,1]]`, `"Jan 2020"` ->
  `[[A,3],[S,1],[D,4]]`.

A column of values is **consistent** when its abstractions agree. Both systems
make this quantitative, and both deliberately treat **missing values as
neutral** (they neither confirm nor break consistency) because real data
columns are full of nulls. The differences are in (a) how the abstraction is
summarized over a column, (b) how consistency is turned into a decision, and
(c) how that decision is lifted from "is this a clean column" to "where are the
table boundaries."

---

# Part A: ExtracTable

ExtracTable's distinguishing claims over prior CSV-only work: it handles
**multiple stacked tables per file**, **ASCII fixed-width tables** (not just
delimited CSV), and **CSV dialects that deviate far from RFC 4180** (multi-char
delimiters, unusual quote/escape). It is a six-stage streaming pipeline.

```
Read file -> Detect parsing instr. -> Apply parsing instr. -> Describe data types -> Build table candidates -> Select tables
```

The governing idea: **don't commit to one interpretation of a line.** Each line
is parsed every plausible way at once (the "solution space"), data-type
consistency across adjacent rows decides which interpretations cohere into a
table, and a global optimization picks the final non-overlapping table set.

## A.1 Definitions the method assumes

- A **data table** is an `m × n` grid (`m ≥ 2`, `n ≥ 2`) with an optional
  header of `h` rows (`0 ≤ h < m`); all values in a column share an attribute,
  so transposed/1-D lists are explicitly *not* tables.
- A **parsing instruction** is a triple `⟨type, B, dialect⟩`: `type` is CSV or
  ASCII, `B` is the set of column boundaries (for ASCII), and `dialect` is the
  triple `⟨d, q, e⟩` of delimiter / quotation / escape strings (for CSV). A
  **dialect** per RFC 4180 needs a delimiter; quote and escape may be absent
  (`ε`) and may be multi-character.
- `parse(line, instr)` applied to a line yields an **interpretation**: a vector
  of field strings (leading/trailing whitespace trimmed).
- The three sub-problems: (1) for each line find *all* valid parsing
  instructions; (2) choose the *correct* one per line (or mark the line as not
  in a table); (3) report each table's line range `[from, to]`.

## A.2 Stage 1 — read and pre-classify lines

Lines are read one at a time (streaming; the design is single-pass except for a
one-time scan to compute `width` = the max tab-expanded line length, which
becomes the fixed coordinate system for ASCII detection). Each line is
pre-classified into three types, after which only **content lines** flow to
parsing:

- **solid line** — only non-alphanumeric, non-whitespace characters
  (e.g. `==========`). Header underlines / table separators. Dropped.
- **helper line** — non-alphanumeric but *includes* whitespace
  (e.g. `==== ====== =====`). Visually indicates column widths but carries no
  data. Used as an alignment hint for ASCII boundary detection, but never
  emitted as a data row.
- **content line** — at least one alphanumeric character. The real candidates.

## A.3 Stage 2a — CSV dialect detection (the on-the-fly parser)

The naïve approach — collect every non-alphanumeric character in a line, form
all `⟨d, q, e⟩` triples, and run a CSV parser per triple — explodes. The thesis
worked example `"Willis, Bruce";05/19/1955;"Germany"` has five distinct
non-alphanumerics and yields 125 candidate dialects of which only **seven** are
valid; multi-character delimiters make it worse. ExtracTable avoids the blowup
with a two-step procedure.

**Step 1 — candidate delimiters.** Replace every run of alphanumerics (and any
character on a configurable **delimiter blacklist**, default
`< > ( ) [ ] { }`, so bracketed content isn't shredded) with a placeholder, and
split on it. What survives are the maximal runs of "potential delimiter"
characters. To allow **multi-character delimiters**, take *every contiguous
substring* of each run (so `", "` contributes `","`, `" "`, and `", "`).
Discard candidates longer than `P_mdl` (max delimiter length, default 4); sort
by `(length, value)`.

**Step 2 — discover quote and escape *while parsing*, via DFS.** Rather than
pre-enumerating quote/escape characters, a custom recursive-descent parser
parses the line with the bare dialect `⟨d, ε, ε⟩`, and **whenever the cursor
sits on a non-alphanumeric content character it speculatively branches**: it
spawns a second parser instance that treats the next up-to-`P_mdl` characters as
a *quotation* string, and (once inside a quoted field) a third that treats a
non-alphanumeric as an *escape* string. Quote is always introduced before
escape, because escape only occurs inside quoted fields. Each branch is a fresh
parse from the cached state, so work isn't repeated; already-started dialects
are tracked to avoid duplicates. A branch dies immediately on any RFC-4180
violation:

- escaping ordinary content,
- ending quotes missing or not followed by a delimiter / end-of-line,
- a trailing escape at end of line,
- a dialect that *declares* a quote/escape it never actually uses (this prunes
  spurious dialects).

Every branch that consumes the whole line without error is a **valid dialect**,
and each yields its own field vector. So one physical line emits many
`(dialect, fields)` interpretations. This is a depth-first search over the
quote/escape space layered on top of delimiter enumeration; it is what lets
ExtracTable reach exotic dialects cheaply.

## A.4 Stage 2b — ASCII fixed-width column boundaries (the distinctive part)

CSV column structure is visible in a single line; fixed-width ASCII structure
is **only** visible across many lines, as columns of whitespace that line up
vertically. ExtracTable detects these "vertical lines" with a streaming bitmap
algorithm that is direction-independent (works front-to-back or reversed).

**Bitmap.** Expand tabs (tab size 8) and pad each line to `width`. Map each
character position to a bit: `1` = whitespace (including the right padding past
end-of-line), `0` = a printing character.

**Vertical run counter.** Maintain one counter per column position. For each new
line, `counter[p] += 1` if that position is whitespace, else reset to `0`. So
`counter[p]` is the number of *consecutive* lines (ending at the current one)
with whitespace at column `p`. Blank lines are skipped so they don't break runs.

**Vertical lines, spacers, significance.** A position whose counter reaches
`P_mrc` (min row count, default 2) is a **vertical line**. Consecutive vertical
lines group into a **spacer** (a gutter between columns). A spacer is
**significant** (essential) iff it spans more than one position **and** is not a
leading or trailing margin (doesn't touch column `0` or `width-1`). Significant
spacers are the real inter-column gaps; single-column gaps and margins are
insignificant.

**Table lifecycle.** A candidate ASCII table **opens** when ≥1 vertical line has
spanned `P_mrc` lines; its start line is `k − P_mrc + 1`. As subsequent lines
arrive, each spacer is **narrowed to the intersection** of its positions with
the new line's whitespace — so gutters progressively tighten to the whitespace
that is consistently present across the whole window. After `essential_row_count`
(default 4) rows, non-trailing gutters **lock** as significant. The table
**closes** when a significant spacer loses its whitespace (text now bridges that
gutter) — that marks the table's end *regardless* of whether other gutters
continue. If gutters were lost mid-table, a **clone** of the table can be
spawned (same spacers minus the discontinued ones) starting at the prior line,
so a layout that loses a column partway is still captured.

**Spacers -> column boundaries.** On close, union all significant spacer
positions, subtract from `{0…width}`; the remaining positions form contiguous
content blocks; each block's `[min, max]` is a **column boundary** `[start,
end)`. (Insignificant spacers are dropped only if the table has fewer than
`P_msr` rows.) Splitting a raw line at those boundaries and trimming yields the
ASCII interpretation. An **eager-table** guard suppresses a newly opened table
that duplicates an existing one's spacer structure (same count, pairwise
intersecting), preventing redundant detections.

This bitmap-of-aligned-whitespace technique is the part of ExtracTable with no
analog in Pytheas (which is delimited-CSV only), and is the most reusable single
idea for handling fixed-width report dumps.

## A.5 Stage 3 — field pattern / data-type abstraction

Every field of every interpretation is abstracted to a **pattern**: a vector of
typed components.

1. If the field matches an **empty** equivalent (empty string, `null`,
   `unknown`, `n/a`, `nan`, or a run of `?`/`-`/`*`/`#`), it is a single
   `Empty` component. Empties are case-insensitive and are treated as the
   *neutral element* in scoring.
2. Else if the field **fully** matches one of a curated list of ~15 **known**
   data-type regexes (boolean, percentage, currency, free text, URL, date,
   time, domain, email, IPv4, file path, bracketed values, …), it collapses to
   a single `Known` component carrying the matching pattern's index. Known
   patterns are matched in list order and short-circuit the field. The catalog
   was built from three sources: data-driven mining of RFC-4180-clean Mendeley
   files, a curated regex library (RegexBuddy), and hand-tuning against the
   working data.
3. Otherwise the value is walked left-to-right, greedily emitting the longest
   `Number` / `String` / `Other` component at each step:
   - **String** = a maximal run of `[A-Za-z]` (case-insensitive; **spaces are
     not part of a string** — they fall to `Other`).
   - **Number** = a fairly elaborate regex covering signed/unsigned integers,
     floats, thousands separators in both `,` and `.` conventions, and
     scientific notation; a leading run of zeros is treated as *not* part of the
     following number.
   - **Other** = the inverse: any maximal run of non-alphanumeric characters.
   Adjacent `Other` runs are coalesced.

A `Number` component records whether it is integer vs float; an `Other`
component records its literal punctuation; a `String` records its length. The
column-consistency score (next) operates on these patterns.

!!! warning "Implementation caveat worth knowing"
    In the reference code the float-precision detector has a latent bug
    (it counts the *literal string* `'point_candidate'` instead of the decimal
    character), so a `Number`'s float-vs-integer flag is effectively always
    "integer." This silently weakens the numeric-uniformity signal and the
    "headers contain no float" check. An implementer re-building the technique
    should fix this; the *intended* design distinguishes int vs float.

## A.6 Stage 4 — the consistency score

This is the closed-form heart of ExtracTable. Given a table `C` (one chosen
interpretation per row), the score is a number in `[0, 1]` (or `-1` if
un-scorable):

$$
\text{score}(C) = \begin{cases}
-1 & \text{if } |rich| = 0 \\
0 & \text{if } |cons| < \lfloor \log_2 |rich| \rfloor \\
\dfrac{|cons|}{|rich|} \cdot \dfrac{1}{|rich|}\displaystyle\sum_{COL \in rich} u(COL) & \text{otherwise}
\end{cases}
$$

The pieces:

- **`rich`** = the non-sparse columns: columns with more than one non-empty
  field. All-empty and single-value columns are excluded so sparse junk doesn't
  dominate. If no column is rich, the table is un-scorable (`-1`).
- **Column pattern homogeneity** (credited to **Guo et al.**): for a column,
  drop empties, then compute the **Herfindahl/Simpson index** — the sum over
  distinct patterns of the squared proportion of that pattern:
  $\text{homo}(COL) = \sum_{PAT} \big(\tfrac{\#\{cells = PAT\}}{\#\{non\text{-}empty\}}\big)^2$.
  This is `1.0` when every value shares one pattern and falls toward `0` as the
  column mixes patterns.
- **`cons`** = the **consistent** columns: rich columns whose homogeneity
  exceeds a threshold (`P_mbs`, min block score, when testing two adjacent rows;
  `P_mcs`, min column score, when scoring a whole table — the two-row test is
  lenient, the whole-table test stricter). A separate **multi-space penalty**
  also excludes columns where too many fields contain a run of ≥2 internal
  spaces (a tell-tale of a *wrong* fixed-width split).
- **The `log₂` gate**: the table scores `0` unless at least
  $\lfloor \log_2 |rich| \rfloor$ columns are consistent (always ≥2 for
  reasonably wide tables, by a separate `max(2, …)` floor in the two-row path).
  This demands the *number* of clean columns scale with table width — one clean
  column in a ten-column table isn't a table.
- **Value uniformity `u(COL)`** — a three-level hierarchy used as the
  tie-breaking refinement among already-consistent tables:
  - *per component*: `Number` -> homogeneity over int-vs-float; `Other` ->
    homogeneity over the literal value; `Known` and `String` -> `1.0` (assumed
    uniform); `Empty` -> undefined/ignored.
  - *per pattern* (pattern uniformity): the **minimum** component uniformity
    across the pattern's components (the weakest link).
  - *per column*: group the column's fields by pattern, weight each pattern's
    uniformity by its share of the column, and take the **maximum** over
    groups.

Final score = (fraction of rich columns that are consistent) × (mean value
uniformity over rich columns). Worked thesis example: a 3×4 table where one
column is sparse -> `rich` = 3 columns, all consistent, uniformities
`0.5, 1.0, 1.0` -> `score = (3/3) · (0.5+1.0+1.0)/3 = 0.83`.

The two-row variant of this score also folds in a **dialect homogeneity** term
(Herfindahl over the row interpretations' `type`/dialect), penalizing a table
that mixes CSV and ASCII rows or mixes dialects.

## A.7 Stage 5 — building table candidates (binning, fallback, headers, compaction)

Lines stream into **bins** keyed by `(column count, parsing instruction)` — so
each distinct *(width + dialect/layout)* grows its own candidate table. A
candidate **opens** on a newly seen bin key and **closes** when its key is
absent on a line (this is how table boundaries and multiple-tables-per-file fall
out: a line that no longer parses to a table's width+dialect ends that table).

Several refinements:

- **Compatibility, not identity, row-to-row.** Before appending a line to a
  growing block, the line's best interpretation is tested against the block's
  last row via the two-row consistency score; it joins only if the score clears
  `P_mbc` (min block compatibility, default 0.71). The thesis observes data-type
  compatibility is effectively *transitive* down a clean column, so a
  row-to-row test suffices.
- **Fallback dialects.** A line whose exact bin key is missing can still join a
  **compatible** dialect's table (same column count; `⟨d,q,e⟩` can fall back to
  `⟨d,q,ε⟩` and `⟨d,ε,ε⟩`). This handles a CSV table where only *some* rows
  needed quoting — the unquoted and quoted rows still belong together. A fresh
  candidate is also started under the original key, and both compete later.
- **Header handling.** A table keeps at most a header block + a data block. When
  an incoming line is incompatible with the data block, the block closes and a
  candidate fires — and if the previous block looks like a plausible header (≤
  `P_mhr` rows, default 4, and contains no float), the candidate is emitted
  **both** with and without that header so the optimizer chooses.
- **Combinatorial interpretation selection + compaction.** Because each row
  carries many interpretations, materializing a concrete table means picking one
  interpretation per row — the Cartesian product is `k^m` (a 30-row table with 2
  interpretations each = `2³⁰ ≈ 10⁹`). ExtracTable bounds this with a
  **compacting strategy**: whenever the running product of per-row interpretation
  counts would exceed `P_csl` (compact size limit, default 100), it scores all
  current combinations, records on each interpretation the *best* table
  consistency it ever participated in, and **prunes each row to its top-N
  interpretations**, ranked by `(best consistency, count of cleanly-typed
  single-component fields, mean 1/#components)`. The block's final close runs one
  more pass with all caps = 1, collapsing each row to its single best
  interpretation. This is "decide as locally as necessary, as globally as
  possible." (The fully general *different-delimiter-per-row* variant exists but
  is **off by default** — it was correct on synthetic data but too slow and
  unrepresented in the ground truth.)

## A.8 Stage 6 — selecting the final tables (shortest path)

The surviving candidates may overlap in line range; the final step picks a
**non-overlapping** subset maximizing total table quality. This maps to a
**shortest-path problem** over a DAG:

- **Vertices** = line indices `0…m`.
- **Edge** per candidate table from its first to its last+1 line, with weight
  $$
  \text{dist}(tc) = -\,\text{score}(data)\cdot(\text{data rows})^2 \;-\; \text{score}(header)\cdot\big((\text{header+data rows})^2 - (\text{data rows})^2\big)\;-\;0.0001\cdot\text{sgn}(h)
  $$
  Weights are **negative**, and **quadratic in row count** — that is what makes
  the method prefer *longer and more consistent* tables (a long clean table
  beats two short ones covering the same span). The tiny last term favors having
  a header on otherwise-equal candidates.
- **Connectivity.** Every adjacent vertex pair is also joined by a weight-0 edge,
  so the gaps between tables (preamble, footnotes, blank lines) are free to skip.
  The graph is dense but acyclic (no edge goes backward).
- **Solve.** **Bellman–Ford** (chosen because weights are negative) from line 0
  to the last line returns the min-total-weight path = the optimal table set.
  Among edges of equal weight (equally consistent, same dimensions), ties break
  by: higher ratio of recognized fields, more rich columns, shorter pattern
  lengths, fewer columns.
- **Heuristic alternative.** Because optimal selection can take minutes on
  candidate-heavy files, a greedy mode sorts all candidates by `(weight,
  tie-breakers)`, repeatedly takes the best and removes every candidate
  overlapping it. Faster, near-optimal, off by default.

Selected tables are written out as clean RFC-4180 CSV.

## A.9 ExtracTable parameters (reference defaults)

| Parameter | Default | Meaning |
|---|---|---|
| `max_character_delimiter_length` (`P_mdl`) | 4 | longest multi-char delimiter / quote / escape considered |
| `max_quotation_escape_length` | 2 | longest quote/escape token the parser speculates |
| `delimiter_character_blacklist` | `< > ( ) [ ] { }` | never treated as delimiters |
| `min_row_count` (`P_mrc`) | 2 | min rows a vertical line must span (ASCII); min data rows |
| `essential_row_count` | 4 | window length after which ASCII gutters lock |
| `min_col_count` (`P_mcc`) | 2 | min columns for a valid table |
| `min_significant_rows` (`P_msr`) | 5 | below this, insignificant ASCII spacers are dropped |
| `min_block_compatibility` (`P_mbc`) | 0.71 | min two-row consistency to extend a block |
| `block_min_column_consistency` (`P_mbs`) | 0.31 | column-homogeneity threshold inside block tests |
| `final_min_column_consistency` (`P_mcs`) | 0.51 | column-homogeneity threshold for the final score |
| `min_table_consistency` (`P_mts`) | 0.51 | drop candidates scoring below this in selection |
| `max_header_rows` (`P_mhr`) | 4 | max header rows in a candidate |
| `compact_size` (`P_csl`) | 100 | max Cartesian-product size before pruning interpretations |
| `enable_ascii` | true | include fixed-width detection |
| `use_heuristic` | false | greedy vs. optimal (Bellman–Ford) selection |

---

# Part B: Pytheas

Pytheas discovers tables in CSV files by **classifying every line** as data or
not-data (and, within that, header / context / subheader / footnote), using a
**fuzzy rule-based model** whose rule weights are *learned* from annotated
files. Reported accuracy is high (>95% table-discovery precision/recall, ~95.6%
per-file line-classification accuracy) and it generalizes across countries'
open-data portals. It also emits a **confidence measure** for each discovered
table.

The philosophy is the inverse of ExtracTable's: instead of enumerating
interpretations and optimizing a closed-form score, Pytheas fixes one parse and
brings a large catalog of weak, human-readable signals to bear, fusing them
probabilistically.

## B.1 Pipeline overview

1. **Parse/sample** — detect encoding and delimiter, read lines, tokenize into a
   rectangular table.
2. **Signatures** — precompute, for every cell, its symbol abstraction and
   derived features (one pass, vectorized).
3. **Fire all rules** over the whole table, producing per-cell and per-line rule
   hits for both the *data* and *not-data* hypotheses.
4. **Discover tables iteratively** — repeatedly find the next table from a file
   offset: classify lines, locate the first data line, detect the header above
   it, find subheaders/aggregations, find the last data line, package the table
   with a confidence, then advance past it. Adjacent tables separated only by
   blanks/headers are merged.

## B.2 Parsing and pre-processing

- **Encoding** via `cchardet` on a 50 KB sample, with sensible remaps
  (e.g. ascii->utf-8, iso-8859-1->cp1252).
- **File rejection**: files whose first non-blank line looks like HTML/XML/JSON
  are discarded (not CSV).
- **Delimiter discovery**: from the ranked candidate set `, ; \t | : # ^`, sample
  lines per delimiter and find the first run of ≥3 consecutive lines with the
  same (>1) field count; pick the delimiter with the highest consistent field
  count, tie-broken by earliest consistency then rank order.
- **Tokenize** each line with a standard quoted-CSV reader; **rectangularize**
  by grouping consecutive equal-width rows, padding to a common width,
  normalizing empty strings to null, and transliterating non-ASCII.
- **Context window**: a cell's "context" (the column evidence it is judged
  against) is the slice of values **below it** in the same column — a forward
  window of up to `max_summary_strength = 6` non-empty values, bounded by
  `max_line_depth = 30`. Only the first ~20 columns are processed. This
  downward, bounded window is important: line classification is causal (a line
  is judged against what follows), which is what lets the same machinery run
  top-down when searching for the *end* of a table.

## B.3 The symbol-sequence abstraction and column "patterns"

**Cell -> train.** Scan the value left-to-right into run-length pairs over `D`
(digit), `A` (letter), `S` (space), and literal punctuation. A reversed train
(`bw_train`) supports right-anchored / suffix patterns. Derived per cell:
symbol *chain* (the symbols without counts), symbol *set*, *case*
(`ALL_CAPS`/`ALL_LOW`/`TITLE`/`MIX`), character length, tokens, numeric flag.

**Number normalization.** If a value's symbol set is a subset of
`{D . , space - + ~ > < ( )}` and contains `D` (≤ one leading sign), the whole
train collapses to a single `[[D, total-digit-count]]`. This makes `1,234.56`,
`1 234`, and `-99` all look like one digit run, so differently-formatted numbers
still "agree" in a column.

**Pattern summary over a context (the key structure).** Given the trains of the
non-empty cells in a context, compute the **forward summary** = the longest
position-wise common symbol prefix:

- start from the first train; for each subsequent train, walk positions;
- if the symbol matches but the count differs, set that position's count to `0`
  ("same symbol, variable length");
- if the symbol disagrees, truncate the summary there and mark the chain
  inconsistent.

So a column summary `[[D,0]]` means "all digit runs of varying length";
`[[D,4]]` means "all exactly four digits"; `[[A,0],[S,1],[D,4]]` means "letters,
one space, four digits." A **backward summary** runs the same algorithm on
reversed trains; companion summaries cover the shared symbol set, shared case,
and `{min,max}` character length. These summaries are what the rules test.

## B.4 The four rule families and how a rule fires

Rules are organized into four catalogs, each a small CSV of
`(group, index, NAME, English description)` — they are meant to be read by
humans and audited. (The catalogs ship in the repo and render to an HTML rule
book.)

- **Data-Line rules** (whole row supports *data*): e.g. the row contains exactly
  one null-equivalent; contains a value naming a datatype; the first value
  carries an aggregation keyword.
- **Not-Data-Line rules** (whole row supports *not-data*): e.g. only the first
  cell is filled (`UP_TO_FIRST_COLUMN_COMPLETE_CONSISTENTLY`); the row is
  all-caps or all-snake/camel-case; cells form an arithmetic sequence (a
  give-away of a header like `2011 2012 2013`); range pairs; metadata-guide
  headers (`field name`, `description`, `data type`, …); footnote markers.
- **Data-Column / Data-Cell rules** (a cell *fits* the column pattern below it):
  e.g. `CONSISTENT_NUMERIC` (the forward summary is a single `D` run),
  `FW_D4` (summary is exactly four digits), `BROAD_NUMERIC`, consistent case,
  consistent character length, `VALUE_REPEATS_*_BELOW`.
- **Not-Data-Column / Not-Data-Cell rules** (a cell *breaks* the column below
  it): e.g. its first symbol disagrees with the column's summary; its symbol
  chain differs from an otherwise-consistent column; its case disagrees; its
  length is a wild outlier (`< 10%`/`< 30%` of min, or `> 150%…190%` of max).
  These are exactly the signals that a *header* sits atop a numeric column.

A rule "fires" as a boolean predicate over the precomputed summaries. Column
rules carry guards: a cell-data rule needs ≥2 patterns with ≥2 non-empty values
in its window (a value can't be "consistent with itself"); not-data column rules
are suppressed inside a cleanly all-numeric column (to avoid false header
signals).

## B.5 The fuzzy aggregation (how votes become a line probability)

This is the mathematical core, and it is **not** a weighted average or a sum. For
each line, separately for the *data* and *not-data* hypotheses:

1. **Per cell, take the single strongest above-threshold rule.** Among the rules
   that fired for a cell, keep the maximum *learned weight*, ignoring weights
   below a floor (`0.4` for data, `0.6` for not-data). Within a cell, evidence
   does **not** accumulate — only the best rule counts. (0 if none.)
2. **Population-weight it.** Multiply by
   $1 - (1-p)^{2\cdot\text{strength}}$ with `p = 0.3`, where `strength` is the
   number of non-empty values backing the cell's context (capped at 6). A
   pattern supported by one value counts ~0.51; by six values, ~0.9998. Evidence
   from larger contexts counts more.
3. **Pool cell votes with line votes.** Each column contributes one
   (population-weighted) cell score; whole-row line rules contribute their
   learned weights (above the same floors). This single pool is the line's
   evidence list for that hypothesis.
4. **Fuse with a noisy-OR (probabilistic sum).** The line's confidence for the
   hypothesis is
   $1 - \prod_s (1 - s)$
   over the evidence scores `s` — i.e. "1 − probability that *every* signal is
   simultaneously wrong." Data and not-data each get their own noisy-OR.

**Decision.** A line is **data** iff its data-confidence exceeds its
not-data-confidence. The method also records the winning confidence, the
difference, and a *confusion index* `(winner − loser)/winner`, all of which feed
the confidence measure. **Null/aggregate imputation**: a null or
weakly-supported cell borrows the agreement/strength of the cell directly above
when the line above was data, so a column of numbers with occasional blanks
doesn't lose its vote.

Two design notes worth adopting: (a) "**max within a cell, noisy-OR across the
line**" deliberately prevents a single noisy column from either dominating or
canceling out the row; (b) the two hypotheses are scored independently and only
compared at the end, so "looks like data" and "looks like a header" can both be
weak (an ambiguous line) and the confidence measure will say so.

## B.6 Training (where the weights come from)

Weights are learned, but training is **non-iterative** — each rule's weight is
its empirical signed precision on labeled data:

$$
\text{weight} = \frac{TP}{PP} - \frac{FP}{PP}
$$

where `PP` = times the rule fired, `TP` = fired on a correctly-classed
line/cell, `FP` = fired on the wrong class. (`confidence = TP/PP` = precision;
`coverage = PP/total`.) A rule that is usually right gets a weight near 1; a
misleading rule can go **negative** (e.g. "one alpha token repeats once below"
learned ≈ −0.65) and thus *argues against* its hypothesis. To counter the data
rows vastly outnumbering header/metadata rows, training **undersamples** — only
the first couple of data lines (and their cells) of the first table are used as
positive data examples. Output is a JSON of per-rule `{weight, confidence,
coverage}` consumed at inference. Because weights are just per-rule statistics,
the rule book stays fully interpretable: you can read why a line was classified
the way it was.

## B.7 Table-boundary discovery

For each table, starting at a file offset:

1. **Classify lines** (§B.5) into the per-line data/not-data confidences for a
   window of up to 100 lines.
2. **First Data Line (FDL).** Find the first run of ≥2 lines labeled data. Then
   — and this is the elegant part — enumerate every possible position of the
   *not-data -> data* transition as a candidate boundary sequence, score each by
   summing signed per-line confidences (positive where the line's label matches
   the candidate's expectation, negative where it conflicts), **softmax** over
   candidates (optionally times a Markov prior), and take the highest-probability
   boundary. This "fuzzy region matching" tolerates a noisy line or two around
   the boundary instead of trusting a single line.
3. **Header** (§B.8) is detected upward from the FDL.
4. **Subheaders & aggregation scope.** Rows whose only filled cell is the first
   column (or that carry an aggregation phrase like `total`, `average`, `CAGR`)
   are tied to the data rows they summarize.
5. **Last Data Line (LDL).** Walk **downward** from the FDL, growing the table
   and re-scoring each line with the same machinery (now top-down). The first few
   rows are accepted on faith; footnote heuristics (first value starts with `*`,
   `source`, `note`, an enumerated marker like `1.`/`a)`/`(1)`, or contains `=`)
   stop the table; an ambiguous line gets one "probation" step in case data
   resumes. The last accepted data line is the bottom boundary.
6. **Package** the table (top boundary, data start/end, header, subheaders,
   columns, confidence) and **advance** the offset past it. The loop finds the
   next table; tables separated only by blank/header lines are **merged** (this
   un-splits a table that was broken at a subheader). Trailing non-blank lines
   become footnotes; lines between the top boundary and data that aren't header
   become context.

## B.8 Header detection (multi-row)

Headers above the FDL are detected by iterating **upward** and, at each step,
forming the *combination* of the candidate header rows down to just above the
data, then testing whether that combined row is a plausible *complete* header —
acceptance depends on how many cells are filled relative to header width (wide
headers tolerate empty index/edge columns but require the interior filled).
**Multi-row headers** are handled by detecting spanning (merged) cells — a cell
empty in the row above is treated as spanned from a neighbor — and stitching
per-column header fragments into one name per column. The walk stops once a
non-duplicate header is achieved (caps at ~4–6 accumulated lines). A trailing
"header" row filled only in column 0 is reclassified as a subheader.

## B.9 The confidence measure

Three nested confidences, all built from windowed agreement of the per-line
classification around a boundary:

- **FDL confidence** — over ≤4 lines above and ≤4 below the first data line,
  average the signed per-line confidences (positive where the label matches the
  expected side of the boundary), combining the confusion-index / confidence /
  difference signals; clamp at 0.
- **LDL confidence** — the analogous windowed agreement around the bottom
  boundary.
- **Table confidence** = `min(FDL confidence, LDL confidence)` — a table is only
  as trustworthy as its least-certain boundary. The paper shows this confidence
  reliably flags the files most likely to be mis-extracted, which is valuable for
  triaging a large corpus.

## B.10 Pytheas constants and curated lists (reference)

- **Context / window**: `max_summary_strength = 6`, `max_line_depth = 30`,
  `max_attributes ≈ 20`, FDL run length `= 2`, confidence window `= 4`.
- **Fuzzy model**: population factor `p = 0.3` (factor `1-(1-p)^{2·strength}`),
  data weight floor `0.4`, not-data weight floor `0.6`, undersample limit `2`.
- **Null-equivalents** (curated, bilingual EN/FR): `nil, na, n/a, n.a., #n/a,
  nd, no data, not available, not applicable, sans objet, -, --, ., .., ...,
  *, void, 0, redacted, confidential, unknown, ?` (plus `'' null nan none`).
- **Footnote keywords**: `* remarque source note nota footnote`.
- **Aggregation tokens**: `total subtotal totaux all less than / more than /
  over / under / higher than / lower than / older than / younger than` and
  French equivalents.
- **Aggregation functions** (phrase -> operation): `total -> sum`, `average -> mean`,
  `change in -> sum`, `variation -> difference`, `cagr -> CAGR`.
- **Metadata-header keywords**: `field name, field, description, example, data
  type`. **Datatype keywords**: `integer, string, text, numeric, boolean`.

---

# Part C: synthesis for an implementer

## The two methods side by side

| Dimension | ExtracTable | Pytheas |
|---|---|---|
| Core stance | enumerate interpretations, score, optimize | fix one parse, classify lines by voting |
| Cell abstraction | typed components `N/S/O/K/E` | symbol-train `D/A/S/punct` + summaries |
| Consistency | closed-form Herfindahl homogeneity + value uniformity | learned-weight rules fused by noisy-OR |
| Tuning | hand-set thresholds | weights *learned* (signed precision) |
| Delimited CSV | yes, incl. exotic multi-char dialects | yes (common delimiters) |
| **Fixed-width ASCII** | **yes — bitmap/spacer detection** | no |
| **Multiple tables/file** | yes (bin keys + shortest-path) | yes (iterative discovery + merge) |
| Header rows | yes, ≤4, header-vs-no-header both scored | yes, incl. multi-row spanning headers |
| Subheaders / aggregation rows | not modeled | yes (scope tied to data rows) |
| Footnotes / preamble | skipped via free gap edges | classified (footnote/context labels) |
| Multi-interpretation per line | yes (the whole point) — pruned by compaction | no (one parse) |
| Global decision | Bellman–Ford shortest path over line DAG | softmax over boundary sequences + min-confidence |
| **Confidence output** | not really (score is internal) | **yes, calibrated, per boundary** |
| Interpretability | scores are mechanical | **rule book is human-readable + auditable** |
| Cost driver | combinatorial blow-up (mitigated by compaction) | rule evaluation over cells (linear-ish) |

## What to borrow from each

If an implementer wants one robust plain-text table extractor, the strongest
design grafts the two:

1. **Front end from ExtracTable.** Its **multi-interpretation parsing** is the
   right way to handle undocumented dialects, and its **bitmap/spacer ASCII
   detector** is the only technique here for fixed-width report dumps — Pytheas
   simply can't read those. Keep the **on-the-fly DFS dialect parser** (cheap
   exotic-dialect discovery) and the **compaction strategy** (bound the
   combinatorial product).
2. **Cell/column coherence: pick one abstraction, keep both tricks.**
   ExtracTable's typed components and Pytheas's symbol-trains are two encodings
   of the same idea. The Pytheas **forward/backward summary** is more expressive
   (it captures fixed-length vs. variable-length and prefix/suffix structure);
   the ExtracTable **Herfindahl homogeneity** is a clean scalar. Use trains +
   summaries for evidence, homogeneity for a quick scalar gate. Keep
   **number-normalization** and **null-as-neutral** from both.
3. **Decision layer from Pytheas.** The **fuzzy noisy-OR voting** with
   **learned, signed, interpretable rule weights** is more robust and more
   auditable than hand-tuned thresholds, degrades gracefully on ambiguous lines,
   and — crucially — produces a **confidence measure**. For a tool diffing
   thousands of files unattended, that confidence is what lets you auto-trust the
   clean cases and flag the rest. Pytheas's **header/subheader/footnote
   labeling** is also richer than ExtracTable's header-or-not.
4. **Global selection.** When you *do* have competing whole-table candidates
   (multiple dialects, ASCII vs CSV), ExtracTable's **shortest-path over a line
   DAG with row-count-quadratic weights** is the clean way to pick a
   non-overlapping cover and prefer long, consistent tables. Pytheas's
   **iterative-discover-and-merge** is simpler when there's a single fixed parse.

## Relevance to binoc

A binoc tabular expand rule that wanted to diff messy real-world CSVs
*as tables* would need exactly this front-half: recover parsing instructions,
find table ranges, separate stacked tables, strip preamble/footnotes, recover
headers. The natural fit is a parse rule that turns a `text/plain` or
mis-typed-`text/csv` node into one or more `tabular_v1` artifacts (each
discovered table), carrying a **confidence** that a renderer can map to a
significance/uncertainty tag — Pytheas's per-boundary confidence is the model
for that. The fixed-width ASCII detector is the piece most likely to surprise:
government data dumps are full of aligned-column text reports that no CSV parser
will touch, and ExtracTable's bitmap-of-whitespace approach reads them. None of
this is committed; it is the technique map for if/when a robust tabular parser
gets built. Decisions belong in an [ADR](../../adr/README.md), which can cite
this survey.

## References

- Leonardo Hübscher. *ExtracTable: Extracting Tables From Plain Text.* MSc
  thesis, Hasso Plattner Institute / University of Potsdam, May 2021.
  (`HPI-Information-Systems/ExtracTable`.)
- Christina Christodoulakis, Eric B. Munson, Moshe Gabel, Angela Demke Brown,
  Renée J. Miller. *Pytheas: Pattern-based Table Discovery in CSV Files.* PVLDB
  13(11):2075–2089, 2020. [DOI 10.14778/3407790.3407810](https://doi.org/10.14778/3407790.3407810).
  (`cchristodoulaki/Pytheas`.)
- The column **homogeneity** measure ExtracTable scores with is credited in the
  thesis to **Guo et al.**; ExtracTable contrasts its pattern-based dialect
  detection with **Burg et al.** ("Wrangling messy CSV files" / CleverCSV).
  Consult the thesis bibliography and the Pytheas paper's related-work section
  for the full citations.
