# Published artifacts for cross-plugin composition

**Date:** 2026-03-19
**Status:** Implemented

## Context

Binoc has two different needs that are easy to blur together:

1. **Private reuse of expensive work.** A comparator may parse source data into an intermediate form that it or a closely related transformer wants to reuse.
2. **Cross-plugin composition.** A downstream transformer may want to operate on a semantic shape of data (for example tabular or relational schema) without knowing how to parse every source format that can produce that shape.

The old "cross-phase cache" framing was adequate for the first need but too weak for the second. A cache suggests an implementation detail: one plugin writes data somewhere and another plugin opportunistically finds it. That does not give a clean composition story, especially across the native plugin boundary where the controller and plugins exchange JSON requests and may not share an address space. A good design should handle both needs with one mechanism rather than maintaining separate infrastructure for private reuse and cross-plugin composition.

Using `DiffNode.source_items` as the main fallback keeps correctness simple, but if that becomes the real interoperability contract then every generic transformer must learn every source format it wants to support. That is a poor fit for composability: a "tabular" transformer should not need CSV, Parquet, Excel, and SQLite readers embedded in itself just to be generally useful.

Attaching arbitrary payloads directly to `DiffNode` or ABI messages was also considered. This keeps discovery explicit, but it overloads the IR/message path with working data, encourages bulky transports, and makes it hard to distinguish a stable cross-plugin contract from a plugin's internal scratch data.

The core architectural rule still stands: the controller remains type-ignorant. It must not gain built-in knowledge of tabular data, relational schema, or any other domain shape.

## Decision

Binoc introduces **artifacts** as the unified mechanism for both private reuse and cross-plugin composition.

### Artifacts replace the old cache concept

A comparator or transformer may publish zero or more **artifacts** describing derived data for a node. Every artifact uses the same infrastructure: a format id, a subject, and an SDK-managed handle. There is no separate "cache" system.

An artifact whose format id is documented and stable is a **public artifact** — the cross-plugin composition contract. An artifact whose format id is undocumented or plugin-internal is a **private artifact** — used for the producer's own optimization (passing between comparators and transformers defined by the same plugin) or for tightly coupled plugins that explicitly agree on a convention. Like private methods in python, this is a semantic distinction, not a technical one -- they use the same storage and API as public ones.

Each published artifact has:

- a **format** — a structured tuple of `(package, name, version)` such as `("binoc", "tabular", 1)` or `("binoc-sql", "relational-schema", 1)`. Serialized as a JSON object `{"package": "binoc", "name": "tabular", "version": 1}`.
- a **subject**: `left`, `right`, or `pair`
- an SDK-managed **handle** to the stored bytes or stored bundle
- a **producer** plugin name for provenance/debugging

The controller remains type-ignorant. It does not deserialize artifacts. It only carries artifact descriptors and handles through the pipeline.

### Artifact formats are structured and package-qualified

An artifact format is a structured tuple `(package, name, version)` rather than a dotted string. This avoids ambiguous parsing across language boundaries and makes each component independently queryable.

The **package** field is a package name resolvable through the language's normal package system (`import binoc`, `cargo add binoc-csv`, etc.). This is not just a naming convention — it is a dependency coordinate.

- `("binoc", "tabular", 1)` -> the `binoc` package (the SDK) defines and owns this format.
- `("binoc-csv", "table", 1)` -> the `binoc-csv` package defines and owns this format.
- `("acme-parquet", "columnar", 1)` -> the `acme-parquet` package defines and owns this format.

Package resolution provides namespacing. Two unrelated packages cannot collide on format names unless they collide on package names, which the package manager already prevents. Given a format, a developer can mechanically determine which package to depend on to get the codec.

### Version is a single integer

The **version** field is a single integer, not semver. Bump it only for breaking schema changes — when consumers of the old version cannot read the new format. Adding optional fields to an existing version does not require a bump: JSON's natural forward-compatibility (unknown fields are ignored, missing fields get defaults) handles compatible extensions transparently. There is no "patch version" for a data format.

### Artifact types are schema/version identifiers, not host-language types

Across ABI and language boundaries, the meaningful type is the versioned format tuple, not a Rust type shared in memory. A consumer knows how to read `("binoc", "tabular", 1)` because it depends on the `binoc` package, which defines that format's schema and its encoder/decoder.

This means artifact composition is schema-first:

- standard formats are defined in the `binoc` package (the SDK) so they are available to all plugins without extra dependencies
- plugin-owned formats are defined in the package that introduces them
- private formats use the same artifact infrastructure but carry no cross-plugin stability guarantee
- versioning is a single integer in the format tuple — bump for breaking changes only

### Codec sharing uses normal package dependencies, not runtime plugin discovery

Binoc uses Python entry points to discover executable plugins. Published artifact codecs are a different problem and are not discovered that way.

When two plugins share an artifact format, they depend on the package that owns the format's namespace. For standard formats like `("binoc", "tabular", 1)`, that package is the SDK itself — every plugin already depends on it. For plugin-owned formats like `("binoc-csv", "table", 1)`, consumers add a dependency on `binoc-csv`.

The owning package defines:

- the format id
- the schema version
- encoding and decoding helpers
- test vectors (to allow creation of compatible libraries for other languages)

This avoids runtime "find another plugin that can decode this" behavior. Consumers decode formats themselves using the owning package as a library. The controller does not broker codec lookup.

### Routing may depend on artifact availability

Artifacts are usable for declarative routing. A transformer that consumes `binoc.tabular.v1` can be invoked only when that artifact is available for the current node, rather than parsing source data speculatively.

Source access remains an escape hatch for cases where:

- no suitable artifact is present
- reparsing is genuinely cheaper or simpler
- the transformer is intentionally source-format-aware

But source parsing is not the primary cross-plugin composition story.

### Multiple artifacts are allowed, but the public surface stays tight

Comparators and transformers may publish more than one artifact, but Binoc standardizes the public shape as:

- zero or more published artifacts
- at most one published artifact per `(subject, format id)`

If an artifact needs multiple files internally (for example data plus an index), that is represented as one artifact handle pointing to a bundle or manifest managed by the SDK. Ad hoc public names like `"data"`, `"tables"`, and `"index"` are not the compositional interface.

### When to standardize a format in the SDK

Because format packages are namespace-qualified, a format with package `"binoc"` is available to every plugin with no extra dependency. When multiple unrelated producers and consumers converge on the same data shape, that format should be defined in the `binoc` package (the SDK) so consumers do not accumulate unnecessary cross-plugin dependencies.

Plugin-owned formats are the default starting point. A format should only be promoted to the `binoc` package when there are concrete producer/consumer pairs across multiple packages that would benefit. Premature standardization is worse than a plugin-owned format that proves its worth first.

### Artifacts are transient session data

Published artifacts are not serialized into the changeset JSON. Like `source_items`, they are session-scoped working data. Extract can regenerate them by replaying the compare/reopen chain for the target node.

## Consequences

- **Composable generic transformers.** A transformer can target semantic shapes such as tabular or relational schema without embedding every source parser.
- **Controller stays ignorant.** The host only moves descriptors and opaque handles, preserving the core/plugin boundary.
- **Better declarative routing.** Artifact presence can become part of transformer applicability.
- **No hidden scratch conventions.** Consumers no longer need to guess where another plugin stored data.
- **Versioned contracts instead of in-memory typing.** Cross-plugin type safety is expressed as schema/version compatibility rather than shared host-language types.
- **Format packages are dependency coordinates.** Given a format's `package` field, a developer can mechanically determine which package to install to get the codec. No registry or discovery protocol needed.
- **A new standardization burden.** Shared formats such as `("binoc", "tabular", 1)` must be designed carefully and should only be standardized when there are real producer/consumer pairs.
- **One mechanism, not two.** Private reuse and cross-plugin composition use the same artifact infrastructure. The difference is whether the format id carries a stability guarantee, not whether a different storage system is involved.

## Alternatives Considered

**Source access as the main interoperability contract.** Simpler and already available, but it makes generic transformers responsible for learning every source format they want to support.

**Universal typed cache in the controller/SDK.** Attractive in-process, but the "real" boundary is JSON/ABI, so the durable type is a versioned schema, not a shared in-memory type. A universal typed object cache would leak false assumptions about address space and language.

**One payload slot attached directly to the node.** Simple protocol, but too restrictive for cases that legitimately need multiple derived views, and it still pushes working data into the IR/message path.

**Arbitrary named payloads (`data`, `tables`, `index`, ...).** Flexible but underspecified. It replaces a clean contract with a bag of ad hoc conventions and makes standardization difficult.

**Runtime discovery of artifact codecs via plugin entry points.** Rejected because codec sharing is better modeled as ordinary package dependencies. Runtime discovery is appropriate for executable plugins, not for low-level data readers/writers used in the hot path.
