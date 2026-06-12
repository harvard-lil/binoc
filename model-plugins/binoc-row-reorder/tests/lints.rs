//! Mechanical lints (tier 2 — see `binoc_sdk::lints`). The descriptor
//! lint for this plugin's single-producer/single-consumer tag handoff
//! lives in tests/test_vectors.rs next to its allowlist rationale.

use std::path::Path;

use binoc_sdk::lints::forbid_tag_wipes;

#[test]
fn no_wholesale_tag_wipes() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    forbid_tag_wipes(&src).assert_clean();
}
