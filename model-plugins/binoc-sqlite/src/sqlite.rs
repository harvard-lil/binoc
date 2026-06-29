use std::path::Path;

use binoc_sdk::*;
use rusqlite::types::ValueRef;
use rusqlite::Connection;

#[derive(Default)]
pub struct SqliteParseRule;

impl ParseRule for SqliteParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-sqlite.parse.sqlite".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".sqlite".into(), ".sqlite3".into(), ".db".into()],
                media_types: Vec::new(),
            },
            // Children carry `tabular_v1`; the database node itself is a plain
            // container and emits no parent artifact.
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let phys = data.local_path(item)?;
        let conn = open_db(&phys)?;
        let table_names = read_table_names(&conn)?;

        // A SQLite database is a namespace of named tables, so it is always a
        // CONTAINER parse: one `tabular_v1` child node per table (even for a
        // single table — a SQL table has an intrinsic name). No parent artifact.
        let children = table_names
            .iter()
            .map(|name| table_child(&item.logical_path, &conn, name))
            .collect::<BinocResult<Vec<_>>>()?;

        Ok(ParseOutput {
            bytes: Vec::new(),
            diagnostics: Vec::new(),
            children,
            artifacts: Vec::new(),
            projection: ProjectionHint::default().item_type("SQLite database"),
        })
    }
}

fn open_db(path: &Path) -> BinocResult<Connection> {
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))
}

/// User table names in deterministic (alphabetical) order, excluding SQLite's
/// internal `sqlite_*` bookkeeping tables.
fn read_table_names(conn: &Connection) -> BinocResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    let names = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?
        .collect::<Result<_, _>>()
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;
    Ok(names)
}

/// Build a single table's child node, carrying its rows as a `tabular_v1`
/// artifact. The SQL table name is used verbatim as the child name — it is the
/// table's intrinsic identity, so the path joins on a decompose boundary
/// (`data.sqlite/>customers`).
fn table_child(db_logical_path: &str, conn: &Connection, table: &str) -> BinocResult<ParsedChild> {
    let table_data = read_table(conn, table)?;
    let bytes = serde_json::to_vec(&table_data)
        .map_err(|e| BinocError::Other(format!("serialize sqlite tabular artifact: {e}")))?;
    let logical_path = decompose_child(db_logical_path, table);
    Ok(ParsedChild {
        item: ItemRef {
            logical_path: logical_path.clone(),
            is_dir: false,
            content_hash: Some(blake3::hash(&bytes).to_hex().to_string()),
            size: Some(bytes.len() as u64),
            media_type: Some("application/vnd.binoc.tabular+json".into()),
            projection_hint: ProjectionHint::default().item_type("tabular"),
            tabular_parse: None,
            handle: logical_path,
        },
        artifacts: vec![ParsedArtifact {
            format: tabular_v1(),
            bytes,
        }],
    })
}

/// Read a table's columns and all rows into the format-neutral tabular model,
/// preserving SQLite storage classes (integers/reals stay numbers, NULL stays
/// null) so the typed tabular diff sees them as such.
fn read_table(conn: &Connection, table: &str) -> BinocResult<TabularData> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {quoted}"))
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    let headers: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let column_count = headers.len();

    let rows = stmt
        .query_map([], |row| {
            (0..column_count)
                .map(|i| Ok(cell_value(row.get_ref(i)?)))
                .collect::<Result<Vec<Value>, rusqlite::Error>>()
        })
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BinocError::Other(format!("sqlite: {e}")))?;

    Ok(TabularData::new(headers, rows))
}

/// Map a SQLite cell into the format-neutral [`Value`] model, preserving the
/// source storage class.
fn cell_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Number(i.into()),
        ValueRef::Real(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(bytes) => Value::String(format!("<blob {} bytes>", bytes.len())),
    }
}
