use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;

use binoc_sdk::{
    file_name, parser_metadata_v1, structured_document_v1, tabular_extract, tabular_v1, BinocError,
    BinocResult, DataAccess, Diagnostic, DiffNode, Edit, EditListWriter, ExtractResult,
    IdentityFailurePolicy, LinkCtx, NodeId, NodeMatch, ParserMetadata, Segment, ShapeFilter,
    StructuredDocument, Summary, TabularData, TabularDataPair, Value, WriteOutput,
    WriterDescriptor,
};
use fastcdc::v2020::StreamCDC;
use rust_strings::{strings, BytesConfig, Encoding};
use serde_json::json;
use similar::{ChangeTag, TextDiff};

use super::parse::JsonSourceFacts;
use super::tabular::load_tabular;

const MAX_CAPTURED_VALUES: usize = 16;
const MAX_VALUE_PREVIEW_BYTES: usize = 120;
const MAX_TEXT_LINE_EXAMPLES: usize = 8;
const MAX_ROW_ALIGNMENT_ROWS: usize = 512;
const AUTO_KEY_MIN_JACCARD: f64 = 0.80;
const AUTO_KEY_MAX_ROWS: usize = 10_000;
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

/// FastCDC parameters for opaque binary localization. The average chunk size is
/// large enough for long-tail binary files while still localizing common small
/// embedded rewrites to useful byte ranges.
const BINARY_CDC_MIN_CHUNK_BYTES: usize = 4 * 1024;
const BINARY_CDC_AVG_CHUNK_BYTES: usize = 16 * 1024;
const BINARY_CDC_MAX_CHUNK_BYTES: usize = 64 * 1024;
/// Cap the resident per-side chunk vectors. At the configured average chunk
/// size this covers roughly 512 MiB per side while keeping metadata bounded.
const MAX_BINARY_CDC_CHUNKS: usize = 32 * 1024;
/// Keep output deterministic and compact even when many regions differ.
const MAX_BINARY_CDC_REGIONS: usize = 32;

pub struct ContainerWriter;

impl EditListWriter for ContainerWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.container".into(),
            formats: vec![],
            input: NodeMatch::default(),
            shape: ShapeFilter::Container,
            fallback: false,
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
            fallback: false,
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

        // Tier-1/tier-2 metadata changes (column labels/formats/value-label set
        // names; table label/name) are diffed independently of cell/row content
        // and appended AFTER the primary table edits, so the changelog reads
        // "what the table did" then "what its metadata did". They are kept out of
        // the keyed/positional row machinery above so they never duplicate a cell
        // diff. See the tiered-artifact-metadata + per-artifact-writer ADRs.
        let metadata_edits = tabular_metadata_edits(&left, &right);

        if !ctx.row_keys.is_empty() {
            if let Some((left_keyed, right_keyed)) = keyed_tables(ctx.row_keys, &left, &right) {
                if keyed_rows_complete(&left_keyed.index, &right_keyed.index) {
                    write_keyed_row_edits(&mut edits, ctx.row_keys, &left_keyed, &right_keyed);
                    edits.extend(metadata_edits);
                    return Ok(Some(WriteOutput { edits, diagnostics }));
                }
                let quality = key_quality(&left_keyed.index, &right_keyed.index);
                push_key_quality_diagnostics(&mut diagnostics, quality, ctx.row_identity_policies);
                if let Some(edit) = key_quality_edit(quality, ctx.row_identity_policies) {
                    edits.push(edit);
                }
            }
        } else if ctx.row_keys.is_empty() {
            if let Some(auto_key) = infer_auto_key(&left, &right) {
                diagnostics.push(Diagnostic::suggestion(
                    "binoc.tabular_auto_key",
                    format!(
                        "inferred row identity column '{}' from unique values with {:.0}% overlap",
                        auto_key.column,
                        auto_key.jaccard * 100.0
                    ),
                ));
                edits.push(
                    Edit::new(
                        "tabular.auto_detected_key",
                        json!({
                            "columns": [auto_key.column.clone()],
                            "overlap": auto_key.jaccard,
                            "left_rows": left.rows.len(),
                            "right_rows": right.rows.len(),
                        }),
                    )
                    .with_item_type("tabular")
                    .with_tag("binoc.row-identity-inferred")
                    .hidden(),
                );
                let keys = vec![auto_key.column];
                if let Some((left_keyed, right_keyed)) = keyed_tables(&keys, &left, &right) {
                    write_keyed_row_edits(&mut edits, &keys, &left_keyed, &right_keyed);
                }
                edits.extend(metadata_edits);
                return Ok(Some(WriteOutput { edits, diagnostics }));
            }
        }

        let common = common_columns(&left, &right);
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
                let left_value = left.rows[index]
                    .get(column.left_index)
                    .unwrap_or(&Value::Null);
                let right_value = right.rows[index]
                    .get(column.right_index)
                    .unwrap_or(&Value::Null);
                if left_value != right_value {
                    edits.push(cell_edit(json!({
                        "row": index,
                        "column": column.name,
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

        edits.extend(metadata_edits);

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
            left: load_tabular(ctx, ctx.link.left, data)?.map(|table| (*table).clone()),
            right: load_tabular(ctx, ctx.link.right, data)?.map(|table| (*table).clone()),
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

fn keyed_rows_complete(left: &KeyedIndex<'_>, right: &KeyedIndex<'_>) -> bool {
    left.complete() && right.complete()
}

#[derive(Debug, Clone, Copy, Default)]
struct KeyQuality {
    has_null: bool,
    has_duplicate: bool,
}

fn key_quality(left: &KeyedIndex<'_>, right: &KeyedIndex<'_>) -> KeyQuality {
    KeyQuality {
        has_null: left.quality.has_null || right.quality.has_null,
        has_duplicate: left.quality.has_duplicate || right.quality.has_duplicate,
    }
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

#[derive(Debug, Clone)]
struct AutoKeyCandidate {
    column: String,
    jaccard: f64,
    total_value_bytes: usize,
}

fn infer_auto_key(left: &TabularData, right: &TabularData) -> Option<AutoKeyCandidate> {
    if left.rows.len() > AUTO_KEY_MAX_ROWS || right.rows.len() > AUTO_KEY_MAX_ROWS {
        return None;
    }
    let right_headers = header_indices(right);
    left.headers
        .iter()
        .filter(|header| right_headers.contains_key(header.as_str()))
        .filter_map(|header| auto_key_candidate(header, left, right))
        .filter(|candidate| candidate.jaccard >= AUTO_KEY_MIN_JACCARD)
        .min_by(|a, b| {
            b.jaccard
                .total_cmp(&a.jaccard)
                .then_with(|| a.total_value_bytes.cmp(&b.total_value_bytes))
                .then_with(|| a.column.cmp(&b.column))
        })
}

fn auto_key_candidate(
    column: &str,
    left: &TabularData,
    right: &TabularData,
) -> Option<AutoKeyCandidate> {
    let keys = [column.to_string()];
    let (left_keyed, right_keyed) = keyed_tables(&keys, left, right)?;
    if !keyed_rows_complete(&left_keyed.index, &right_keyed.index) {
        return None;
    }
    if !auto_key_would_change_alignment(&left_keyed.index, &right_keyed.index) {
        return None;
    }
    let left_values = column_signatures(left, left_keyed.columns.indices[0]);
    let right_values = column_signatures(right, right_keyed.columns.indices[0]);
    let intersection = left_values.intersection(&right_values).count();
    let union = left_values.union(&right_values).count();
    if union == 0 {
        return None;
    }
    Some(AutoKeyCandidate {
        column: column.to_string(),
        jaccard: intersection as f64 / union as f64,
        total_value_bytes: total_column_value_bytes(left, left_keyed.columns.indices[0])
            + total_column_value_bytes(right, right_keyed.columns.indices[0]),
    })
}

fn auto_key_would_change_alignment(left: &KeyedIndex<'_>, right: &KeyedIndex<'_>) -> bool {
    left.rows.iter().any(|(signature, left_row)| {
        right
            .rows
            .get(signature)
            .is_some_and(|right_row| left_row.index != right_row.index)
    })
}

fn column_signatures(table: &TabularData, index: usize) -> BTreeSet<Cow<'_, str>> {
    table
        .rows
        .iter()
        .filter_map(|row| row.get(index))
        .map(Value::as_text)
        .collect()
}

fn total_column_value_bytes(table: &TabularData, index: usize) -> usize {
    table
        .rows
        .iter()
        .map(|row| row.get(index).unwrap_or(&Value::Null).as_text().len())
        .sum()
}

#[derive(Debug, Clone)]
struct KeyColumns {
    indices: Vec<usize>,
}

fn resolve_key_columns(table: &TabularData, keys: &[String]) -> Option<KeyColumns> {
    keys.iter()
        .map(|key| table.column_index(key))
        .collect::<Option<Vec<_>>>()
        .map(|indices| KeyColumns { indices })
}

fn keyed_tables<'a>(
    keys: &[String],
    left: &'a TabularData,
    right: &'a TabularData,
) -> Option<(KeyedTable<'a>, KeyedTable<'a>)> {
    let left_columns = resolve_key_columns(left, keys)?;
    let right_columns = resolve_key_columns(right, keys)?;
    let left_index = KeyedIndex::build(left, &left_columns);
    let right_index = KeyedIndex::build(right, &right_columns);
    Some((
        KeyedTable {
            table: left,
            columns: left_columns,
            index: left_index,
        },
        KeyedTable {
            table: right,
            columns: right_columns,
            index: right_index,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RowSignature<'a>(Box<[Cow<'a, str>]>);

fn row_signature_for_key<'a>(
    row: &'a [Value],
    key_columns: &KeyColumns,
) -> Option<RowSignature<'a>> {
    let mut signature = Vec::with_capacity(key_columns.indices.len());
    for &index in &key_columns.indices {
        let value = row.get(index).unwrap_or(&Value::Null);
        if value.is_blank() {
            return None;
        }
        signature.push(value.as_text());
    }
    Some(RowSignature(signature.into_boxed_slice()))
}

#[derive(Debug, Clone, Copy)]
struct KeyedRow<'a> {
    index: usize,
    row: &'a Vec<Value>,
}

#[derive(Debug, Clone, Copy)]
enum RowBucket<'a> {
    Unique(KeyedRow<'a>),
    Duplicate,
}

#[derive(Debug)]
struct KeyedIndex<'a> {
    rows: BTreeMap<RowSignature<'a>, KeyedRow<'a>>,
    quality: KeyQuality,
}

impl<'a> KeyedIndex<'a> {
    fn build(table: &'a TabularData, key_columns: &KeyColumns) -> Self {
        let mut quality = KeyQuality::default();
        let mut buckets = BTreeMap::new();

        for (index, row) in table.rows.iter().enumerate() {
            let Some(signature) = row_signature_for_key(row, key_columns) else {
                quality.has_null = true;
                continue;
            };
            match buckets.entry(signature) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(RowBucket::Unique(KeyedRow { index, row }));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    quality.has_duplicate = true;
                    entry.insert(RowBucket::Duplicate);
                }
            }
        }

        let rows = buckets
            .into_iter()
            .filter_map(|(signature, bucket)| match bucket {
                RowBucket::Unique(row) => Some((signature, row)),
                RowBucket::Duplicate => None,
            })
            .collect();

        Self { rows, quality }
    }

    fn complete(&self) -> bool {
        !self.quality.has_null && !self.quality.has_duplicate
    }
}

#[derive(Debug)]
struct KeyedTable<'a> {
    table: &'a TabularData,
    columns: KeyColumns,
    index: KeyedIndex<'a>,
}

fn key_json(keys: &[String], key_columns: &KeyColumns, row: &[Value]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (key, index) in keys.iter().zip(&key_columns.indices) {
        object.insert(
            key.clone(),
            row.get(*index).unwrap_or(&Value::Null).to_json(),
        );
    }
    serde_json::Value::Object(object)
}

#[derive(Debug, Clone)]
struct CommonColumn<'a> {
    name: &'a String,
    left_index: usize,
    right_index: usize,
}

fn header_indices(table: &TabularData) -> BTreeMap<&str, usize> {
    table
        .headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.as_str(), index))
        .collect()
}

fn common_columns<'a>(left: &'a TabularData, right: &TabularData) -> Vec<CommonColumn<'a>> {
    common_columns_by(left, right, |_| true)
}

fn common_non_key_columns<'a>(
    left: &'a TabularData,
    right: &TabularData,
    keys: &[String],
) -> Vec<CommonColumn<'a>> {
    let key_names: BTreeSet<&str> = keys.iter().map(String::as_str).collect();
    common_columns_by(left, right, |header| !key_names.contains(header))
}

fn common_columns_by<'a>(
    left: &'a TabularData,
    right: &TabularData,
    include: impl Fn(&str) -> bool,
) -> Vec<CommonColumn<'a>> {
    let right_headers = header_indices(right);
    let mut columns = Vec::new();
    for (left_index, header) in left.headers.iter().enumerate() {
        if !include(header) {
            continue;
        }
        if let Some(&right_index) = right_headers.get(header.as_str()) {
            columns.push(CommonColumn {
                name: header,
                left_index,
                right_index,
            });
        }
    }
    columns
}

fn write_keyed_row_edits(
    edits: &mut Vec<Edit>,
    keys: &[String],
    left: &KeyedTable<'_>,
    right: &KeyedTable<'_>,
) {
    let left_keys: BTreeSet<&RowSignature<'_>> = left.index.rows.keys().collect();
    let right_keys: BTreeSet<&RowSignature<'_>> = right.index.rows.keys().collect();

    for sig in left_keys.difference(&right_keys) {
        let keyed_row = left.index.rows.get(*sig).expect("known left key");
        edits.push(row_remove_edit(json!({
            "index": keyed_row.index,
            "key": key_json(keys, &left.columns, keyed_row.row),
            "values": capture_row(keyed_row.row)
        })));
    }
    for sig in right_keys.difference(&left_keys) {
        let keyed_row = right.index.rows.get(*sig).expect("known right key");
        edits.push(row_add_edit(json!({
            "index": keyed_row.index,
            "key": key_json(keys, &right.columns, keyed_row.row),
            "values": capture_row(keyed_row.row)
        })));
    }

    let common = common_non_key_columns(left.table, right.table, keys);
    for sig in left_keys.intersection(&right_keys) {
        let left_row = left.index.rows.get(*sig).expect("known left key");
        let right_row = right.index.rows.get(*sig).expect("known right key");
        for column in &common {
            let left_value = left_row.row.get(column.left_index).unwrap_or(&Value::Null);
            let right_value = right_row
                .row
                .get(column.right_index)
                .unwrap_or(&Value::Null);
            if left_value != right_value {
                edits.push(cell_edit(json!({
                    "key": key_json(keys, &left.columns, left_row.row),
                    "column": column.name,
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

fn row_alignment_basis(
    left: &TabularData,
    right: &TabularData,
    common: &[CommonColumn<'_>],
) -> Edit {
    let left_rows: Vec<String> = left
        .rows
        .iter()
        .map(|row| row_signature(row, common.iter().map(|column| column.left_index)))
        .collect();
    let right_rows: Vec<String> = right
        .rows
        .iter()
        .map(|row| row_signature(row, common.iter().map(|column| column.right_index)))
        .collect();
    let captured_right: Vec<serde_json::Value> =
        right.rows.iter().map(|row| capture_row(row)).collect();

    Edit::new(
        "tabular.row_alignment_basis",
        json!({
            "columns": common.iter().map(|column| column.name.as_str()).collect::<Vec<_>>(),
            "left": left_rows,
            "right": right_rows,
            "right_rows": captured_right,
        }),
    )
    .hidden()
}

fn row_signature(row: &[Value], indices: impl IntoIterator<Item = usize>) -> String {
    let mut hasher = blake3::Hasher::new();
    for index in indices {
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

// ── Metadata rendering (CFM-82) ─────────────────────────────────────────────
//
// Three metadata tiers (see the tiered-artifact-metadata ADR) are diffed into a
// single, format-neutral `metadata.value_change` vocabulary:
//   * tier 1 — per-column metadata (label / display format / value-label set
//     name), keyed to a column, emitted by `TabularWriter`;
//   * tier 2 — per-table metadata (dataset name/label), keyed to the table,
//     emitted by `TabularWriter`;
//   * tier 3 — per-parse / file-level metadata (source-format identity, version,
//     encoding, value-label dictionaries, creator/tooling provenance), emitted
//     by `ParserMetadataWriter` from the `parser_metadata_v1` artifact.
//
// Every emitted edit is factual and richly tagged so the renderer/config layer
// can weight a relabeled column, a dropped value-label set, and a file-level
// provenance rename differently (AGENTS rule 3) — significance lives in config,
// not here. The base tag `binoc.metadata-change` marks all of them; a scope tag
// (`binoc.metadata.column` / `.table` / `.file`) and one or more semantic tags
// (`binoc.metadata.column-label`, `binoc.metadata.value-label-set`,
// `binoc.metadata.display-format`, `binoc.metadata.provenance`) let config map
// significance precisely.

const MAX_METADATA_CHANGES: usize = 32;

/// One changed key within a metadata bag, as JSON for an edit's `changes` array.
/// `from`/`to` carry the raw JSON value (the renderer formats and truncates it);
/// an absent key is recorded as JSON `null`.
fn metadata_key_change(
    kind: &str,
    key: &str,
    from: Option<&serde_json::Value>,
    to: Option<&serde_json::Value>,
) -> serde_json::Value {
    json!({
        "kind": kind,
        "key": key,
        "from": from.cloned().unwrap_or(serde_json::Value::Null),
        "to": to.cloned().unwrap_or(serde_json::Value::Null),
    })
}

/// Diff two metadata objects key-by-key, returning the changed keys and the set
/// of semantic tags those keys imply. Non-object inputs are treated as empty.
fn diff_metadata_object(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> (Vec<serde_json::Value>, BTreeSet<String>) {
    let empty = serde_json::Map::new();
    let left_map = left.as_object().unwrap_or(&empty);
    let right_map = right.as_object().unwrap_or(&empty);

    let mut keys: BTreeSet<&String> = BTreeSet::new();
    keys.extend(left_map.keys());
    keys.extend(right_map.keys());

    let mut changes = Vec::new();
    let mut tags = BTreeSet::new();
    for key in keys {
        let left_value = left_map.get(key);
        let right_value = right_map.get(key);
        // Treat JSON null as absent so "label: null -> "X"" reads as an addition.
        let left_present = left_value.is_some_and(|value| !value.is_null());
        let right_present = right_value.is_some_and(|value| !value.is_null());
        if left_value == right_value {
            continue;
        }
        let kind = match (left_present, right_present) {
            (false, true) => "added",
            (true, false) => "removed",
            _ => "changed",
        };
        if changes.len() < MAX_METADATA_CHANGES {
            changes.push(metadata_key_change(kind, key, left_value, right_value));
        }
        if let Some(tag) = metadata_key_tag(key) {
            tags.insert(tag.to_string());
        }
    }
    (changes, tags)
}

/// Map a metadata key to a semantic tag so config can weight it. Unknown keys
/// get no specific tag (they still carry the scope + base tags).
fn metadata_key_tag(key: &str) -> Option<&'static str> {
    match key {
        "label" | "dataset_label" | "dataset_name" => Some("binoc.metadata.column-label"),
        "value_label_set" | "value_labels" => Some("binoc.metadata.value-label-set"),
        "format" => Some("binoc.metadata.display-format"),
        "release" | "version" | "sas_version" | "file_encoding" | "cell_encoding"
        | "compression" | "endianness" | "vendor" => Some("binoc.metadata.provenance"),
        _ => None,
    }
}

/// Build a `metadata.value_change` edit, or `None` when nothing changed.
fn metadata_value_change_edit(
    scope: &str,
    scope_tag: &str,
    locator: serde_json::Value,
    changes: Vec<serde_json::Value>,
    semantic_tags: BTreeSet<String>,
    truncated: bool,
) -> Option<Edit> {
    if changes.is_empty() {
        return None;
    }
    let mut params = serde_json::Map::new();
    params.insert("scope".into(), json!(scope));
    if !locator.is_null() {
        params.insert("locator".into(), locator);
    }
    params.insert("changes".into(), json!(changes));
    params.insert("examples_truncated".into(), json!(truncated));

    let mut edit = Edit::new("metadata.value_change", serde_json::Value::Object(params))
        .with_item_type("metadata")
        .with_tag("binoc.metadata-change")
        .with_tag(scope_tag);
    for tag in semantic_tags {
        edit = edit.with_tag(tag);
    }
    Some(edit)
}

/// Diff tier-1 (per-column) and tier-2 (per-table) metadata carried on
/// `tabular_v1`. Column metadata is matched by column NAME (so a relabeled
/// column reads as a metadata change, and a reordered column does not produce a
/// spurious metadata diff). Columns added/removed are already reported by the
/// schema edits, so a column present on only one side is skipped here.
fn tabular_metadata_edits(left: &TabularData, right: &TabularData) -> Vec<Edit> {
    let mut edits = Vec::new();

    // Tier 1: per-column metadata, matched by column name.
    for (right_index, header) in right.headers.iter().enumerate() {
        let Some(left_index) = left.column_index(header) else {
            continue;
        };
        let left_meta = left
            .column_metadata
            .get(left_index)
            .unwrap_or(&serde_json::Value::Null);
        let right_meta = right
            .column_metadata
            .get(right_index)
            .unwrap_or(&serde_json::Value::Null);
        let (changes, semantic_tags) = diff_metadata_object(left_meta, right_meta);
        let truncated = changes.len() >= MAX_METADATA_CHANGES;
        if let Some(edit) = metadata_value_change_edit(
            "column",
            "binoc.metadata.column",
            json!({ "column": header }),
            changes,
            semantic_tags,
            truncated,
        ) {
            edits.push(edit);
        }
    }

    // Tier 2: per-table metadata.
    let (changes, semantic_tags) =
        diff_metadata_object(&left.table_metadata, &right.table_metadata);
    let truncated = changes.len() >= MAX_METADATA_CHANGES;
    if let Some(edit) = metadata_value_change_edit(
        "table",
        "binoc.metadata.table",
        serde_json::Value::Null,
        changes,
        semantic_tags,
        truncated,
    ) {
        edits.push(edit);
    }

    edits
}

/// Renders tier-3 parser metadata (`parser_metadata_v1`). The sole writer for
/// that format (`fallback: false`), so provenance/extract routing is
/// unambiguous. Composes alongside `TabularWriter`/`ContainerWriter` on the same
/// node under the per-artifact dispatch (CFM-81).
pub struct ParserMetadataWriter;

impl EditListWriter for ParserMetadataWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.parser_metadata".into(),
            formats: vec![parser_metadata_v1()],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
            fallback: false,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let (Some(left), Some(right)) = (
            load_parser_metadata(ctx, ctx.link.left, data)?,
            load_parser_metadata(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };

        let mut edits = Vec::new();

        // A source-format change (e.g. .dta re-saved as .sas7bdat) is itself a
        // file-level provenance fact.
        if left.format != right.format {
            edits.push(
                Edit::new(
                    "metadata.value_change",
                    json!({
                        "scope": "file",
                        "changes": [metadata_key_change(
                            "changed",
                            "source_format",
                            Some(&json!(left.format)),
                            Some(&json!(right.format)),
                        )],
                        "examples_truncated": false,
                    }),
                )
                .with_item_type("metadata")
                .with_tag("binoc.metadata-change")
                .with_tag("binoc.metadata.file")
                .with_tag("binoc.metadata.provenance"),
            );
        }

        let (changes, mut semantic_tags) = diff_metadata_object(&left.value, &right.value);
        // File-level provenance is the default weighting bucket for tier 3, so a
        // changed key with no more-specific tag still routes as provenance.
        if !changes.is_empty() {
            semantic_tags.insert("binoc.metadata.provenance".into());
        }
        let truncated = changes.len() >= MAX_METADATA_CHANGES;
        if let Some(edit) = metadata_value_change_edit(
            "file",
            "binoc.metadata.file",
            serde_json::Value::Null,
            changes,
            semantic_tags,
            truncated,
        ) {
            edits.push(edit);
        }

        Ok(Some(edits.into()))
    }
}

fn load_parser_metadata(
    ctx: &LinkCtx<'_>,
    id: NodeId,
    data: &dyn DataAccess,
) -> BinocResult<Option<ParserMetadata>> {
    let Some(bytes) = ctx.view.artifact_bytes(id, &parser_metadata_v1(), data)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| BinocError::Other(format!("decode parser metadata artifact: {err}")))
}

pub struct BinaryChunkWriter;

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
            fallback: false,
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
            fallback: false,
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

impl EditListWriter for BinaryChunkWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.binary_chunks".into(),
            formats: vec![],
            input: NodeMatch {
                is_dir: Some(false),
                ..NodeMatch::default()
            },
            shape: ShapeFilter::Leaf,
            fallback: true,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let left = ctx.view.item(ctx.link.left);
        let right = ctx.view.item(ctx.link.right);
        if left.resolve_hash(data)? == right.resolve_hash(data)? {
            return Ok(Some(Vec::new().into()));
        }

        Ok(Some(
            binary_chunk_diff(left, right, data)
                .into_iter()
                .collect::<Vec<_>>()
                .into(),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BinaryChunkKey {
    digest: [u8; 32],
    len: u64,
}

#[derive(Debug, Clone)]
struct BinaryChunk {
    start: u64,
    len: u64,
    key: BinaryChunkKey,
}

#[derive(Debug)]
struct BinaryChunkList {
    chunks: Vec<BinaryChunk>,
    analyzed_bytes: u64,
    scan_truncated: bool,
}

#[derive(Debug, Clone, Copy)]
struct BinaryChunkMatch {
    left_index: usize,
    right_index: usize,
}

#[derive(Debug, Clone)]
struct BinaryChangedRegion {
    left_start: u64,
    left_len: u64,
    right_start: u64,
    right_len: u64,
}

fn binary_chunk_diff(
    left: &binoc_sdk::ItemRef,
    right: &binoc_sdk::ItemRef,
    data: &dyn DataAccess,
) -> Option<Edit> {
    let left_size = left.resolve_size(data).ok()?;
    let right_size = right.resolve_size(data).ok()?;
    let left_chunks = collect_binary_chunks(left, data).ok()?;
    let right_chunks = collect_binary_chunks(right, data).ok()?;
    if left_chunks.chunks.is_empty() || right_chunks.chunks.is_empty() {
        return None;
    }

    let matches = align_binary_chunks(&left_chunks.chunks, &right_chunks.chunks);
    let unchanged_bytes = matches
        .iter()
        .map(|matched| left_chunks.chunks[matched.left_index].len)
        .sum::<u64>();
    if unchanged_bytes == 0 {
        return None;
    }

    let (changed_region_count, regions, regions_truncated) = changed_regions(
        &left_chunks,
        &right_chunks,
        &matches,
        MAX_BINARY_CDC_REGIONS,
    );
    if changed_region_count == 0 {
        return None;
    }

    let unchanged_ratio = if left_size + right_size == 0 {
        1.0
    } else {
        (2.0 * unchanged_bytes as f64) / (left_size + right_size) as f64
    };
    let chunk_scan_truncated = left_chunks.scan_truncated || right_chunks.scan_truncated;

    let params = json!({
        "left_size": left_size,
        "right_size": right_size,
        "left_analyzed_bytes": left_chunks.analyzed_bytes,
        "right_analyzed_bytes": right_chunks.analyzed_bytes,
        "unchanged_bytes": unchanged_bytes,
        "unchanged_ratio": unchanged_ratio,
        "changed_region_count": changed_region_count,
        "regions_truncated": regions_truncated,
        "chunk_scan_truncated": chunk_scan_truncated,
        "chunk_count_limit": MAX_BINARY_CDC_CHUNKS,
        "chunking": {
            "algorithm": "fastcdc.v2020",
            "min_bytes": BINARY_CDC_MIN_CHUNK_BYTES,
            "avg_bytes": BINARY_CDC_AVG_CHUNK_BYTES,
            "max_bytes": BINARY_CDC_MAX_CHUNK_BYTES,
        },
        "regions": regions.iter().map(|region| json!({
            "left_start": region.left_start,
            "left_len": region.left_len,
            "right_start": region.right_start,
            "right_len": region.right_len,
        })).collect::<Vec<_>>(),
    });

    Some(
        Edit::new("binary.byte_ranges_changed", params)
            .with_item_type("file")
            .with_tag("binoc.content-changed")
            .with_tag("binoc.binary-byte-range-change")
            .with_summary(binary_chunk_summary(
                changed_region_count,
                unchanged_ratio,
                regions.first(),
                regions_truncated,
                chunk_scan_truncated,
            )),
    )
}

fn collect_binary_chunks(
    item: &binoc_sdk::ItemRef,
    data: &dyn DataAccess,
) -> BinocResult<BinaryChunkList> {
    let reader = data.open_read(item)?;
    let chunker = StreamCDC::new(
        reader,
        BINARY_CDC_MIN_CHUNK_BYTES,
        BINARY_CDC_AVG_CHUNK_BYTES,
        BINARY_CDC_MAX_CHUNK_BYTES,
    );
    let mut chunks = Vec::new();
    let mut analyzed_bytes = 0u64;
    let mut scan_truncated = false;

    for entry in chunker {
        let chunk = entry.map_err(|err| BinocError::Io(io::Error::from(err)))?;
        if chunks.len() >= MAX_BINARY_CDC_CHUNKS {
            scan_truncated = true;
            break;
        }
        let digest = blake3::hash(&chunk.data);
        let len = chunk.length as u64;
        chunks.push(BinaryChunk {
            start: chunk.offset,
            len,
            key: BinaryChunkKey {
                digest: *digest.as_bytes(),
                len,
            },
        });
        analyzed_bytes = chunk.offset + len;
    }

    Ok(BinaryChunkList {
        chunks,
        analyzed_bytes,
        scan_truncated,
    })
}

fn align_binary_chunks(left: &[BinaryChunk], right: &[BinaryChunk]) -> Vec<BinaryChunkMatch> {
    let mut right_by_key: HashMap<BinaryChunkKey, Vec<usize>> = HashMap::new();
    for (index, chunk) in right.iter().enumerate().rev() {
        right_by_key.entry(chunk.key).or_default().push(index);
    }

    let mut matches = Vec::new();
    let mut min_right_index = 0usize;
    for (left_index, left_chunk) in left.iter().enumerate() {
        let Some(candidates) = right_by_key.get_mut(&left_chunk.key) else {
            continue;
        };
        while let Some(right_index) = candidates.pop() {
            if right_index >= min_right_index {
                matches.push(BinaryChunkMatch {
                    left_index,
                    right_index,
                });
                min_right_index = right_index.saturating_add(1);
                break;
            }
        }
    }
    matches
}

fn changed_regions(
    left: &BinaryChunkList,
    right: &BinaryChunkList,
    matches: &[BinaryChunkMatch],
    region_limit: usize,
) -> (usize, Vec<BinaryChangedRegion>, bool) {
    let mut count = 0usize;
    let mut retained = Vec::new();
    let mut left_cursor = 0u64;
    let mut right_cursor = 0u64;

    for matched in matches {
        let left_chunk = &left.chunks[matched.left_index];
        let right_chunk = &right.chunks[matched.right_index];
        push_changed_region(
            &mut count,
            &mut retained,
            region_limit,
            left_cursor,
            left_chunk.start.saturating_sub(left_cursor),
            right_cursor,
            right_chunk.start.saturating_sub(right_cursor),
        );
        left_cursor = left_chunk.start + left_chunk.len;
        right_cursor = right_chunk.start + right_chunk.len;
    }
    push_changed_region(
        &mut count,
        &mut retained,
        region_limit,
        left_cursor,
        left.analyzed_bytes.saturating_sub(left_cursor),
        right_cursor,
        right.analyzed_bytes.saturating_sub(right_cursor),
    );

    let truncated = count > retained.len();
    (count, retained, truncated)
}

fn push_changed_region(
    count: &mut usize,
    retained: &mut Vec<BinaryChangedRegion>,
    limit: usize,
    left_start: u64,
    left_len: u64,
    right_start: u64,
    right_len: u64,
) {
    if left_len == 0 && right_len == 0 {
        return;
    }
    *count += 1;
    if retained.len() < limit {
        retained.push(BinaryChangedRegion {
            left_start,
            left_len,
            right_start,
            right_len,
        });
    }
}

fn binary_chunk_summary(
    changed_region_count: usize,
    unchanged_ratio: f64,
    first_region: Option<&BinaryChangedRegion>,
    regions_truncated: bool,
    chunk_scan_truncated: bool,
) -> Summary {
    let mut summary = Summary(vec![
        Segment::Uint(changed_region_count as u64),
        Segment::Text(format!(
            " changed byte range{}; ",
            if changed_region_count == 1 { "" } else { "s" }
        )),
        Segment::Float(unchanged_ratio * 100.0),
        Segment::Text("% unchanged".into()),
    ]);
    if let Some(region) = first_region {
        summary = summary
            .text("; first range left [")
            .uint(region.left_start)
            .text(", ")
            .uint(region.left_start + region.left_len)
            .text(") to right [")
            .uint(region.right_start)
            .text(", ")
            .uint(region.right_start + region.right_len)
            .text(")");
    }
    if regions_truncated {
        summary = summary.text("; regions truncated");
    }
    if chunk_scan_truncated {
        summary = summary.text("; scan truncated");
    }
    summary
}

impl EditListWriter for FallbackWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.fallback".into(),
            formats: vec![],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
            fallback: true,
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
