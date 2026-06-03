# Transformer-Initiated Recompare as a Correspondence Contract

**Date:** 2026-06-03
**Status:** Implemented

## Context

Some transformers discover that two existing diff leaves represent the same
logical item even though ordinary path-based dispatch reported them as an
unrelated remove and add. Fuzzy rename detection does this from content
similarity. Declared file correspondence does this from dataset config.

In both cases the transformer should not parse raw file formats itself. The
controller already owns comparator dispatch, and comparators already publish the
artifacts later transformers need.

## Decision

`DiffNode::pending_recompare` is the contract for a transformer-discovered
correspondence to ask the controller to re-run comparator dispatch for an
explicit `ItemPair` and merge the result into the transformer's semantic wrapper
node.

The transformer owns the semantic wrapper: action, logical path, source path,
tags, summaries, and details such as correspondence rule names. The controller
owns re-dispatch and merge behavior:

- content artifacts, comparator name, source items, child nodes, tags, and
  details from the recomparison are merged into the wrapper;
- recomparison failures are diagnostics, not silent no-ops;
- identical recomparisons convert plain `modify` wrappers to `identical` so
  they can be pruned, while semantic wrappers such as moves remain reportable;
- durable source/destination path details can be used to replay extraction when
  a wrapper's logical path differs from the physical snapshot paths.

`pending_recompare` remains transient session data and is stripped before
changeset output.

## Alternatives Considered

Transformers could call comparators directly. That would duplicate controller
dispatch logic inside plugins and make ordering harder to reason about.

Declared correspondence could be implemented as a comparator. That would put
dataset-level identity policy into a format-specific layer and would not compose
with nested or plugin-produced leaves.

The controller could ignore identical recomparisons. That worked for some fuzzy
move cases, but it leaves phantom `modify` nodes for declared correspondences
whose content is unchanged.
