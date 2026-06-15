//! Mechanical lints (tier 2 — see `binoc_sdk::lints`). This file doubles
//! as the reference for plugin authors: add a `tests/lints.rs` like this
//! one, call the lint helpers you care about, and finish each test with
//! `assert_clean()`. Warnings are visible via `just lint`.

use std::path::Path;

use binoc_sdk::lints::forbid_tag_wipes;

#[test]
fn no_wholesale_tag_wipes() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    forbid_tag_wipes(&src).assert_clean();
}
