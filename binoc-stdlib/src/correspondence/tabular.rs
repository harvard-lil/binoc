use std::sync::Arc;

use binoc_sdk::{tabular_v1, BinocError, BinocResult, DataAccess, LinkCtx, NodeId, TabularData};

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
