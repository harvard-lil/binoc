# Composable Per-Artifact Writers: the Artifact Is the Rendering Unit

**Date:** 2026-06-15
**Status:** Implemented (CFM-81)

## Context

A node can now carry more than one content artifact. The tiered-artifact-metadata
work (see that ADR) put a `parser_metadata_v1` bag on the same node as a
`tabular_v1` leaf, and on a multi-table container alongside its `tabular_v1`
children. That is the first time a single link legitimately has two *orthogonal*
artifacts — not two encodings of the same thing, but two different kinds of
information (the table, and facts about the source format) that a reader could
each want rendered.

The edit-list dispatch was not built for that. For each link the driver walks the
registered writers, takes the **first** writer whose declared `formats` are all
present and whose shape matches, runs it, takes its edit list, and `break`s
(`binoc-core/src/correspondence/driver.rs`). One writer owns the link. This works
only because, until now, every node has had exactly one content artifact — so
"the writer for this node" and "the writer for its one artifact" were the same
thing. The loop quietly does two jobs at once: *selecting* a writer and
*composing* the link's edit list. With one artifact, composition is trivial, so
the conflation has been invisible.

With two artifacts it stops being invisible. The obvious patch — have
`TabularWriter` also read the `parser_metadata_v1` artifact and fold its changes
into the tabular edit list — is not composable: it couples two unrelated content
types through one writer, hard-codes which writer "owns" the secondary artifact,
and does not generalize (a third-party plugin attaching metadata to CSV nodes
would have to patch stdlib's `TabularWriter`). The single-writer-per-link
assumption is also baked into bookkeeping: `RunStats.writer_used` is one name per
link, `extract_line` looks up *the* writer for a link, and
`LinkDescriptionCost.writer` is a single column.

A separate, older idea is worth naming because it is easy to conflate with this
one: writers as **substitutable dialects** — an edit-list rule speaks `tabular_v1`
*or* a hypothetical `sparkly_tabular_v3`, and the engine picks whichever the node
carries. That is real, but it is substitutability *within* a content type, an
`OR` over alternative encodings of "a table." It is a different axis from
composing *across* distinct content types, and the current first-match loop reads
as if those two axes were the same.

## Decision

Make **the artifact the unit of rendering.** A link's edit list is the
composition of per-artifact contributions plus node-level structural
contributions, not the output of a single owning writer. Concretely:

1. **Two writer kinds, named explicitly.**
   - *Artifact writers* declare a non-empty `formats` and render one artifact
     each (`TabularWriter` → `tabular_v1`, `StructuredDocumentWriter` →
     `structured_document_v1`, a new `ParserMetadataWriter` → `parser_metadata_v1`).
   - *Structural writers* declare empty `formats` and describe the node/tree
     itself: `ContainerWriter` (child add/remove from structure), `TextWriter`
     (extension-gated leaf bytes), `FallbackWriter` (byte/hash diff of last
     resort).

2. **Dispatch composes, then selects.** For each link: run one artifact writer
   per *present* artifact format and concatenate their edits; also run the
   applicable structural contribution (e.g. container child-tracking). The
   `OR`/substitutability axis lives *inside* the per-format step — among writers
   that speak a given format, pick one by priority. `FallbackWriter` fires only
   when nothing else produced edits. So "run all" composes across formats and
   "pick one" selects within a format; the current loop collapsed both because
   there was only ever one artifact.

3. **Edit provenance is the enabling primitive.** Tag each `Edit` with the
   artifact format (or writer) that produced it. This is what lets the downstream
   machinery stay per-content-type even though the edits now live in one merged
   list: compaction rules rewrite only the segment of their own format,
   `extract()` routes an aspect request to the writer that produced the relevant
   edits, and summary/projection can group heterogeneous edits coherently.

4. **Determinism and format-scoped compaction.** Concatenation order is fixed
   (e.g. by artifact format, structural last). Compaction stops receiving a link's
   whole mixed edit list and instead operates on the provenance-scoped segment it
   declares — a tabular compaction rule must never see or rewrite metadata edits.

5. **Bookkeeping becomes per-writer-set.** `writer_used` becomes a set per link;
   `extract`, the trace recorder, `perf_report`, and the single `writer` column in
   `LinkDescriptionCost` follow. These are mechanical migrations, but they are the
   load-bearing places where "one writer per link" is currently assumed.

Why this and not the owner-reads-siblings patch: the current design is the
*degenerate case* of this one — when each node has exactly one content artifact,
"per-artifact rendering" reduces exactly to "one writer owns the link." Adopting
the general form is therefore not a new model bolted on; it is removing an
accidental restriction that only held while nodes were single-artifact. It keeps
the plugin seam intact (a format's writer is registered, not ventriloquized by a
neighbor), and it makes the `parser_metadata_v1` case — and any future second
artifact — compose for free.

This ADR records the decision and the shape; it is **Proposed** and not yet
implemented. The tiered-metadata channels already exist and are populated; this
is the rendering model they will be consumed through (tracker item CFM-81, with
metadata rendering itself as CFM-82).

## Alternatives Considered

- **Owner-reads-siblings (status quo + a hack).** Keep one writer per link and
  have the owning writer (`TabularWriter`, `ContainerWriter`) read and diff the
  node's other artifacts. Rejected: not composable, couples unrelated content
  types, hard-codes ownership, and breaks the plugin seam — a new artifact format
  could not be rendered without editing whoever happens to own the node.

- **Keep one-writer-per-link permanently; never carry two content artifacts.**
  Forbid the situation by folding all of a node's facts into a single artifact
  (e.g. extend `tabular_v1` to also hold parser-level metadata). Rejected: that is
  the blob that made metadata homeless in the first place, it conflates grains
  with different keys, and it pushes every future "second kind of fact" back into
  an over-stuffed primary artifact. The multi-artifact-per-node model already
  exists; the dispatch should match it.

- **Codecs that render themselves (a `render()` method on the artifact type).**
  Rejected: it bypasses the registration/plugin seam and the renderer/config
  significance layer. "Each artifact prints itself" is the right intuition, but
  the right spelling is *one registered writer per artifact format, dispatched
  per artifact* — not a method on the data type.

- **A single writer that declares multiple formats** (`formats: [tabular_v1,
  parser_metadata_v1]`). Rejected as the general mechanism: it only matches nodes
  that have *all* listed formats and still produces one fused edit list, so it
  neither composes orthogonally nor degrades when only one artifact is present. It
  remains available for the rare case where a writer genuinely needs to correlate
  two formats, but it is not how independent artifacts should compose.

- **Significance baked into the writers.** Have each metadata writer decide how
  loud its changes are. Rejected per AGENTS.md rule 3: significance is a
  renderer/config concern mapped from tags, not an IR or writer concern. The
  artifact writers emit factual, provenance-tagged edits; weighting a relabeled
  column vs. a dropped value-label set vs. a creator rename is config (CFM-82).
