use binoc_sdk::{
    tabular_v1, BinocResult, CompactionRule, DataAccess, Edit, LinkCtx, Summary, TabularData, Value,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

use super::tabular::{is_row_alignment_basis_edit, load_tabular, MAX_ROW_ALIGNMENT_ROWS};

const COLUMN_RENAME_MIN_MATCHES: usize = 2;
const COLUMN_RENAME_MIN_MATCH_RATIO: f64 = 0.10;

pub struct ColumnReorder;

pub struct ColumnRename;

pub struct TypeOnlyColumnChange;

#[derive(Debug, Clone)]
pub struct ReducedPrecision {
    suppression_sentinels: BTreeSet<String>,
}

impl Default for ReducedPrecision {
    fn default() -> Self {
        Self::new(["*", "(D)", "(S)", ""])
    }
}

impl ReducedPrecision {
    pub fn new(suppression_sentinels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            suppression_sentinels: suppression_sentinels
                .into_iter()
                .map(|sentinel| sentinel.into().trim().to_string())
                .collect(),
        }
    }
}

impl CompactionRule for ColumnReorder {
    fn name(&self) -> &str {
        "binoc.compact.column_reorder"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        let Some(header) = edits.iter().find(|edit| edit.verb == "tabular.set_headers") else {
            return Ok(None);
        };
        let Some(from) = header.params.get("from").and_then(string_array) else {
            return Ok(None);
        };
        let Some(to) = header.params.get("to").and_then(string_array) else {
            return Ok(None);
        };
        let common_from: Vec<&String> = from.iter().filter(|header| to.contains(header)).collect();
        let common_to: Vec<&String> = to.iter().filter(|header| from.contains(header)).collect();
        if common_from.len() < 2 || common_from == common_to {
            return Ok(None);
        }
        let moved = common_from.iter().any(|name| {
            from.iter().position(|header| header == *name)
                != to.iter().position(|header| header == *name)
        });
        if !moved {
            return Ok(None);
        }
        let mut out = Vec::new();
        for edit in edits {
            if edit.verb == "tabular.set_headers" {
                out.push(
                    Edit::new("tabular.reorder_columns", json!({ "order": common_to }))
                        .with_item_type("tabular")
                        .with_tag("binoc.column-reorder")
                        .with_tag("binoc.schema-change")
                        .with_summary("Columns reordered"),
                );
            } else {
                out.push(edit.clone());
            }
        }
        Ok(Some(out))
    }
}

impl CompactionRule for ColumnRename {
    fn name(&self) -> &str {
        "binoc.compact.column_rename"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        ctx: &LinkCtx<'_>,
        edits: &[Edit],
        data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        let (Some(left), Some(right)) = (
            load_tabular(ctx, ctx.link.left, data)?,
            load_tabular(ctx, ctx.link.right, data)?,
        ) else {
            return Ok(None);
        };
        Ok(rewrite_column_renames(edits, &left, &right))
    }
}

impl CompactionRule for TypeOnlyColumnChange {
    fn name(&self) -> &str {
        "binoc.compact.type_only_column_change"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        Ok(rewrite_type_only_column_changes(edits))
    }
}

impl CompactionRule for ReducedPrecision {
    fn name(&self) -> &str {
        "binoc.compact.reduced_precision"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        Ok(rewrite_reduced_precision(
            edits,
            &self.suppression_sentinels,
        ))
    }
}

fn rewrite_type_only_column_changes(edits: &[Edit]) -> Option<Vec<Edit>> {
    let type_only = type_only_column_groups(edits);
    if type_only.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    let mut rewrote = false;
    for (index, edit) in edits.iter().enumerate() {
        let Some(column) = edit_cell_column(edit) else {
            out.push(edit.clone());
            continue;
        };
        let Some(group) = type_only.get(column) else {
            out.push(edit.clone());
            continue;
        };
        if edit_cell_is_type_only(edit) {
            if index == group.first_index {
                out.push(type_only_column_edit(column, group));
            }
            rewrote = true;
        } else {
            out.push(edit.clone());
        }
    }

    rewrote.then_some(out)
}

#[derive(Debug, Clone)]
struct TypeOnlyColumnGroup {
    first_index: usize,
    from_type: &'static str,
    to_type: &'static str,
    count: usize,
}

fn type_only_column_groups(edits: &[Edit]) -> BTreeMap<String, TypeOnlyColumnGroup> {
    let mut candidates: BTreeMap<String, TypeOnlyColumnGroup> = BTreeMap::new();
    let mut disqualified = Vec::new();

    for (index, edit) in edits
        .iter()
        .enumerate()
        .filter(|(_, edit)| edit.verb == "tabular.edit_cell")
    {
        let Some(column) = edit_cell_column(edit).map(str::to_string) else {
            continue;
        };
        if !edit_cell_is_type_only(edit) {
            disqualified.push(column);
            continue;
        }
        let from_type = cell_type_name(&edit.params["from"]);
        let to_type = cell_type_name(&edit.params["to"]);
        let column_key = column.clone();
        candidates
            .entry(column)
            .and_modify(|group| {
                if group.from_type != from_type || group.to_type != to_type {
                    disqualified.push(column_key.clone());
                }
                group.first_index = group.first_index.min(index);
                group.count += 1;
            })
            .or_insert(TypeOnlyColumnGroup {
                first_index: index,
                from_type,
                to_type,
                count: 1,
            });
    }

    for column in disqualified {
        candidates.remove(&column);
    }
    candidates
}

fn edit_cell_column(edit: &Edit) -> Option<&str> {
    (edit.verb == "tabular.edit_cell")
        .then(|| edit.params.get("column")?.as_str())
        .flatten()
}

fn edit_cell_is_type_only(edit: &Edit) -> bool {
    let Some(from) = edit.params.get("from") else {
        return false;
    };
    let Some(to) = edit.params.get("to") else {
        return false;
    };
    from != to && cell_type_name(from) != cell_type_name(to) && canonical_cell_equal(from, to)
}

fn type_only_column_edit(column: &str, group: &TypeOnlyColumnGroup) -> Edit {
    Edit::new(
        "tabular.column_type_changed",
        json!({
            "column": column,
            "from_type": group.from_type,
            "to_type": group.to_type,
            "cells": group.count,
        }),
    )
    .with_item_type("tabular")
    .with_tag("binoc.column-type-change")
    .with_tag("binoc.schema-change")
    .with_summary(
        Summary::new()
            .text("Column type changed: '")
            .text(column.to_string())
            .text("' ")
            .text(group.from_type)
            .text(" -> ")
            .text(group.to_type),
    )
}

fn cell_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Conservative cross-type cell equality for detecting representation-only
/// tabular changes.
///
/// Policy:
/// - Values with the same JSON type use ordinary JSON equality.
/// - Numeric JSON values and numeric strings compare equal by conservative
///   decimal value. This makes `1.0` equal to `"1"` while `"007"` is not equal
///   to `7`, and whitespace remains significant.
/// - Booleans do not equal strings (`true` != `"true"`).
/// - Null does not equal an empty string.
/// - Dates and timestamps have no special parsing; date-like strings are only
///   equal by exact string equality. We do not normalize time zones, calendars,
///   or formats.
/// - Arrays/objects do not cross-compare with strings, because stringified
///   nested values are often lossy producer choices rather than typed cells.
fn canonical_cell_equal(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    if left == right {
        return true;
    }
    if let (Some(left), Some(right)) = (NumericCell::parse(left), NumericCell::parse(right)) {
        return left.value == right.value;
    }
    match (left, right) {
        (serde_json::Value::Number(number), serde_json::Value::String(string))
        | (serde_json::Value::String(string), serde_json::Value::Number(number)) => {
            !string.is_empty() && number.to_string() == *string
        }
        _ => false,
    }
}

fn rewrite_reduced_precision(
    edits: &[Edit],
    suppression_sentinels: &BTreeSet<String>,
) -> Option<Vec<Edit>> {
    let semantic_edits: Vec<Edit> = edits
        .iter()
        .filter(|edit| !edit_cell_is_numeric_noop(edit))
        .cloned()
        .collect();
    let removed_numeric_noops = semantic_edits.len() != edits.len();
    if semantic_edits.is_empty() && removed_numeric_noops {
        return Some(vec![numeric_noop_edit()]);
    }

    let suppressed = suppressed_value_groups(&semantic_edits, suppression_sentinels);
    let rounded = rounded_value_groups(&semantic_edits);
    if suppressed.is_empty() && rounded.is_empty() {
        return removed_numeric_noops.then_some(semantic_edits);
    }

    let mut out = Vec::new();
    let mut rewrote = false;
    for (index, edit) in semantic_edits.iter().enumerate() {
        if let Some(group) = suppressed.get(&index) {
            out.push(suppressed_values_edit(group));
            rewrote = true;
            continue;
        }
        if suppressed
            .values()
            .any(|group| group.indices.contains(&index))
        {
            rewrote = true;
            continue;
        }
        if let Some(group) = rounded.get(&index) {
            out.push(rounded_values_edit(group));
            rewrote = true;
            continue;
        }
        if rounded.values().any(|group| group.indices.contains(&index)) {
            rewrote = true;
            continue;
        }
        out.push(edit.clone());
    }

    (rewrote || removed_numeric_noops).then_some(out)
}

#[derive(Debug, Clone)]
struct CellGroup {
    first_index: usize,
    indices: Vec<usize>,
    column: String,
    count: usize,
    basis: Option<RoundingBasis>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RoundingBasis {
    Modulus(NumericValue),
}

fn suppressed_value_groups(
    edits: &[Edit],
    suppression_sentinels: &BTreeSet<String>,
) -> BTreeMap<usize, CellGroup> {
    let mut by_column: BTreeMap<String, CellGroup> = BTreeMap::new();
    for (index, edit) in edits.iter().enumerate() {
        let Some(column) = edit_cell_column(edit).map(str::to_string) else {
            continue;
        };
        if !edit_cell_is_value_suppressed(edit, suppression_sentinels) {
            continue;
        }
        by_column
            .entry(column.clone())
            .and_modify(|group| {
                group.indices.push(index);
                group.count += 1;
            })
            .or_insert(CellGroup {
                first_index: index,
                indices: vec![index],
                column,
                count: 1,
                basis: None,
            });
    }
    by_column
        .into_values()
        .filter(|group| group.count >= 2)
        .map(|group| (group.first_index, group))
        .collect()
}

fn rounded_value_groups(edits: &[Edit]) -> BTreeMap<usize, CellGroup> {
    let mut by_column_and_basis: BTreeMap<(String, RoundingBasis), CellGroup> = BTreeMap::new();
    for (index, edit) in edits.iter().enumerate() {
        let Some(column) = edit_cell_column(edit).map(str::to_string) else {
            continue;
        };
        let Some(basis) = edit_cell_rounding_basis(edit) else {
            continue;
        };
        by_column_and_basis
            .entry((column.clone(), basis.clone()))
            .and_modify(|group| {
                group.indices.push(index);
                group.count += 1;
            })
            .or_insert(CellGroup {
                first_index: index,
                indices: vec![index],
                column,
                count: 1,
                basis: Some(basis),
            });
    }
    by_column_and_basis
        .into_values()
        .filter(|group| group.count >= 2)
        .map(|group| (group.first_index, group))
        .collect()
}

fn edit_cell_is_numeric_noop(edit: &Edit) -> bool {
    if edit.verb != "tabular.edit_cell" {
        return false;
    }
    let Some(from) = edit.params.get("from") else {
        return false;
    };
    let Some(to) = edit.params.get("to") else {
        return false;
    };
    from != to
        && cell_type_name(from) == cell_type_name(to)
        && NumericCell::parse(from)
            .zip(NumericCell::parse(to))
            .is_some_and(|(from, to)| from.value == to.value)
}

fn edit_cell_is_value_suppressed(edit: &Edit, suppression_sentinels: &BTreeSet<String>) -> bool {
    if edit.verb != "tabular.edit_cell" {
        return false;
    }
    let Some(from) = edit.params.get("from") else {
        return false;
    };
    let Some(to) = edit.params.get("to") else {
        return false;
    };
    value_is_present(from) && value_is_suppression_sentinel(from, to, suppression_sentinels)
}

fn edit_cell_rounding_basis(edit: &Edit) -> Option<RoundingBasis> {
    if edit.verb != "tabular.edit_cell" {
        return None;
    }
    let from = NumericCell::parse(edit.params.get("from")?)?;
    let to = NumericCell::parse(edit.params.get("to")?)?;
    if from.value == to.value {
        return None;
    }
    let modulus = to.rounding_modulus()?;
    (from.value.round_to_nearest(&modulus)? == to.value).then_some(RoundingBasis::Modulus(modulus))
}

fn numeric_noop_edit() -> Edit {
    let mut edit = Edit::new(
        "tabular.numeric_canonical_equal",
        json!({
            "reason": "numeric cells differ only in representation",
        }),
    )
    .with_item_type("tabular")
    .hidden()
    .with_summary("Numeric cells differ only in representation");
    edit.projection.hint.action = Some("identical".into());
    edit
}

fn suppressed_values_edit(group: &CellGroup) -> Edit {
    Edit::new(
        "tabular.values_suppressed",
        json!({
            "column": group.column,
            "cells": group.count,
        }),
    )
    .with_item_type("tabular")
    .with_tag("binoc.value-suppressed")
    .with_tag("binoc.cell-change")
    .with_summary(
        Summary::new()
            .text("Suppressed ")
            .count(group.count as u64, "cell")
            .text(" in '")
            .text(group.column.clone())
            .text("'"),
    )
}

fn rounded_values_edit(group: &CellGroup) -> Edit {
    let basis = match group.basis.as_ref().expect("rounding group has basis") {
        RoundingBasis::Modulus(modulus) => {
            json!({
                "kind": "modulus",
                "value": modulus.to_display_string(),
            })
        }
    };
    Edit::new(
        "tabular.values_rounded",
        json!({
            "column": group.column,
            "cells": group.count,
            "basis": basis,
        }),
    )
    .with_item_type("tabular")
    .with_tag("binoc.value-rounded")
    .with_tag("binoc.cell-change")
    .with_summary(
        Summary::new()
            .text("Rounded ")
            .count(group.count as u64, "cell")
            .text(" in '")
            .text(group.column.clone())
            .text("' to nearest ")
            .text(
                match group.basis.as_ref().expect("rounding group has basis") {
                    RoundingBasis::Modulus(modulus) => modulus.to_display_string(),
                },
            ),
    )
}

fn value_is_present(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(text) => !text.trim().is_empty(),
        _ => true,
    }
}

fn value_is_suppression_sentinel(
    from: &serde_json::Value,
    to: &serde_json::Value,
    suppression_sentinels: &BTreeSet<String>,
) -> bool {
    match to {
        serde_json::Value::Null => {
            NumericCell::parse(from).is_some() && suppression_sentinels.contains("")
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            suppression_sentinels.contains(trimmed)
                && (!trimmed.is_empty() || NumericCell::parse(from).is_some())
        }
        _ => false,
    }
}

#[derive(Debug, Clone)]
struct NumericCell {
    value: NumericValue,
    declared_scale: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NumericValue {
    coeff: i128,
    scale: u32,
}

impl NumericCell {
    fn parse(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Number(number) => parse_numeric_text(&number.to_string()),
            serde_json::Value::String(text) => parse_numeric_text(text),
            _ => None,
        }
    }

    fn rounding_modulus(&self) -> Option<NumericValue> {
        if self.value.coeff == 0 {
            return None;
        }
        if self.declared_scale > 0 {
            return Some(NumericValue {
                coeff: 1,
                scale: self.declared_scale,
            });
        }
        let trailing_zeroes = decimal_trailing_zeroes(self.value.coeff.unsigned_abs());
        (trailing_zeroes > 0)
            .then(|| {
                Some(NumericValue {
                    coeff: pow10(trailing_zeroes)?,
                    scale: 0,
                })
            })
            .flatten()
    }
}

impl NumericValue {
    fn normalized(mut coeff: i128, mut scale: u32) -> Self {
        while scale > 0 && coeff % 10 == 0 {
            coeff /= 10;
            scale -= 1;
        }
        Self { coeff, scale }
    }

    fn round_to_nearest(&self, modulus: &NumericValue) -> Option<NumericValue> {
        if modulus.coeff <= 0 {
            return None;
        }
        let scale = self.scale.max(modulus.scale);
        let value = self.coeff.checked_mul(pow10(scale - self.scale)?)?;
        let modulus = modulus.coeff.checked_mul(pow10(scale - modulus.scale)?)?;
        let quotient = div_round_nearest(value, modulus)?;
        Some(NumericValue::normalized(
            quotient.checked_mul(modulus)?,
            scale,
        ))
    }

    fn to_display_string(&self) -> String {
        if self.scale == 0 {
            return self.coeff.to_string();
        }
        let sign = if self.coeff < 0 { "-" } else { "" };
        let digits = self.coeff.unsigned_abs().to_string();
        let scale = self.scale as usize;
        if digits.len() <= scale {
            format!("{sign}0.{}{}", "0".repeat(scale - digits.len()), digits)
        } else {
            let split = digits.len() - scale;
            format!("{sign}{}.{}", &digits[..split], &digits[split..])
        }
    }
}

fn parse_numeric_text(text: &str) -> Option<NumericCell> {
    if text.is_empty() || text.trim() != text {
        return None;
    }
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if rest.is_empty() {
        return None;
    }
    let (mantissa, exponent) = split_exponent(rest)?;
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    let integer = strip_grouping_commas(integer)?;
    if integer.len() > 1 && integer.starts_with('0') {
        return None;
    }
    if !integer.bytes().all(|ch| ch.is_ascii_digit())
        || !fraction.bytes().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{integer}{fraction}");
    if digits.is_empty() || digits.len() > 36 || !digits.bytes().any(|ch| ch != b'0') {
        return if digits.bytes().all(|ch| ch == b'0') {
            Some(NumericCell {
                value: NumericValue { coeff: 0, scale: 0 },
                declared_scale: fraction.len() as u32,
            })
        } else {
            None
        };
    }
    let mut coeff = digits.parse::<i128>().ok()?;
    if negative {
        coeff = -coeff;
    }
    let declared_scale = fraction.len() as u32;
    let scale = declared_scale as i32 - exponent;
    if scale < 0 {
        coeff = coeff.checked_mul(pow10((-scale) as u32)?)?;
        Some(NumericCell {
            value: NumericValue::normalized(coeff, 0),
            declared_scale: 0,
        })
    } else {
        Some(NumericCell {
            value: NumericValue::normalized(coeff, scale as u32),
            declared_scale: scale as u32,
        })
    }
}

fn split_exponent(text: &str) -> Option<(&str, i32)> {
    if let Some(index) = text.find(['e', 'E']) {
        let exponent = text[index + 1..].parse::<i32>().ok()?;
        Some((&text[..index], exponent))
    } else {
        Some((text, 0))
    }
}

fn strip_grouping_commas(integer: &str) -> Option<String> {
    if !integer.contains(',') {
        return Some(integer.to_string());
    }
    let mut groups = integer.split(',');
    let first = groups.next()?;
    if first.is_empty() || first.len() > 3 || !first.bytes().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let mut out = first.to_string();
    for group in groups {
        if group.len() != 3 || !group.bytes().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        out.push_str(group);
    }
    Some(out)
}

fn decimal_trailing_zeroes(mut value: u128) -> u32 {
    let mut count = 0;
    while value > 0 && value.is_multiple_of(10) {
        value /= 10;
        count += 1;
    }
    count
}

fn pow10(exp: u32) -> Option<i128> {
    let mut value = 1i128;
    for _ in 0..exp {
        value = value.checked_mul(10)?;
    }
    Some(value)
}

fn div_round_nearest(value: i128, modulus: i128) -> Option<i128> {
    if modulus <= 0 {
        return None;
    }
    let sign = if value < 0 { -1 } else { 1 };
    let absolute = value.checked_abs()?;
    let quotient = absolute / modulus;
    let remainder = absolute % modulus;
    let rounded = if remainder.checked_mul(2)? >= modulus {
        quotient.checked_add(1)?
    } else {
        quotient
    };
    rounded.checked_mul(sign)
}

// Category-collapse and row-aggregation stay out of this pass pending the #120
// value-domain / aggregation design discussion.

fn rewrite_column_renames(
    edits: &[Edit],
    left: &TabularData,
    right: &TabularData,
) -> Option<Vec<Edit>> {
    let matches = column_rename_matches(edits, left, right);
    if matches.is_empty() {
        return None;
    }
    let redundant_headers = redundant_header_edits(edits, &matches);

    let mut out = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        if redundant_headers.contains(&index) {
            continue;
        }
        if let Some(rename) = matches.iter().find(|rename| rename.first_index() == index) {
            out.push(
                Edit::new(
                    "tabular.rename_column",
                    json!({ "from": rename.from, "to": rename.to }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.column-rename")
                .with_tag("binoc.schema-change")
                .with_summary(
                    binoc_sdk::Summary::new()
                        .text("Column renamed: '")
                        .text(rename.from.clone())
                        .text("' -> '")
                        .text(rename.to.clone())
                        .text("'"),
                ),
            );
            out.extend(rename.value_edits.clone());
            continue;
        }
        if matches.iter().any(|rename| rename.contains(index)) {
            continue;
        }
        out.push(edit.clone());
    }

    Some(out)
}

#[derive(Debug, Clone)]
struct ColumnRenameMatch {
    remove_index: usize,
    add_index: usize,
    from: String,
    to: String,
    score: usize,
    value_edits: Vec<Edit>,
}

impl ColumnRenameMatch {
    fn contains(&self, index: usize) -> bool {
        self.remove_index == index || self.add_index == index
    }

    fn first_index(&self) -> usize {
        self.remove_index.min(self.add_index)
    }
}

#[derive(Debug, Clone)]
struct ColumnEdit {
    index: usize,
    name: String,
    values: Vec<Value>,
}

fn column_rename_matches(
    edits: &[Edit],
    left: &TabularData,
    right: &TabularData,
) -> Vec<ColumnRenameMatch> {
    let removes = removed_columns(edits, left, right);
    let adds = added_columns(edits, left, right);
    if removes.is_empty() || adds.is_empty() {
        return Vec::new();
    }
    let row_alignment = edits
        .iter()
        .find(|edit| is_row_alignment_basis_edit(edit))
        .and_then(RowAlignmentBasis::from_edit)
        .map(|basis| basis.alignment_pairs());

    let mut candidates = Vec::new();
    for remove in &removes {
        for add in &adds {
            if let Some((score, value_edits)) = column_rename_score(
                &add.name,
                &remove.values,
                &add.values,
                row_alignment.as_deref(),
            ) {
                candidates.push(ColumnRenameMatch {
                    remove_index: remove.index,
                    add_index: add.index,
                    from: remove.name.clone(),
                    to: add.name.clone(),
                    score,
                    value_edits,
                });
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| a.remove_index.cmp(&b.remove_index))
            .then_with(|| a.add_index.cmp(&b.add_index))
    });

    let mut used_removes = Vec::new();
    let mut used_adds = Vec::new();
    let mut matches = Vec::new();
    for candidate in candidates {
        if used_removes.contains(&candidate.remove_index)
            || used_adds.contains(&candidate.add_index)
        {
            continue;
        }
        used_removes.push(candidate.remove_index);
        used_adds.push(candidate.add_index);
        matches.push(candidate);
    }
    matches.sort_by_key(ColumnRenameMatch::first_index);
    matches
}

fn removed_columns(edits: &[Edit], left: &TabularData, right: &TabularData) -> Vec<ColumnEdit> {
    column_edits(edits, "tabular.remove_column")
        .into_iter()
        .filter_map(|(index, name)| {
            if right.headers.contains(&name) {
                return None;
            }
            let values = left.column_values(&name)?.into_iter().cloned().collect();
            Some(ColumnEdit {
                index,
                name,
                values,
            })
        })
        .collect()
}

fn added_columns(edits: &[Edit], left: &TabularData, right: &TabularData) -> Vec<ColumnEdit> {
    column_edits(edits, "tabular.add_column")
        .into_iter()
        .filter_map(|(index, name)| {
            if left.headers.contains(&name) {
                return None;
            }
            let values = right.column_values(&name)?.into_iter().cloned().collect();
            Some(ColumnEdit {
                index,
                name,
                values,
            })
        })
        .collect()
}

fn redundant_header_edits(edits: &[Edit], matches: &[ColumnRenameMatch]) -> Vec<usize> {
    edits
        .iter()
        .enumerate()
        .filter_map(|(index, edit)| {
            (edit.verb == "tabular.set_headers"
                && header_change_is_explained_by_renames(edit, matches))
            .then_some(index)
        })
        .collect()
}

fn header_change_is_explained_by_renames(edit: &Edit, matches: &[ColumnRenameMatch]) -> bool {
    let Some(from) = edit.params.get("from") else {
        return false;
    };
    let Some(to_value) = edit.params.get("to") else {
        return false;
    };
    let Some(mut renamed) = string_array(from) else {
        return false;
    };
    let Some(to) = string_array(to_value) else {
        return false;
    };
    for header in &mut renamed {
        if let Some(rename) = matches.iter().find(|rename| rename.from == *header) {
            *header = rename.to.clone();
        }
    }
    renamed == to
}

fn column_edits(edits: &[Edit], verb: &str) -> Vec<(usize, String)> {
    edits
        .iter()
        .enumerate()
        .filter_map(|(index, edit)| {
            if edit.verb != verb {
                return None;
            }
            let name = edit.params.get("name")?.as_str()?.to_string();
            Some((index, name))
        })
        .collect()
}

fn column_rename_score(
    to_name: &str,
    left: &[Value],
    right: &[Value],
    row_alignment: Option<&[(usize, usize)]>,
) -> Option<(usize, Vec<Edit>)> {
    if let Some(row_alignment) = row_alignment {
        return column_rename_score_for_aligned_rows(to_name, left, right, row_alignment);
    }

    let value_alignment = lcs_pairs(left, right);
    let matches = value_alignment
        .iter()
        .filter(|(left_index, right_index)| {
            let left_value = &left[*left_index];
            let right_value = &right[*right_index];
            !left_value.is_blank() && left_value == right_value
        })
        .count();
    let comparable = left
        .iter()
        .filter(|value| !value.is_blank())
        .count()
        .min(right.iter().filter(|value| !value.is_blank()).count());
    if comparable == 0
        || matches < COLUMN_RENAME_MIN_MATCHES
        || (matches as f64 / comparable as f64) < COLUMN_RENAME_MIN_MATCH_RATIO
    {
        return None;
    }

    if left.len() == right.len() {
        let value_edits = left
            .iter()
            .zip(right)
            .enumerate()
            .filter(|(_, (from, to))| from != to)
            .map(|(row, (from, to))| column_value_edit(row, to_name, from, to))
            .collect();
        return Some((matches, value_edits));
    }

    let shorter = left.len().min(right.len());
    (value_alignment.len() == shorter).then_some((matches, Vec::new()))
}

fn column_rename_score_for_aligned_rows(
    to_name: &str,
    left: &[Value],
    right: &[Value],
    row_alignment: &[(usize, usize)],
) -> Option<(usize, Vec<Edit>)> {
    let comparable = row_alignment
        .iter()
        .filter(|(left_index, right_index)| {
            left.get(*left_index).is_some_and(|value| !value.is_blank())
                && right
                    .get(*right_index)
                    .is_some_and(|value| !value.is_blank())
        })
        .count();
    let matches = row_alignment
        .iter()
        .filter(|(left_index, right_index)| {
            let Some(left_value) = left.get(*left_index) else {
                return false;
            };
            let Some(right_value) = right.get(*right_index) else {
                return false;
            };
            !left_value.is_blank() && left_value == right_value
        })
        .count();
    if comparable == 0
        || matches < COLUMN_RENAME_MIN_MATCHES
        || (matches as f64 / comparable as f64) < COLUMN_RENAME_MIN_MATCH_RATIO
    {
        return None;
    }

    let value_edits = row_alignment
        .iter()
        .filter_map(|(left_index, right_index)| {
            let from = left.get(*left_index)?;
            let to = right.get(*right_index)?;
            (from != to).then(|| column_value_edit(*right_index, to_name, from, to))
        })
        .collect();
    Some((matches, value_edits))
}

fn column_value_edit(row: usize, column: &str, from: &Value, to: &Value) -> Edit {
    Edit::new(
        "tabular.edit_cell",
        json!({
            "row": row,
            "column": column,
            "from": from.to_json(),
            "to": to.to_json(),
        }),
    )
    .with_item_type("tabular")
    .with_tag("binoc.cell-change")
}

pub struct RowAdditionConsolidation;

pub struct RowAlignment;

impl CompactionRule for RowAlignment {
    fn name(&self) -> &str {
        "binoc.compact.row_alignment"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        Ok(rewrite_row_alignment(edits))
    }
}

fn rewrite_row_alignment(edits: &[Edit]) -> Option<Vec<Edit>> {
    let basis_index = edits.iter().position(is_row_alignment_basis_edit)?;
    let Some(basis) = RowAlignmentBasis::from_edit(&edits[basis_index]) else {
        return Some(strip_row_alignment_bases(edits));
    };
    if basis.columns.is_empty() {
        return Some(strip_row_alignment_bases(edits));
    }
    let alignment = basis.alignment_pairs();
    let mut matched_left = vec![false; basis.left.len()];
    let mut matched_right = vec![false; basis.right.len()];
    for (left, right) in alignment {
        matched_left[left] = true;
        matched_right[right] = true;
    }

    let unmatched_left = matched_left.iter().filter(|matched| !**matched).count();
    let unmatched_right: Vec<usize> = matched_right
        .iter()
        .enumerate()
        .filter_map(|(index, matched)| (!*matched).then_some(index))
        .collect();

    let mut out: Vec<Edit> = edits
        .iter()
        .filter(|edit| {
            !is_row_alignment_basis_edit(edit)
                && !basis.owns_cell_edit(edit)
                && edit.verb != "tabular.add_row"
                && edit.verb != "tabular.remove_row"
        })
        .cloned()
        .collect();

    if unmatched_left == 0 && !unmatched_right.is_empty() {
        for index in unmatched_right {
            out.push(
                Edit::new(
                    "tabular.add_row",
                    json!({
                        "index": index,
                        "values": basis.right_rows.get(index).cloned().unwrap_or_else(|| json!([])),
                    }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.row-addition"),
            );
        }
    } else {
        out.extend(
            edits
                .iter()
                .filter(|edit| {
                    !is_row_alignment_basis_edit(edit)
                        && (basis.owns_cell_edit(edit)
                            || edit.verb == "tabular.add_row"
                            || edit.verb == "tabular.remove_row")
                })
                .cloned(),
        );
    }

    Some(out)
}

fn strip_row_alignment_bases(edits: &[Edit]) -> Vec<Edit> {
    edits
        .iter()
        .filter(|edit| !is_row_alignment_basis_edit(edit))
        .cloned()
        .collect()
}

struct RowAlignmentBasis {
    columns: Vec<String>,
    left: Vec<String>,
    right: Vec<String>,
    right_rows: Vec<serde_json::Value>,
    pairs: Option<Vec<(usize, usize)>>,
}

impl RowAlignmentBasis {
    fn from_edit(edit: &Edit) -> Option<Self> {
        let columns = string_array(edit.params.get("columns")?)?;
        let left = string_array(edit.params.get("left")?)?;
        let right = string_array(edit.params.get("right")?)?;
        if left.len() > MAX_ROW_ALIGNMENT_ROWS || right.len() > MAX_ROW_ALIGNMENT_ROWS {
            return None;
        }
        let right_rows = edit.params.get("right_rows")?.as_array()?.clone();
        if right_rows.len() > MAX_ROW_ALIGNMENT_ROWS {
            return None;
        }
        let pairs = match edit.params.get("pairs").and_then(|pairs| pairs.as_array()) {
            Some(pairs) => Some(
                pairs
                    .iter()
                    .map(|pair| {
                        Some((
                            pair.get("left")?.as_u64()? as usize,
                            pair.get("right")?.as_u64()? as usize,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
            None => None,
        };
        if let Some(pairs) = &pairs {
            if pairs.iter().any(|(left_index, right_index)| {
                *left_index >= left.len() || *right_index >= right.len()
            }) {
                return None;
            }
        }
        Some(Self {
            columns,
            left,
            right,
            right_rows,
            pairs,
        })
    }

    fn alignment_pairs(&self) -> Vec<(usize, usize)> {
        self.pairs
            .clone()
            .unwrap_or_else(|| lcs_pairs(&self.left, &self.right))
    }

    fn owns_cell_edit(&self, edit: &Edit) -> bool {
        edit.verb == "tabular.edit_cell"
            && edit.params.get("key").is_none()
            && (edit.params.get("row").is_some()
                || edit.params.get("left_row").is_some()
                || edit.params.get("right_row").is_some())
            && edit
                .params
                .get("column")
                .and_then(|value| value.as_str())
                .is_some_and(|column| self.columns.iter().any(|basis| basis == column))
    }
}

fn string_array(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn lcs_pairs<T: PartialEq>(left: &[T], right: &[T]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            lengths[left_index][right_index] = if left[left_index] == right[right_index] {
                lengths[left_index + 1][right_index + 1] + 1
            } else {
                lengths[left_index + 1][right_index].max(lengths[left_index][right_index + 1])
            };
        }
    }

    let mut pairs = Vec::new();
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            pairs.push((left_index, right_index));
            left_index += 1;
            right_index += 1;
        } else if lengths[left_index + 1][right_index] >= lengths[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    pairs
}

impl CompactionRule for RowAdditionConsolidation {
    fn name(&self) -> &str {
        "binoc.compact.row_addition"
    }

    fn format(&self) -> Option<binoc_sdk::ArtifactFormat> {
        Some(tabular_v1())
    }

    fn rewrite(
        &self,
        _ctx: &LinkCtx<'_>,
        edits: &[Edit],
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>> {
        Ok(rewrite_row_addition_consolidation(edits))
    }
}

fn rewrite_row_addition_consolidation(edits: &[Edit]) -> Option<Vec<Edit>> {
    let mut out = Vec::new();
    let mut run = Vec::new();
    let mut rewrote = false;

    fn flush(out: &mut Vec<Edit>, run: &mut Vec<(u64, serde_json::Value)>, rewrote: &mut bool) {
        if run.len() >= 2 {
            let start = run[0].0;
            let rows: Vec<serde_json::Value> =
                run.iter().map(|(_, values)| values.clone()).collect();
            out.push(
                Edit::new(
                    "tabular.append_rows",
                    json!({ "start": start, "rows": rows }),
                )
                .with_item_type("tabular")
                .with_tag("binoc.row-addition"),
            );
            *rewrote = true;
        } else {
            for (index, values) in run.drain(..) {
                out.push(
                    Edit::new(
                        "tabular.add_row",
                        json!({ "index": index, "values": values }),
                    )
                    .with_item_type("tabular")
                    .with_tag("binoc.row-addition"),
                );
            }
        }
        run.clear();
    }

    for edit in edits {
        let row = (edit.verb == "tabular.add_row")
            .then(|| {
                Some((
                    edit.params.get("index")?.as_u64()?,
                    edit.params.get("values")?.clone(),
                ))
            })
            .flatten();
        match row {
            Some((index, values)) => {
                if let Some((last, _)) = run.last() {
                    if index != last + 1 {
                        flush(&mut out, &mut run, &mut rewrote);
                    }
                }
                run.push((index, values));
            }
            None => {
                flush(&mut out, &mut run, &mut rewrote);
                out.push(edit.clone());
            }
        }
    }
    flush(&mut out, &mut run, &mut rewrote);

    rewrote.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_column(name: &str, values: &[&str]) -> Edit {
        Edit::new(
            "tabular.add_column",
            json!({
                "name": name,
                "values": {
                    "total_values": values.len(),
                    "truncated": false,
                    "values": values,
                }
            }),
        )
        .with_item_type("tabular")
        .with_tag("binoc.column-addition")
        .with_tag("binoc.schema-change")
    }

    fn remove_column(name: &str, values: &[&str]) -> Edit {
        Edit::new(
            "tabular.remove_column",
            json!({
                "name": name,
                "values": {
                    "total_values": values.len(),
                    "truncated": false,
                    "values": values,
                }
            }),
        )
        .with_item_type("tabular")
        .with_tag("binoc.column-removal")
        .with_tag("binoc.schema-change")
    }

    fn cell(row: u64, column: &str, from: serde_json::Value, to: serde_json::Value) -> Edit {
        Edit::new(
            "tabular.edit_cell",
            json!({
                "row": row,
                "column": column,
                "from": from,
                "to": to,
            }),
        )
        .with_item_type("tabular")
        .with_tag("binoc.cell-change")
    }

    fn keyed_cell(
        key: serde_json::Value,
        column: &str,
        from: serde_json::Value,
        to: serde_json::Value,
    ) -> Edit {
        Edit::new(
            "tabular.edit_cell",
            json!({
                "key": key,
                "column": column,
                "from": from,
                "to": to,
            }),
        )
        .with_item_type("tabular")
        .with_tag("binoc.cell-change")
    }

    fn table(headers: &[&str], rows: &[&[&str]]) -> TabularData {
        TabularData::from_string_rows(
            headers.iter().map(|header| (*header).to_string()).collect(),
            rows.iter()
                .map(|row| row.iter().map(|value| (*value).to_string()).collect())
                .collect(),
        )
    }

    #[test]
    fn canonical_cell_equality_is_conservative() {
        assert!(canonical_cell_equal(&json!(2024), &json!("2024")));
        assert!(canonical_cell_equal(&json!("2024"), &json!(2024)));
        assert!(canonical_cell_equal(&json!(1.0), &json!("1.0")));
        assert!(canonical_cell_equal(&json!("1.0"), &json!("1")));
        assert!(!canonical_cell_equal(&json!(7), &json!("007")));
        assert!(!canonical_cell_equal(&json!(true), &json!("true")));
        assert!(!canonical_cell_equal(&json!(null), &json!("")));
        assert!(!canonical_cell_equal(
            &json!("2024-01-01T00:00:00Z"),
            &json!("2024-01-01")
        ));
    }

    #[test]
    fn type_only_column_change_collapses_to_one_claim() {
        let edits = vec![
            cell(0, "year", json!(2024), json!("2024")),
            cell(1, "year", json!(2025), json!("2025")),
            cell(0, "name", json!("Alice"), json!("Alicia")),
        ];

        let rewritten = rewrite_type_only_column_changes(&edits).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.column_type_changed");
        assert_eq!(
            rewritten[0].params,
            json!({
                "column": "year",
                "from_type": "number",
                "to_type": "string",
                "cells": 2,
            })
        );
        assert_eq!(
            rewritten[0]
                .projection
                .hint
                .summary
                .as_ref()
                .expect("summary")
                .plain_text(),
            "Column type changed: 'year' number -> string"
        );
        assert_eq!(rewritten[1].verb, "tabular.edit_cell");
        assert_eq!(rewritten[1].params["column"], json!("name"));
    }

    #[test]
    fn type_only_column_change_keeps_mixed_semantic_column_changes() {
        let edits = vec![
            cell(0, "year", json!(2024), json!("2024")),
            cell(1, "year", json!(2025), json!("FY2025")),
        ];

        assert!(rewrite_type_only_column_changes(&edits).is_none());
    }

    #[test]
    fn type_only_column_change_rewrites_keyed_cell_edits() {
        let edits = vec![
            keyed_cell(json!({"id": 1}), "year", json!(2024), json!("2024")),
            keyed_cell(json!({"id": 2}), "year", json!(2025), json!("2025")),
        ];

        let rewritten = rewrite_type_only_column_changes(&edits).expect("rewrite");

        assert_eq!(rewritten.len(), 1);
        assert_eq!(rewritten[0].verb, "tabular.column_type_changed");
        assert_eq!(rewritten[0].params["cells"], json!(2));
    }

    #[test]
    fn reduced_precision_removes_numeric_representation_noops() {
        let edits = vec![
            cell(0, "rate", json!("1.0"), json!("1")),
            cell(1, "rate", json!("2.50"), json!("2.5")),
        ];

        let rewritten =
            rewrite_reduced_precision(&edits, &default_suppression_sentinels()).expect("rewrite");

        assert_eq!(rewritten.len(), 1);
        assert_eq!(rewritten[0].verb, "tabular.numeric_canonical_equal");
        assert!(!rewritten[0].projection.visible);
        assert_eq!(
            rewritten[0].projection.hint.action.as_deref(),
            Some("identical")
        );
    }

    #[test]
    fn reduced_precision_collapses_suppressed_cells_by_column() {
        let edits = vec![
            cell(0, "count", json!("123"), json!("*")),
            cell(1, "count", json!("456"), json!("(D)")),
            cell(2, "name", json!("Alice"), json!("Alicia")),
        ];

        let rewritten =
            rewrite_reduced_precision(&edits, &default_suppression_sentinels()).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.values_suppressed");
        assert_eq!(
            rewritten[0].params,
            json!({
                "column": "count",
                "cells": 2,
            })
        );
        assert!(rewritten[0]
            .projection
            .hint
            .tags
            .contains(&"binoc.value-suppressed".into()));
        assert_eq!(
            rewritten[0]
                .projection
                .hint
                .summary
                .as_ref()
                .expect("summary")
                .plain_text(),
            "Suppressed 2 cells in 'count'"
        );
        assert_eq!(rewritten[1].verb, "tabular.edit_cell");
    }

    #[test]
    fn reduced_precision_honors_custom_suppression_sentinels() {
        let edits = vec![
            cell(0, "count", json!("123"), json!("N/A")),
            cell(1, "count", json!("456"), json!("N/A")),
            cell(2, "name", json!("Alice"), json!("Alicia")),
        ];

        let suppression_sentinels = ["N/A", ""].into_iter().map(str::to_string).collect();
        let rewritten = rewrite_reduced_precision(&edits, &suppression_sentinels).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.values_suppressed");
        assert_eq!(rewritten[0].params["cells"], json!(2));
        assert_eq!(rewritten[1].verb, "tabular.edit_cell");
    }

    #[test]
    fn reduced_precision_collapses_numeric_rounding_by_column_and_modulus() {
        let edits = vec![
            cell(0, "population", json!("12,345"), json!("12,000")),
            cell(1, "population", json!("67,890"), json!("68,000")),
            cell(2, "name", json!("Alpha"), json!("Alfa")),
        ];

        let rewritten =
            rewrite_reduced_precision(&edits, &default_suppression_sentinels()).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.values_rounded");
        assert_eq!(
            rewritten[0].params,
            json!({
                "column": "population",
                "cells": 2,
                "basis": {
                    "kind": "modulus",
                    "value": "1000",
                },
            })
        );
        assert!(rewritten[0]
            .projection
            .hint
            .tags
            .contains(&"binoc.value-rounded".into()));
        assert_eq!(
            rewritten[0]
                .projection
                .hint
                .summary
                .as_ref()
                .expect("summary")
                .plain_text(),
            "Rounded 2 cells in 'population' to nearest 1000"
        );
        assert_eq!(rewritten[1].verb, "tabular.edit_cell");
    }

    fn basis(left: &[&str], right: &[&str], right_rows: Vec<serde_json::Value>) -> Edit {
        basis_with_columns(&["name", "age"], left, right, right_rows)
    }

    fn basis_with_columns(
        columns: &[&str],
        left: &[&str],
        right: &[&str],
        right_rows: Vec<serde_json::Value>,
    ) -> Edit {
        Edit::new(
            "tabular.row_alignment_basis",
            json!({
                "columns": columns,
                "left": left,
                "right": right,
                "right_rows": right_rows,
            }),
        )
        .hidden()
    }

    #[test]
    fn column_rename_rewrites_matching_add_remove_columns() {
        let edits = vec![
            Edit::new(
                "tabular.set_headers",
                json!({"from": ["id", "a", "score"], "to": ["id", "b", "score"]}),
            )
            .with_item_type("tabular"),
            add_column("b", &["alpha", "beta", "gamma", "delta"]),
            remove_column("a", &["alpha", "beta", "delta"]),
            Edit::new("tabular.add_row", json!({"index": 2})).with_item_type("tabular"),
        ];

        let left = table(
            &["id", "a", "score"],
            &[
                &["1", "alpha", "10"],
                &["2", "beta", "20"],
                &["4", "delta", "40"],
            ],
        );
        let right = table(
            &["id", "b", "score"],
            &[
                &["1", "alpha", "10"],
                &["2", "beta", "20"],
                &["3", "gamma", "30"],
                &["4", "delta", "40"],
            ],
        );

        let rewritten = rewrite_column_renames(&edits, &left, &right).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.rename_column");
        assert_eq!(rewritten[0].params, json!({"from": "a", "to": "b"}));
        assert!(rewritten[0]
            .projection
            .hint
            .tags
            .contains(&"binoc.column-rename".into()));
        assert!(!rewritten
            .iter()
            .any(|edit| edit.verb == "tabular.add_column" || edit.verb == "tabular.remove_column"));
    }

    #[test]
    fn column_rename_keeps_changed_values_as_cell_edits() {
        let edits = vec![
            Edit::new(
                "tabular.set_headers",
                json!({"from": ["id", "status"], "to": ["id", "state"]}),
            )
            .with_item_type("tabular"),
            add_column("state", &["active", "paused", "closed", "archived"]),
            remove_column("status", &["active", "pending", "closed", "archived"]),
        ];

        let left = table(
            &["id", "status"],
            &[
                &["1", "active"],
                &["2", "pending"],
                &["3", "closed"],
                &["4", "archived"],
            ],
        );
        let right = table(
            &["id", "state"],
            &[
                &["1", "active"],
                &["2", "paused"],
                &["3", "closed"],
                &["4", "archived"],
            ],
        );

        let rewritten = rewrite_column_renames(&edits, &left, &right).expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0].verb, "tabular.rename_column");
        assert_eq!(
            rewritten[0].params,
            json!({"from": "status", "to": "state"})
        );
        assert_eq!(rewritten[1].verb, "tabular.edit_cell");
        assert_eq!(
            rewritten[1].params,
            json!({"row": 1, "column": "state", "from": "pending", "to": "paused"})
        );
    }

    #[test]
    fn column_rename_uses_explicit_row_alignment_pairs() {
        let edits = vec![
            Edit::new(
                "tabular.row_alignment_basis",
                json!({
                    "columns": [],
                    "left": ["id=1", "id=2", "id=3"],
                    "right": ["id=2", "id=3", "id=1"],
                    "right_rows": [
                        {"values": ["2", "closed"], "total_values": 2, "truncated": false},
                        {"values": ["3", "archived"], "total_values": 2, "truncated": false},
                        {"values": ["1", "active"], "total_values": 2, "truncated": false}
                    ],
                    "pairs": [
                        {"left": 0, "right": 2},
                        {"left": 1, "right": 0},
                        {"left": 2, "right": 1}
                    ]
                }),
            )
            .hidden(),
            Edit::new(
                "tabular.set_headers",
                json!({"from": ["id", "status"], "to": ["id", "state"]}),
            )
            .with_item_type("tabular"),
            add_column("state", &["closed", "archived", "active"]),
            remove_column("status", &["active", "pending", "archived"]),
        ];

        let left = table(
            &["id", "status"],
            &[&["1", "active"], &["2", "pending"], &["3", "archived"]],
        );
        let right = table(
            &["id", "state"],
            &[&["2", "closed"], &["3", "archived"], &["1", "active"]],
        );

        let rewritten = rewrite_column_renames(&edits, &left, &right).expect("rewrite");

        assert_eq!(rewritten[0].verb, "tabular.row_alignment_basis");
        assert_eq!(rewritten[1].verb, "tabular.rename_column");
        assert_eq!(
            rewritten[2].params,
            json!({"row": 0, "column": "state", "from": "pending", "to": "closed"})
        );
    }

    #[test]
    fn column_rename_keeps_header_change_when_reorder_remains() {
        let edits = vec![
            Edit::new(
                "tabular.set_headers",
                json!({"from": ["id", "status", "score"], "to": ["score", "id", "state"]}),
            )
            .with_item_type("tabular"),
            add_column("state", &["active", "pending"]),
            remove_column("status", &["active", "pending"]),
        ];

        let left = table(
            &["id", "status", "score"],
            &[&["1", "active", "10"], &["2", "pending", "20"]],
        );
        let right = table(
            &["score", "id", "state"],
            &[&["10", "1", "active"], &["20", "2", "pending"]],
        );

        let renamed = rewrite_column_renames(&edits, &left, &right).expect("rename rewrite");
        assert_eq!(renamed[0].verb, "tabular.set_headers");
        assert_eq!(renamed[1].verb, "tabular.rename_column");
    }

    #[test]
    fn column_rename_ignores_unrelated_add_remove_columns() {
        let edits = vec![
            add_column("email", &["a@example.test", "b@example.test"]),
            remove_column("legacy", &["alpha", "beta"]),
        ];

        let left = table(&["legacy"], &[&["alpha"], &["beta"]]);
        let right = table(&["email"], &[&["a@example.test"], &["b@example.test"]]);

        assert!(rewrite_column_renames(&edits, &left, &right).is_none());
    }

    #[test]
    fn column_rename_requires_more_than_one_matching_value() {
        let edits = vec![
            add_column("status", &["active", "draft", "hold", "closed"]),
            remove_column("legacy", &["active", "x", "y", "z"]),
        ];

        let left = table(&["legacy"], &[&["active"], &["x"], &["y"], &["z"]]);
        let right = table(
            &["status"],
            &[&["active"], &["draft"], &["hold"], &["closed"]],
        );

        assert!(rewrite_column_renames(&edits, &left, &right).is_none());
    }

    #[test]
    fn row_alignment_rewrites_mid_table_insertion() {
        let edits = vec![
            Edit::new(
                "tabular.reorder_columns",
                json!({"order": ["city", "name", "age"]}),
            )
            .with_item_type("tabular")
            .with_tag("binoc.column-reorder"),
            Edit::new(
                "tabular.add_column",
                json!({"name": "email", "values": {"values": ["alice@example.test", "bob@example.test", "charlie@example.test"]}}),
            )
            .with_item_type("tabular")
            .with_tag("binoc.column-addition"),
            basis(
                &["alice", "charlie"],
                &["alice", "bob", "charlie"],
                vec![
                    json!({"values": ["Alice", "30"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Bob", "25"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Charlie", "35"], "total_values": 2, "truncated": false}),
                ],
            ),
            Edit::new(
                "tabular.edit_cell",
                json!({"row": 1, "column": "name", "from": "Charlie", "to": "Bob"}),
            ),
            Edit::new(
                "tabular.edit_cell",
                json!({"row": 1, "column": "age", "from": "35", "to": "25"}),
            ),
            Edit::new(
                "tabular.add_row",
                json!({
                    "index": 2,
                    "values": {"values": ["Charlie", "35"], "total_values": 2, "truncated": false}
                }),
            ),
        ];

        let rewritten = rewrite_row_alignment(&edits).expect("rewrite");

        assert_eq!(rewritten.len(), 3);
        assert_eq!(rewritten[0].verb, "tabular.reorder_columns");
        assert_eq!(rewritten[1].verb, "tabular.add_column");
        assert_eq!(rewritten[2].verb, "tabular.add_row");
        assert_eq!(rewritten[2].params["index"], json!(1));
        assert_eq!(
            rewritten[2].params["values"],
            json!({"values": ["Bob", "25"], "total_values": 2, "truncated": false})
        );
    }

    #[test]
    fn row_alignment_precedes_reduced_precision_for_inserted_sentinels() {
        let edits = vec![
            basis_with_columns(
                &["name", "count"],
                &["alpha", "delta", "epsilon"],
                &["alpha", "beta", "gamma", "delta", "epsilon"],
                vec![
                    json!({"values": ["Alpha", "10"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Beta", "*"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Gamma", "(D)"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Delta", "40"], "total_values": 2, "truncated": false}),
                    json!({"values": ["Epsilon", "50"], "total_values": 2, "truncated": false}),
                ],
            ),
            cell(1, "name", json!("Delta"), json!("Beta")),
            cell(1, "count", json!("40"), json!("*")),
            cell(2, "name", json!("Epsilon"), json!("Gamma")),
            cell(2, "count", json!("50"), json!("(D)")),
            Edit::new(
                "tabular.add_row",
                json!({
                    "index": 3,
                    "values": {"values": ["Delta", "40"], "total_values": 2, "truncated": false}
                }),
            )
            .with_item_type("tabular")
            .with_tag("binoc.row-addition"),
            Edit::new(
                "tabular.add_row",
                json!({
                    "index": 4,
                    "values": {"values": ["Epsilon", "50"], "total_values": 2, "truncated": false}
                }),
            )
            .with_item_type("tabular")
            .with_tag("binoc.row-addition"),
        ];

        let prematurely_reduced =
            rewrite_reduced_precision(&edits, &default_suppression_sentinels())
                .expect("old-order rewrite");
        assert!(prematurely_reduced
            .iter()
            .any(|edit| edit.verb == "tabular.values_suppressed"));

        let aligned = rewrite_row_alignment(&edits).expect("alignment rewrite");
        assert!(!aligned.iter().any(|edit| edit.verb == "tabular.edit_cell"));
        assert!(rewrite_reduced_precision(&aligned, &default_suppression_sentinels()).is_none());
    }

    #[test]
    fn row_alignment_keeps_visible_edits_when_left_rows_are_unmatched() {
        let cell = Edit::new(
            "tabular.edit_cell",
            json!({"row": 0, "column": "name", "from": "Alice", "to": "Alicia"}),
        );
        let edits = vec![
            basis(
                &["alice"],
                &["alicia"],
                vec![json!({"values": ["Alicia"], "total_values": 1, "truncated": false})],
            ),
            cell.clone(),
        ];

        let rewritten = rewrite_row_alignment(&edits).expect("basis cleanup");

        assert_eq!(rewritten, vec![cell]);
    }

    #[test]
    fn row_alignment_strips_oversized_basis_it_cannot_parse() {
        let left = (0..=MAX_ROW_ALIGNMENT_ROWS)
            .map(|index| format!("left-{index}"))
            .collect::<Vec<_>>();
        let right = (0..=MAX_ROW_ALIGNMENT_ROWS)
            .map(|index| format!("right-{index}"))
            .collect::<Vec<_>>();
        let visible = Edit::new("tabular.visible", json!({"kept": true}));
        let edits = vec![
            Edit::new(
                "tabular.row_alignment_basis",
                json!({
                    "columns": ["name"],
                    "left": left,
                    "right": right,
                    "right_rows": [],
                }),
            )
            .hidden(),
            visible.clone(),
        ];

        let rewritten = rewrite_row_alignment(&edits).expect("basis cleanup");

        assert_eq!(rewritten, vec![visible]);
    }

    fn default_suppression_sentinels() -> BTreeSet<String> {
        ["*", "(D)", "(S)", ""]
            .into_iter()
            .map(str::to_string)
            .collect()
    }
}
