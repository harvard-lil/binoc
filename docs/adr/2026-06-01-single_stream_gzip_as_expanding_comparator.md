# Single-stream gzip as an expanding comparator

**Date:** 2026-06-01
**Status:** Implemented

## Context

Some datasets ship one logical file inside a single-stream compression wrapper,
for example `data.csv.gz` or `census.txt.gz`. Unlike zip or tar, gzip has no
member list to fan out. The useful comparison target is the decompressed byte
stream with the wrapper suffix stripped so existing comparators can dispatch on
the inner name.

The controller must remain type-ignorant, and transformers operate only on the
completed IR with no raw data access. The input loader should not acquire
format knowledge that would bypass the plugin dispatch model.

## Decision

Implement single-stream gzip in `binoc-stdlib` as an expanding comparator,
`binoc.gzip`, ordered after `binoc.tar` and before ordinary file comparators.
It claims `.gz`, streams the decompressed bytes into a `DataAccess` workspace
with a bounded maximum output size, strips only the final `.gz` suffix from the
logical path, and returns one child `ItemPair`. The controller then re-dispatches
that child normally, so `data.csv.gz` is compared as `data.csv`.

Tar remains earlier in the default order so `.tar.gz` continues to use the tar
fan-out path. Gzip does not sniff or reinterpret decompressed content; for
example, `data.csv.gz` redispatches as `data.csv`, while `notes.txt.gz`
redispatches as `notes.txt`.

## Alternatives Considered

**Input-loader decompression.** This would make gzip invisible to the comparator
pipeline, but it would put format knowledge in the host boundary and make
stdlib behavior special relative to third-party plugins. Rejected because it
violates the type-ignorant controller/input boundary.

**Pre-pass transformer.** Transformers see IR after comparison and have no raw
data access by design. Decompression is acquisition/parsing work, not an IR
optimization pass. Rejected.

**Treat gzip like zip.** Zip expansion enumerates members and produces a
directory-shaped subtree. Gzip has exactly one stream and no stable member path,
so the correct child identity is the stripped inner filename, not a synthetic
archive member list.

**Sniff delimited `.txt` content.** Some motivating `.txt.gz` files contain
pipe-delimited tables, but identifying those tables is format-specific parsing,
not compression acquisition. Rejected for this change; generic text table
detection remains a separate follow-up.
