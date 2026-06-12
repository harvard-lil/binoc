//! Write-set declarations: every stdlib transformer must declare its
//! write-sets, the declarations must hold (enforced per-call by
//! `AbiTransformer` in the test-vector harness), and no tag may quietly
//! become a single-producer/single-consumer dispatch channel.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use binoc_sdk::lints::single_producer_single_consumer_tags;
use binoc_sdk::test_support::{undeclared_emissions, AbiTransformer, WriteFacts};
use binoc_sdk::*;

fn stdlib_descriptors() -> Vec<TransformerDescriptor> {
    let registry = binoc_stdlib::default_registry();
    registry
        .transformer_names()
        .into_iter()
        .map(|name| registry.get_transformer(&name).unwrap().descriptor())
        .collect()
}

/// Every stdlib transformer declares all four write-sets. `None` is the
/// escape hatch for legacy third-party plugins, not for stdlib.
#[test]
fn stdlib_transformers_declare_write_sets() {
    for desc in stdlib_descriptors() {
        assert!(
            desc.emits_tags.is_some()
                && desc.emits_actions.is_some()
                && desc.emits_item_types.is_some()
                && desc.publishes_artifacts.is_some(),
            "stdlib transformer '{}' must declare emits_tags, emits_actions, \
             emits_item_types, and publishes_artifacts",
            desc.name
        );
    }
}

/// No stdlib tag is produced by exactly one transformer and consumed by
/// exactly one other transformer's match_tags. (binoc.cell-change is
/// matched by the out-of-tree binoc-row-reorder plugin, which is fine —
/// renderer group configs also consume it; its own test covers that.)
#[test]
fn no_single_producer_single_consumer_tags_in_stdlib() {
    let warnings = single_producer_single_consumer_tags(&stdlib_descriptors(), &[]);
    assert!(
        warnings.is_empty(),
        "single-producer/single-consumer tags in stdlib:\n  {}",
        warnings.join("\n  ")
    );
}

#[test]
fn lint_flags_single_producer_single_consumer_tag() {
    let producer = TransformerDescriptor::new("test.producer")
        .with_emits_tags(vec!["test.handoff".into(), "test.shared".into()]);
    let consumer =
        TransformerDescriptor::new("test.consumer").with_match_tags(vec!["test.handoff".into()]);
    let other_consumer =
        TransformerDescriptor::new("test.other").with_match_tags(vec!["test.shared".into()]);
    let second_consumer =
        TransformerDescriptor::new("test.second").with_match_tags(vec!["test.shared".into()]);
    let descs = [producer, consumer, other_consumer, second_consumer];

    let warnings = single_producer_single_consumer_tags(&descs, &[]);
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert!(warnings[0].contains("test.handoff"));
    assert!(warnings[0].contains("test.producer"));
    assert!(warnings[0].contains("test.consumer"));

    // Allowlisted tags are not flagged.
    assert!(single_producer_single_consumer_tags(&descs, &["test.handoff"]).is_empty());
}

/// A transformer whose declaration is incomplete: it declares one tag but
/// emits another, and changes the action without declaring it.
struct Misdeclared;

impl Transformer for Misdeclared {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("test.misdeclared")
            .with_match_types(vec!["text".into()])
            .with_emits_tags(vec!["test.declared".into()])
            .with_emits_actions(vec![])
            .with_emits_item_types(vec![])
            .with_publishes_artifacts(vec![])
    }

    fn transform(
        &self,
        mut node: DiffNode,
        _data: &dyn DataAccess,
        _config: &serde_json::Value,
    ) -> TransformResult {
        node.tags.insert("test.declared".into());
        node.tags.insert("test.undeclared".into());
        node.action = "test.surprise".into();
        TransformResult::Replace(Box::new(node))
    }
}

#[test]
fn undeclared_emissions_reports_each_violation() {
    let input = DiffNode::new("modify", "text", "a.txt");
    let input_facts = WriteFacts::from_tree(&input);
    let TransformResult::Replace(output) =
        Misdeclared.transform(input, &LocalDataAccess::new(), &serde_json::Value::Null)
    else {
        panic!("expected Replace");
    };

    let violations = undeclared_emissions(&Misdeclared.descriptor(), &input_facts, &[&output]);
    assert_eq!(violations.len(), 2, "violations: {violations:?}");
    assert!(violations
        .iter()
        .any(|v| v.contains("test.misdeclared") && v.contains("tag 'test.undeclared'")));
    assert!(violations
        .iter()
        .any(|v| v.contains("test.misdeclared") && v.contains("action 'test.surprise'")));
}

/// Tags already present on the input do not count as emissions, and a
/// legacy descriptor (nothing declared) is exempt from enforcement.
#[test]
fn undeclared_emissions_ignores_preexisting_and_legacy() {
    let input = DiffNode::new("modify", "text", "a.txt").with_tag("test.preexisting");
    let input_facts = WriteFacts::from_tree(&input);
    let output = input.clone().with_tag("test.undeclared");

    let declared =
        TransformerDescriptor::new("test.declared").with_emits_tags(vec!["test.undeclared".into()]);
    assert!(undeclared_emissions(&declared, &input_facts, &[&output]).is_empty());

    let legacy = TransformerDescriptor::new("test.legacy");
    assert!(undeclared_emissions(&legacy, &input_facts, &[&output]).is_empty());
}

/// The harness path: AbiTransformer fails the test (panics) when a
/// transformer emits outside its declared write-set, naming the
/// transformer and the emission.
#[test]
fn abi_transformer_enforces_write_sets() {
    let wrapped = AbiTransformer::new(Misdeclared, Arc::new(AtomicU64::new(0)));
    let node = DiffNode::new("modify", "text", "a.txt");
    let data = LocalDataAccess::new();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wrapped.transform(node, &data, &serde_json::Value::Null);
    }))
    .expect_err("undeclared emission must fail the harness");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| panic.downcast_ref::<&str>().unwrap_or(&"").to_string());
    assert!(
        message.contains("test.misdeclared") && message.contains("tag 'test.undeclared'"),
        "panic message should name the transformer and emission: {message}"
    );
}
