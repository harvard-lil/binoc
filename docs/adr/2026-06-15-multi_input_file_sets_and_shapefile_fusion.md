# Multi-Input Claims: Grouping Sibling Files into One Logical Dataset

**Date:** 2026-06-15
**Status:** Implemented (CFM-83; supersedes the earlier registry/composite-node framing of this ADR)

## Context

Some datasets are not one file. A **file set** is a group of sibling files that
share a basename and together form one logical artifact. The motivating case is
the **shapefile**:

| Member | Required | Content |
|---|---|---|
| `.shp` | yes | feature geometry (points / polylines / polygons / multipatch) |
| `.shx` | yes | geometry offset index into `.shp` |
| `.dbf` | yes | per-feature attribute table (one record per geometry) |
| `.prj` | no | coordinate reference system, as WKT |
| `.cpg` | no | character encoding of the `.dbf` |
| `.sbn`/`.sbx` | no | ESRI spatial index |

Shapefiles are not niche: geospatial vector data is a large fraction of open
government data (Census TIGER alone is ~25% of data.gov datasets — see the
[data.gov inventory analysis](../core-developers/research/data-gov-inventory-analysis.md)).
And the pattern recurs well beyond shapefiles: GeoTIFF + world file
(`.tif`/`.tfw`), ENVI raster (`.hdr`/`.dat`), NIfTI (`.hdr`/`.img`), and others
are all "one dataset, several sibling files."

binoc already parses the `.dbf` attribute table on its own
(`model-plugins/binoc-dbf` → `tabular_v1`), and as of 2026-06-15 it parses the
`.shp` geometry on its own (`model-plugins/binoc-shapefile` → a
`structured_document_v1` summary tagged `format: "shapefile"`). Two gaps remain:

- **(A) Geometry parsing** — *solved* by the new `.shp` parser.
- **(B) Fusion** — *unsolved*. binoc sees `roads.shp`, `roads.shx`, `roads.dbf`,
  `roads.prj` as four unrelated loose files. The `.prj` (CRS) cannot inform the
  `.shp` parse, and a changelog reports four sibling changes instead of "the
  roads layer changed."

This ADR records the design decision for gap B.

### What "a claim" is today, and why it stops at one node

binoc's tree-shaping rules each *claim* a unit of work:

- `ExpandRule::expand(item: &ItemRef, …)` claims **one** node, unfolds it into
  children (zip/tar/dir → members).
- `ParseRule::parse(item: &ItemRef, …)` claims **one** node, decodes it into an
  artifact (+ optional children).
- Both descriptors carry a single `NodeMatch` (`is_dir`/`extensions`/
  `media_types`) tested against one `ItemRef`, plus a narrow in-body content
  check. "First successful claim wins for that node" (AGENTS rule 5). A rule that
  reads the node and decides it is not what it expected returns empty output —
  **decline** — leaving the node for another rule.
- A rule body may read other files opportunistically via `DataAccess`, but it is
  handed only its own node and **cannot mark siblings consumed**, so each sibling
  is still claimed independently.

`PairRule` is the only multi-node-aware family, but it relates the two *snapshot
sides* (`LinkProposal { left, right }`); it does not fuse siblings *within* one
snapshot. Pairing is the wrong tool.

The unit of claiming is one node. A file set needs the unit of claiming to be a
**correlated set of nodes**. That is the whole of the problem.

## Decision

**Generalize the parse claim from one node to a correlated set of nodes.** A
parse rule's input becomes an ordered list of member-matches with a correlation
key; the engine enumerates candidate sibling groups, hands each group to the
rule, and the rule's ordinary parse-or-decline is the authority on whether the
group is real. Single-file parsing is the size-1 degenerate case of the same
mechanism.

This reuses the existing matcher and the existing decline semantics; it adds
group enumeration, an arity-based precedence rule, and exactly one new store
primitive (subsume).

### 1. A claim is a correlated set of member-matches

```text
input:       [ MemberMatch { match: NodeMatch, required: bool }, … ]
correlation: <key>     // default: same parent container + shared basename stem
```

- A single-file parser is `[{ match, required: true }]` with trivial
  correlation — the common case, kept ergonomic by a constructor that takes one
  `NodeMatch`. The internal model widens; plugin authors of ordinary parsers do
  not feel it.
- The shapefile is
  `[{.shp, required}, {.shx, opt}, {.dbf, opt}, {.prj, opt}, {.cpg, opt}]`,
  correlated by stem.

The `NodeMatch` vocabulary is reused unchanged for each member slot. The format
knowledge (which extensions, whether the bytes validate) lives entirely in the
claim and `parse()`; the engine knows only the correlation key.

### 2. The correlation key keeps enumeration linear (and core format-ignorant)

Something must say `roads.dbf` binds to `roads.shp` and not `rivers.shp`. That is
the correlation **key**, and it must be a key, never a brute-force "find any
subset that validates" (combinatorial). The default key is **same parent
container + shared basename stem**: the engine groups a container's children by
stem in O(n), fills each member slot by `NodeMatch`, and hands the assembled
group to `parse()`. The shared-stem rule is the only generic grouping knowledge
core needs, and it is format-agnostic — so core and the directory/zip expanders
stay geospatial-ignorant (AGENTS rules 1–3).

For formats whose sidecars are named off the anchor rather than by exact stem
equality (`data.tif` + `data.tfw`, `data.tif` + `data.tif.aux.xml`), the
correlation generalizes to capture-the-anchor-stem + template-the-members —
**the same vocabulary `DeclaredPair` already uses** (`selector_captures` /
`expand_template` in `pair.rs`). Start at exact shared-stem; reach for the
templated form only when a real format needs it. Both are key-based and stay
non-combinatorial.

### 3. The parser's parse-or-decline is the authority

The assembled group goes to `parse()`. If the bytes validate as a shapefile, the
rule **claims** the group; if they do not (e.g. a standalone dBASE `.dbf` that
merely shares a stem with an unrelated `.shp`), it **declines** by returning
empty output, and the members are released to smaller claims. This is the
existing decline mechanism doing double duty — both "under what circumstances
should I try" (the declarative member-matches) and "I tried, it is not what I
expected, I do not claim this" (the in-body content check). There is no separate
grouping registry to disagree with the parser: the parser is the single source
of truth for "is this a shapefile."

### 4. Precedence: the largest *successful* claim wins; decline releases

This generalizes today's "first claim wins per node":

1. Attempt claims **arity-descending** (registration order breaks ties within an
   arity).
2. A claim that **validates** *subsumes* its members — they leave the dispatch
   frontier and are no longer offered to any other rule.
3. A claim that **declines** releases its members, which fall through to smaller
   claims.

So `roads.dbf` is offered to the size-5 shapefile claim first; if the `.shp`
validates, the `.dbf` is subsumed; if it declines, the `.dbf` falls to the
size-1 `binoc-dbf` parser and renders as an ordinary `tabular_v1` table. The
decline path *is* the conflict/degradation policy — a missing required member
means no group forms, and an invalid group dissolves into loose files — so no
separate "required-member" rule engine is needed. Single-file parse is the size-1
floor of this ladder.

### 5. The one new primitive: subsume (the structural fold)

Everything above is matcher reuse plus router logic. The one genuinely new
capability is in the store: **a node can be marked subsumed by a claim.** Until
now nodes are only ever *added* (expand and parse-children grow the tree);
fusion is the first operation that removes nodes from the visible tree. Two
constraints:

- **Mark subsumed, do not delete.** `NodeId`s are index-based; deletion shifts
  indices. Subsumption is a flag: subsumed nodes are excluded from dispatch and
  from projection as siblings, but survive as **provenance** the result node
  attributes to (member-level changelog attribution and CFM-71 reconciliation
  both want this).
- **Two-phase claiming.** Match → `parse()` → mark subsumed only on success.
  This is the shape single-node parse already has ("parse, then mark parsed if
  non-empty"), lifted from one node to a set. A claim must also be retried as the
  frontier grows across saturation rounds, since siblings can appear in different
  rounds (e.g. after a zip expands) — again the same retry parse already does.

`subsume` is the dual of `add_child`: `add_child` is the 1→N unfold, `subsume`
is the N→1 fold. The result node a fusing claim emits is the *same kind of node*
a parsed SQLite/Excel container emits — a node with a named `item_type`
("Shapefile layer"), `tabular_v1` children (the `.dbf` attribute table), and a
`parser_metadata_v1` artifact (CRS/encoding). Fusion produces the CFM-69/80/81
node shape from N inputs instead of 1; nothing downstream needs to know it was
fused.

### Why this seam

- **One source of truth.** The parser decides both grouping (its declared
  member-matches) and validity (its `parse()`), instead of a registry deciding
  grouping by extension before any byte is read and a parser deciding validity
  afterward. The blunt step (group by stem) is demoted from judge to nominator.
- **Maximal reuse.** The `NodeMatch` matcher, the decline mechanism, the
  capture/template correlation vocabulary, and the parse-children/multi-artifact
  output node all already exist. The new surface is one widened descriptor, an
  arity-descending precedence pass, and `subsume`.
- **It completes the arity matrix.** Within-snapshot the engine has had unfold
  (expand, 1→N) and never had fold; `subsume` is fold (N→1). It is the
  structural dual of expand and the input-side counterpart to CFM-81's
  output-side move ("the artifact is the rendering unit"): here, *the claim is
  the parse unit, not the node*.
- **It composes with what shipped.** The fusing parser reuses the single-input
  `.shp` reader and the `binoc-dbf` `tabular_v1` producer; rendering its several
  artifacts is exactly the composable-per-artifact-writer model (CFM-81).

## Relationship to the rest of the engine arc

- **CFM-81 (composable per-artifact writers) is a prerequisite for fusion
  *rendering*.** A shapefile node carries geometry + attribute table + CRS
  metadata simultaneously; only per-artifact composition can render "geometry
  changed + 3 attribute rows edited + CRS reprojected" coherently. Fusion is a
  third consumer of CFM-81 alongside metadata rendering (CFM-82) and a cleaner
  CFM-71.
- **CFM-71 (reconciliation)** shares fusion's member-attribution provenance, and
  "loose files → fused layer" or "shapefile → geopackage" is a container
  reshape it must project honestly.
- **CFM-72 (split/merge)** is the *across-snapshot* N↔1 of links; fusion is the
  *within-snapshot* N→1 of structure. Fusion makes a file set one node per side,
  so "the layer changed" is ordinary 1:1 pairing — fusion *reduces* the pressure
  on split/merge rather than competing with it.

## Alternatives Considered

### (a) Group siblings into a synthetic composite node at expand time, then parse it

A plugin-contributed set-definition *registry* recognizes sibling sets during
container expansion and rewrites the child list to emit one synthetic composite
`ItemRef` (e.g. `roads.shapefile`) in place of the members; an ordinary
single-input parse rule then claims the composite. **This was the earlier
decision of this ADR; it is now rejected.** It splits one judgment — "is this a
shapefile" — across two authorities (the registry that groups by extension, and
the parser that reads bytes), which can disagree: the registry greedily fuses a
standalone `.dbf` that merely shares a stem, destroying the member nodes into a
composite before the real authority (the `.shp` reader) can object. It also
needs a synthetic composite identity (a fake `.shapefile` extension or a tag
predicate) purely to route dispatch. The multi-input-claim model removes both
problems: there is no second authority and no synthetic node — the parser claims
the real member nodes and emits its result node directly. (Note: option (a) still
needed a subsume-equivalent — the composite *replaces* the members — so it was
never cheaper, only less honest about where the grouping decision lives.)

### (b-narrow) A declared sibling read-set bolted onto single-node parse

An earlier sketch added `siblings: Vec<SiblingSpec>` to `ParseDescriptor` and had
the engine resolve+inject siblings into an otherwise single-node parse. The
decision generalizes this rather than bolting it on: instead of "a single-node
parse with extra resolved siblings," the input *is* a correlated member-set, of
which single-node is the size-1 case. That keeps one model instead of two
(single-node parse and sibling-augmented parse) and gives the arity-descending
precedence and decline-releases-members behavior for free.

> The earlier framing weighted these alternatives by ABI-freeze cost (parse is
> the first family slated for a C ABI, so widening its trait was ranked most
> expensive). That weighting is set aside: this branch has already rewritten the
> plugin surface wholesale, and the ABI-stable tier has not graduated for
> expand/parse. The decision is made on architectural merits — one source of
> truth, maximal reuse — not ABI cost.

### (c) Opportunistically read siblings from inside the `.shp` parser

The shipped `.shp` parser could `open` a sibling `.prj` to add CRS. Rejected as
the answer to gap B: it does not subsume the duplicate sibling nodes
(`.dbf`/`.prj`/`.shx` still appear as independent changes), has no
engine-sanctioned sibling view, and buries an implicit multi-input contract in
one plugin instead of solving it once for every file-set format. The shipped
`.shp` parser stays honestly single-input and serves as the geometry reader the
fusing claim reuses.

## Consequences

- **`binoc-sdk`:** `ParseDescriptor.input` generalizes from one `NodeMatch` to a
  correlated member-match set (with an ergonomic single-`NodeMatch`
  constructor); `ParseRule::parse` receives the resolved group. Ships behind the
  in-process proposed tier until it settles.
- **`binoc-core`:** group enumeration by correlation key; arity-descending
  claim precedence with decline-releases-members; the `subsume` store primitive
  (a flag, not a deletion) plus its exclusion from dispatch and sibling
  projection and its retention as result-node provenance; cross-round retry of
  unsatisfied claims.
- **Plugins:** a `binoc-shapefile` fusing claim that reads `.shp` + `.dbf` +
  `.prj`, emits geometry + `tabular_v1` attribute child + `parser_metadata_v1`
  CRS, and declines cleanly when the group is not a real shapefile. The shipped
  single-input `.shp` parser remains valid for a bare `.shp`.

## Open Questions

- **Correlation precision.** Exact stem extraction (`roads.shp` vs
  `roads.v2.shp`) and case-sensitivity (`.SHP`); when to escalate from
  shared-stem to the capture/template form.
- **Precedence determinism** when two equal-arity claims want overlapping
  members — registration order is the tiebreak, but the rule must be stated and
  tested.
- **Subsume + projection.** Exactly how subsumed members surface (or do not) in
  the changeset, and how the result node attributes a change to a specific
  member when useful.
- **Cross-round retry bounds.** Ensuring an unsatisfied multi-input claim is
  re-offered as siblings appear without re-scanning the whole frontier every
  round.

These are implementation-design questions; resolving them is the next step before
building the mechanism. The shipped `.shp`/`.dbf` single-input parsers are the
proof inputs; the first fusion vector is a `roads.{shp,shx,dbf,prj}` set whose
CRS changes while geometry is unchanged.
