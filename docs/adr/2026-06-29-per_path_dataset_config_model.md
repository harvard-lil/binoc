# A Unified Per-Path Dataset Config Model

**Date:** 2026-06-29
**Status:** Accepted

## Context

Five queued issues each want to attach information to a *path within a dataset*,
and each is currently inclined to add its own top-level config key:

- **#107** — override how a path *dispatches* (pin a content type, or force a
  specific rule to claim it) — e.g. treat extensionless `northamerica` as text,
  not binary.
- **#69** — declare or sniff CSV dialect (delimiter, quote, escape, BOM) for a path.
- **#104** — declare a table's physical shape: `has_header: false`,
  `header_line`/`skip_lines`, key-by-position.
- **#105** — name the nested record collection inside a document
  (`records_path: "$.objects"`) so keying can reach it.
- **#106** — declare a structured-tree key attribute
  (`key_attribute: identifier`) for XML/USLM-style documents.

Today, core treats `DatasetConfig.dataset` as an opaque `serde_json::Value`
(`binoc-core/src/config.rs`) and passes it through untouched — rule #1, the
controller is type-ignorant. The stdlib deserializes it into a
`DatasetSemanticsV1` whose only per-path facet so far is row keying:

```
dataset:
  tables:
    defaults: { row_identity: { columns: [...] } }
    entries:  [ { <path matcher>, row_identity: { columns: [...] } } ]
```

`row_identity_for_paths` (`binoc-stdlib/src/correspondence/mod.rs:477`) resolves
this with first-match-wins over a default — but only for paths that
`is_tabular_path` already classified as tabular by extension. So the bones of a
per-path override system exist; the risk is that five issues bolt five sibling
keys onto it, each with its own matcher syntax and merge rules, producing a
config surface where you cannot say "this one path is text *and* keyed" and where
the matcher behaves differently in each facet.

## Decision

Generalize the existing `tables.entries` mechanism into **one ordered list of
per-path entries, each a `selector` plus optional `facets`**, owned and
interpreted entirely by the stdlib under the opaque core blob. There is one path
matcher, one precedence rule (first match wins, merged over `defaults`), and one
place to learn how binoc decides what to do with a path.

```
dataset:
  defaults:
    row_identity: { columns: [...] }      # facet defaults, as today
  paths:
    - match: "**/SDN.CSV"                  # glob selector
      content_type: text/csv               # dispatch override (#107)
      dialect: { delimiter: "," }          # dialect facet (#69)
      shape:  { has_header: false }        # shape facet (#104)
      row_identity: { by_position: [1] }   #   key-by-position (#104)
    - match: "**/northamerica"
      content_type: text/plain             # promote out of binary (#107)
    - match: "**/legacy.dat"
      rule: binoc.xml.parse                # force a rule when no media fits (#107)
    - match: "**/*.stix.json"
      records_path: "$.objects"            # nested-collection facet (#105)
      row_identity: { columns: ["id"] }    #   keying reaches into it (#105)
    - match: "**/usc/**/*.xml"
      node_identity: { key_attribute: "identifier" }  # tree-key facet (#106)
```

The selector is a **glob** (`**/SDN.CSV`, `**/northamerica`). A `regex:`-style
opt-in can be added later if a target needs anchoring a glob can't express; glob
is the no-surprise default and matches how most users think about paths.

The **dispatch override** (#107) is open, not a closed kind enum. It takes one of
two forms: `content_type:` pins a media type and lets normal declarative dispatch
(rule #5's media filter) run — so any rule, stdlib or plugin, that filters on that
media gets a fair claim; `rule:` forces a named rule to claim the node, the escape
hatch for when no media type disambiguates or two rules filter the same media.
`text/csv`/`text/plain`/binary are just common *outcomes* of stdlib's
`projection_for()`, not the schema — keeping the override a media/rule reference
(rather than a `text|tabular|binary` enum) is what lets it reach plugin formats
without a schema change. It is also the same mechanism as #107's content sniff:
sniffing *infers* a content type (disclosed, per the inference ADR), config
*declares* one (silent).

Note the *keying facets* sit beside this override, and none of them is a fourth
projection kind: `row_identity` keys a flat table, `records_path` + `row_identity`
keys a table nested in a document, and `node_identity` keys nodes in a
structured-document tree (#106). Tree-ness is a *parser* outcome (JSON, or XML via
`binoc-xml`, becomes a `structured_document` artifact) — config does not *select*
it, it only supplies the key. That is why there is no `as: tree`.

Resolution order is the load-bearing part:

1. **Selector match** → first entry whose `match` matches the logical path.
2. **Dispatch override (`content_type:` / `rule:`)** feeds declarative dispatch —
   and runs *before* the tabular gate. This is the fix for the current ordering
   bug: today `row_identity_for_paths` filters to `is_tabular_path` first, so
   config can only key paths the *extension* already called tabular. Under the new
   model, `content_type: text/csv` lets a path dispatch *into* the tabular parse
   rule (or `rule:` forces it), after which the resulting artifact's facets apply.
3. **Kind selects which facets are meaningful.** `dialect`/`shape`/`row_identity`
   apply to tabular; `records_path` + `row_identity` apply to a document that
   contains a table; `node_identity` applies to a structured-document tree. A
   facet that doesn't match the kind is a config error, not silently ignored
   (no-guess path, per AGENTS.md rule #5).

Defaults merge under entries facet-by-facet (as `row_identity` already does at
`mod.rs:494-501`), so an entry only states what it overrides.

This keeps the schema in stdlib (rule #1), reuses the existing precedence
semantics, and means each of #69/#104/#105/#106/#107 becomes "add one facet to a
known shape and teach the resolver one new branch," not "invent a top-level key."

## Alternatives Considered

**Flat sibling keys** (`dataset.tables`, `dataset.dialects`, `dataset.projections`,
`dataset.trees`, each its own list). Rejected: five matchers that will drift in
syntax and precedence, duplicated selector logic, and no way to express a single
path that is both *promoted to a kind* and *configured within that kind* (e.g.
OFAC's `SDN.CSV` needs projection + dialect + shape + position-key together).
This is the path of least resistance for five parallel agents and exactly the
incoherence this ADR exists to prevent.

**One selector list, but facets typed as separate parallel sub-lists keyed by the
same selector string.** Rejected: you still match the same path three times and
must keep three lists in sync; the "one path, one rule" readability is lost.

**Put the schema in core so every plugin shares it.** Rejected: violates the
type-ignorant controller (rule #1). Core carries the blob; the pack that owns the
domain owns the schema. A third-party pack with its own facets deserializes its
own slice, exactly as stdlib does.

**Keep `tables`/`trees` as two typed top-level lists** (tables and trees are
genuinely different artifact kinds, so two lists is arguably honest). This is the
closest rejected alternative and a reasonable fallback. Rejected in favor of one
list because the *selector* and *precedence* are what most need unifying, and a
single `as:`-discriminated entry makes "what kind is this path, and how is it
configured" answerable in one lookup. Open for discussion: if validation by kind
proves awkward, splitting `paths` into `tables`/`trees` while sharing the selector
+ precedence helpers is an acceptable retreat.

## Open sub-decisions (flagged for review, not yet decided here)

- **Position-keying representation** (`row_identity.by_position: [1]`) and how it
  composes with synthesized positional column names from `shape.has_header:false`
  (#104 calls for both).
- Whether the `node_identity` facet (#106's keyed-tree, the heaviest issue) lands
  in this round or is reserved — the schema slot is defined now so the model is
  forward-compatible either way, since it is just another keying facet alongside
  `row_identity`.
