# SDK Annotation Package Identity Stays Explicit

**Date:** 2026-06-29
**Status:** Accepted

## Context

The annotation API currently requires every caller to spell the package
namespace explicitly:

```rust
node.annotate_from("binoc-sqlite", "row_count", json!(42));
```

The desired ergonomic shortcut would be an ambient `annotate(key, value)` that
implicitly uses the caller's package id. The question is what defines that
package id and whether it is worth threading through the SDK surface.

Today, the SDK does not expose rule-pack metadata as an ambient identity source.
Rule descriptors carry per-rule names such as `binoc-sqlite.parse.sqlite` and
`binoc-xml.parse.xml`, but that is rule identity, not pack identity. There is no
pack-level descriptor in `binoc-sdk`, and `DiffNode` construction has no access
to a current rule-pack context. Introducing implicit annotation identity would
therefore require new SDK plumbing, and likely additional ABI shape if the
ambient identity were to cross plugin boundaries.

## Decision

Keep annotation identity explicit. `annotate_from(package, key, value)` remains
the supported API for renderer-visible annotations, and `binoc_annotation(key)`
continues to be the sugar for the host's own `binoc` namespace.

We do not add `annotate(key, value)` at this time.

## Alternatives Considered

**Derive the package id from rule descriptor names.** Rejected. Rule names are
already namespaced for dispatch and diagnostics, but they do not define a pack
identity in the SDK. Deriving a shared ambient package id from them would be a
guess, not a stable contract.

**Add a pack-level descriptor or ambient rule-pack context.** Rejected for now.
That would be new API surface and, if used across the plugin boundary, new
plumbing for a convenience feature that only pays off if many plugins need the
implicit self form.

**Add `annotate(key, value)` anyway and keep `annotate_from`.** Rejected. The
explicit form already covers both self and cross-package annotations without
new ambient state, and the current plugin set does not justify the extra API
surface.
