//! Mechanical lints over stdlib plugins (tier 2 — see `binoc_sdk::lints`
//! for the tier overview): descriptor lints plus source scans. Hard
//! behavioral checks live in `tests/write_sets.rs`; warnings printed here
//! are visible via `just lint`.

use std::path::Path;

use binoc_sdk::lints::{forbid_tag_wipes, lint_transformer_descriptors};

fn src_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// No transformer may clear or overwrite the tag set wholesale — tags are
/// facts owned by whichever plugin set them (the ColumnReorderDetector
/// `tags.clear()` bug class). Targeted `tags.remove(...)` of a fact a
/// transformer is changing remains legitimate.
#[test]
fn no_wholesale_tag_wipes_in_stdlib() {
    forbid_tag_wipes(&src_root()).assert_clean();
}

/// Descriptor lints over the default registry. Errors fail; warnings
/// (e.g. a future single-producer/single-consumer tag) print and pass —
/// hard requirements like "stdlib declares all write-sets" stay in
/// tests/write_sets.rs.
#[test]
fn stdlib_descriptor_lints() {
    let registry = binoc_stdlib::default_registry();
    let descriptors: Vec<_> = registry
        .transformer_names()
        .into_iter()
        .map(|name| registry.get_transformer(&name).unwrap().descriptor())
        .collect();
    lint_transformer_descriptors(&descriptors, &[]).assert_clean();
}
