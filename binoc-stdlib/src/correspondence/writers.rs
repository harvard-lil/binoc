use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::{
    file_name, structured_document_v1, tabular_extract, tabular_v1, BinocError, BinocResult,
    DataAccess, Diagnostic, DiffNode, Edit, EditListWriter, ExtractResult, IdentityFailurePolicy,
    LinkCtx, NodeId, NodeMatch, ShapeFilter, StructuredDocument, TabularData, TabularDataPair,
    Value, WriteOutput, WriterDescriptor,
};
use rust_strings::{strings, BytesConfig, Encoding};
use serde_json::json;
use similar::{ChangeTag, TextDiff};

use super::parse::JsonSourceFacts;

const MAX_CAPTURED_VALUES: usize = 16;
const MAX_VALUE_PREVIEW_BYTES: usize = 120;
const MAX_TEXT_LINE_EXAMPLES: usize = 8;
const MAX_ROW_ALIGNMENT_ROWS: usize = 512;
const MAX_JSON_CHANGE_EXAMPLES: usize = 16;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Minimum run length (in characters) for an extracted-strings fallback. Matches
/// the conventional `strings(1)` default; fixed so extraction is deterministic.
const STRINGS_MIN_LENGTH: usize = 4;
/// Cap on the number of distinct added/removed string examples surfaced per
/// side, so the strings projection stays bounded for large binaries.
const MAX_STRINGS_EXAMPLES: usize = 32;
/// Cap on total bytes scanned per side when extracting strings, so the
/// projection stays bounded for very large binaries. Extraction beyond this
/// prefix is skipped and flagged via `scan_truncated`.
const MAX_STRINGS_SCAN_BYTES: usize = 1 << 20; // 1 MiB

pub struct ContainerWriter;

impl EditListWriter for ContainerWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.container".into(),
            formats: vec![],
            input: NodeMatch::default(),
            shape: ShapeFilter::Container,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, _data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let mut edits = Vec::new();
        for left_id in ctx.view.children(ctx.link.left) {
            if !ctx.view.is_linked(left_id) {
                edits.push(
                    Edit::new(
                        "container.remove_child",
                        json!({ "name": file_name(&ctx.view.item(left_id).logical_path) }),
                    )
                    .hidden(),
                );
            }
        }
        for right_id in ctx.view.children(ctx.link.right) {
            if !ctx.view.is_linked(right_id) {
                edits.push(
                    Edit::new(
                        "container.add_child",
                        json!({ "name": file_name(&ctx.view.item(right_id).logical_path) }),
                    )
                    .hidden(),
                );
            }
        }
        Ok(Some(edits.into()))
    }
}

pub struct TabularWriter;

impl EditListWriter for TabularWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.tabular".into(),
            formats: vec![tabular_v1()],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let (Some(left), Some(right)) = (
            load_tabular(ctx, ctx.link.left, data)?,
            load_tabular(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };

        let mut edits = Vec::new();
        let mut diagnostics = Vec::new();
        if left.headers != right.headers {
            edits.push(
                Edit::new(
                    "tabular.set_headers",
                    json!({ "from": left.headers.clone(), "to": right.headers.clone() }),
                )
                .with_item_type("tabular"),
            );
        }

        for header in &right.headers {
            if !left.headers.contains(header) {
                edits.push(Edit::new(
                    "tabular.add_column",
                    json!({
                        "name": header,
                        "values": capture_values(right.column_values(header).unwrap_or_default())
                    }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.column-addition")
                .with_tag("binoc.schema-change"));
            }
        }
        for header in &left.headers {
            if !right.headers.contains(header) {
                edits.push(
                    Edit::new(
                        "tabular.remove_column",
                        json!({
                            "name": header,
                            "values": capture_values(left.column_values(header).unwrap_or_default())
                        }),
                    )
                    .with_item_type("tabular")
                    .with_tag("binoc.column-removal")
                    .with_tag("binoc.schema-change"),
                );
            }
        }

        if !ctx.row_keys.is_empty() && keys_present(ctx.row_keys, &left, &right) {
            if keyed_rows_complete(ctx.row_keys, &left, &right) {
                write_keyed_row_edits(&mut edits, ctx.row_keys, &left, &right);
                return Ok(Some(WriteOutput { edits, diagnostics }));
            }
            let quality = key_quality(ctx.row_keys, &left, &right);
            push_key_quality_diagnostics(&mut diagnostics, quality, ctx.row_identity_policies);
            if let Some(edit) = key_quality_edit(quality, ctx.row_identity_policies) {
                edits.push(edit);
            }
        }

        let common: Vec<&String> = left
            .headers
            .iter()
            .filter(|header| right.headers.contains(header))
            .collect();
        if left.rows.len() != right.rows.len()
            && left.rows.len() <= MAX_ROW_ALIGNMENT_ROWS
            && right.rows.len() <= MAX_ROW_ALIGNMENT_ROWS
            && !common.is_empty()
        {
            edits.push(row_alignment_basis(&left, &right, &common));
        }
        let min_rows = left.rows.len().min(right.rows.len());
        for index in 0..min_rows {
            for column in &common {
                let left_index = left.column_index(column).expect("common column");
                let right_index = right.column_index(column).expect("common column");
                let left_value = left.rows[index].get(left_index).unwrap_or(&Value::Null);
                let right_value = right.rows[index].get(right_index).unwrap_or(&Value::Null);
                if left_value != right_value {
                    edits.push(cell_edit(json!({
                        "row": index,
                        "column": column,
                        "from": value_preview(left_value),
                        "to": value_preview(right_value)
                    })));
                }
            }
        }
        for (index, row) in right.rows.iter().enumerate().skip(min_rows) {
            edits.push(row_add_edit(
                json!({ "index": index, "values": capture_row(row) }),
            ));
        }
        for (index, row) in left.rows.iter().enumerate().skip(min_rows) {
            edits.push(row_remove_edit(
                json!({ "index": index, "values": capture_row(row) }),
            ));
        }

        Ok(Some(WriteOutput { edits, diagnostics }))
    }

    fn extract(
        &self,
        ctx: &LinkCtx<'_>,
        _edits: &[Edit],
        aspect: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<Option<ExtractResult>> {
        let pair = TabularDataPair {
            left: load_tabular(ctx, ctx.link.left, data)?,
            right: load_tabular(ctx, ctx.link.right, data)?,
        };
        if pair.left.is_none() && pair.right.is_none() {
            return Ok(None);
        }
        if aspect == "column_order" {
            return Ok(Some(column_order_extract(&pair)));
        }
        Ok(tabular_extract(
            &pair,
            &DiffNode::new(
                "modify",
                "tabular",
                &ctx.view.item(ctx.link.right).logical_path,
            ),
            aspect,
        ))
    }
}

fn column_order_extract(pair: &TabularDataPair) -> ExtractResult {
    let mut out = String::new();
    if let Some(left) = &pair.left {
        out.push_str("before: ");
        out.push_str(&left.headers.join(", "));
        out.push('\n');
    }
    if let Some(right) = &pair.right {
        out.push_str("after:  ");
        out.push_str(&right.headers.join(", "));
        out.push('\n');
    }
    ExtractResult::Text(out)
}

fn load_tabular(
    ctx: &LinkCtx<'_>,
    id: NodeId,
    data: &dyn DataAccess,
) -> BinocResult<Option<TabularData>> {
    let Some(bytes) = ctx.view.artifact_bytes(id, &tabular_v1(), data)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| BinocError::Other(format!("decode tabular artifact: {err}")))
}

fn keys_present(keys: &[String], left: &TabularData, right: &TabularData) -> bool {
    keys.iter()
        .all(|key| left.column_index(key).is_some() && right.column_index(key).is_some())
}

fn keyed_rows_complete(keys: &[String], left: &TabularData, right: &TabularData) -> bool {
    table_has_complete_unique_keys(left, keys) && table_has_complete_unique_keys(right, keys)
}

#[derive(Debug, Clone, Copy, Default)]
struct KeyQuality {
    has_null: bool,
    has_duplicate: bool,
}

fn key_quality(keys: &[String], left: &TabularData, right: &TabularData) -> KeyQuality {
    let left = table_key_quality(left, keys);
    let right = table_key_quality(right, keys);
    KeyQuality {
        has_null: left.has_null || right.has_null,
        has_duplicate: left.has_duplicate || right.has_duplicate,
    }
}

fn table_key_quality(table: &TabularData, keys: &[String]) -> KeyQuality {
    let mut quality = KeyQuality::default();
    let mut counts = BTreeMap::new();
    for row in &table.rows {
        let Some(key) = row_key(table, keys, row) else {
            quality.has_null = true;
            continue;
        };
        *counts.entry(key.signature).or_insert(0usize) += 1;
    }
    quality.has_duplicate = counts.values().any(|count| *count > 1);
    quality
}

fn push_key_quality_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    quality: KeyQuality,
    policies: binoc_sdk::RowIdentityPolicies,
) {
    push_key_quality_diagnostic(
        diagnostics,
        quality.has_null,
        policies.on_null_key,
        "configured row keys had null values",
    );
    push_key_quality_diagnostic(
        diagnostics,
        quality.has_duplicate,
        policies.on_duplicate_key,
        "configured row keys had duplicate values",
    );
}

fn push_key_quality_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    present: bool,
    policy: IdentityFailurePolicy,
    reason: &str,
) {
    if !present || policy == IdentityFailurePolicy::Ignore {
        return;
    }
    let message = format!("{reason}; fell back to positional row comparison");
    diagnostics.push(match policy {
        IdentityFailurePolicy::Diagnostic => {
            Diagnostic::warning("binoc.keyed_row_identity_degraded", message)
        }
        IdentityFailurePolicy::Error => {
            Diagnostic::error("binoc.keyed_row_identity_degraded", message)
        }
        IdentityFailurePolicy::Ignore => unreachable!("ignore returned above"),
    });
}

fn key_quality_edit(quality: KeyQuality, policies: binoc_sdk::RowIdentityPolicies) -> Option<Edit> {
    let include_null = quality.has_null && policies.on_null_key != IdentityFailurePolicy::Ignore;
    let include_duplicate =
        quality.has_duplicate && policies.on_duplicate_key != IdentityFailurePolicy::Ignore;
    if !include_null && !include_duplicate {
        return None;
    }
    let mut edit = Edit::new("tabular.row_identity_degraded", json!({}))
        .with_item_type("tabular")
        .with_tag("binoc.identity-diagnostic")
        .with_tag("binoc.row-identity-ambiguous")
        .hidden();
    if include_null {
        edit = edit.with_tag("binoc.null-key");
    }
    if include_duplicate {
        edit = edit.with_tag("binoc.duplicate-key");
    }
    Some(edit)
}

fn table_has_complete_unique_keys(table: &TabularData, keys: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    for row in &table.rows {
        let Some(key) = row_key(table, keys, row) else {
            return false;
        };
        if !seen.insert(key.signature) {
            return false;
        }
    }
    true
}

/// A row's identity key: a `signature` of the cells' flat text for
/// grouping/ordering (so ordering matches the legacy all-string behavior) plus
/// the typed cell `values` for rendering.
#[derive(Debug, Clone)]
struct RowKey {
    signature: Vec<String>,
    values: Vec<Value>,
}

fn row_key(table: &TabularData, keys: &[String], row: &[Value]) -> Option<RowKey> {
    let mut values = Vec::with_capacity(keys.len());
    let mut signature = Vec::with_capacity(keys.len());
    for key in keys {
        let value = row.get(table.column_index(key)?).unwrap_or(&Value::Null);
        if value.is_blank() {
            return None;
        }
        signature.push(value.as_text().into_owned());
        values.push(value.clone());
    }
    Some(RowKey { signature, values })
}

fn unique_rows_by_key<'a>(
    table: &'a TabularData,
    keys: &[String],
) -> BTreeMap<Vec<String>, (RowKey, usize, &'a Vec<Value>)> {
    let mut counts = BTreeMap::new();
    let mut rows = BTreeMap::new();
    for (index, row) in table.rows.iter().enumerate() {
        let Some(key) = row_key(table, keys, row) else {
            continue;
        };
        *counts.entry(key.signature.clone()).or_insert(0usize) += 1;
        rows.entry(key.signature.clone())
            .or_insert((key, index, row));
    }
    rows.retain(|sig, _| counts.get(sig).copied().unwrap_or(0) == 1);
    rows
}

fn key_json(keys: &[String], values: &[Value]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (key, value) in keys.iter().zip(values) {
        object.insert(key.clone(), value.to_json());
    }
    serde_json::Value::Object(object)
}

fn write_keyed_row_edits(
    edits: &mut Vec<Edit>,
    keys: &[String],
    left: &TabularData,
    right: &TabularData,
) {
    let left_rows = unique_rows_by_key(left, keys);
    let right_rows = unique_rows_by_key(right, keys);
    let left_keys: BTreeSet<Vec<String>> = left_rows.keys().cloned().collect();
    let right_keys: BTreeSet<Vec<String>> = right_rows.keys().cloned().collect();

    for sig in left_keys.difference(&right_keys) {
        let (key, index, row) = left_rows.get(sig).expect("known left key");
        edits.push(row_remove_edit(json!({
            "index": index,
            "key": key_json(keys, &key.values),
            "values": capture_row(row)
        })));
    }
    for sig in right_keys.difference(&left_keys) {
        let (key, index, row) = right_rows.get(sig).expect("known right key");
        edits.push(row_add_edit(json!({
            "index": index,
            "key": key_json(keys, &key.values),
            "values": capture_row(row)
        })));
    }

    let common: Vec<&String> = left
        .headers
        .iter()
        .filter(|header| right.headers.contains(header) && !keys.contains(header))
        .collect();
    for sig in left_keys.intersection(&right_keys) {
        let (key, _, left_row) = left_rows.get(sig).expect("known left key");
        let (_, _, right_row) = right_rows.get(sig).expect("known right key");
        for column in &common {
            let left_index = left.column_index(column).expect("common column");
            let right_index = right.column_index(column).expect("common column");
            let left_value = left_row.get(left_index).unwrap_or(&Value::Null);
            let right_value = right_row.get(right_index).unwrap_or(&Value::Null);
            if left_value != right_value {
                edits.push(cell_edit(json!({
                    "key": key_json(keys, &key.values),
                    "column": column,
                    "from": value_preview(left_value),
                    "to": value_preview(right_value)
                })));
            }
        }
    }
}

fn capture_row(row: &[Value]) -> serde_json::Value {
    let values: Vec<serde_json::Value> = row
        .iter()
        .take(MAX_CAPTURED_VALUES)
        .map(value_preview)
        .collect();
    json!({
        "values": values,
        "total_values": row.len(),
        "truncated": row.len() > MAX_CAPTURED_VALUES,
    })
}

fn row_alignment_basis(left: &TabularData, right: &TabularData, common: &[&String]) -> Edit {
    let left_rows: Vec<String> = left
        .rows
        .iter()
        .map(|row| row_signature(left, row, common))
        .collect();
    let right_rows: Vec<String> = right
        .rows
        .iter()
        .map(|row| row_signature(right, row, common))
        .collect();
    let captured_right: Vec<serde_json::Value> =
        right.rows.iter().map(|row| capture_row(row)).collect();

    Edit::new(
        "tabular.row_alignment_basis",
        json!({
            "columns": common.iter().map(|column| column.as_str()).collect::<Vec<_>>(),
            "left": left_rows,
            "right": right_rows,
            "right_rows": captured_right,
        }),
    )
    .hidden()
}

fn row_signature(table: &TabularData, row: &[Value], common: &[&String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for column in common {
        let index = table.column_index(column).expect("common column");
        row.get(index)
            .unwrap_or(&Value::Null)
            .hash_into(&mut hasher);
    }
    hasher.finalize().to_hex().to_string()
}

fn capture_values(values: Vec<&Value>) -> serde_json::Value {
    let preview: Vec<serde_json::Value> = values
        .iter()
        .take(MAX_CAPTURED_VALUES)
        .map(|value| value_preview(value))
        .collect();
    json!({
        "values": preview,
        "total_values": values.len(),
        "truncated": values.len() > MAX_CAPTURED_VALUES,
    })
}

/// A previewed cell value as JSON. String cells are truncated and stay JSON
/// strings (preserving CSV byte-stability); all other variants pass through as
/// their natural JSON.
fn value_preview(value: &Value) -> serde_json::Value {
    match value {
        Value::String(s) => serde_json::Value::String(truncate_preview(s)),
        other => other.to_json(),
    }
}

fn truncate_preview(value: &str) -> String {
    if value.len() <= MAX_VALUE_PREVIEW_BYTES {
        return value.to_string();
    }
    let mut end = MAX_VALUE_PREVIEW_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

fn row_add_edit(params: serde_json::Value) -> Edit {
    Edit::new("tabular.add_row", params)
        .with_item_type("tabular")
        .with_tag("binoc.row-addition")
}

fn row_remove_edit(params: serde_json::Value) -> Edit {
    Edit::new("tabular.remove_row", params)
        .with_item_type("tabular")
        .with_tag("binoc.row-removal")
}

fn cell_edit(params: serde_json::Value) -> Edit {
    Edit::new("tabular.edit_cell", params)
        .with_item_type("tabular")
        .with_tag("binoc.cell-change")
}

pub struct FallbackWriter;

pub struct TextWriter;

/// Diffs the generic `structured_document` artifact (JSON, YAML, TOML, INI,
/// CBOR, MessagePack, BSON, Plist, Ion, ...). The emitted `item_type` is the
/// document's source format, and the vocabulary is format-neutral
/// (`document.value_change` / `document.serialization_change`).
pub struct StructuredDocumentWriter;

impl EditListWriter for StructuredDocumentWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.structured_document".into(),
            formats: vec![structured_document_v1()],
            input: NodeMatch::default(),
            shape: ShapeFilter::Leaf,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let (Some(left), Some(right)) = (
            load_structured_document(ctx, ctx.link.left, data)?,
            load_structured_document(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };
        // The node's item_type is the document's source format (json, yaml,
        // toml, cbor, ...) so the output names the format honestly.
        let item_type = if right.format.is_empty() {
            left.format.clone()
        } else {
            right.format.clone()
        };
        if left.value == right.value {
            let left_bytes = data.read_bytes(ctx.view.item(ctx.link.left))?;
            let right_bytes = data.read_bytes(ctx.view.item(ctx.link.right))?;
            if left_bytes == right_bytes {
                return Ok(Some(Vec::new().into()));
            }
            return Ok(Some(
                vec![Edit::new(
                    "document.serialization_change",
                    json!({
                        "kinds": json_serialization_change_kinds(&left, &right),
                        "left": &left.source,
                        "right": &right.source,
                    }),
                )
                .with_item_type(item_type)
                .with_tag("binoc.serialization-change")
                .with_tag("binoc.document-serialization-change")
                .with_summary("Document serialization changed")]
                .into(),
            ));
        }

        let changes = json_value_changes(&left.value, &right.value);
        Ok(Some(
            vec![
                Edit::new(
                    "document.value_change",
                    json!({
                        "changes": changes,
                        "examples_truncated": json_change_count(&left.value, &right.value) > MAX_JSON_CHANGE_EXAMPLES,
                    }),
                )
                .with_item_type(item_type)
                .with_tag("binoc.content-changed")
                .with_tag("binoc.document-value-change")
                .with_summary("Document values changed"),
            ]
            .into(),
        ))
    }
}

fn load_structured_document(
    ctx: &LinkCtx<'_>,
    id: NodeId,
    data: &dyn DataAccess,
) -> BinocResult<Option<StructuredDocument>> {
    let Some(bytes) = ctx
        .view
        .artifact_bytes(id, &structured_document_v1(), data)?
    else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| BinocError::Other(format!("decode structured document artifact: {err}")))
}

fn json_source_facts(doc: &StructuredDocument) -> JsonSourceFacts {
    serde_json::from_value(doc.source.clone()).unwrap_or(JsonSourceFacts {
        byte_len: 0,
        trailing_newline: false,
        line_ending: None,
        indentation: None,
        object_key_orders: Vec::new(),
    })
}

fn json_serialization_change_kinds(
    left: &StructuredDocument,
    right: &StructuredDocument,
) -> Vec<&'static str> {
    let left = json_source_facts(left);
    let right = json_source_facts(right);
    let mut kinds = Vec::new();
    if left.object_key_orders != right.object_key_orders {
        kinds.push("object_key_order");
    }
    if left.line_ending != right.line_ending
        || left.indentation != right.indentation
        || left.trailing_newline != right.trailing_newline
        || left.byte_len != right.byte_len
    {
        kinds.push("formatting");
    }
    if kinds.is_empty() {
        kinds.push("serialization");
    }
    kinds
}

fn json_value_changes(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut changes = Vec::new();
    collect_json_value_changes("$", left, right, &mut changes, MAX_JSON_CHANGE_EXAMPLES);
    changes
}

fn json_change_count(left: &serde_json::Value, right: &serde_json::Value) -> usize {
    let mut changes = Vec::new();
    collect_json_value_changes("$", left, right, &mut changes, usize::MAX);
    changes.len()
}

fn collect_json_value_changes(
    path: &str,
    left: &serde_json::Value,
    right: &serde_json::Value,
    changes: &mut Vec<serde_json::Value>,
    limit: usize,
) {
    if left == right || changes.len() >= limit {
        return;
    }
    match (left, right) {
        (serde_json::Value::Object(left_map), serde_json::Value::Object(right_map)) => {
            for key in left_map.keys() {
                if !right_map.contains_key(key) {
                    push_json_change(
                        changes,
                        limit,
                        "remove",
                        &json_child_path(path, key),
                        Some(&left_map[key]),
                        None,
                    );
                }
            }
            for key in right_map.keys() {
                if !left_map.contains_key(key) {
                    push_json_change(
                        changes,
                        limit,
                        "add",
                        &json_child_path(path, key),
                        None,
                        Some(&right_map[key]),
                    );
                }
            }
            for key in left_map.keys().filter(|key| right_map.contains_key(*key)) {
                collect_json_value_changes(
                    &json_child_path(path, key),
                    &left_map[key],
                    &right_map[key],
                    changes,
                    limit,
                );
            }
        }
        (serde_json::Value::Array(left_items), serde_json::Value::Array(right_items)) => {
            if left_items.len() != right_items.len() {
                push_json_change(
                    changes,
                    limit,
                    "array_length",
                    path,
                    Some(&json!(left_items.len())),
                    Some(&json!(right_items.len())),
                );
            }
            let common = left_items.len().min(right_items.len());
            for index in 0..common {
                collect_json_value_changes(
                    &format!("{path}[{index}]"),
                    &left_items[index],
                    &right_items[index],
                    changes,
                    limit,
                );
            }
            for (index, item) in left_items.iter().enumerate().skip(common) {
                push_json_change(
                    changes,
                    limit,
                    "remove",
                    &format!("{path}[{index}]"),
                    Some(item),
                    None,
                );
            }
            for (index, item) in right_items.iter().enumerate().skip(common) {
                push_json_change(
                    changes,
                    limit,
                    "add",
                    &format!("{path}[{index}]"),
                    None,
                    Some(item),
                );
            }
        }
        _ => push_json_change(changes, limit, "replace", path, Some(left), Some(right)),
    }
}

fn push_json_change(
    changes: &mut Vec<serde_json::Value>,
    limit: usize,
    kind: &str,
    path: &str,
    from: Option<&serde_json::Value>,
    to: Option<&serde_json::Value>,
) {
    if changes.len() >= limit {
        return;
    }
    changes.push(json!({
        "kind": kind,
        "path": path,
        "from": from.map(json_value_preview),
        "to": to.map(json_value_preview),
    }));
}

fn json_value_preview(value: &serde_json::Value) -> String {
    serde_json::to_string(value)
        .map(|text| truncate_preview(&text))
        .unwrap_or_else(|_| "<unrenderable>".into())
}

fn json_child_path(parent: &str, key: &str) -> String {
    if key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        format!("{parent}.{key}")
    } else {
        format!("{parent}[{}]", serde_json::to_string(key).expect("string"))
    }
}

impl EditListWriter for TextWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.text".into(),
            formats: vec![],
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".txt".into(), ".md".into(), ".vcf".into()],
                ..NodeMatch::default()
            },
            shape: ShapeFilter::Leaf,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let left = ctx.view.item(ctx.link.left);
        let right = ctx.view.item(ctx.link.right);
        let left_bytes = data.read_bytes(left)?;
        let right_bytes = data.read_bytes(right)?;
        if left_bytes == right_bytes {
            return Ok(Some(Vec::new().into()));
        }

        let left_facts = TextFacts::from_bytes(&left_bytes);
        let right_facts = TextFacts::from_bytes(&right_bytes);
        let mut edits = text_fact_edits(&left_facts, &right_facts);
        if left_facts.normalized_text == right_facts.normalized_text {
            return Ok(Some(edits.into()));
        }
        if whitespace_signature(&left_facts.normalized_text)
            == whitespace_signature(&right_facts.normalized_text)
        {
            edits.push(
                Edit::new(
                    "text.whitespace_only_changed",
                    json!({
                        "left_line_count": left_facts.lines.len(),
                        "right_line_count": right_facts.lines.len(),
                    }),
                )
                .with_item_type("text")
                .with_tag("binoc.whitespace-only-change"),
            );
            return Ok(Some(edits.into()));
        }

        let left_text = left_facts.normalized_text.as_str();
        let right_text = right_facts.normalized_text.as_str();
        let left_lines: Vec<&str> = left_text.lines().collect();
        let right_lines: Vec<&str> = right_text.lines().collect();
        let diff = TextDiff::from_lines(left_text, right_text);
        let mut lines_added = 0u64;
        let mut lines_removed = 0u64;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => lines_added += 1,
                ChangeTag::Delete => lines_removed += 1,
                ChangeTag::Equal => {}
            }
        }
        let common = left_lines.len().min(right_lines.len());
        let mut examples = Vec::new();
        for index in 0..common {
            if left_lines[index] != right_lines[index] {
                examples.push(json!({
                    "line": index + 1,
                    "from": truncate_preview(left_lines[index]),
                    "to": truncate_preview(right_lines[index]),
                }));
            }
            if examples.len() >= MAX_TEXT_LINE_EXAMPLES {
                break;
            }
        }
        let mut edit = Edit::new(
            "text.replace_lines",
            json!({
                "left_line_count": left_lines.len(),
                "right_line_count": right_lines.len(),
                "lines_added": lines_added,
                "lines_removed": lines_removed,
                "examples": examples,
                "examples_truncated": examples.len() >= MAX_TEXT_LINE_EXAMPLES,
            }),
        )
        .with_item_type("text")
        .with_tag("binoc.content-changed");
        if lines_added > 0 {
            edit = edit.with_tag("binoc.lines-added");
        }
        if lines_removed > 0 {
            edit = edit.with_tag("binoc.lines-removed");
        }
        edits.push(edit);
        Ok(Some(edits.into()))
    }

    fn extract(
        &self,
        ctx: &LinkCtx<'_>,
        _edits: &[Edit],
        aspect: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<Option<ExtractResult>> {
        let left = ctx.view.item(ctx.link.left);
        let right = ctx.view.item(ctx.link.right);
        let left_bytes = data.read_bytes(left)?;
        let right_bytes = data.read_bytes(right)?;
        let left_text = String::from_utf8_lossy(&left_bytes);
        let right_text = String::from_utf8_lossy(&right_bytes);
        Ok(match aspect {
            "content_left" => Some(ExtractResult::Text(left_text.into_owned())),
            "content_right" => Some(ExtractResult::Text(right_text.into_owned())),
            "content" | "full" => Some(ExtractResult::Text(format!(
                "--- left\n{}+++ right\n{}",
                ensure_trailing_newline(&left_text),
                ensure_trailing_newline(&right_text)
            ))),
            "diff" => Some(ExtractResult::Text(text_diff_extract(
                &left_text,
                &right_text,
            ))),
            _ => None,
        })
    }
}

#[derive(Debug, Clone)]
struct TextFacts {
    has_utf8_bom: bool,
    utf8_valid: bool,
    line_ending: &'static str,
    normalized_text: String,
    lines: Vec<String>,
}

impl TextFacts {
    fn from_bytes(bytes: &[u8]) -> Self {
        let has_utf8_bom = bytes.starts_with(UTF8_BOM);
        let body = if has_utf8_bom {
            &bytes[UTF8_BOM.len()..]
        } else {
            bytes
        };
        let utf8_valid = std::str::from_utf8(body).is_ok();
        let text = String::from_utf8_lossy(body).into_owned();
        let line_ending = detect_line_ending(&text);
        let normalized_text = normalize_line_endings(&text);
        let lines = normalized_text.lines().map(str::to_string).collect();
        Self {
            has_utf8_bom,
            utf8_valid,
            line_ending,
            normalized_text,
            lines,
        }
    }
}

fn text_fact_edits(left: &TextFacts, right: &TextFacts) -> Vec<Edit> {
    let mut edits = Vec::new();
    if left.line_ending != right.line_ending {
        edits.push(
            Edit::new(
                "text.line_endings_changed",
                json!({ "from": left.line_ending, "to": right.line_ending }),
            )
            .with_item_type("text")
            .with_tag("binoc.line-ending-change"),
        );
    }
    if left.has_utf8_bom != right.has_utf8_bom {
        edits.push(
            Edit::new(
                "text.bom_changed",
                json!({ "from": left.has_utf8_bom, "to": right.has_utf8_bom }),
            )
            .with_item_type("text")
            .with_tag("binoc.bom-change")
            .with_tag("binoc.encoding-change"),
        );
    }
    if left.utf8_valid != right.utf8_valid {
        edits.push(
            Edit::new(
                "text.encoding_changed",
                json!({ "from_utf8_valid": left.utf8_valid, "to_utf8_valid": right.utf8_valid }),
            )
            .with_item_type("text")
            .with_tag("binoc.encoding-change"),
        );
    }
    edits
}

fn detect_line_ending(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    let total_lf = text.matches('\n').count();
    let lone_lf = total_lf.saturating_sub(crlf);
    let bytes = text.as_bytes();
    let lone_cr = bytes
        .iter()
        .enumerate()
        .filter(|(index, byte)| **byte == b'\r' && bytes.get(index + 1).copied() != Some(b'\n'))
        .count();
    match (crlf > 0, lone_lf > 0, lone_cr > 0) {
        (false, false, false) => "none",
        (true, false, false) => "crlf",
        (false, true, false) => "lf",
        (false, false, true) => "cr",
        _ => "mixed",
    }
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn whitespace_signature(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

fn ensure_trailing_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

fn text_diff_extract(left: &str, right: &str) -> String {
    let mut out = String::new();
    for change in TextDiff::from_lines(left, right).iter_all_changes() {
        let prefix = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        out.push_str(prefix);
        out.push_str(change.value());
    }
    out
}

impl EditListWriter for FallbackWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.fallback".into(),
            formats: vec![],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let left = ctx.view.item(ctx.link.left);
        let right = ctx.view.item(ctx.link.right);
        if left.is_dir || right.is_dir {
            return Ok(Some(Vec::new().into()));
        }
        // The BLAKE3/byte hash is the SOLE equality oracle. Equal hash ⇒ no
        // change, regardless of any extracted strings.
        if left.resolve_hash(data)? == right.resolve_hash(data)? {
            return Ok(Some(Vec::new().into()));
        }
        // Hashes differ: the binary content changed. This fact stands on its
        // own. The strings diff below is a purely ADDITIVE projection layered
        // on top — it never decides equality and never suppresses this fact
        // (e.g. a PDF whose only change is an embedded timestamp still reports
        // "binary content changed" even if extracted strings are identical).
        let mut edit = Edit::new("binary.contents-differ", json!({}))
            .with_item_type("file")
            .with_tag("binoc.content-changed");

        let left_bytes = data.read_bytes(left)?;
        let right_bytes = data.read_bytes(right)?;
        if let Some(strings_diff) = extract_strings_diff(&left_bytes, &right_bytes) {
            edit.params = json!({ "strings": strings_diff.params });
            edit = edit
                .with_tag("binoc.strings-changed")
                .with_summary(strings_diff.summary);
        }

        Ok(Some(vec![edit].into()))
    }
}

/// A bounded, deterministic extracted-strings diff between two opaque binary
/// leaves whose byte hashes already differ.
struct StringsDiff {
    params: serde_json::Value,
    summary: String,
}

/// Extract printable string runs from each side and diff them, so a renderer
/// can show a strings-level view of otherwise-unreadable files.
///
/// ADDITIVE ONLY: the caller invokes this exclusively when the byte hashes
/// already differ. This function never reports equality; if the extracted
/// strings happen to match on both sides it reports an empty added/removed set
/// (and the renderer can note that non-string bytes changed), but the
/// underlying "binary content changed" fact is unaffected.
///
/// Determinism: fixed [`STRINGS_MIN_LENGTH`], ASCII + UTF-16LE encodings,
/// stable lexicographic ordering, and a per-side scan/example cap. Returns
/// `None` only when extraction yields nothing on either side (e.g. a file with
/// no printable runs), in which case the bare "binary content changed" edit is
/// emitted unadorned.
fn extract_strings_diff(left_bytes: &[u8], right_bytes: &[u8]) -> Option<StringsDiff> {
    let (left, left_truncated) = extract_strings(left_bytes);
    let (right, right_truncated) = extract_strings(right_bytes);
    if left.is_empty() && right.is_empty() {
        return None;
    }

    // Stable, deduplicated set difference over the extracted runs.
    let added: Vec<&String> = right.difference(&left).collect();
    let removed: Vec<&String> = left.difference(&right).collect();

    let added_total = added.len();
    let removed_total = removed.len();
    let added_examples: Vec<String> = added
        .iter()
        .take(MAX_STRINGS_EXAMPLES)
        .map(|s| truncate_preview(s))
        .collect();
    let removed_examples: Vec<String> = removed
        .iter()
        .take(MAX_STRINGS_EXAMPLES)
        .map(|s| truncate_preview(s))
        .collect();

    let summary = match (added_total, removed_total) {
        (0, 0) => "Binary content changed (extracted strings unchanged)".to_string(),
        (a, 0) => format!(
            "Binary content changed; {}",
            count_phrase(a, "extracted string added", "extracted strings added")
        ),
        (0, r) => format!(
            "Binary content changed; {}",
            count_phrase(r, "extracted string removed", "extracted strings removed")
        ),
        (a, r) => format!(
            "Binary content changed; {}, {}",
            count_phrase(a, "extracted string added", "extracted strings added"),
            count_phrase(r, "extracted string removed", "extracted strings removed")
        ),
    };

    let params = json!({
        "min_length": STRINGS_MIN_LENGTH,
        "left_count": left.len(),
        "right_count": right.len(),
        "added_count": added_total,
        "removed_count": removed_total,
        "added": added_examples,
        "removed": removed_examples,
        "examples_truncated": added_total > MAX_STRINGS_EXAMPLES
            || removed_total > MAX_STRINGS_EXAMPLES,
        "scan_truncated": left_truncated || right_truncated,
    });

    Some(StringsDiff { params, summary })
}

fn count_phrase(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// Extract the set of printable string runs from a byte buffer, scanning at most
/// [`MAX_STRINGS_SCAN_BYTES`]. Returns the deduplicated, sorted runs and a flag
/// indicating whether the buffer was truncated before extraction.
fn extract_strings(bytes: &[u8]) -> (BTreeSet<String>, bool) {
    let truncated = bytes.len() > MAX_STRINGS_SCAN_BYTES;
    let scanned = &bytes[..bytes.len().min(MAX_STRINGS_SCAN_BYTES)];
    let config = BytesConfig::new(scanned.to_vec())
        .with_min_length(STRINGS_MIN_LENGTH)
        .with_encoding(Encoding::ASCII)
        .with_encoding(Encoding::UTF16LE);
    // `strings` only errors on I/O against a file source; a byte buffer cannot
    // fail, so an empty extraction is the only "no strings" outcome.
    let extracted = strings(&config).unwrap_or_default();
    let set = extracted
        .into_iter()
        .map(|(text, _offset)| sanitize_run(&text))
        .collect();
    (set, truncated)
}

/// Normalize whitespace control characters (`\t`, `\n`, `\r`) that
/// `rust-strings` treats as printable into visible escapes, so the strings
/// projection is single-line and safe for any renderer. Deterministic and
/// order-preserving.
fn sanitize_run(text: &str) -> String {
    text.replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}
