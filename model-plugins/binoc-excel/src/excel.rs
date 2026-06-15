use std::path::Path;

use binoc_sdk::*;
use calamine::{open_workbook_auto, Data, Range, Reader};

/// Spreadsheet file extensions handled by the parse rule.
const EXCEL_EXTENSIONS: &[&str] = &[".xlsx", ".xls", ".xlsm", ".xlsb", ".ods"];

fn excel_extensions() -> Vec<String> {
    EXCEL_EXTENSIONS.iter().map(|e| (*e).into()).collect()
}

/// A workbook sheet that contains at least one cell.
struct NonEmptySheet {
    name: String,
    range: Range<Data>,
}

/// Open the workbook at `path` and return every non-empty sheet in workbook
/// order. Empty sheets are dropped so blank scratch sheets do not surface as
/// child nodes.
fn read_non_empty_sheets(path: &Path) -> BinocResult<Vec<NonEmptySheet>> {
    let mut workbook =
        open_workbook_auto(path).map_err(|e| BinocError::Other(format!("excel: {e}")))?;
    let mut sheets = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| BinocError::Other(format!("excel: sheet {name:?}: {e}")))?;
        if range.is_empty() {
            continue;
        }
        sheets.push(NonEmptySheet { name, range });
    }
    Ok(sheets)
}

/// Map a calamine cell into the format-neutral [`Value`] model, preserving the
/// source type so the typed diff sees numbers as numbers and bools as bools.
fn cell_value(cell: &Data) -> Value {
    match cell {
        Data::Int(i) => Value::Number((*i).into()),
        Data::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Data::Bool(b) => Value::Bool(*b),
        Data::String(s) => Value::String(s.clone()),
        Data::DateTimeIso(s) => Value::String(s.clone()),
        Data::DurationIso(s) => Value::String(s.clone()),
        Data::DateTime(dt) => dt
            .as_datetime()
            .map(|naive| Value::String(naive.format("%Y-%m-%dT%H:%M:%S").to_string()))
            .unwrap_or(Value::Null),
        Data::Error(_) | Data::Empty => Value::Null,
    }
}

/// Build a typed [`TabularData`] from a sheet: first populated row supplies the
/// headers, remaining rows are the records.
fn tabular_from_range(range: &Range<Data>) -> TabularData {
    let mut rows_iter = range.rows();
    let headers: Vec<String> = rows_iter
        .next()
        .map(|header| header.iter().map(|c| c.to_string()).collect())
        .unwrap_or_default();
    let rows: Vec<Vec<Value>> = rows_iter
        .map(|row| row.iter().map(cell_value).collect())
        .collect();
    let mut table = TabularData::new(headers, rows);
    table.has_header = true;
    table
}

/// Parses spreadsheet workbooks into Binoc's format-neutral tabular model.
///
/// A workbook is a namespace of named sheets, so this is always a CONTAINER
/// parse: the workbook node carries no artifact, and every non-empty sheet
/// becomes a child node (`book.xlsx/>Sheet1`) carrying a `tabular_v1` artifact —
/// even when the workbook holds a single sheet, since sheets have intrinsic
/// names.
#[derive(Default)]
pub struct ExcelParse;

impl ParseRule for ExcelParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-excel.parse".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: excel_extensions(),
                media_types: Vec::new(),
            },
            output: tabular_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let phys = data.local_path(item)?;
        let sheets = read_non_empty_sheets(&phys)?;
        if sheets.is_empty() {
            return Ok(ParseOutput::default());
        }
        let children = children_from_sheets(&item.logical_path, &sheets);
        Ok(ParseOutput {
            bytes: Vec::new(),
            diagnostics: Vec::new(),
            children,
            artifacts: Vec::new(),
            projection: ProjectionHint::default().item_type("Excel workbook"),
        })
    }
}

/// Build one leaf child node (carrying a `tabular_v1` artifact) per sheet,
/// named verbatim by the sheet's intrinsic name and parented under the workbook
/// via the decompose boundary (`book.xlsx/>Sheet1`).
fn children_from_sheets(logical_path: &str, sheets: &[NonEmptySheet]) -> Vec<ParsedChild> {
    let mut children = Vec::new();
    for sheet in sheets {
        let child_path = decompose_child(logical_path, &sheet.name);
        let table = tabular_from_range(&sheet.range);
        let bytes = serde_json::to_vec(&table)
            .expect("serializing parse-owned tabular sheet should not fail");
        children.push(ParsedChild {
            item: ItemRef {
                logical_path: child_path.clone(),
                is_dir: false,
                content_hash: Some(blake3::hash(&bytes).to_hex().to_string()),
                size: Some(bytes.len() as u64),
                media_type: Some("application/vnd.binoc.tabular+json".into()),
                projection_hint: ProjectionHint::default().item_type("tabular"),
                handle: child_path,
            },
            artifacts: vec![ParsedArtifact {
                format: tabular_v1(),
                bytes,
            }],
        });
    }
    children
}
