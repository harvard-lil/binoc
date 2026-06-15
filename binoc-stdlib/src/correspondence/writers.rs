use std::collections::{BTreeMap, BTreeSet};

use binoc_sdk::{
    file_name, tabular_collection_v1, tabular_extract, tabular_v1, BinocError, BinocResult,
    DataAccess, Diagnostic, DiffNode, Edit, EditListWriter, ExtractResult, IdentityFailurePolicy,
    LinkCtx, NodeId, NodeMatch, ShapeFilter, TableMember, TabularCollectionData, TabularData,
    TabularDataPair, WriteOutput, WriterDescriptor,
};
use serde_json::json;
use similar::{ChangeTag, TextDiff};

const MAX_CAPTURED_VALUES: usize = 16;
const MAX_VALUE_PREVIEW_BYTES: usize = 120;
const MAX_TEXT_LINE_EXAMPLES: usize = 8;
const MAX_ROW_ALIGNMENT_ROWS: usize = 512;

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
        if let Some(collection_edits) = write_stacked_table_edits(&left, &right) {
            return Ok(Some(WriteOutput {
                edits: collection_edits,
                diagnostics,
            }));
        }
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
                let left_value = left.rows[index]
                    .get(left_index)
                    .map(String::as_str)
                    .unwrap_or("");
                let right_value = right.rows[index]
                    .get(right_index)
                    .map(String::as_str)
                    .unwrap_or("");
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
        *counts.entry(key).or_insert(0usize) += 1;
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
        if !seen.insert(key) {
            return false;
        }
    }
    true
}

fn row_key(table: &TabularData, keys: &[String], row: &[String]) -> Option<Vec<String>> {
    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        let value = row
            .get(table.column_index(key)?)
            .cloned()
            .unwrap_or_default();
        if value.is_empty() {
            return None;
        }
        values.push(value);
    }
    Some(values)
}

fn unique_rows_by_key<'a>(
    table: &'a TabularData,
    keys: &[String],
) -> BTreeMap<Vec<String>, (usize, &'a Vec<String>)> {
    let mut counts = BTreeMap::new();
    let mut rows = BTreeMap::new();
    for (index, row) in table.rows.iter().enumerate() {
        let Some(key) = row_key(table, keys, row) else {
            continue;
        };
        *counts.entry(key.clone()).or_insert(0usize) += 1;
        rows.entry(key).or_insert((index, row));
    }
    rows.retain(|key, _| counts.get(key).copied().unwrap_or(0) == 1);
    rows
}

fn key_json(keys: &[String], values: &[String]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (key, value) in keys.iter().zip(values) {
        object.insert(key.clone(), json!(value));
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

    for key in left_keys.difference(&right_keys) {
        let (index, row) = left_rows.get(key).expect("known left key");
        edits.push(row_remove_edit(json!({
            "index": index,
            "key": key_json(keys, key),
            "values": capture_row(row)
        })));
    }
    for key in right_keys.difference(&left_keys) {
        let (index, row) = right_rows.get(key).expect("known right key");
        edits.push(row_add_edit(json!({
            "index": index,
            "key": key_json(keys, key),
            "values": capture_row(row)
        })));
    }

    let common: Vec<&String> = left
        .headers
        .iter()
        .filter(|header| right.headers.contains(header) && !keys.contains(header))
        .collect();
    for key in left_keys.intersection(&right_keys) {
        let (_, left_row) = left_rows.get(key).expect("known left key");
        let (_, right_row) = right_rows.get(key).expect("known right key");
        for column in &common {
            let left_index = left.column_index(column).expect("common column");
            let right_index = right.column_index(column).expect("common column");
            let left_value = left_row.get(left_index).map(String::as_str).unwrap_or("");
            let right_value = right_row.get(right_index).map(String::as_str).unwrap_or("");
            if left_value != right_value {
                edits.push(cell_edit(json!({
                    "key": key_json(keys, key),
                    "column": column,
                    "from": value_preview(left_value),
                    "to": value_preview(right_value)
                })));
            }
        }
    }
}

fn capture_row(row: &[String]) -> serde_json::Value {
    let values: Vec<String> = row
        .iter()
        .take(MAX_CAPTURED_VALUES)
        .map(|value| value_preview(value))
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

fn row_signature(table: &TabularData, row: &[String], common: &[&String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for column in common {
        let index = table.column_index(column).expect("common column");
        let value = row.get(index).map(String::as_str).unwrap_or("");
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn capture_values(values: Vec<&str>) -> serde_json::Value {
    let preview: Vec<String> = values
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

fn value_preview(value: &str) -> String {
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

#[derive(Debug)]
struct StackedSection {
    name: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn write_stacked_table_edits(left: &TabularData, right: &TabularData) -> Option<Vec<Edit>> {
    let left_sections = detect_stacked_sections(left);
    let right_sections = detect_stacked_sections(right);
    if left_sections.len() < 2 && right_sections.len() < 2 {
        return None;
    }

    let left_by_name = left_sections
        .iter()
        .map(|section| (section.name.as_str(), section))
        .collect::<BTreeMap<_, _>>();
    let right_by_name = right_sections
        .iter()
        .map(|section| (section.name.as_str(), section))
        .collect::<BTreeMap<_, _>>();
    let names = left_by_name
        .keys()
        .chain(right_by_name.keys())
        .copied()
        .collect::<BTreeSet<_>>();

    let mut edits = Vec::new();
    for name in names {
        match (left_by_name.get(name), right_by_name.get(name)) {
            (Some(left), Some(right))
                if left.headers == right.headers && left.rows == right.rows => {}
            (Some(left), Some(right)) => {
                let mut edit =
                    Edit::new("tabular_collection.change_table", json!({ "table": name }))
                        .with_item_type("tabular_collection")
                        .with_tag("binoc.tabular-collection-change")
                        .with_tag("binoc.table-change");
                if right.rows.len() > left.rows.len() {
                    edit = edit.with_tag("binoc.row-addition");
                }
                if left.rows.len() > right.rows.len() {
                    edit = edit.with_tag("binoc.row-removal");
                }
                if left.headers != right.headers {
                    edit = edit.with_tag("binoc.schema-change");
                }
                edits.push(edit);
            }
            (Some(_), None) => edits.push(
                Edit::new("tabular_collection.remove_table", json!({ "table": name }))
                    .with_item_type("tabular_collection")
                    .with_tag("binoc.tabular-collection-change")
                    .with_tag("binoc.table-removal"),
            ),
            (None, Some(_)) => edits.push(
                Edit::new("tabular_collection.add_table", json!({ "table": name }))
                    .with_item_type("tabular_collection")
                    .with_tag("binoc.tabular-collection-change")
                    .with_tag("binoc.table-addition"),
            ),
            (None, None) => {}
        }
    }

    Some(edits)
}

fn detect_stacked_sections(table: &TabularData) -> Vec<StackedSection> {
    let rows = raw_rows(table);
    let mut sections = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        while i < rows.len() && normalized_width(&rows[i]) <= 1 {
            i += 1;
        }
        if i >= rows.len() {
            break;
        }
        let width = normalized_width(&rows[i]);
        if width < 2 || !looks_like_header(&rows[i]) {
            i += 1;
            continue;
        }
        let headers = trim_to_width(&rows[i], width);
        let mut section_rows = Vec::new();
        let mut j = i + 1;
        while j < rows.len() {
            let row_width = normalized_width(&rows[j]);
            if row_width == 0 || row_width != width {
                break;
            }
            let row = trim_to_width(&rows[j], width);
            if row != headers {
                section_rows.push(row);
            }
            j += 1;
        }
        if !section_rows.is_empty() {
            sections.push(StackedSection {
                name: format!("table_{}", sections.len() + 1),
                headers,
                rows: section_rows,
            });
        }
        i = j + 1;
    }
    sections
}

fn raw_rows(table: &TabularData) -> Vec<Vec<String>> {
    let mut rows = Vec::with_capacity(table.rows.len() + 1);
    rows.push(table.headers.clone());
    rows.extend(table.rows.clone());
    rows
}

fn normalized_width(row: &[String]) -> usize {
    row.iter()
        .rposition(|cell| !cell.trim().is_empty())
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn trim_to_width(row: &[String], width: usize) -> Vec<String> {
    (0..width)
        .map(|index| row.get(index).cloned().unwrap_or_default())
        .collect()
}

fn looks_like_header(row: &[String]) -> bool {
    let width = normalized_width(row);
    if width < 2 {
        return false;
    }
    let trimmed = trim_to_width(row, width);
    let non_empty = trimmed
        .iter()
        .filter(|cell| !cell.trim().is_empty())
        .count();
    let unique = trimmed.iter().collect::<BTreeSet<_>>().len();
    non_empty == width && unique == width
}

pub struct FallbackWriter;

pub struct TextWriter;

pub struct TabularCollectionWriter;

impl EditListWriter for TabularCollectionWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.tabular_collection".into(),
            formats: vec![tabular_collection_v1()],
            input: NodeMatch::default(),
            shape: ShapeFilter::Any,
        }
    }

    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>> {
        let (Some(left), Some(right)) = (
            load_tabular_collection(ctx, ctx.link.left, data)?,
            load_tabular_collection(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };
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
            .collect::<BTreeSet<_>>();

        let mut edits = Vec::new();
        for name in names {
            match (left_tables.get(name), right_tables.get(name)) {
                (Some(left_table), Some(right_table)) if *left_table == *right_table => {}
                (Some(left_table), Some(right_table)) => {
                    let mut edit =
                        Edit::new("tabular_collection.change_table", json!({ "table": name }))
                            .with_item_type("tabular_collection")
                            .with_tag("binoc.tabular-collection-change")
                            .with_tag("binoc.table-change");
                    if right_table.shape.row_count > left_table.shape.row_count {
                        edit = edit.with_tag("binoc.row-addition");
                    }
                    if left_table.shape.row_count > right_table.shape.row_count {
                        edit = edit.with_tag("binoc.row-removal");
                    }
                    let added_columns = columns_added(left_table, right_table);
                    let removed_columns = columns_added(right_table, left_table);
                    if !added_columns.is_empty() {
                        edit = edit
                            .with_tag("binoc.column-addition")
                            .with_tag("binoc.schema-change");
                    }
                    if !removed_columns.is_empty() {
                        edit = edit
                            .with_tag("binoc.column-removal")
                            .with_tag("binoc.schema-change");
                    }
                    edits.push(edit);
                }
                (Some(_), None) => edits.push(
                    Edit::new("tabular_collection.remove_table", json!({ "table": name }))
                        .with_item_type("tabular_collection")
                        .with_tag("binoc.tabular-collection-change")
                        .with_tag("binoc.table-removal"),
                ),
                (None, Some(_)) => edits.push(
                    Edit::new("tabular_collection.add_table", json!({ "table": name }))
                        .with_item_type("tabular_collection")
                        .with_tag("binoc.tabular-collection-change")
                        .with_tag("binoc.table-addition"),
                ),
                (None, None) => {}
            }
        }

        Ok(Some(edits.into()))
    }
}

fn load_tabular_collection(
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
        .map_err(|err| BinocError::Other(format!("decode tabular collection artifact: {err}")))
}

fn columns_added(left: &TableMember, right: &TableMember) -> Vec<String> {
    right
        .shape
        .columns
        .iter()
        .filter(|column| !left.shape.columns.contains(*column))
        .cloned()
        .collect()
}

impl EditListWriter for TextWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.write.text".into(),
            formats: vec![],
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".txt".into(), ".md".into()],
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
        let left_text = String::from_utf8_lossy(&left_bytes);
        let right_text = String::from_utf8_lossy(&right_bytes);
        let left_lines: Vec<&str> = left_text.lines().collect();
        let right_lines: Vec<&str> = right_text.lines().collect();
        let diff = TextDiff::from_lines(&left_text, &right_text);
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
                    "from": value_preview(left_lines[index]),
                    "to": value_preview(right_lines[index]),
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
        Ok(Some(vec![edit].into()))
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
        if left.resolve_hash(data)? == right.resolve_hash(data)? {
            Ok(Some(Vec::new().into()))
        } else {
            Ok(Some(
                vec![Edit::new("binary.contents-differ", json!({}))
                    .with_item_type("file")
                    .with_tag("binoc.content-changed")]
                .into(),
            ))
        }
    }
}
