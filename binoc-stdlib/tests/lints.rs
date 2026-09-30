//! Tier-2 mechanical lints for the standard correspondence rule pack.

use std::path::Path;

use binoc_sdk::lints::{forbid_tag_wipes, lint_pair_descriptors};
use binoc_sdk::CoreRule;

#[test]
fn no_wholesale_tag_wipes() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    forbid_tag_wipes(&src).assert_clean();
}

#[test]
fn pair_descriptors_are_honest() {
    let descriptors = binoc_stdlib::correspondence::default_engine_config()
        .rules
        .into_iter()
        .filter_map(|rule| match rule {
            CoreRule::Pair(rule) => Some(rule.descriptor()),
            CoreRule::Expand(_) | CoreRule::Parse(_) => None,
        })
        .collect::<Vec<_>>();
    lint_pair_descriptors(&descriptors).assert_clean();
}
