//! Config-declared file correspondence.
//!
//! This root-scope pass runs before heuristic correlation. It pairs residual
//! add/remove file leaves by user-declared identity rules and asks the
//! controller to re-dispatch each pair through the normal comparator pipeline.

use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;
use regex::Regex;

use super::correlation::{apply_rewrite, parent_path_of, RewritePlan};

pub struct DeclaredCorrespondence;

impl Transformer for DeclaredCorrespondence {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.declared_correspondence")
            .with_node_shape(NodeShapeFilter::Root)
            .with_emits_tags(vec![
                "binoc.declared-correspondence".into(),
                "binoc.path-change".into(),
            ])
            .with_emits_actions(vec!["move".into(), "modify".into()])
            .with_emits_item_types(vec![])
            .with_publishes_artifacts(vec![])
    }

    fn transform(
        &self,
        mut node: DiffNode,
        _data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let dataset = dataset_config(config);
        let semantics: DatasetSemanticsV1 =
            serde_json::from_value(dataset.clone()).unwrap_or_default();
        if semantics.files.correspondences.is_empty() {
            return TransformResult::Unchanged;
        }

        let mut adds = Vec::new();
        let mut removes = Vec::new();
        collect_file_leaves(&node, &mut adds, &mut removes);
        if adds.is_empty() || removes.is_empty() {
            return TransformResult::Unchanged;
        }

        let container_paths = collect_container_paths(&node);
        let mut used_adds = BTreeSet::new();
        let mut used_removes = BTreeSet::new();
        let mut plan = RewritePlan::default();

        for rule in &semantics.files.correspondences {
            let mut ctx = RuleContext {
                adds: &adds,
                removes: &removes,
                container_paths: &container_paths,
                used_adds: &mut used_adds,
                used_removes: &mut used_removes,
                plan: &mut plan,
                diagnostics_node: &mut node,
            };
            apply_rule(rule, &mut ctx);
        }

        if plan.is_empty() {
            if node.diagnostics.is_empty() {
                TransformResult::Unchanged
            } else {
                TransformResult::Replace(Box::new(node))
            }
        } else {
            TransformResult::Replace(Box::new(apply_rewrite(node, &plan)))
        }
    }
}

fn dataset_config(config: &serde_json::Value) -> &serde_json::Value {
    config.get("dataset").unwrap_or(config)
}

#[derive(Debug, Clone)]
struct FileLeaf {
    path: String,
    item_type: String,
    item: ItemRef,
}

fn collect_file_leaves(node: &DiffNode, adds: &mut Vec<FileLeaf>, removes: &mut Vec<FileLeaf>) {
    if node.children.is_empty() {
        if let Some(leaf) = as_file_leaf(node) {
            match node.action.as_str() {
                "add" => adds.push(leaf),
                "remove" => removes.push(leaf),
                _ => {}
            }
        }
        return;
    }
    for child in &node.children {
        collect_file_leaves(child, adds, removes);
    }
}

fn as_file_leaf(node: &DiffNode) -> Option<FileLeaf> {
    let pair = node.source_items.as_ref()?;
    let item = match node.action.as_str() {
        "add" => pair.right.clone()?,
        "remove" => pair.left.clone()?,
        _ => return None,
    };
    if item.is_dir {
        return None;
    }
    Some(FileLeaf {
        path: node.path.clone(),
        item_type: node.item_type.clone(),
        item,
    })
}

fn collect_container_paths(root: &DiffNode) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_container_paths_inner(root, &mut out);
    out
}

fn collect_container_paths_inner(node: &DiffNode, out: &mut BTreeSet<String>) {
    if !node.children.is_empty() {
        out.insert(node.path.clone());
    }
    for child in &node.children {
        collect_container_paths_inner(child, out);
    }
}

struct RuleContext<'a> {
    adds: &'a [FileLeaf],
    removes: &'a [FileLeaf],
    container_paths: &'a BTreeSet<String>,
    used_adds: &'a mut BTreeSet<usize>,
    used_removes: &'a mut BTreeSet<usize>,
    plan: &'a mut RewritePlan,
    diagnostics_node: &'a mut DiffNode,
}

fn apply_rule(rule: &FileCorrespondenceRule, ctx: &mut RuleContext<'_>) {
    if rule.cardinality != Cardinality::OneToOne {
        push_identity_diagnostic(
            ctx.diagnostics_node,
            IdentityFailurePolicy::Diagnostic,
            "binoc.declared_correspondence.unsupported_cardinality",
            format!(
                "File correspondence rule '{}' requested unsupported cardinality",
                rule.name
            ),
            None,
        );
        return;
    }

    let Ok(left_selector) = compile_selector(&rule.left) else {
        push_identity_diagnostic(
            ctx.diagnostics_node,
            IdentityFailurePolicy::Diagnostic,
            "binoc.declared_correspondence.invalid_left_regex",
            format!(
                "File correspondence rule '{}' has an invalid left path_regex",
                rule.name
            ),
            None,
        );
        return;
    };
    let Ok(right_selector) = compile_selector(&rule.right) else {
        push_identity_diagnostic(
            ctx.diagnostics_node,
            IdentityFailurePolicy::Diagnostic,
            "binoc.declared_correspondence.invalid_right_regex",
            format!(
                "File correspondence rule '{}' has an invalid right path_regex",
                rule.name
            ),
            None,
        );
        return;
    };

    let left_coverage = selector_coverage(&left_selector, ctx.removes, ctx.container_paths);
    let right_coverage = selector_coverage(&right_selector, ctx.adds, ctx.container_paths);
    if !left_coverage.matched_file || !right_coverage.matched_file {
        report_unmatched_rule(rule, &left_coverage, &right_coverage, ctx.diagnostics_node);
        return;
    }

    let left = build_index(
        rule,
        Side::Left,
        ctx.removes,
        ctx.used_removes,
        &left_selector,
        ctx.diagnostics_node,
    );
    let right = build_index(
        rule,
        Side::Right,
        ctx.adds,
        ctx.used_adds,
        &right_selector,
        ctx.diagnostics_node,
    );

    for key in left.keys().filter(|key| right.contains_key(*key)) {
        let left_matches = &left[key];
        let right_matches = &right[key];
        if left_matches.len() != 1 || right_matches.len() != 1 {
            report_duplicate(
                rule,
                key,
                left_matches.len(),
                right_matches.len(),
                ctx.diagnostics_node,
            );
            continue;
        }

        let remove_idx = left_matches[0].index;
        let add_idx = right_matches[0].index;
        if ctx.used_removes.contains(&remove_idx) || ctx.used_adds.contains(&add_idx) {
            continue;
        }

        let remove = &ctx.removes[remove_idx];
        let add = &ctx.adds[add_idx];
        let logical_path = rule
            .logical_path
            .as_ref()
            .and_then(|template| expand_template(template, &right_matches[0].captures))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| add.path.clone());
        let node = build_correspondence_node(rule, key, remove, add, &logical_path);
        let parent = insertion_parent(ctx.container_paths, parent_path_of(&logical_path));

        ctx.plan.schedule_remove(&remove.path);
        ctx.plan.schedule_remove(&add.path);
        ctx.plan.schedule_insert(parent, node);
        ctx.used_removes.insert(remove_idx);
        ctx.used_adds.insert(add_idx);
    }
}

#[derive(Debug)]
struct CompiledSelector {
    path: Option<String>,
    path_regex: Option<Regex>,
}

fn compile_selector(selector: &FileSelector) -> Result<CompiledSelector, regex::Error> {
    Ok(CompiledSelector {
        path: selector.path.clone(),
        path_regex: selector.path_regex.as_deref().map(Regex::new).transpose()?,
    })
}

#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

impl Side {
    fn label(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// What a side's selector matched among residual leaves, used to warn when a
/// rule is ineffective. `container` is only probed when no leaf matched, so a
/// rule whose regex targets an archive (which Phase 1 always expands) gets a
/// pointed "containers are unsupported" message instead of a generic one.
struct SelectorCoverage {
    matched_file: bool,
    container: Option<String>,
}

fn selector_coverage(
    selector: &CompiledSelector,
    leaves: &[FileLeaf],
    container_paths: &BTreeSet<String>,
) -> SelectorCoverage {
    let matched_file = leaves
        .iter()
        .any(|leaf| match_path(selector, &leaf.path).is_some());
    let container = if matched_file {
        None
    } else {
        container_paths
            .iter()
            .find(|path| !path.is_empty() && match_path(selector, path).is_some())
            .cloned()
    };
    SelectorCoverage {
        matched_file,
        container,
    }
}

fn report_unmatched_rule(
    rule: &FileCorrespondenceRule,
    left: &SelectorCoverage,
    right: &SelectorCoverage,
    diagnostics_node: &mut DiffNode,
) {
    let mut clauses = Vec::new();
    let mut matched_container = false;
    for (side, coverage, kind) in [(Side::Left, left, "removed"), (Side::Right, right, "added")] {
        if coverage.matched_file {
            continue;
        }
        match &coverage.container {
            Some(path) => {
                matched_container = true;
                clauses.push(format!(
                    "the {} selector matched only the container '{}'",
                    side.label(),
                    path
                ));
            }
            None => clauses.push(format!(
                "the {} selector matched no {} files",
                side.label(),
                kind
            )),
        }
    }
    let (code, hint) = if matched_container {
        (
            "binoc.declared_correspondence.container_unsupported",
            "; correspondences between containers are not supported, so consider declaring them for the files inside instead",
        )
    } else {
        ("binoc.declared_correspondence.no_matching_files", "")
    };
    diagnostics_node.push_diagnostic(Diagnostic::warning(
        code,
        format!(
            "File correspondence rule '{}' had no effect: {}{}",
            rule.name,
            clauses.join(" and "),
            hint
        ),
    ));
}

#[derive(Debug, Clone)]
struct KeyedMatch {
    index: usize,
    captures: BTreeMap<String, String>,
}

fn build_index(
    rule: &FileCorrespondenceRule,
    side: Side,
    leaves: &[FileLeaf],
    used: &BTreeSet<usize>,
    selector: &CompiledSelector,
    diagnostics_node: &mut DiffNode,
) -> BTreeMap<String, Vec<KeyedMatch>> {
    let mut out: BTreeMap<String, Vec<KeyedMatch>> = BTreeMap::new();
    for (index, leaf) in leaves.iter().enumerate() {
        if used.contains(&index) {
            continue;
        }
        let Some(captures) = match_path(selector, &leaf.path) else {
            continue;
        };
        let Some(key) = expand_template(&rule.key, &captures).filter(|s| !s.is_empty()) else {
            handle_null_key(rule, side, &leaf.path, diagnostics_node);
            continue;
        };
        out.entry(key)
            .or_default()
            .push(KeyedMatch { index, captures });
    }
    out.retain(|key, matches| {
        if matches.len() > 1 {
            report_side_duplicate(rule, side, key, matches.len(), diagnostics_node);
            false
        } else {
            true
        }
    });
    out
}

fn match_path(selector: &CompiledSelector, path: &str) -> Option<BTreeMap<String, String>> {
    if selector
        .path
        .as_deref()
        .is_some_and(|expected| expected != path)
    {
        return None;
    }
    let Some(regex) = &selector.path_regex else {
        return Some(BTreeMap::new());
    };
    let captures = regex.captures(path)?;
    let mut out = BTreeMap::new();
    for name in regex.capture_names().flatten() {
        if let Some(value) = captures.name(name) {
            out.insert(name.to_string(), value.as_str().to_string());
        }
    }
    Some(out)
}

fn expand_template(template: &str, captures: &BTreeMap<String, String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let end = after_start.find('}')?;
        let name = &after_start[..end];
        let value = captures.get(name)?;
        out.push_str(value);
        rest = &after_start[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn handle_null_key(
    rule: &FileCorrespondenceRule,
    side: Side,
    path: &str,
    diagnostics_node: &mut DiffNode,
) {
    match rule.on_null_key {
        IdentityFailurePolicy::Ignore => {}
        IdentityFailurePolicy::Diagnostic | IdentityFailurePolicy::Error => {
            push_identity_diagnostic(
                diagnostics_node,
                rule.on_null_key,
                "binoc.declared_correspondence.null_key",
                format!(
                    "File correspondence rule '{}' produced no key for {} path '{}'",
                    rule.name,
                    side.label(),
                    path
                ),
                Some(path),
            );
        }
    }
}

fn report_duplicate(
    rule: &FileCorrespondenceRule,
    key: &str,
    left_count: usize,
    right_count: usize,
    diagnostics_node: &mut DiffNode,
) {
    match rule.on_duplicate_key {
        IdentityFailurePolicy::Ignore => {}
        IdentityFailurePolicy::Diagnostic | IdentityFailurePolicy::Error => {
            push_identity_diagnostic(
                diagnostics_node,
                rule.on_duplicate_key,
                "binoc.declared_correspondence.duplicate_key",
                format!(
                    "File correspondence rule '{}' has ambiguous key '{}' (left matches: {}, right matches: {})",
                    rule.name, key, left_count, right_count
                ),
                None,
            );
        }
    }
}

fn report_side_duplicate(
    rule: &FileCorrespondenceRule,
    side: Side,
    key: &str,
    count: usize,
    diagnostics_node: &mut DiffNode,
) {
    match rule.on_duplicate_key {
        IdentityFailurePolicy::Ignore => {}
        IdentityFailurePolicy::Diagnostic | IdentityFailurePolicy::Error => {
            push_identity_diagnostic(
                diagnostics_node,
                rule.on_duplicate_key,
                "binoc.declared_correspondence.duplicate_key",
                format!(
                    "File correspondence rule '{}' has duplicate {} key '{}' (matches: {})",
                    rule.name,
                    side.label(),
                    key,
                    count
                ),
                None,
            );
        }
    }
}

fn push_identity_diagnostic(
    node: &mut DiffNode,
    policy: IdentityFailurePolicy,
    code: impl Into<String>,
    message: impl Into<String>,
    location: Option<&str>,
) {
    let diagnostic = match policy {
        IdentityFailurePolicy::Error => Diagnostic::error(code, message),
        IdentityFailurePolicy::Diagnostic => Diagnostic::warning(code, message),
        IdentityFailurePolicy::Ignore => return,
    };
    node.push_diagnostic(match location {
        Some(path) => diagnostic.with_location(path),
        None => diagnostic,
    });
}

fn build_correspondence_node(
    rule: &FileCorrespondenceRule,
    key: &str,
    remove: &FileLeaf,
    add: &FileLeaf,
    logical_path: &str,
) -> DiffNode {
    let mut left = remove.item.clone();
    let mut right = add.item.clone();
    left.logical_path = logical_path.to_string();
    right.logical_path = logical_path.to_string();

    let action = if rule.report_path_change {
        "move"
    } else {
        "modify"
    };
    let mut node = DiffNode::new(action, &add.item_type, logical_path)
        .with_tag("binoc.declared-correspondence")
        .with_detail("correspondence_rule", serde_json::json!(&rule.name))
        .with_detail("correspondence_key", serde_json::json!(key))
        .with_detail("source_path", serde_json::json!(remove.path))
        .with_detail("destination_path", serde_json::json!(add.path));
    if rule.report_path_change {
        node.source_path = Some(remove.path.clone());
        node.summary = Some(
            Summary::new()
                .text("Moved from ")
                .path(&remove.path, binoc_sdk::Side::From),
        );
        node.tags.insert("binoc.path-change".into());
    }
    node.pending_recompare = Some(ItemPair::both(left, right));
    node
}

fn insertion_parent<'a>(container_paths: &'a BTreeSet<String>, parent: &'a str) -> &'a str {
    let mut current = parent;
    loop {
        if container_paths.contains(current) {
            return current;
        }
        if current.is_empty() {
            return "";
        }
        current = parent_path_of(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_core::data_access::LocalDataAccess;

    fn file_item(path: &str) -> ItemRef {
        ItemRef {
            logical_path: path.to_string(),
            is_dir: false,
            content_hash: None,
            size: None,
            media_type: None,
            handle: String::new(),
        }
    }

    /// Root tree with two one-sided, already-expanded archives: data.zip
    /// removed and archive.zip added, each holding one CSV leaf.
    fn expanded_archive_fixture() -> DiffNode {
        let mut removed_csv = DiffNode::new("remove", "file", "data.zip/file.csv");
        removed_csv.source_items = Some(ItemPair::removed(file_item("data.zip/file.csv")));
        let mut removed_zip = DiffNode::new("remove", "zip_archive", "data.zip");
        removed_zip.source_items = Some(ItemPair::removed(file_item("data.zip")));
        removed_zip.children.push(removed_csv);

        let mut added_csv = DiffNode::new("add", "file", "archive.zip/file.csv");
        added_csv.source_items = Some(ItemPair::added(file_item("archive.zip/file.csv")));
        let mut added_zip = DiffNode::new("add", "zip_archive", "archive.zip");
        added_zip.source_items = Some(ItemPair::added(file_item("archive.zip")));
        added_zip.children.push(added_csv);

        let mut root = DiffNode::new("modify", "directory", "");
        root.children.push(removed_zip);
        root.children.push(added_zip);
        root
    }

    fn correspondence_config(left_regex: &str, right_regex: &str) -> serde_json::Value {
        serde_json::json!({
            "dataset": {
                "files": {
                    "correspondences": [{
                        "name": "archive-pair",
                        "key": "archive",
                        "left": { "path_regex": left_regex },
                        "right": { "path_regex": right_regex }
                    }]
                }
            }
        })
    }

    #[test]
    fn rule_matching_only_containers_warns_unsupported() {
        let data = LocalDataAccess::new();
        let config = correspondence_config("^data\\.zip$", "^archive\\.zip$");

        let result = DeclaredCorrespondence.transform(expanded_archive_fixture(), &data, &config);

        let TransformResult::Replace(node) = result else {
            panic!("expected node replacement carrying diagnostics");
        };
        assert_eq!(node.children.len(), 2, "tree should be left untouched");
        assert_eq!(node.diagnostics.len(), 1);
        let diagnostic = &node.diagnostics[0];
        assert_eq!(
            diagnostic.code,
            "binoc.declared_correspondence.container_unsupported"
        );
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert!(
            diagnostic.message.contains("'data.zip'")
                && diagnostic.message.contains("'archive.zip'"),
            "message should name both containers: {}",
            diagnostic.message
        );
    }

    #[test]
    fn rule_matching_nothing_warns_no_matching_files() {
        let data = LocalDataAccess::new();
        let config = correspondence_config("^missing\\.csv$", "^archive\\.zip/file\\.csv$");

        let result = DeclaredCorrespondence.transform(expanded_archive_fixture(), &data, &config);

        let TransformResult::Replace(node) = result else {
            panic!("expected node replacement carrying diagnostics");
        };
        assert_eq!(node.diagnostics.len(), 1);
        let diagnostic = &node.diagnostics[0];
        assert_eq!(
            diagnostic.code,
            "binoc.declared_correspondence.no_matching_files"
        );
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert!(
            diagnostic.message.contains("no removed files"),
            "message should say the left side matched nothing: {}",
            diagnostic.message
        );
    }

    #[test]
    fn rule_matching_leaves_on_both_sides_does_not_warn() {
        let data = LocalDataAccess::new();
        let config = correspondence_config("^data\\.zip/file\\.csv$", "^archive\\.zip/file\\.csv$");

        let result = DeclaredCorrespondence.transform(expanded_archive_fixture(), &data, &config);

        let TransformResult::Replace(node) = result else {
            panic!("expected node replacement with the paired leaves");
        };
        assert!(
            node.diagnostics.is_empty(),
            "effective rule should not warn: {:?}",
            node.diagnostics
        );
    }

    #[test]
    fn template_expands_named_captures() {
        let captures = BTreeMap::from([
            ("table".to_string(), "records".to_string()),
            ("year".to_string(), "2026".to_string()),
        ]);
        assert_eq!(
            expand_template("${table}:${year}", &captures).as_deref(),
            Some("records:2026")
        );
    }

    #[test]
    fn template_missing_capture_is_null() {
        let captures = BTreeMap::from([("table".to_string(), "records".to_string())]);
        assert_eq!(expand_template("${table}:${year}", &captures), None);
    }
}
