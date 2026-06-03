# Progressive Renderer Annotations

**Date:** 2026-06-03
**Status:** Implemented

## Context

Comparators and transformers sometimes learn renderer-visible facts that do not
belong in the core action/type/tag model. For example, the tabular distribution
stats annotator can summarize a numeric distribution shift after the CSV
comparator has produced the tabular diff tree. A renderer should be able to
surface that fact without the transformer emitting Markdown and without the
core controller learning about tabular data.

The first implementation path for distribution stats made Markdown aware of a
specific annotation shape. That worked for the feature, but it coupled a
general renderer to one transformer too early. The more important design
pressure is simple plugin composability: a transformer should be able to attach
small facts, and renderers should have a predictable fallback even when they do
not know the package-specific meaning.

## Decision

Add a renderer-visible `Annotation` record to the SDK/IR:

```rust
pub struct Annotation {
    pub package: String,
    pub key: String,
    pub value: serde_json::Value,
}
```

`DiffNode::annotate_from(package, key, value)` attaches an explicitly
namespaced annotation. Reusing the same `package` and `key` replaces the prior
value. The SDK deliberately does not provide `DiffNode::annotate(key, value)`
yet, because there is no ambient "current package" identity in comparator or
transformer calls. A future context-aware API can add that shorthand once it can
default to the calling package rather than to `binoc`.

Annotation values are progressively typed JSON. Producers can start with a
string, a list of strings, or another simple JSON value. Renderers can consume
the generic JSON shape immediately, and can later add package/key-specific
handling if a stable semantic contract emerges. There is no separate version
field for now; the current contract is the `package`, `key`, and JSON `value`
shape itself.

Markdown renders annotations generically:

- scalar values render inline under a humanized annotation label;
- arrays of strings render as bounded bullet lists;
- other JSON values render as compact JSON.

Markdown deliberately does not special-case annotation types for distribution
stats. The stats annotator publishes human-readable strings under
`binoc/distribution_shifts`, demonstrating that transformers can provide useful
annotations without requiring renderer-specific code. The existing
`content_summary` and `tabular_summary` annotations remain special only in the
sense that Markdown suppresses their generic rendering because they are already
consumed as move/correlation trailer text.

## Alternatives Considered

Keep `annotations` as a map from key to JSON value.

: This is convenient, but it has no package namespace. Third-party plugin packs
  would collide on common keys such as `summary`, `note`, or `stats`, and the
  renderer could not distinguish first-party conventions from external ones.

Add explicit annotation versions immediately.

: Versioning will matter once renderers start depending on package/key-specific
  structured values. It is premature for the initial composability contract,
  where the default renderer is intentionally duck-typing simple JSON shapes.
  A future typed annotation can encode its version in its key, package contract,
  or value object when a real consumer needs that stability.

Restrict annotations to strings.

: Strings are the simplest interoperable value, and many annotations should use
  them. However, allowing JSON lets producers group strings or publish compact
  structured values without another IR migration. The renderer still treats
  unknown structure generically.

Let transformers emit Markdown.

: That would make transformer output renderer-specific and would bypass the
  project rule that significance and presentation remain renderer concerns.

Add Markdown handlers for each known annotation.

: That would optimize for the first feature instead of the extension point.
  The current default renderer should prove the general contract before adding
  package/key-specific display rules.
