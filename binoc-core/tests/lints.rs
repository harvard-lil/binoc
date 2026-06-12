//! Mechanical lints over binoc-core source (tier 2 — see
//! `binoc_sdk::lints` for the tier overview). Run with warnings visible
//! via `just lint`.

use std::path::Path;

use binoc_sdk::lints::forbid_write_set_reads;

/// The controller and dispatcher must never read write-set declarations
/// (`emits_*`, `publishes_artifacts`): write-sets exist for verification,
/// lint, and capability negotiation — never for scheduling or dispatch.
/// This is the tripwire for the write-sets ADR's hard constraint; if you
/// hit it, the answer is "don't", not an allow comment.
#[test]
fn core_never_reads_write_set_declarations() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    forbid_write_set_reads(&src).assert_clean();
}
