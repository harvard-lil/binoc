use std::sync::Arc;

use binoc_sdk::{
    tabular_v1, BinocError, BinocResult, DataAccess, Edit, LinkCtx, NodeId, TabularData,
};

pub(crate) const MAX_ROW_ALIGNMENT_ROWS: usize = 512;
pub(crate) const ROW_ALIGNMENT_BASIS_VERB: &str = "tabular.row_alignment_basis";

pub(crate) fn is_row_alignment_basis_edit(edit: &Edit) -> bool {
    edit.verb == ROW_ALIGNMENT_BASIS_VERB
}

pub(crate) fn load_tabular(
    ctx: &LinkCtx<'_>,
    id: NodeId,
    data: &dyn DataAccess,
) -> BinocResult<Option<Arc<TabularData>>> {
    let format = tabular_v1();
    ctx.artifact_cache.get_or_try_insert_with(id, &format, || {
        let Some(bytes) = ctx.view.artifact_bytes(id, &format, data)? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| BinocError::Other(format!("decode tabular artifact: {err}")))
    })
}
