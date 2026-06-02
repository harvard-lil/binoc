use std::collections::BTreeMap;
use std::path::Path;

use binoc_sdk::*;
use rusqlite::types::ValueRef;
use rusqlite::Connection;

#[derive(Default)]
pub struct SqliteComparator;

#[derive(Debug, Clone, serde::Serialize)]
struct TableInfo {
    columns: Vec<ColumnInfo>,
    row_count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[allow(dead_code)]
struct ColumnInfo {
    name: String,
    col_type: String,
    notnull: bool,
    pk: bool,
}

fn open_db(path: &Path) -> BinocResult<Connection> {
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))
}

fn read_schema(conn: &Connection) -> BinocResult<BTreeMap<String, TableInfo>> {
    let mut tables = BTreeMap::new();

    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?
        .collect::<Result<_, _>>()
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    for name in table_names {
        let columns = read_columns(conn, &name)?;
        let row_count = read_row_count(conn, &name)?;
        tables.insert(name, TableInfo { columns, row_count });
    }

    Ok(tables)
}

fn read_columns(conn: &Connection, table: &str) -> BinocResult<Vec<ColumnInfo>> {
    let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""));
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    let cols = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                name: row.get(1)?,
                col_type: row.get(2)?,
                notnull: row.get::<_, bool>(3)?,
                pk: row.get::<_, i32>(5)? != 0,
            })
        })
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    Ok(cols)
}

fn read_row_count(conn: &Connection, table: &str) -> BinocResult<u64> {
    let sql = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
    let count: i64 = conn
        .query_row(&sql, [], |row| row.get(0))
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    u64::try_from(count).map_err(|e| BinocError::Other(format!("sqlite: {e}")))
}

fn read_table_data(conn: &Connection, table: &str, info: &TableInfo) -> BinocResult<TabularData> {
    let sql = format!("SELECT * FROM \"{}\"", table.replace('"', "\"\""));
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;
    let column_count = info.columns.len();
    let mut rows = stmt
        .query([])
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;
    let mut tabular_rows = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?
    {
        let mut values = Vec::with_capacity(column_count);
        for idx in 0..column_count {
            let value = row
                .get_ref(idx)
                .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;
            values.push(sqlite_value_to_string(value));
        }
        tabular_rows.push(values);
    }

    Ok(TabularData {
        headers: info.columns.iter().map(|c| c.name.clone()).collect(),
        rows: tabular_rows,
    })
}

fn sqlite_value_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(n) => n.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).to_string(),
        ValueRef::Blob(bytes) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

fn table_node_path(logical_path: &str, table_name: &str) -> String {
    format!("{logical_path}::{table_name}")
}

fn publish_tabular(
    data: &dyn DataAccess,
    tabular: &TabularData,
    subject: ArtifactSubject,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(tabular)
        .map_err(|e| BinocError::Other(format!("serialize tabular artifact: {e}")))?;
    data.publish_artifact(&tabular_v1(), subject, "binoc-sqlite.sqlite", &bytes)
}

fn collection_from_schema(
    logical_path: &str,
    schema: &BTreeMap<String, TableInfo>,
) -> TabularCollectionData {
    TabularCollectionData {
        tables: schema
            .iter()
            .map(|(name, info)| TableMember {
                logical_name: name.clone(),
                node_path: table_node_path(logical_path, name),
                source: TableSourceLocation {
                    item_path: logical_path.into(),
                    kind: "sqlite_table".into(),
                    locator: BTreeMap::from([("table".into(), serde_json::json!(name))]),
                },
                shape: TableShape {
                    columns: info.columns.iter().map(|c| c.name.clone()).collect(),
                    row_count: Some(info.row_count),
                },
                metadata: BTreeMap::new(),
            })
            .collect(),
    }
}

fn publish_collection(
    data: &dyn DataAccess,
    collection: &TabularCollectionData,
    subject: ArtifactSubject,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(collection)
        .map_err(|e| BinocError::Other(format!("serialize tabular collection artifact: {e}")))?;
    data.publish_artifact(
        &tabular_collection_v1(),
        subject,
        "binoc-sqlite.sqlite",
        &bytes,
    )
}

fn table_add_node(
    logical_path: &str,
    table_name: &str,
    info: &TableInfo,
    artifact: ArtifactDescriptor,
) -> DiffNode {
    let table_path = table_node_path(logical_path, table_name);
    let col_names: Vec<&str> = info.columns.iter().map(|c| c.name.as_str()).collect();
    let summary = format!(
        "Table added ({} column{}, {} row{})",
        info.columns.len(),
        if info.columns.len() == 1 { "" } else { "s" },
        info.row_count,
        if info.row_count == 1 { "" } else { "s" },
    );
    DiffNode::new("add", "tabular", &table_path)
        .with_summary(summary)
        .with_tag("binoc-sqlite.table-addition")
        .with_tag("binoc.table-addition")
        .with_tag("binoc.schema-change")
        .with_detail("logical_name", serde_json::json!(table_name))
        .with_detail("columns", serde_json::json!(col_names))
        .with_detail("row_count", serde_json::json!(info.row_count))
        .with_artifact(artifact)
}

fn table_remove_node(
    logical_path: &str,
    table_name: &str,
    info: &TableInfo,
    artifact: ArtifactDescriptor,
) -> DiffNode {
    let table_path = table_node_path(logical_path, table_name);
    let col_names: Vec<&str> = info.columns.iter().map(|c| c.name.as_str()).collect();
    let summary = format!(
        "Table removed ({} column{}, {} row{})",
        info.columns.len(),
        if info.columns.len() == 1 { "" } else { "s" },
        info.row_count,
        if info.row_count == 1 { "" } else { "s" },
    );
    DiffNode::new("remove", "tabular", &table_path)
        .with_summary(summary)
        .with_tag("binoc-sqlite.table-removal")
        .with_tag("binoc.table-removal")
        .with_tag("binoc.schema-change")
        .with_detail("logical_name", serde_json::json!(table_name))
        .with_detail("columns", serde_json::json!(col_names))
        .with_detail("row_count", serde_json::json!(info.row_count))
        .with_artifact(artifact)
}

struct TableDiffInput<'a> {
    logical_path: &'a str,
    table_name: &'a str,
    left: &'a TableInfo,
    right: &'a TableInfo,
    left_data: &'a TabularData,
    right_data: &'a TabularData,
    left_artifact: ArtifactDescriptor,
    right_artifact: ArtifactDescriptor,
}

fn table_has_change(
    left: &TableInfo,
    right: &TableInfo,
    left_data: &TabularData,
    right_data: &TabularData,
) -> bool {
    if left_data != right_data {
        return true;
    }

    let right_cols: BTreeMap<&str, &ColumnInfo> =
        right.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    left.columns.iter().any(|left_col| {
        right_cols
            .get(left_col.name.as_str())
            .is_some_and(|right_col| left_col.col_type != right_col.col_type)
    })
}

fn diff_table(input: TableDiffInput<'_>) -> Option<DiffNode> {
    let TableDiffInput {
        logical_path,
        table_name,
        left,
        right,
        left_data,
        right_data,
        left_artifact,
        right_artifact,
    } = input;
    let left_cols: BTreeMap<&str, &ColumnInfo> =
        left.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    let right_cols: BTreeMap<&str, &ColumnInfo> =
        right.columns.iter().map(|c| (c.name.as_str(), c)).collect();

    let cols_added: Vec<&str> = right_cols
        .keys()
        .filter(|k| !left_cols.contains_key(*k))
        .copied()
        .collect();
    let cols_removed: Vec<&str> = left_cols
        .keys()
        .filter(|k| !right_cols.contains_key(*k))
        .copied()
        .collect();

    let cols_type_changed: Vec<(&str, &str, &str)> = left_cols
        .iter()
        .filter_map(|(&name, &lc)| {
            right_cols.get(name).and_then(|rc| {
                if lc.col_type != rc.col_type {
                    Some((name, lc.col_type.as_str(), rc.col_type.as_str()))
                } else {
                    None
                }
            })
        })
        .collect();

    let rows_added = right.row_count.saturating_sub(left.row_count);
    let rows_removed = left.row_count.saturating_sub(right.row_count);

    let has_schema_change =
        !cols_added.is_empty() || !cols_removed.is_empty() || !cols_type_changed.is_empty();
    let has_row_change = left_data != right_data;

    if !has_schema_change && !has_row_change {
        return None;
    }

    let table_path = table_node_path(logical_path, table_name);

    let left_col_names: Vec<&str> = left.columns.iter().map(|c| c.name.as_str()).collect();
    let right_col_names: Vec<&str> = right.columns.iter().map(|c| c.name.as_str()).collect();

    let mut node = DiffNode::new("modify", "tabular", &table_path)
        .with_artifact(left_artifact)
        .with_artifact(right_artifact)
        .with_detail("logical_name", serde_json::json!(table_name))
        .with_detail("columns_left", serde_json::json!(left_col_names))
        .with_detail("columns_right", serde_json::json!(right_col_names))
        .with_detail("rows_left", serde_json::json!(left.row_count))
        .with_detail("rows_right", serde_json::json!(right.row_count));

    if !cols_added.is_empty() {
        node.tags.insert("binoc-sqlite.column-addition".into());
        node.tags.insert("binoc.column-addition".into());
        node = node.with_detail("columns_added", serde_json::json!(cols_added));
    }
    if !cols_removed.is_empty() {
        node.tags.insert("binoc-sqlite.column-removal".into());
        node.tags.insert("binoc.column-removal".into());
        node = node.with_detail("columns_removed", serde_json::json!(cols_removed));
    }
    if !cols_type_changed.is_empty() {
        node.tags.insert("binoc-sqlite.column-type-change".into());
        let changes: Vec<serde_json::Value> = cols_type_changed
            .iter()
            .map(|(name, from, to)| serde_json::json!({"column": name, "from": from, "to": to}))
            .collect();
        node = node.with_detail("columns_type_changed", serde_json::json!(changes));
    }
    if has_schema_change {
        node.tags.insert("binoc-sqlite.schema-change".into());
        node.tags.insert("binoc.schema-change".into());
    }
    if rows_added > 0 {
        node.tags.insert("binoc-sqlite.row-addition".into());
        node.tags.insert("binoc.row-addition".into());
        node = node.with_detail("rows_added", serde_json::json!(rows_added));
    }
    if rows_removed > 0 {
        node.tags.insert("binoc-sqlite.row-removal".into());
        node.tags.insert("binoc.row-removal".into());
        node = node.with_detail("rows_removed", serde_json::json!(rows_removed));
    }

    let mut parts = Vec::new();
    if !cols_added.is_empty() {
        parts.push(fmt_count(cols_added.len(), "column", "columns", "added"));
    }
    if !cols_removed.is_empty() {
        parts.push(fmt_count(
            cols_removed.len(),
            "column",
            "columns",
            "removed",
        ));
    }
    if !cols_type_changed.is_empty() {
        parts.push(fmt_count(
            cols_type_changed.len(),
            "column type",
            "column types",
            "changed",
        ));
    }
    if rows_added > 0 {
        parts.push(format!(
            "{rows_added} row{} added ({}\u{2009}\u{2192}\u{2009}{} rows)",
            if rows_added == 1 { "" } else { "s" },
            left.row_count,
            right.row_count
        ));
    }
    if rows_removed > 0 {
        parts.push(format!(
            "{rows_removed} row{} removed ({}\u{2009}\u{2192}\u{2009}{} rows)",
            if rows_removed == 1 { "" } else { "s" },
            left.row_count,
            right.row_count
        ));
    }

    node.summary = Some(capitalize(&parts.join("; ")));
    Some(node)
}

fn fmt_count(n: usize, singular: &str, plural: &str, verb: &str) -> String {
    if n == 1 {
        format!("1 {singular} {verb}")
    } else {
        format!("{n} {plural} {verb}")
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

impl Comparator for SqliteComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc-sqlite.sqlite")
            .with_extensions(vec![".sqlite".into(), ".sqlite3".into(), ".db".into()])
            .with_media_types(vec![
                "application/vnd.sqlite3".into(),
                "application/x-sqlite3".into(),
            ])
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => self.compare_both(left, right, pair.logical_path(), data),
            (None, Some(right)) => {
                let phys = data.local_path(right)?;
                let conn = open_db(&phys)?;
                let schema = read_schema(&conn)?;
                let table_names: Vec<&str> = schema.keys().map(|s| s.as_str()).collect();
                let total_rows: u64 = schema.values().map(|t| t.row_count).sum();
                let collection = collection_from_schema(&right.logical_path, &schema);
                let collection_art = publish_collection(data, &collection, ArtifactSubject::Right)?;

                let summary = format!(
                    "New database ({} table{}, {} row{} total)",
                    schema.len(),
                    if schema.len() == 1 { "" } else { "s" },
                    total_rows,
                    if total_rows == 1 { "" } else { "s" },
                );

                let bytes = serde_json::to_vec(&schema).unwrap();
                let art = data.publish_artifact(
                    &ArtifactFormat::new("binoc-sqlite", "relational-schema", 1),
                    ArtifactSubject::Right,
                    "binoc-sqlite.sqlite",
                    &bytes,
                )?;

                let mut children = Vec::new();
                for (name, info) in &schema {
                    let tabular = read_table_data(&conn, name, info)?;
                    let artifact = publish_tabular(data, &tabular, ArtifactSubject::Right)?;
                    children.push(table_add_node(&right.logical_path, name, info, artifact));
                }

                let node = DiffNode::new("add", "tabular_collection", &right.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc-sqlite.content-changed")
                    .with_detail("tables", serde_json::json!(table_names))
                    .with_detail("total_rows", serde_json::json!(total_rows))
                    .with_children(children)
                    .with_artifact(collection_art)
                    .with_artifact(art);

                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let phys = data.local_path(left)?;
                let conn = open_db(&phys)?;
                let schema = read_schema(&conn)?;
                let table_names: Vec<&str> = schema.keys().map(|s| s.as_str()).collect();
                let total_rows: u64 = schema.values().map(|t| t.row_count).sum();
                let collection = collection_from_schema(&left.logical_path, &schema);
                let collection_art = publish_collection(data, &collection, ArtifactSubject::Left)?;

                let summary = format!(
                    "Database removed ({} table{}, {} row{} total)",
                    schema.len(),
                    if schema.len() == 1 { "" } else { "s" },
                    total_rows,
                    if total_rows == 1 { "" } else { "s" },
                );

                let bytes = serde_json::to_vec(&schema).unwrap();
                let art = data.publish_artifact(
                    &ArtifactFormat::new("binoc-sqlite", "relational-schema", 1),
                    ArtifactSubject::Left,
                    "binoc-sqlite.sqlite",
                    &bytes,
                )?;

                let mut children = Vec::new();
                for (name, info) in &schema {
                    let tabular = read_table_data(&conn, name, info)?;
                    let artifact = publish_tabular(data, &tabular, ArtifactSubject::Left)?;
                    children.push(table_remove_node(&left.logical_path, name, info, artifact));
                }

                let node = DiffNode::new("remove", "tabular_collection", &left.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc-sqlite.content-changed")
                    .with_detail("tables", serde_json::json!(table_names))
                    .with_detail("total_rows", serde_json::json!(total_rows))
                    .with_children(children)
                    .with_artifact(collection_art)
                    .with_artifact(art);

                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}

impl SqliteComparator {
    fn compare_both(
        &self,
        left: &ItemRef,
        right: &ItemRef,
        logical_path: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<CompareResult> {
        let phys_l = data.local_path(left)?;
        let phys_r = data.local_path(right)?;
        let conn_l = open_db(&phys_l)?;
        let conn_r = open_db(&phys_r)?;
        let schema_l = read_schema(&conn_l)?;
        let schema_r = read_schema(&conn_r)?;

        let mut children = Vec::new();

        for (name, info_l) in &schema_l {
            if let Some(info_r) = schema_r.get(name) {
                let data_l = read_table_data(&conn_l, name, info_l)?;
                let data_r = read_table_data(&conn_r, name, info_r)?;
                if !table_has_change(info_l, info_r, &data_l, &data_r) {
                    continue;
                }
                let art_l = publish_tabular(data, &data_l, ArtifactSubject::Left)?;
                let art_r = publish_tabular(data, &data_r, ArtifactSubject::Right)?;
                if let Some(node) = diff_table(TableDiffInput {
                    logical_path,
                    table_name: name,
                    left: info_l,
                    right: info_r,
                    left_data: &data_l,
                    right_data: &data_r,
                    left_artifact: art_l,
                    right_artifact: art_r,
                }) {
                    children.push(node);
                }
            }
        }

        for (name, info) in &schema_r {
            if !schema_l.contains_key(name) {
                let table_data = read_table_data(&conn_r, name, info)?;
                let artifact = publish_tabular(data, &table_data, ArtifactSubject::Right)?;
                children.push(table_add_node(logical_path, name, info, artifact));
            }
        }

        for (name, info) in &schema_l {
            if !schema_r.contains_key(name) {
                let table_data = read_table_data(&conn_l, name, info)?;
                let artifact = publish_tabular(data, &table_data, ArtifactSubject::Left)?;
                children.push(table_remove_node(logical_path, name, info, artifact));
            }
        }

        if children.is_empty() {
            return Ok(CompareResult::Identical);
        }

        let tables_l: Vec<&str> = schema_l.keys().map(|s| s.as_str()).collect();
        let tables_r: Vec<&str> = schema_r.keys().map(|s| s.as_str()).collect();
        let collection_l = collection_from_schema(logical_path, &schema_l);
        let collection_r = collection_from_schema(logical_path, &schema_r);

        let bytes_l = serde_json::to_vec(&schema_l).unwrap();
        let bytes_r = serde_json::to_vec(&schema_r).unwrap();
        let format = ArtifactFormat::new("binoc-sqlite", "relational-schema", 1);
        let collection_art_l = publish_collection(data, &collection_l, ArtifactSubject::Left)?;
        let collection_art_r = publish_collection(data, &collection_r, ArtifactSubject::Right)?;
        let art_l = data.publish_artifact(
            &format,
            ArtifactSubject::Left,
            "binoc-sqlite.sqlite",
            &bytes_l,
        )?;
        let art_r = data.publish_artifact(
            &format,
            ArtifactSubject::Right,
            "binoc-sqlite.sqlite",
            &bytes_r,
        )?;

        let node = DiffNode::new("modify", "tabular_collection", logical_path)
            .with_children(children)
            .with_detail("tables_left", serde_json::json!(tables_l))
            .with_detail("tables_right", serde_json::json!(tables_r))
            .with_artifact(collection_art_l)
            .with_artifact(collection_art_r)
            .with_artifact(art_l)
            .with_artifact(art_r);

        Ok(CompareResult::Leaf(node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::LocalDataAccess;

    fn create_test_db(path: &Path, sql: &[&str]) {
        let conn = Connection::open(path).unwrap();
        for s in sql {
            conn.execute_batch(s).unwrap();
        }
    }

    fn make_pair(data: &dyn DataAccess, left: &Path, right: &Path, logical: &str) -> ItemPair {
        ItemPair::both(
            data.register_local(left, logical).unwrap(),
            data.register_local(right, logical).unwrap(),
        )
    }

    #[test]
    fn identical_databases() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");
        let b = dir.path().join("b.sqlite");

        let sql = &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
            "INSERT INTO users VALUES (1, 'Alice');",
        ];
        create_test_db(&a, sql);
        create_test_db(&b, sql);

        let data = LocalDataAccess::new();
        let cmp = SqliteComparator;
        let pair = make_pair(&data, &a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &data).unwrap();
        assert!(matches!(result, CompareResult::Identical));
    }

    #[test]
    fn row_addition() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");
        let b = dir.path().join("b.sqlite");

        create_test_db(
            &a,
            &[
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
                "INSERT INTO users VALUES (1, 'Alice');",
            ],
        );
        create_test_db(
            &b,
            &[
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
                "INSERT INTO users VALUES (1, 'Alice');",
                "INSERT INTO users VALUES (2, 'Bob');",
            ],
        );

        let data = LocalDataAccess::new();
        let cmp = SqliteComparator;
        let pair = make_pair(&data, &a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &data).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.action, "modify");
                assert_eq!(node.item_type, "tabular_collection");
                assert_eq!(node.children.len(), 1);
                let child = &node.children[0];
                assert_eq!(child.item_type, "tabular");
                assert_eq!(child.path, "test.sqlite::users");
                assert!(child.tags.contains("binoc-sqlite.row-addition"));
                assert_eq!(child.details["rows_left"], serde_json::json!(1));
                assert_eq!(child.details["rows_right"], serde_json::json!(2));
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn table_addition() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");
        let b = dir.path().join("b.sqlite");

        create_test_db(
            &a,
            &["CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);"],
        );
        create_test_db(
            &b,
            &[
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT, user_id INTEGER);",
                "INSERT INTO posts VALUES (1, 'Hello', 1);",
            ],
        );

        let data = LocalDataAccess::new();
        let cmp = SqliteComparator;
        let pair = make_pair(&data, &a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &data).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.action, "modify");
                assert_eq!(node.children.len(), 1);
                let child = &node.children[0];
                assert_eq!(child.action, "add");
                assert_eq!(child.item_type, "tabular");
                assert_eq!(child.path, "test.sqlite::posts");
                assert!(child.tags.contains("binoc-sqlite.table-addition"));
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn column_addition() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");
        let b = dir.path().join("b.sqlite");

        create_test_db(
            &a,
            &[
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
                "INSERT INTO users VALUES (1, 'Alice');",
            ],
        );
        create_test_db(
            &b,
            &[
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT);",
                "INSERT INTO users VALUES (1, 'Alice', 'alice@example.com');",
            ],
        );

        let data = LocalDataAccess::new();
        let cmp = SqliteComparator;
        let pair = make_pair(&data, &a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &data).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.children.len(), 1);
                let child = &node.children[0];
                assert!(child.tags.contains("binoc-sqlite.column-addition"));
                assert!(child.tags.contains("binoc-sqlite.schema-change"));
                assert_eq!(child.details["columns_added"], serde_json::json!(["email"]));
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn database_added() {
        let dir = tempfile::tempdir().unwrap();
        let b = dir.path().join("b.sqlite");

        create_test_db(
            &b,
            &[
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
                "INSERT INTO users VALUES (1, 'Alice');",
            ],
        );

        let data = LocalDataAccess::new();
        let cmp = SqliteComparator;
        let item = data.register_local(&b, "new.sqlite").unwrap();
        let pair = ItemPair::added(item);
        let result = cmp.compare(&pair, &data).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.action, "add");
                assert_eq!(node.item_type, "tabular_collection");
                assert_eq!(node.children.len(), 1);
                assert!(node.summary.unwrap().contains("1 table"));
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn database_removed() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");

        create_test_db(
            &a,
            &["CREATE TABLE t1 (x INTEGER);", "CREATE TABLE t2 (y TEXT);"],
        );

        let data = LocalDataAccess::new();
        let cmp = SqliteComparator;
        let item = data.register_local(&a, "old.sqlite").unwrap();
        let pair = ItemPair::removed(item);
        let result = cmp.compare(&pair, &data).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.action, "remove");
                assert_eq!(node.item_type, "tabular_collection");
                assert_eq!(node.children.len(), 2);
                assert!(node.summary.unwrap().contains("2 tables"));
            }
            _ => panic!("expected Leaf"),
        }
    }
}
