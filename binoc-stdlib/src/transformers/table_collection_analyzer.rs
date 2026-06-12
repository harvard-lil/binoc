use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::*;

/// Analyzes `tabular_collection_v1` manifests to report table additions,
/// removals, and changed table members without knowing the source format.
pub struct TableCollectionAnalyzer;

impl Transformer for TableCollectionAnalyzer {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.table_collection_analyzer")
            .with_match_artifacts(vec![tabular_collection_v1()])
            .with_emits_tags(vec![
                "binoc.tabular-collection-change".into(),
                "binoc.table-addition".into(),
                "binoc.table-removal".into(),
                "binoc.table-change".into(),
                "binoc.table-identity-ambiguous".into(),
            ])
            .with_emits_actions(vec![])
            .with_emits_item_types(vec![])
            .with_publishes_artifacts(vec![])
    }

    fn transform(
        &self,
        mut node: DiffNode,
        data: &dyn DataAccess,
        _config: &serde_json::Value,
    ) -> TransformResult {
        let Some(pair) = TabularCollectionDataPair::from_artifacts(&node, data) else {
            return TransformResult::Unchanged;
        };

        let left = pair.left.as_ref().map(member_map).unwrap_or_default();
        let right = pair.right.as_ref().map(member_map).unwrap_or_default();
        let duplicate_names = duplicate_logical_names(pair.left.as_ref())
            .into_iter()
            .chain(duplicate_logical_names(pair.right.as_ref()))
            .collect::<BTreeSet<_>>();

        if !duplicate_names.is_empty() {
            node.tags.insert("binoc.table-identity-ambiguous".into());
            node.details.insert(
                "ambiguous_tables".into(),
                serde_json::json!(duplicate_names.into_iter().collect::<Vec<_>>()),
            );
        }

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();

        for name in right.keys() {
            if !left.contains_key(name) {
                added.push(name.clone());
            }
        }
        for name in left.keys() {
            if !right.contains_key(name) {
                removed.push(name.clone());
            }
        }
        for (name, left_member) in &left {
            if let Some(right_member) = right.get(name) {
                if table_changed(left_member, right_member, &node.children) {
                    changed.push(name.clone());
                }
            }
        }

        tag_children(
            &mut node.children,
            &added,
            &removed,
            &changed,
            &left,
            &right,
        );

        if added.is_empty() && removed.is_empty() && changed.is_empty() {
            if node.tags.contains("binoc.table-identity-ambiguous") {
                return TransformResult::Replace(Box::new(node));
            }
            return TransformResult::Unchanged;
        }

        node.tags.insert("binoc.tabular-collection-change".into());
        if !added.is_empty() {
            node.tags.insert("binoc.table-addition".into());
            node.details
                .insert("tables_added".into(), serde_json::json!(added));
        }
        if !removed.is_empty() {
            node.tags.insert("binoc.table-removal".into());
            node.details
                .insert("tables_removed".into(), serde_json::json!(removed));
        }
        if !changed.is_empty() {
            node.tags.insert("binoc.table-change".into());
            node.details
                .insert("tables_changed".into(), serde_json::json!(changed));
        }

        node.summary = Some(collection_summary(&node.children, &left, &right));

        TransformResult::Replace(Box::new(node))
    }
}

fn member_map(collection: &TabularCollectionData) -> BTreeMap<String, &TableMember> {
    collection
        .tables
        .iter()
        .map(|member| (member.logical_name.clone(), member))
        .collect()
}

fn duplicate_logical_names(collection: Option<&TabularCollectionData>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    if let Some(collection) = collection {
        for member in &collection.tables {
            if !seen.insert(member.logical_name.clone()) {
                duplicates.insert(member.logical_name.clone());
            }
        }
    }
    duplicates
}

fn table_changed(left: &TableMember, right: &TableMember, children: &[DiffNode]) -> bool {
    if left.shape != right.shape {
        return true;
    }
    children.iter().any(|child| {
        child.action != "identical"
            && (child.path == left.node_path
                || child.path == right.node_path
                || child
                    .details
                    .get("logical_name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|name| name == left.logical_name))
    })
}

fn tag_children(
    children: &mut [DiffNode],
    added: &[String],
    removed: &[String],
    changed: &[String],
    left: &BTreeMap<String, &TableMember>,
    right: &BTreeMap<String, &TableMember>,
) {
    let added: BTreeSet<&str> = added.iter().map(String::as_str).collect();
    let removed: BTreeSet<&str> = removed.iter().map(String::as_str).collect();
    let changed: BTreeSet<&str> = changed.iter().map(String::as_str).collect();

    for child in children {
        let logical_name = logical_name_for_child(child, left, right);
        let Some(logical_name) = logical_name else {
            continue;
        };
        child
            .details
            .entry("logical_name".into())
            .or_insert_with(|| serde_json::json!(logical_name));

        if added.contains(logical_name.as_str()) {
            child.tags.insert("binoc.table-addition".into());
        }
        if removed.contains(logical_name.as_str()) {
            child.tags.insert("binoc.table-removal".into());
        }
        if changed.contains(logical_name.as_str()) {
            child.tags.insert("binoc.table-change".into());
        }
    }
}

fn logical_name_for_child(
    child: &DiffNode,
    left: &BTreeMap<String, &TableMember>,
    right: &BTreeMap<String, &TableMember>,
) -> Option<String> {
    if let Some(name) = child.details.get("logical_name").and_then(|v| v.as_str()) {
        return Some(name.to_string());
    }
    for (name, member) in left.iter().chain(right.iter()) {
        if child.path == member.node_path {
            return Some(name.clone());
        }
    }
    None
}

fn collection_summary(
    children: &[DiffNode],
    left: &BTreeMap<String, &TableMember>,
    right: &BTreeMap<String, &TableMember>,
) -> Summary {
    let mut parts: Vec<Summary> = Vec::new();
    for child in children {
        let Some(logical_name) = logical_name_for_child(child, left, right) else {
            continue;
        };
        let label = table_label(&logical_name, child.action.as_str());
        let detail = child
            .summary
            .clone()
            .unwrap_or_else(|| fallback_table_summary(child, left, right, &logical_name));
        // Compose the per-table clause structurally so the child's typed
        // counts keep their grouping policy through to the renderer.
        let mut part = Summary::new().text(format!("{label}: "));
        part.extend(lower_first_summary(detail));
        parts.push(part);
    }

    if parts.is_empty() {
        return "Table collection changed".into();
    }
    let mut out = Summary::new();
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            out = out.text("; ");
        }
        out.extend(part);
    }
    out
}

fn table_label(logical_name: &str, action: &str) -> String {
    match action {
        "add" => format!("Table {logical_name} added"),
        "remove" => format!("Table {logical_name} removed"),
        _ => format!("Table {logical_name} changed"),
    }
}

fn fallback_table_summary(
    child: &DiffNode,
    left: &BTreeMap<String, &TableMember>,
    right: &BTreeMap<String, &TableMember>,
    logical_name: &str,
) -> Summary {
    let member = match child.action.as_str() {
        "remove" => left.get(logical_name).copied(),
        _ => right
            .get(logical_name)
            .copied()
            .or_else(|| left.get(logical_name).copied()),
    };
    if let Some(member) = member {
        let columns = member.shape.columns.len() as u64;
        let rows = member.shape.row_count.unwrap_or(0);
        return Summary::new()
            .count(columns, "column")
            .text(", ")
            .count(rows, "row");
    }
    match child.action.as_str() {
        "add" => "table added".into(),
        "remove" => "table removed".into(),
        _ => "table changed".into(),
    }
}

/// Lowercase the first character of a summary's leading text segment (the
/// inverse of [`Summary::capitalize_first`]), so a child clause reads
/// naturally after `Table X changed: `.
fn lower_first_summary(mut summary: Summary) -> Summary {
    if let Some(Segment::Text(text)) = summary.0.first_mut() {
        let mut chars = text.chars();
        if let Some(first) = chars.next() {
            *text = first.to_lowercase().to_string() + chars.as_str();
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::LocalDataAccess;

    fn member(name: &str, path: &str, rows: u64) -> TableMember {
        TableMember {
            logical_name: name.into(),
            node_path: path.into(),
            source: TableSourceLocation {
                item_path: "data.sqlite".into(),
                kind: "sqlite_table".into(),
                locator: BTreeMap::from([("table".into(), serde_json::json!(name))]),
            },
            shape: TableShape {
                columns: vec!["id".into()],
                row_count: Some(rows),
            },
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn summarizes_child_table_changes() {
        let data = LocalDataAccess::new();
        let left = TabularCollectionData {
            tables: vec![member("users", "data.sqlite::users", 1)],
        };
        let right = TabularCollectionData {
            tables: vec![
                member("users", "data.sqlite::users", 2),
                member("posts", "data.sqlite::posts", 1),
            ],
        };
        let left_art = data
            .publish_artifact(
                &tabular_collection_v1(),
                ArtifactSubject::Left,
                "test",
                &serde_json::to_vec(&left).unwrap(),
            )
            .unwrap();
        let right_art = data
            .publish_artifact(
                &tabular_collection_v1(),
                ArtifactSubject::Right,
                "test",
                &serde_json::to_vec(&right).unwrap(),
            )
            .unwrap();
        let node = DiffNode::new("modify", "tabular_collection", "data.sqlite")
            .with_artifact(left_art)
            .with_artifact(right_art)
            .with_children(vec![
                DiffNode::new("modify", "tabular", "data.sqlite::users")
                    .with_summary("1 row added"),
                DiffNode::new("add", "tabular", "data.sqlite::posts")
                    .with_summary("New table (1 columns, 1 rows)"),
            ]);

        let TransformResult::Replace(node) =
            TableCollectionAnalyzer.transform(node, &data, &serde_json::Value::Null)
        else {
            panic!("expected replacement");
        };

        assert!(node.tags.contains("binoc.tabular-collection-change"));
        assert!(node.tags.contains("binoc.table-addition"));
        assert!(node.tags.contains("binoc.table-change"));
        assert_eq!(
            node.summary.as_ref().map(|s| s.plain_text()).as_deref(),
            Some("Table users changed: 1 row added; Table posts added: new table (1 columns, 1 rows)")
        );
        assert!(node.children[0].tags.contains("binoc.table-change"));
        assert!(node.children[1].tags.contains("binoc.table-addition"));
    }
}
