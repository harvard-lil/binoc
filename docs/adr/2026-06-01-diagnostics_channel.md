# Diagnostics Channel for Non-Fatal Warnings and Suggestions

**Date:** 2026-06-01
**Status:** Implemented

## Context

Binoc needs a lightweight way to tell callers about missing metadata,
configuration opportunities, or fallback behavior without failing a diff.
Several follow-up issues will emit this kind of guidance, so the shape needs to
be structured, durable in changeset JSON, and independent of any one renderer.

The guidance is advisory rather than factual change IR. It should not be baked
into node tags or pre-rendered prose, and it must stay bounded so a noisy plugin
cannot flood output.

## Decision

Binoc adds a top-level `Changeset.diagnostics` list. Each record has:

- `severity`: `warning` or `suggestion`
- `code`: a stable, package-qualified string such as `binoc.binary-fallback`
- `message`: human-facing advisory text
- `location`: optional logical node path

Comparators and transformers emit diagnostics on `DiffNode.diagnostics` during a
live diff session. The controller hoists them into `Changeset.diagnostics` after
all transforms complete, de-duplicates by `(code, location)`, caps the stored
list, and then strips node-local transient diagnostics before returning the
changeset.

Renderers consume `Changeset.diagnostics` directly. The Markdown renderer shows
short `Warnings` and `Suggestions` sections. Renderer implementations may also
add renderer-specific diagnostics during rendering via the renderer trait hook;
those are merged into a cloned changeset rather than mutating stored JSON.

## Conventions

- `code` values are stable API surface. Prefer `package.topic` naming.
- `message` wording should be suggestive, not scolding.
- `location` should be the logical node path when the advice applies to one
  node; omit it for whole-changeset guidance.
- The initial stdlib code is `binoc.binary-fallback`:
  "Compared as binary; a plugin may provide a more semantic diff."

## Alternatives Considered

## Put diagnostics in tags or annotations

Rejected because tags are for factual semantic classification and annotations
are node-local metadata. Diagnostics are advisory records with severity,
message, and optional location, and they need to survive in JSON output without
being coupled to one node payload shape.

## Let renderers invent diagnostics ad hoc from prose

Rejected because later producers need a structured channel visible in JSON and
available to every renderer. Renderer prose alone would not give downstream
tools a stable machine-readable contract.

## Unlimited diagnostics

Rejected because a plugin could spam low-value advice and bury the changelog.
The controller cap makes diagnostics non-fatal and bounded by default.
