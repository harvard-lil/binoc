# Shapefile support in binoc: the multi-input problem and a design proposal

**Date:** 2026-06-15
**Status:** Proposal (Part 1 investigation) + shipped single-input `.shp` parser (Part 2)

## Background

A "shapefile" is not a single file. It is a set of sibling files that share a
basename and together form one logical geospatial vector dataset:

| Member | Required | Content |
|---|---|---|
| `.shp` | yes | feature geometry (points / polylines / polygons / multipatch) |
| `.shx` | yes | geometry offset index into `.shp` |
| `.dbf` | yes | per-feature attribute table (one record per geometry) |
| `.prj` | no | coordinate reference system, as WKT |
| `.cpg` | no | character encoding of the `.dbf` |
| `.sbn`/`.sbx` | no | ESRI spatial index |

binoc already parses the `.dbf` attribute table on its own
(`model-plugins/binoc-dbf`, producing `tabular_v1`). Two gaps remain:

- **(A) No `.shp` geometry parser.** Nothing reads the geometry.
- **(B) No fusion.** binoc sees `roads.shp`, `roads.shx`, `roads.dbf`,
  `roads.prj` as four unrelated loose files, not one dataset. The `.prj`
  (CRS) cannot inform the `.shp` parse, and a changelog reports four sibling
  changes instead of "the roads layer changed."

Geospatial vector data is a large fraction of open-data portals (Census TIGER
alone is ~25% of data.gov), so closing both gaps matters. **Gap A is solved in
this change (Part 2). Gap B — the multi-input problem — is the subject of this
proposal.**

## Part 1 verdict: can a rule consume more than one node today?

**No. Every expand and parse rule is strictly single-node in, and there is no
rule family whose *output* is an artifact or child set derived from multiple
sibling nodes.** This is definitive from the SDK trait signatures
(`binoc-sdk/src/correspondence.rs`):

```rust
pub trait ExpandRule {
    fn descriptor(&self) -> ExpandDescriptor;        // input: NodeMatch  (ONE node)
    fn expand(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ExpandOutput>;
}                                       //        ^^^^^^^^^^^^^ exactly one node

pub trait ParseRule {
    fn descriptor(&self) -> ParseDescriptor;         // input: NodeMatch  (ONE node)
    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput>;
}                                      //        ^^^^^^^^^^^^^ exactly one node
```

- `ExpandRule` turns **one** node into children (`item: &ItemRef` -> `Vec<ItemRef>`).
- `ParseRule` turns **one** node into an artifact (`item: &ItemRef` -> bytes).
- Dispatch is per-node: `ExpandDescriptor.input` / `ParseDescriptor.input` are a
  single `NodeMatch` (`is_dir`/`extensions`/`media_types`), matched against one
  `ItemRef` at a time. "First successful expand/parse claim wins for that node"
  (AGENTS rule 5) — the unit of claiming is one node.
- A rule body *can* read other files opportunistically (`DataAccess` exposes the
  workspace), but it is handed only its own node, has no engine-blessed view of
  siblings, and cannot register the sibling files as consumed — so a second rule
  will still independently claim each sibling. There is no "request my siblings"
  channel on either descriptor.

The **only** multi-node-aware family is `PairRule`, which gets a whole-tree
`EngineView` (`view.children(id)`, `view.item(id)`, …). But a pair rule's output
is a *link proposal between a left node and a right node* (`LinkProposal { left,
right, evidence }`) — it relates the two snapshot sides, it does not fuse
siblings *within* one snapshot into a composite, and it produces no artifact or
child geometry. So pairing is the wrong tool for "treat `.shp`+`.shx`+`.dbf`+
`.prj` as one dataset."

**Conclusion: multi-input parsing is not supported. A shapefile-as-one-dataset
requires a new mechanism.** The options below are evaluated against binoc's
expand-vs-parse model and the pre-1.0 tiering constraint (AGENTS rule 8 / the
[tiered plugin surface ADR](../../docs/adr/2026-06-12-tiered_plugin_surface_pre_1_0.md)):
do not destabilize trait signatures lightly before 1.0; expand/parse packs are
the *first* tier slated to graduate to a frozen ABI, so changes to those
signatures are the most expensive ones to make.

## Options

### (a) A grouping expand rule that emits a synthetic composite node

A new expand rule fires on a **container** (a directory or an
already-extracted zip), scans its immediate children for a shapefile set
(basenames where a `.shp` exists with `.shx`/`.dbf`/`.prj` siblings), and
rewrites the tree: it emits one synthetic composite node (e.g.
`roads.shapefile`, `is_dir = false`, a tag marking it a fused set) that carries
references to all members, and it suppresses the individual member nodes so they
are not independently claimed. A normal single-input parse rule then claims the
composite node and reads every member through `DataAccess` (it knows the member
paths because they are the composite's recorded inputs).

- **Fits the model?** Mostly. Expand already "turns a node into children" and
  already rewrites tree shape (zip/tar extraction). Grouping is the dual —
  collapsing siblings into a parent — but it is still tree-shaping, which is
  expand's job. The parse step that follows is an ordinary single-input parse of
  the composite, so the high-value, soon-to-freeze parse trait is untouched.
- **Dispatch implications:** the composite needs a distinguishable identity
  (a synthetic extension like `.shapefile`, or a tag-based `NodeMatch`
  predicate) so exactly one parse rule claims it and the raw `.shp` rule does
  not double-fire. Suppressing the member nodes needs an expand output that can
  *remove* siblings, which `ExpandOutput { children }` does not express today —
  expand fires *on* the container and returns the children it should have, so
  the grouping rule would return the surviving children (composite + non-member
  files) and omit the members. That is expressible if the grouping rule is the
  directory/zip expander itself (it owns the full child list) but **not** if it
  is a separate rule layered after directory expansion, because there is no
  "claim/consume these already-emitted sibling nodes" channel. So (a) realistically
  means *teaching the container expanders (or a dedicated post-expand pass) to
  recognize sets* — which bleeds domain knowledge into stdlib's directory/zip
  rules, or needs (c).
- **Tiering:** no trait signature change if done inside existing expand
  descriptors; a clean composite-emitting variant is additive. Lowest ABI risk
  of the three.

### (b) Extend the parse-rule trait so a rule can request siblings

Add a declared sibling read-set to `ParseDescriptor` (e.g.
`siblings: Vec<SiblingSpec>` by glob/extension/shared-basename) and widen
`ParseRule::parse` to receive the resolved sibling `ItemRef`s alongside the
primary node. The engine resolves siblings from the same container, hands them
in, and marks them consumed so they are not independently claimed.

- **Fits the model?** It is the most *direct* expression of "this parser is
  multi-input," and it keeps geometry+attributes+CRS fused at the natural place
  (the parser that understands the format). The `PairDescriptor.reads` field is
  precedent for descriptors declaring a read-set the engine must satisfy before
  the rule runs — so a `ParseDescriptor.siblings` read-set is idiomatic.
- **Dispatch implications:** the engine gains a sibling-resolution and
  consumption phase (which nodes count as "consumed" so they don't re-fire).
  Needs a defined scope for "sibling" (same parent container, shared basename
  stem) and a conflict policy when two multi-input parsers want the same member.
- **Tiering:** **this changes the parse trait signature and descriptor** — the
  single most expensive change under AGENTS rule 8, because expand/parse packs
  are explicitly the first family slated to graduate to a frozen C ABI. Doing
  this pre-1.0, before the parse vocabulary has otherwise settled, is exactly
  what rule 8 says to avoid "lightly." Highest ABI risk.

### (c) A sibling-aware pre-pass that fuses related files before rules run

A dedicated pre-saturation pass (not a rule) walks each container after
expansion, recognizes file sets by basename+extension membership, and fuses
each set into one composite node (as in (a)) before the rule worklist runs —
generically, driven by a registry of set definitions (`shapefile = {.shp
required; .shx,.dbf,.prj,.cpg optional}`), so the fusion logic is not specific
to any one format and stdlib's directory/zip expanders stay format-ignorant.

- **Fits the model?** It sits *beside* the expand/parse/pair families rather
  than inside them: a tree-normalization step, analogous to how extraction
  produces a navigable tree before rules see it. It cleanly solves the
  "suppress the member nodes" problem that (a) struggles with as a layered rule,
  because the pre-pass owns the child list at fusion time.
- **Dispatch implications:** introduces a new engine phase and a set-definition
  registry config surface. Composite nodes still need a parse claim (same as
  (a)). The set registry is plugin-contributed data (which extensions form a
  set), so a shapefile plugin contributes its set definition without any core
  geospatial knowledge.
- **Tiering:** no rule-trait change (good), but it adds a **new core phase and a
  new config/registration surface** — a larger core commitment than (a), and one
  that wants its own ADR. Medium ABI risk: the seam is new data, not a changed
  trait, but it is core machinery.

## Recommendation: (a), realized as a generic set-grouping expander

**Recommend (a) — a grouping/composite-emitting expand step — with the set
definitions supplied as plugin-contributed data so the grouping logic itself is
format-neutral** (borrowing the "registry of set definitions" idea from (c)
without standing up a whole new engine phase).

Rationale, weighted by the pre-1.0 tiering constraint:

1. **It touches the cheapest seam.** Grouping is tree-shaping, which already
   lives in expand. The downstream parse of the composite is an *ordinary
   single-input parse* — so the parse trait (the first thing slated to freeze
   into an ABI) is **not** changed. Option (b) changes exactly that trait and is
   the most expensive move rule 8 warns against; option (c) adds a new core
   phase.
2. **It matches an existing mental model.** "A node expands into a different set
   of nodes" is what zip/tar/directory expansion already does; collapsing
   siblings into a composite is the same kind of rewrite, just folding instead
   of unfolding.
3. **It keeps domain knowledge out of core.** The set definition
   (`shapefile = .shp + {.shx,.dbf,.prj,.cpg}`) is contributed by the shapefile
   plugin as data; stdlib's directory/zip expanders and `binoc-core` stay
   geospatial-ignorant (AGENTS rules 1–3).
4. **It composes with what shipped.** The composite parser reuses the `.shp`
   geometry reader shipped in Part 2 and can additionally pull the sibling
   `.dbf` (already a `tabular_v1` producer) and `.prj` (CRS) to publish a richer
   per-layer summary plus the attribute table as a child — exactly the
   "one logical dataset" projection the changelog should show.

**The one honest gap in (a):** as noted above, *suppressing* the member nodes so
they are not also claimed individually is not expressible by a parse/expand rule
layered *after* directory expansion — `ExpandOutput` has no "consume these
already-emitted siblings" channel. The clean realization therefore makes the
grouping a property of the **container expansion result** (the directory/zip
expander, or a thin post-expand grouping hook it delegates to, applies the
plugin-contributed set registry while it still owns the full child list and can
emit the composite *in place of* the members). That post-expand grouping hook is
the minimal new core seam — strictly smaller than (b)'s trait change or (c)'s
full new phase — and it is the recommended path. It deserves a short follow-up
ADR before implementation.

### Why not just opportunistically read siblings from inside the `.shp` parser?

The shipped `.shp` parser (Part 2) *could* try to `open` a sibling `.prj` from
the workspace to add CRS to its summary. We deliberately **do not** rely on that
as the answer to gap B: it does not suppress the duplicate sibling nodes (the
`.dbf`/`.prj`/`.shx` still appear as independent changes), it has no
engine-sanctioned sibling view, and it would bury an implicit multi-input
contract inside one plugin instead of solving it once for every file-set format
(GeoTIFF world files, ENVI `.hdr`, NIfTI `.hdr`/`.img`, …). Part 2 keeps the
`.shp` parser honestly single-input and reports CRS only when the bytes it is
given carry it (they don't, for raw `.shp`); CRS fusion is explicitly deferred
to the (a) composite.

## What shipped now (Part 2)

A single-input `.shp` geometry parser (`binoc-shapefile`), modeled on
`binoc-dbf` / `binoc-binformats`. It reads the `.shp` geometry alone (via the
`shapefile` crate's `ShapeReader`, which needs neither `.shx` nor `.dbf`) and
emits a `structured_document_v1` artifact tagged `format: "shapefile"`
summarizing: feature count, geometry type, and overall bounding box. CRS is
deferred (not reachable from `.shp` bytes alone; arrives with the (a)
composite). See the crate README/source and the artifact-choice justification in
the final report.
