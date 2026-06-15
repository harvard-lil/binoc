use std::collections::BTreeMap;

use binoc_sdk::*;
use serde_json::json;

pub struct RowReorderWriter;

impl EditListWriter for RowReorderWriter {
    fn descriptor(&self) -> WriterDescriptor {
        WriterDescriptor {
            name: "binoc.row_reorder_writer".into(),
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

        if left.headers != right.headers || left.rows.len() != right.rows.len() {
            return Ok(None);
        }
        if left.rows == right.rows {
            return Ok(Some(Vec::new().into()));
        }
        if row_multiset(&left.rows) != row_multiset(&right.rows) {
            return Ok(None);
        }

        Ok(Some(
            vec![Edit::new(
                "tabular.reorder_rows",
                json!({ "row_count": right.rows.len() }),
            )
            .with_item_type("tabular")
            .with_tag("binoc.row-reorder")
            .with_summary("Rows reordered (same data, different sort order)")]
            .into(),
        ))
    }
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

fn row_multiset(rows: &[Vec<String>]) -> BTreeMap<Vec<String>, usize> {
    let mut bag = BTreeMap::new();
    for row in rows {
        *bag.entry(row.clone()).or_insert(0) += 1;
    }
    bag
}
