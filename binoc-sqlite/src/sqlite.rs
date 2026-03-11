use std::collections::BTreeMap;
use std::path::Path;

use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;
use rusqlite::Connection;

pub struct SqliteComparator;

#[derive(Debug, Clone)]
struct TableInfo {
    columns: Vec<ColumnInfo>,
    row_count: u64,
}

#[derive(Debug, Clone)]
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
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))
}

fn diff_table(
    logical_path: &str,
    table_name: &str,
    left: &TableInfo,
    right: &TableInfo,
) -> Option<DiffNode> {
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
    let has_row_change = left.row_count != right.row_count;

    if !has_schema_change && !has_row_change {
        return None;
    }

    let table_path = format!("{logical_path}/{table_name}");

    let left_col_names: Vec<&str> = left.columns.iter().map(|c| c.name.as_str()).collect();
    let right_col_names: Vec<&str> = right.columns.iter().map(|c| c.name.as_str()).collect();

    let mut node = DiffNode::new("modify", "sqlite_table", &table_path)
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
    fn name(&self) -> &str {
        "binoc-sqlite.sqlite"
    }

    fn handles_extensions(&self) -> &[&str] {
        &[".sqlite", ".sqlite3", ".db"]
    }

    fn handles_media_types(&self) -> &[&str] {
        &["application/vnd.sqlite3", "application/x-sqlite3"]
    }

    fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => self.compare_both(left, right, pair.logical_path()),
            (None, Some(right)) => {
                let conn = open_db(&right.physical_path)?;
                let schema = read_schema(&conn)?;
                let table_names: Vec<&str> = schema.keys().map(|s| s.as_str()).collect();
                let total_rows: u64 = schema.values().map(|t| t.row_count).sum();

                let summary = format!(
                    "New database ({} table{}, {} row{} total)",
                    schema.len(),
                    if schema.len() == 1 { "" } else { "s" },
                    total_rows,
                    if total_rows == 1 { "" } else { "s" },
                );

                let node = DiffNode::new("add", "sqlite_database", &right.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc-sqlite.content-changed")
                    .with_detail("tables", serde_json::json!(table_names))
                    .with_detail("total_rows", serde_json::json!(total_rows));

                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let conn = open_db(&left.physical_path)?;
                let schema = read_schema(&conn)?;
                let table_names: Vec<&str> = schema.keys().map(|s| s.as_str()).collect();
                let total_rows: u64 = schema.values().map(|t| t.row_count).sum();

                let summary = format!(
                    "Database removed ({} table{}, {} row{} total)",
                    schema.len(),
                    if schema.len() == 1 { "" } else { "s" },
                    total_rows,
                    if total_rows == 1 { "" } else { "s" },
                );

                let node = DiffNode::new("remove", "sqlite_database", &left.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc-sqlite.content-changed")
                    .with_detail("tables", serde_json::json!(table_names))
                    .with_detail("total_rows", serde_json::json!(total_rows));

                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}

impl SqliteComparator {
    fn compare_both(
        &self,
        left: &Item,
        right: &Item,
        logical_path: &str,
    ) -> BinocResult<CompareResult> {
        let conn_l = open_db(&left.physical_path)?;
        let conn_r = open_db(&right.physical_path)?;
        let schema_l = read_schema(&conn_l)?;
        let schema_r = read_schema(&conn_r)?;

        let mut children = Vec::new();

        // Tables in both snapshots — diff schema + row counts
        for (name, info_l) in &schema_l {
            if let Some(info_r) = schema_r.get(name) {
                if let Some(node) = diff_table(logical_path, name, info_l, info_r) {
                    children.push(node);
                }
            }
        }

        // Tables added
        for (name, info) in &schema_r {
            if !schema_l.contains_key(name) {
                let table_path = format!("{logical_path}/{name}");
                let col_names: Vec<&str> = info.columns.iter().map(|c| c.name.as_str()).collect();
                let summary = format!(
                    "Table added ({} column{}, {} row{})",
                    info.columns.len(),
                    if info.columns.len() == 1 { "" } else { "s" },
                    info.row_count,
                    if info.row_count == 1 { "" } else { "s" },
                );
                let node = DiffNode::new("add", "sqlite_table", &table_path)
                    .with_summary(summary)
                    .with_tag("binoc-sqlite.table-addition")
                    .with_tag("binoc.schema-change")
                    .with_detail("columns", serde_json::json!(col_names))
                    .with_detail("row_count", serde_json::json!(info.row_count));
                children.push(node);
            }
        }

        // Tables removed
        for (name, info) in &schema_l {
            if !schema_r.contains_key(name) {
                let table_path = format!("{logical_path}/{name}");
                let col_names: Vec<&str> = info.columns.iter().map(|c| c.name.as_str()).collect();
                let summary = format!(
                    "Table removed ({} column{}, {} row{})",
                    info.columns.len(),
                    if info.columns.len() == 1 { "" } else { "s" },
                    info.row_count,
                    if info.row_count == 1 { "" } else { "s" },
                );
                let node = DiffNode::new("remove", "sqlite_table", &table_path)
                    .with_summary(summary)
                    .with_tag("binoc-sqlite.table-removal")
                    .with_tag("binoc.schema-change")
                    .with_detail("columns", serde_json::json!(col_names))
                    .with_detail("row_count", serde_json::json!(info.row_count));
                children.push(node);
            }
        }

        if children.is_empty() {
            return Ok(CompareResult::Identical);
        }

        let tables_l: Vec<&str> = schema_l.keys().map(|s| s.as_str()).collect();
        let tables_r: Vec<&str> = schema_r.keys().map(|s| s.as_str()).collect();

        let node = DiffNode::new("modify", "sqlite_database", logical_path)
            .with_children(children)
            .with_detail("tables_left", serde_json::json!(tables_l))
            .with_detail("tables_right", serde_json::json!(tables_r));

        Ok(CompareResult::Leaf(node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create_test_db(path: &Path, sql: &[&str]) {
        let conn = Connection::open(path).unwrap();
        for s in sql {
            conn.execute_batch(s).unwrap();
        }
    }

    fn make_pair(left: &Path, right: &Path, logical: &str) -> ItemPair {
        ItemPair::both(
            Item::new(left.to_path_buf(), logical),
            Item::new(right.to_path_buf(), logical),
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

        let cmp = SqliteComparator;
        let ctx = CompareContext::new();
        let pair = make_pair(&a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &ctx).unwrap();
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

        let cmp = SqliteComparator;
        let ctx = CompareContext::new();
        let pair = make_pair(&a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &ctx).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.kind, "modify");
                assert_eq!(node.item_type, "sqlite_database");
                assert_eq!(node.children.len(), 1);
                let child = &node.children[0];
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

        let cmp = SqliteComparator;
        let ctx = CompareContext::new();
        let pair = make_pair(&a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &ctx).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.kind, "modify");
                assert_eq!(node.children.len(), 1);
                let child = &node.children[0];
                assert_eq!(child.kind, "add");
                assert_eq!(child.item_type, "sqlite_table");
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

        let cmp = SqliteComparator;
        let ctx = CompareContext::new();
        let pair = make_pair(&a, &b, "test.sqlite");
        let result = cmp.compare(&pair, &ctx).unwrap();

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

        let cmp = SqliteComparator;
        let ctx = CompareContext::new();
        let pair = ItemPair::added(Item::new(b, "new.sqlite"));
        let result = cmp.compare(&pair, &ctx).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.kind, "add");
                assert_eq!(node.item_type, "sqlite_database");
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

        let cmp = SqliteComparator;
        let ctx = CompareContext::new();
        let pair = ItemPair::removed(Item::new(a, "old.sqlite"));
        let result = cmp.compare(&pair, &ctx).unwrap();

        match result {
            CompareResult::Leaf(node) => {
                assert_eq!(node.kind, "remove");
                assert_eq!(node.item_type, "sqlite_database");
                assert!(node.summary.unwrap().contains("2 tables"));
            }
            _ => panic!("expected Leaf"),
        }
    }
}
