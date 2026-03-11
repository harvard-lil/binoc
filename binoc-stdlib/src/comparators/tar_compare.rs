use std::path::Path;

use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;

/// Extracts both sides to temp dirs, expands into child item pairs.
/// Handles `.tar`, `.tar.gz`, and `.tgz` archives.
pub struct TarComparator;

fn is_gzipped(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".tar.gz") || name.ends_with(".tgz")
}

fn extract_tar(tar_path: &Path, dest: &Path) -> BinocResult<()> {
    let file = std::fs::File::open(tar_path).map_err(BinocError::Io)?;

    if is_gzipped(tar_path) {
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(dest)
            .map_err(|e| BinocError::Tar(e.to_string()))?;
    } else {
        let mut archive = tar::Archive::new(file);
        archive
            .unpack(dest)
            .map_err(|e| BinocError::Tar(e.to_string()))?;
    }

    Ok(())
}

impl Comparator for TarComparator {
    fn name(&self) -> &str {
        "binoc.tar"
    }

    fn handles_extensions(&self) -> &[&str] {
        &[".tar", ".tar.gz", ".tgz"]
    }

    fn handles_media_types(&self) -> &[&str] {
        &["application/x-tar"]
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        child_path: &str,
        ctx: &CompareContext,
    ) -> BinocResult<ItemPair> {
        let _ = child_path;

        let left = if let Some(item) = &pair.left {
            let tmp = tempfile::tempdir().map_err(BinocError::Io)?;
            extract_tar(&item.physical_path, tmp.path())?;
            let path = ctx.keep_temp_dir(tmp);
            Some(Item::new(path, item.logical_path.clone()))
        } else {
            None
        };

        let right = if let Some(item) = &pair.right {
            let tmp = tempfile::tempdir().map_err(BinocError::Io)?;
            extract_tar(&item.physical_path, tmp.path())?;
            let path = ctx.keep_temp_dir(tmp);
            Some(Item::new(path, item.logical_path.clone()))
        } else {
            None
        };

        match (left, right) {
            (Some(l), Some(r)) => Ok(ItemPair::both(l, r)),
            (None, Some(r)) => Ok(ItemPair::added(r)),
            (Some(l), None) => Ok(ItemPair::removed(l)),
            (None, None) => Err(BinocError::Extract(
                "both sides missing in tar reopen".into(),
            )),
        }
    }

    fn compare(&self, pair: &ItemPair, ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let tmp_l = tempfile::tempdir().map_err(BinocError::Io)?;
                let tmp_r = tempfile::tempdir().map_err(BinocError::Io)?;

                extract_tar(&left.physical_path, tmp_l.path())?;
                extract_tar(&right.physical_path, tmp_r.path())?;

                let path_l = ctx.keep_temp_dir(tmp_l);
                let path_r = ctx.keep_temp_dir(tmp_r);

                let logical = &right.logical_path;
                let item_l = Item::new(path_l, logical.clone());
                let item_r = Item::new(path_r, logical.clone());

                let dir_pair = ItemPair::both(item_l, item_r);
                let node = DiffNode::new("modify", "tar_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, Some(right)) => {
                let tmp_r = tempfile::tempdir().map_err(BinocError::Io)?;
                extract_tar(&right.physical_path, tmp_r.path())?;
                let path_r = ctx.keep_temp_dir(tmp_r);

                let logical = &right.logical_path;
                let item_r = Item::new(path_r, logical.clone());
                let dir_pair = ItemPair::added(item_r);
                let node = DiffNode::new("add", "tar_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (Some(left), None) => {
                let tmp_l = tempfile::tempdir().map_err(BinocError::Io)?;
                extract_tar(&left.physical_path, tmp_l.path())?;
                let path_l = ctx.keep_temp_dir(tmp_l);

                let logical = &left.logical_path;
                let item_l = Item::new(path_l, logical.clone());
                let dir_pair = ItemPair::removed(item_l);
                let node = DiffNode::new("remove", "tar_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
