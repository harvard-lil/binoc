use std::collections::BTreeMap;
use std::path::Path;

use binoc_sdk::*;
use rusqlite::Connection;

#[derive(Default)]
pub struct SqliteParseRule;

#[derive(Default)]
pub struct SqliteCollectionWriter;

impl ParseRule for SqliteParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-sqlite.parse.sqlite".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".sqlite".into(), ".sqlite3".into(), ".db".into()],
                media_types: Vec::new(),
            },
            output: tabular_collection_v1(),
            requires_link: true,
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let phys = data.local_path(item)?;
        let conn = open_db(&phys)?;
        let schema = read_schema(&conn)?;
        serde_json::to_vec(&collection_from_schema(&item.logical_path, &schema))
            .map(ParseOutput::from)
            .map_err(|e| BinocError::Other(format!("serialize sqlite collection artifact: {e}")))
    }
}

impl EditListWriter for SqliteCollectionWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc-sqlite.write.collection".into(),
            formats: vec![tabular_collection_v1()],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let (Some(left), Some(right)) = (
            load_collection(ctx, ctx.link.left, data)?,
            load_collection(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };
        if !is_sqlite_collection(&left) && !is_sqlite_collection(&right) {
            return Ok(None);
        }
        if left == right {
            return Ok(Some(Vec::new().into()));
        }

        let left_tables = left
            .tables
            .iter()
            .map(|table| (table.logical_name.as_str(), table))
            .collect::<BTreeMap<_, _>>();
        let right_tables = right
            .tables
            .iter()
            .map(|table| (table.logical_name.as_str(), table))
            .collect::<BTreeMap<_, _>>();
        let names = left_tables
            .keys()
            .chain(right_tables.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        let mut edits = Vec::new();
        for name in names {
            match (left_tables.get(name), right_tables.get(name)) {
                (Some(left_table), Some(right_table)) if *left_table == *right_table => {}
                (Some(left_table), Some(right_table)) => {
                    let mut edit =
                        Edit::new("sqlite.change_table", serde_json::json!({ "table": name }))
                            .with_item_type("tabular_collection")
                            .with_tag("binoc.tabular-collection-change")
                            .with_tag("binoc.table-change");
                    if right_table.shape.row_count > left_table.shape.row_count {
                        edit = edit
                            .with_tag("binoc-sqlite.row-addition")
                            .with_tag("binoc.row-addition");
                    }
                    if left_table.shape.row_count > right_table.shape.row_count {
                        edit = edit
                            .with_tag("binoc-sqlite.row-removal")
                            .with_tag("binoc.row-removal");
                    }
                    if left_table.shape.columns != right_table.shape.columns {
                        edit = edit
                            .with_tag("binoc-sqlite.schema-change")
                            .with_tag("binoc.schema-change");
                    }
                    edits.push(edit);
                }
                (Some(_), None) => edits.push(
                    Edit::new("sqlite.remove_table", serde_json::json!({ "table": name }))
                        .with_item_type("tabular_collection")
                        .with_tag("binoc-sqlite.table-removal")
                        .with_tag("binoc.table-removal")
                        .with_tag("binoc.schema-change")
                        .with_tag("binoc.tabular-collection-change"),
                ),
                (None, Some(_)) => edits.push(
                    Edit::new("sqlite.add_table", serde_json::json!({ "table": name }))
                        .with_item_type("tabular_collection")
                        .with_tag("binoc-sqlite.table-addition")
                        .with_tag("binoc.table-addition")
                        .with_tag("binoc.schema-change")
                        .with_tag("binoc.tabular-collection-change"),
                ),
                (None, None) => {}
            }
        }

        Ok(Some(edits.into()))
    }
}

fn load_collection(
    ctx: &LinkCtx<'_>,
    id: NodeId,
    data: &dyn DataAccess,
) -> BinocResult<Option<TabularCollectionData>> {
    let Some(bytes) = ctx
        .view
        .artifact_bytes(id, &tabular_collection_v1(), data)?
    else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| BinocError::Other(format!("decode sqlite collection artifact: {err}")))
}

// Writer dispatch is by artifact format, which says nothing about the
// producer. A specialized writer claiming a shared format must decline
// foreign payloads by returning `None`, so dispatch falls through to the
// generic collection writer ordered after it.
fn is_sqlite_collection(collection: &TabularCollectionData) -> bool {
    collection
        .tables
        .iter()
        .any(|table| table.source.kind == "sqlite_table")
}

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

fn table_node_path(logical_path: &str, table_name: &str) -> String {
    format!("{logical_path}::{table_name}")
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
