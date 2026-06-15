# Finding: directory-of-CSVs → SQLite-database (the "container-reshape" case)

**Date:** 2026-06-13
**Status:** Diagnostic prototype — not a shipped feature
**Branch under test:** `pre-refactor` (correspondence-first engine), tip `14eda03`
**Reproduction:** `model-plugins/binoc-sqlite/tests/csv_dir_to_sqlite_prototype.rs`

> **Editor's note (post-merge, 2026-06-13):** captured at `pre-refactor` tip
> `14eda03`, *before* CFM-62 landed. CFM-62 has since deleted the
> `ParseDescriptor.requires_link` field — parse link-gating is now **derived**
> from pair `reads` — which resolves the §3.6 ordering deadlock (a SQLite table
> linker simply declares its `reads`). The §1 and §3.6 references to
> `SqliteParseRule.requires_link: true` describe the pre-CFM-62 code; the rest of
> the analysis stands. Scheduled as CFM-63 in CORRESPONDENCE_ENGINE_TRACKER.md.

## The question

When snapshot A is a **directory of CSV files** (`customers.csv`, `orders.csv` at
depth 1) and snapshot B is a **single `data.sqlite`** holding the corresponding
`customers` and `orders` tables, does binoc's projection read honestly, or does
it produce mush?

**Verdict: it produces mush, and the mush is not (only) a missing-link problem.**
The current baseline renders "2 CSVs removed, 1 sqlite file added." More
importantly, the structural representation of a SQLite database is *asymmetric*
with the representation of a directory, and that asymmetry is what defeats an
honest projection — table-level content linking, even once it exists, does not
fix it on its own.

---

## 1. How binoc-sqlite currently represents a database

**Collection-artifact, NOT a subtree of child nodes.**

A SQLite database is a **single engine node** (the `data.sqlite` file) that
carries a `tabular_collection_v1` *artifact*. The tables are *data members*
inside that artifact, never nodes in the correspondence tree.

Evidence:

- `model-plugins/binoc-sqlite/src/sqlite.rs:13-36` — `SqliteParseRule` is a
  `ParseRule` whose `input` matches a non-dir file with extension
  `.sqlite/.sqlite3/.db`, whose `output` is `tabular_collection_v1()`, and which
  has `requires_link: true`. Its `parse()` opens the DB, reads the schema, and
  serializes `collection_from_schema(...)` into a single artifact blob.
- `model-plugins/binoc-sqlite/src/sqlite.rs:202-225` — `collection_from_schema`
  builds a `TabularCollectionData { tables: Vec<TableMember> }`. Each
  `TableMember` has a `node_path` like `data.sqlite::customers`, but that is a
  *string field inside the artifact* — it is never registered as an `ItemRef`
  / engine node. There is **no `SqliteExpand` rule**; the plugin registers only
  a parse rule and a writer (`model-plugins/binoc-sqlite/src/lib.rs:9-14`).
- `model-plugins/binoc-sqlite/src/sqlite.rs:38-65` — `SqliteCollectionWriter`
  consumes the two linked nodes' `tabular_collection_v1` artifacts and emits
  table-level edits via `tabular_collection_name_edits(...)`, **keyed on table
  `logical_name`** (`binoc-sdk/src/correspondence.rs:437-500`,
  `member_map` keys on `member.logical_name`).

Contrast with the directory side. A directory is expanded by
`DirectoryExpand` (`binoc-stdlib/src/correspondence/expand.rs:77-95`) into real
**child nodes**, one per file, each `customers.csv` / `orders.csv` getting its
own `tabular_v1` artifact via `CsvParse`
(`binoc-stdlib/src/correspondence/parse.rs:13-35`). A directory of CSVs
therefore has the shape:

```
""  (directory node)
├── customers.csv   (tabular node, tabular_v1 artifact)
└── orders.csv      (tabular node, tabular_v1 artifact)
```

while the sqlite side has the shape:

```
""  (directory node)
└── data.sqlite     (file node, tabular_collection_v1 artifact whose
                     members {customers, orders} are DATA, not nodes)
```

The two snapshots **disagree about what node is "the collection" and at what
depth its members live**: on the left the collection is the root directory and
members live at depth 1; on the right the collection is `data.sqlite` at depth 1
and members live "inside" it as artifact data (a notional depth 2 that the
engine never materializes as nodes).

Note: a *stacked-CSV* file is the one case where stdlib materializes table
children as real nodes (`binoc-stdlib/src/correspondence/parse.rs:213-268`,
`CsvStackedTablesParse` emits `ParsedChild` nodes with `tabular_v1` artifacts).
SQLite deliberately does **not** do this — it stays on the collection-artifact
representation.

---

## 2. Captured CURRENT output and what's wrong with it

### 2a. Baseline (no forced links) — `csv_dir_to_sqlite_baseline`

Left = dir `{customers.csv, orders.csv}`; right = `data.sqlite` with
`customers` (Alice/Bob/**Carol added**) and `orders` (order 101 total
**17→99**).

Markdown:

```
# Changelog: snapshot-a (dir of CSVs) → snapshot-b (data.sqlite)

Claims: none

- **(root)**: 0 edits
  - Sources
    -  (from, modify, binoc.pair.root)
- **customers.csv**: Removed
- **orders.csv**: Removed
- **data.sqlite**: Added
```

JSON (abridged): root `modify` directory with three children — `customers.csv`
`remove`, `orders.csv` `remove`, `data.sqlite` `add` (item_type `"file"`), each
tagged `binoc.content-changed`.

**What's wrong:**

- The reshape reads as **total replacement**: every CSV "removed", an opaque
  binary "added." Zero table-level signal. A reader learns nothing about
  customers/orders continuity, the added Carol row, or the 17→99 cell change.
- `data.sqlite` is rendered as item_type **`"file"`** — the
  `tabular_collection_v1` artifact is never even produced, because
  `SqliteParseRule.requires_link: true` and `data.sqlite` is never linked.
- The root says **"0 edits"** despite a wholesale dataset reshape.

### 2b. Forced container link — `csv_dir_to_sqlite_forced_container_link`

Here I inject a `DeclaredPair` linking the left **root directory** (`""`) to the
right **`data.sqlite`** file — i.e. I hand the engine exactly the
is_dir-crossing container link that `ContainerFromChildEvidence` would
eventually vote for if table children existed. Result:

```
- **(root)**: 0 edits
- **data.sqlite**: Moved from
  - Sources
    -  (from, move, binoc.pair.declared)
  - tags: binoc.declared-correspondence, binoc.folder-move, binoc.move
- **customers.csv**: Removed
- **orders.csv**: Removed
```

**What's wrong (this is the important one):**

- Even with the container link resolved, the projection narrates a bare
  **"data.sqlite: Moved from "** (a folder rename), tagged `binoc.folder-move`.
  The directory→sqlite reshape is mis-described as a *move*, not a
  container-type change.
- The sqlite tables **still never surface.** `SqliteCollectionWriter`
  (`sqlite.rs:48-64`) requires BOTH linked nodes to carry a
  `tabular_collection_v1` artifact; the left node is a *directory*, which has no
  such artifact, so `load_collection` returns `None` and the writer bails. No
  table edits, no Carol row, no 17→99 change.
- The CSVs remain dangling `Removed` siblings *underneath the same root* even
  though their content now lives in the thing the root is linked to — a
  many-to-many tangle the projection does not reconcile.

---

## 3. The "hard 70%": projection problems that survive even if table linking is solved

Assume the separately-built content-based tabular pair rule lands and somehow
links `customers.csv` ↔ the `customers` table and `orders.csv` ↔ the `orders`
table. The following problems are **independent of that linker** and remain:

1. **The sqlite table has no node to be the link endpoint.** A pair rule links
   two `NodeId`s. The `customers` table is not a node — it is a `TableMember`
   inside `data.sqlite`'s collection artifact. There is literally nothing on the
   sqlite side for the CSV `customers.csv` node to link *to*. So "table-level
   cross-format linking" cannot even be expressed in the current node model
   without first promoting tables to nodes (see §4). This is a representation
   gap, not a linker gap.

2. **Depth/parent mismatch breaks `NameUnderPairedParent` and
   `ContainerFromChildEvidence`.**
   - `NameUnderPairedParent` (`pair.rs:467-512`) only pairs children of
     already-linked parents and keys on `file_name` *including extension*. The
     CSV child is `customers.csv`; the (hypothetical) table node would be
     `data.sqlite::customers` — names never match, and their parents (`""`
     directory vs `data.sqlite` file) are at different depths and not yet
     linked. The rule cannot bootstrap.
   - `ContainerFromChildEvidence` (`pair.rs:669-777`) votes parents together
     from child links. The CSV children's parent is `""`; the table "children"
     parent would be `data.sqlite`. It *does not gate on `is_dir`*, so it
     *would* happily vote `"" ↔ data.sqlite` — but that produces exactly the
     mis-rendered "folder move" of §2b unless the projection layer learns to
     describe a dir↔file container as a reshape.

3. **Container-type change is rendered as a move/rename.**
   `move_hint_if_paths_differ` (`pair.rs:779-793`) tags any cross-path container
   link `binoc.folder-move` + `binoc.move` whenever either side `is_dir` or has
   children. A directory (`is_dir = true`) linked to `data.sqlite`
   (`is_dir = false`) is *not* a move — it is a container-type transformation
   ("this directory became a database"). There is no projection vocabulary for
   that today; the renderer only knows move/add/remove/modify.

4. **Many-to-many root tangle is unresolved.** Even with table links, the left
   root directory is the parent of the CSV nodes AND is the natural container
   counterpart of `data.sqlite`. The projection has to choose: is `""` linked to
   the right root, or to `data.sqlite`, or both? Currently nothing collapses the
   "CSVs removed" siblings into "their content moved into data.sqlite"; they
   stay as dangling removals next to the added/moved sqlite node (§2b).

5. **The two diff engines never meet.** CSV content diffs run through the
   *compaction* pipeline on `tabular_v1` edits
   (`binoc-stdlib/src/correspondence/compact.rs` — row alignment, column rename,
   cell edits). SQLite table diffs run through `SqliteCollectionWriter` /
   `tabular_collection_name_edits`, which only compares **shape** (row counts,
   column sets) keyed by name — it never compares cell values across formats.
   So even a perfectly-linked `customers.csv` ↔ `customers` table would get a
   row-count-only summary on the sqlite path and a rich cell-level diff on the
   CSV path, and there is no shared writer that produces an honest
   CSV-row ↔ table-row cell diff. The 17→99 change would be invisible or
   reduced to "row count unchanged."

6. **`requires_link` ordering deadlock.** `SqliteParseRule.requires_link: true`
   means the collection artifact is only produced *after* the sqlite node is
   linked. But the linker that would link it (a content-based tabular rule)
   needs the artifact to exist to compare content. Today the artifact and the
   link have a circular dependency that the engine's saturation loop does not
   obviously break for the cross-format case.

---

## 4. Recommendation

**Is the container-reshape work worth doing?** Yes in principle (container-type
migration — dir↔zip↔sqlite — is a real archival scenario), but it is **not a
clean follow-on** to the content-based tabular linker. It is a distinct,
larger body of work in the node model and the projection layer.

**Representation: SQLite should move to materializing tables as child NODES**
(the uniform-subtree representation), the way `CsvStackedTablesParse` already
does for stacked CSVs (`parse.rs:213-268`). Concretely: add a `SqliteExpand`
rule (or have the parse rule emit `ParsedChild` nodes) so that `data.sqlite`
expands into `data.sqlite::customers` / `data.sqlite::orders` nodes each
carrying a `tabular_v1` artifact. Benefits:

- Tables become real link endpoints, so the cross-format tabular linker can
  actually pair `customers.csv` ↔ `data.sqlite::customers`.
- `ContainerFromChildEvidence` can then legitimately vote `"" ↔ data.sqlite`
  from child evidence, and the shared `tabular_v1` compaction pipeline (row
  alignment, cell edits) handles the *same* artifact on both sides — the 17→99
  change renders identically whether the data came from CSV or SQLite.
- The asymmetry in §1 disappears: both snapshots become "a container with
  tabular children."

Keep the `tabular_collection_v1` *artifact* as a roll-up summary on the parent
node if useful, but it must not be the *only* representation of the tables.

**Projection changes still required even after the node move:**

- A container-type-change descriptor: when a linked pair has
  `left.is_dir != right.is_dir` (or crosses directory/file/archive/db
  categories), render "directory of CSVs became a SQLite database" instead of
  "moved." This means `move_hint_if_paths_differ` (`pair.rs:779-793`) and the
  stdlib projection annotator (`correspondence/mod.rs:146-163`) need a
  container-reshape branch and the Markdown renderer needs vocabulary for it.
- Many-to-many parent reconciliation: when child links cross a container
  boundary, the projection must hoist the children under the linked container
  and suppress the "removed" siblings, rather than leaving both.
- A `requires_link` ordering fix so the sqlite collection/table artifacts are
  available to the linker (e.g. parse-on-expand, or a two-phase
  parse-then-link-then-reparse).

---

## 5. Go / No-Go

**No-go as a clean follow-on to the content-based tabular linker. This is a
genuine swamp** — but a bounded and worthwhile one if scoped deliberately.

- The linker alone buys nothing here: there is no sqlite-table node to link to
  (§3.1), and even a forced container link renders as a mislabeled folder move
  with the tables still invisible (§2b). The "70%" that survives the linker is
  the representation change (tables→nodes), the container-type-change projection
  vocabulary, the many-to-many parent reconciliation, and the
  requires-link/reparse ordering — none of which the linker touches.
- Recommended sequencing: (1) land the tables-as-child-nodes representation for
  SQLite (mirrors stacked-CSV, low risk, immediately makes sqlite↔sqlite and
  sqlite↔csv-dir tractable); (2) add the container-type-change projection
  descriptor + many-to-many hoist; (3) only then lean on the cross-format
  tabular linker, which will now have real endpoints to work with.

Treat this finding as the justification for doing (1) and (2) as their own
tracked work, not as a rider on the tabular-linker PR.

---

## Appendix: running the reproduction

```bash
cargo test -p binoc-sqlite --test csv_dir_to_sqlite_prototype -- --nocapture
```

Two tests:
- `csv_dir_to_sqlite_baseline` — the honest current output (§2a).
- `csv_dir_to_sqlite_forced_container_link` — forces the dir↔sqlite container
  link to expose the move-mislabel + missing-tables behavior (§2b).

Both pass (they assert only that a root node exists; the value is the printed
JSON/Markdown). Neither asserts "correct" output — they are diagnostic capture.

> Note on `just check`: the `pre-refactor` branch tip has 3 pre-existing clippy
> `question_mark` / `filter_map_bool_then` warnings in
> `binoc-stdlib/src/correspondence/compact.rs` (commit `14eda03`, untouched by
> this prototype) that a newer clippy escalates to errors under `-D warnings`.
> `cargo clippy -p binoc-sqlite --tests` is clean for the code added here.
