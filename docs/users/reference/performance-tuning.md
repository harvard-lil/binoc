---
audience: anyone authoring a dataset config
---

# Performance tuning

Binoc defaults to correctness over speed. Performance flags should be treated as
explicit tradeoffs: enable them only when the possible loss of provenance is
acceptable for the dataset and workflow.

## `expand_renamed_unchanged_collections`

```yaml
dataset:
  correspondence:
    expand_renamed_unchanged_collections: true
```

Default: `true`.

When a directory or archive is renamed but its own contents are unchanged, the
default engine still expands inside it. This preserves provenance for facts such
as a file copied out of the renamed collection.

Set the flag to `false` for a faster short-circuit posture:

```yaml
dataset:
  correspondence:
    expand_renamed_unchanged_collections: false
```

With `false`, binoc can settle renamed unchanged collections without inspecting
their descendants. The diff is faster on large renamed trees, but copy/move
provenance involving children inside that collection may be less specific.

## Plugin-specific knobs

Plugins may expose their own performance knobs under `dataset` or plugin config
sections. Follow the same rule when evaluating them:

- Correct-by-default settings belong in examples and shared configs.
- Fast-over-complete settings should be opt-in and documented with the facts
  they may hide or degrade.
- A conservative bail should surface a diagnostic when the user would otherwise
  mistake "not checked" for "not present".

See the SQLite plugin docs and test vectors for the pattern of documenting
domain-specific identity and degradation behavior.
