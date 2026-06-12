---
audience: anyone evaluating binoc
---

# Why binoc exists

Datasets published by governments, research institutions, and public bodies
are *living artifacts*. They get updated, corrected, restructured,
re-licensed, and silently re-exported — often without a changelog, often with
the same filename, often without a version bump.

The communities that consume these datasets — archivists, data scientists,
public-interest researchers, journalists, civic-tech projects — need tooling
that can answer:

- Did this dataset actually change since the last download, or are the bytes
  just different?
- *What* changed? Not "the file changed" but "three columns were reordered
  (clerical) and one column was split into two (substantive)."
- Is this change something I need to act on, or is it housekeeping I can
  ignore?
- Can I get the *new* records out of the diff so I can ingest them into my
  pipeline?

Generic diff tools (`diff`, `git diff`, `cmp`) don't understand data
formats — they work on bytes or lines, not columns or schemas. Version
control systems can detect that a file changed, but they have no idea what
the change *means* in the dataset's terms. Specialized tools exist for
specific formats (SQL schema diffs, Excel comparison utilities, CSV diff
plugins), but they don't compose, and they don't handle the messy reality
of real-world dataset distributions: nested zips, mixed formats inside a
release, renamed files, snapshots downloaded by different people on
different days.

Binoc bridges this gap. It is built around three commitments:

1. **Format-aware where it matters.** A CSV reorder is recognized as a
   reorder, not a rewrite. A row addition is reported as a row addition, not
   as "12 KB of bytes differ." Domain-format support is the whole point.

2. **Pluggable everywhere else.** The standard library handles directories,
   archives, CSVs, and text. Everything else — SQLite, FASTA, Parquet, your
   institution's bespoke binary format — is a [plugin](../../plugin-developers/explanation/plugin-model.md). The
   core engine has no built-in formats.

3. **Significance is the user's call.** Whether a column reorder counts as
   "important" depends on whether you're a data steward (yes, audit trail) or
   a downstream consumer (no, my parser handles it). Binoc separates *what
   changed* (a fact, in the IR) from *what it means* (a judgment, in the
   renderer config). See [Significance classification](significance-classification.md).

## The m × n × o problem

A dataset has *m* formats. A workflow has *n* analyses you want to run
across changes. A team has *o* opinions about what counts as significant.
A monolithic tool would either pick one combination per use-case
(unsustainable) or stitch every combination together (combinatorial
explosion).

Binoc's architecture turns m × n × o into m + n + o:

- *m* comparators parse formats.
- *n* transformers detect cross-cutting patterns.
- *o* renderer configs decide what each pattern means in your domain.

A new format costs one comparator. A new analysis costs one transformer. A
new domain opinion costs zero code — just a config edit. See
[Architecture overview](../../plugin-developers/explanation/architecture.md) for how this plays out concretely.

## Who binoc is for

- **Data stewards and archivists** tracking changes to public datasets they
  ingest periodically.
- **Pipeline integrators** who need a stable, structured changeset feed for
  downstream automation.
- **Domain-format plugin authors** who want to teach binoc about a new file
  type without building a new tool.
- **Core contributors** who care about the architectural commitments above
  and want to extend the engine itself.

If you fit one of those, the [tutorial](../../tutorial.md) is the next stop.
If you want the full design story, see the
[architecture overview](../../plugin-developers/explanation/architecture.md).
