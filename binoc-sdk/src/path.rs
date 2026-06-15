//! Logical-path construction and decomposition.
//!
//! Binoc logical paths use two separators with distinct meaning (see the
//! parsed-children ADR, `docs/adr/2026-06-14-parsed_children_and_decompose_boundaries.md`):
//!
//! - [`MEMBER_SEP`] (`/`) — **membership**: structure that already existed as a
//!   navigable tree (directory entries, paths *inside* an extracted archive). No
//!   format had to be decoded to reveal it.
//! - [`DECOMPOSE_SEP`] (`/>`) — **decompose boundary**: a node binoc had to open
//!   a format to reveal (the immediate members of an archive expansion, and every
//!   parsed table / sheet / section).
//!
//! Segment names beginning with `>` are escaped as `\>` so an ordinary member
//! named `>q1.csv` is written `dir/\>q1.csv`, not `dir/>q1.csv`.
//!
//! The separators are cosmetic: nothing in the engine decides behavior by parsing
//! a path string. Parent/child relationships come from the IR tree, and child
//! kind rides on `ItemRef.projection_hint.item_type`. These helpers exist so that
//! the few places that *do* need to construct or walk a path (expansion,
//! container parsers, projection nesting) share one implementation instead of
//! hand-rolling `format!`/`split` calls.

/// Separator between members of an already-navigable container.
pub const MEMBER_SEP: char = '/';

/// Marker introducing a decompose-boundary child (a node revealed by opening a
/// format). Two characters: a slash followed by `>`.
pub const DECOMPOSE_SEP: &str = "/>";

/// Escape a logical path segment for inclusion after either separator.
///
/// The escape is intentionally tiny: only leading `>` and leading `\` are
/// rewritten, because only the first byte after `/` participates in the
/// decompose-boundary grammar. A literal leading `>` becomes `\>`; a literal
/// leading `\` becomes `\\` so the escaped spelling itself can round-trip.
pub fn escape_segment(name: &str) -> String {
    if name.starts_with(['>', '\\']) {
        format!("\\{name}")
    } else {
        name.to_string()
    }
}

/// Append `name` as an ordinary member of `parent` (the `/` separator).
///
/// Used for directory entries and for structure inside an extracted archive.
/// An empty `parent` yields `name` unchanged (root-level entries).
pub fn member_child(parent: &str, name: &str) -> String {
    let name = escape_segment(name);
    if parent.is_empty() {
        name
    } else {
        format!("{parent}{MEMBER_SEP}{name}")
    }
}

/// Append `name` as a decompose-boundary child of `parent` (the `/>` separator).
///
/// Used for the immediate members of an archive expansion and for parsed
/// table/sheet/section children. `parent` is never meaningfully empty here — a
/// decompose child always hangs off the node whose format was opened.
pub fn decompose_child(parent: &str, name: &str) -> String {
    let name = escape_segment(name);
    format!("{parent}{DECOMPOSE_SEP}{name}")
}

/// The final segment of a logical path, after the last separator of either kind.
pub fn file_name(path: &str) -> &str {
    match path.rfind(MEMBER_SEP) {
        // A `/` immediately followed by `>` is a decompose boundary; the name
        // starts after the `>`. Otherwise it is an ordinary member separator.
        Some(slash) if path[slash + 1..].starts_with('>') => &path[slash + 2..],
        Some(slash) => &path[slash + 1..],
        None => path,
    }
}

/// Walk `path`, yielding `(cumulative_path, segment_name)` for each segment in
/// order. The cumulative path preserves the original separators, so it matches
/// node paths constructed with [`member_child`]/[`decompose_child`] and can be
/// used directly as a projection node key.
///
/// Empty segments (from leading/trailing/doubled separators) are skipped.
pub fn segments(path: &str) -> Vec<(&str, &str)> {
    let bytes = path.as_bytes();
    let mut out = Vec::new();
    let mut seg_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' {
            if i > seg_start {
                out.push((&path[..i], &path[seg_start..i]));
            }
            // Consume the separator: `/>` (decompose) or `/` (member).
            if bytes.get(i + 1) == Some(&b'>') {
                i += 2;
            } else {
                i += 1;
            }
            seg_start = i;
        } else {
            i += 1;
        }
    }
    if seg_start < bytes.len() {
        out.push((path, &path[seg_start..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_and_decompose_construction() {
        assert_eq!(member_child("", "a.txt"), "a.txt");
        assert_eq!(member_child("dir", "a.txt"), "dir/a.txt");
        assert_eq!(decompose_child("data.zip", "inner"), "data.zip/>inner");
        assert_eq!(decompose_child("data.csv", "table_1"), "data.csv/>table_1");
    }

    #[test]
    fn segment_escape_disambiguates_leading_decompose_marker() {
        assert_eq!(escape_segment(">q1.csv"), r"\>q1.csv");
        assert_eq!(escape_segment(r"\>q1.csv"), r"\\>q1.csv");
        assert_eq!(member_child("dir", ">q1.csv"), r"dir/\>q1.csv");
        assert_eq!(member_child("dir", r"\>q1.csv"), r"dir/\\>q1.csv");
        assert_eq!(
            decompose_child("book.xlsx", ">Summary"),
            r"book.xlsx/>\>Summary"
        );
    }

    #[test]
    fn file_name_handles_both_separators() {
        assert_eq!(file_name("a.txt"), "a.txt");
        assert_eq!(file_name("dir/a.txt"), "a.txt");
        assert_eq!(file_name("data.csv/>table_1"), "table_1");
        assert_eq!(file_name(r"dir/\>q1.csv"), r"\>q1.csv");
        assert_eq!(file_name("dir/data.zip/>reports/q1.csv"), "q1.csv");
        assert_eq!(
            file_name("dir/data.zip/>reports/q1.csv/>table_2"),
            "table_2"
        );
    }

    #[test]
    fn segments_preserve_separators_in_cumulative_paths() {
        assert_eq!(segments("a.txt"), vec![("a.txt", "a.txt")]);
        assert_eq!(
            segments("dir/a.txt"),
            vec![("dir", "dir"), ("dir/a.txt", "a.txt")]
        );
        assert_eq!(
            segments("data.csv/>table_1"),
            vec![("data.csv", "data.csv"), ("data.csv/>table_1", "table_1")]
        );
        assert_eq!(
            segments(r"dir/\>q1.csv"),
            vec![("dir", "dir"), (r"dir/\>q1.csv", r"\>q1.csv")]
        );
        assert_eq!(
            segments("dir/data.zip/>reports/q1.csv/>table_2"),
            vec![
                ("dir", "dir"),
                ("dir/data.zip", "data.zip"),
                ("dir/data.zip/>reports", "reports"),
                ("dir/data.zip/>reports/q1.csv", "q1.csv"),
                ("dir/data.zip/>reports/q1.csv/>table_2", "table_2"),
            ]
        );
    }

    #[test]
    fn segments_skip_empty() {
        assert_eq!(segments(""), Vec::<(&str, &str)>::new());
        assert_eq!(segments("/"), Vec::<(&str, &str)>::new());
        assert_eq!(segments("a//b"), vec![("a", "a"), ("a//b", "b")]);
    }
}
