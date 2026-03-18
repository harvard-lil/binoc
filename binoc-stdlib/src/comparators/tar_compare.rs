use std::path::Path;

use binoc_sdk::*;

/// Extracts both sides to temp dirs, expands into child item pairs.
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

fn extract_side(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ItemRef> {
    let phys = data.local_path(item)?;
    let ws = data.workspace()?;
    extract_tar(&phys, &ws)?;
    data.register_local(&ws, &item.logical_path)
}

impl Comparator for TarComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.tar")
            .with_extensions(vec![".tar".into(), ".tar.gz".into(), ".tgz".into()])
            .with_media_types(vec!["application/x-tar".into()])
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        _child_path: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<ItemPair> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let item_l = extract_side(left, data)?;
                let item_r = extract_side(right, data)?;
                Ok(ItemPair::both(item_l, item_r))
            }
            (None, Some(right)) => {
                let item_r = extract_side(right, data)?;
                Ok(ItemPair::added(item_r))
            }
            (Some(left), None) => {
                let item_l = extract_side(left, data)?;
                Ok(ItemPair::removed(item_l))
            }
            (None, None) => Err(BinocError::Extract("empty pair in tar reopen".into())),
        }
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let item_l = extract_side(left, data)?;
                let item_r = extract_side(right, data)?;

                let logical = &right.logical_path;
                let dir_pair = ItemPair::both(item_l, item_r);
                let node = DiffNode::new("modify", "tar_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, Some(right)) => {
                let item_r = extract_side(right, data)?;

                let logical = &right.logical_path;
                let dir_pair = ItemPair::added(item_r);
                let node = DiffNode::new("add", "tar_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (Some(left), None) => {
                let item_l = extract_side(left, data)?;

                let logical = &left.logical_path;
                let dir_pair = ItemPair::removed(item_l);
                let node = DiffNode::new("remove", "tar_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
