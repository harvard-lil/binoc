use std::io::Read;
use std::path::Path;

use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;

/// Extracts both sides to temp dirs, expands into child item pairs.
/// Handles nested zips by re-entering the controller queue.
pub struct ZipComparator;

fn extract_zip(zip_path: &Path, dest: &Path) -> BinocResult<()> {
    let file = std::fs::File::open(zip_path).map_err(BinocError::Io)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| BinocError::Zip(e.to_string()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| BinocError::Zip(e.to_string()))?;
        let Some(entry_path) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let out_path = dest.join(&entry_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(BinocError::Io)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(BinocError::Io)?;
            }
            let mut outfile = std::fs::File::create(&out_path).map_err(BinocError::Io)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(BinocError::Io)?;
            std::io::Write::write_all(&mut outfile, &buf).map_err(BinocError::Io)?;
        }
    }

    Ok(())
}

impl Comparator for ZipComparator {
    fn name(&self) -> &str {
        "binoc.zip"
    }

    fn handles_extensions(&self) -> &[&str] {
        &[".zip"]
    }

    fn handles_media_types(&self) -> &[&str] {
        &["application/zip"]
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        child_path: &str,
        ctx: &CompareContext,
    ) -> BinocResult<ItemPair> {
        // Extract zip contents to temp dirs and return a directory pair.
        // The child_path for a zip is typically the same as the zip's logical path
        // (the zip comparator expands into a single directory pair with the same path).
        let _ = child_path;

        let left = if let Some(item) = &pair.left {
            let tmp = tempfile::tempdir().map_err(BinocError::Io)?;
            extract_zip(&item.physical_path, tmp.path())?;
            let path = ctx.keep_temp_dir(tmp);
            Some(Item::new(path, item.logical_path.clone()))
        } else {
            None
        };

        let right = if let Some(item) = &pair.right {
            let tmp = tempfile::tempdir().map_err(BinocError::Io)?;
            extract_zip(&item.physical_path, tmp.path())?;
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
                "both sides missing in zip reopen".into(),
            )),
        }
    }

    fn compare(&self, pair: &ItemPair, ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let tmp_l = tempfile::tempdir().map_err(BinocError::Io)?;
                let tmp_r = tempfile::tempdir().map_err(BinocError::Io)?;

                extract_zip(&left.physical_path, tmp_l.path())?;
                extract_zip(&right.physical_path, tmp_r.path())?;

                let path_l = ctx.keep_temp_dir(tmp_l);
                let path_r = ctx.keep_temp_dir(tmp_r);

                let logical = &right.logical_path;
                let item_l = Item::new(path_l, logical.clone());
                let item_r = Item::new(path_r, logical.clone());

                let dir_pair = ItemPair::both(item_l, item_r);
                let node = DiffNode::new("modify", "zip_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, Some(right)) => {
                let tmp_r = tempfile::tempdir().map_err(BinocError::Io)?;
                extract_zip(&right.physical_path, tmp_r.path())?;
                let path_r = ctx.keep_temp_dir(tmp_r);

                let logical = &right.logical_path;
                let item_r = Item::new(path_r, logical.clone());
                let dir_pair = ItemPair::added(item_r);
                let node = DiffNode::new("add", "zip_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (Some(left), None) => {
                let tmp_l = tempfile::tempdir().map_err(BinocError::Io)?;
                extract_zip(&left.physical_path, tmp_l.path())?;
                let path_l = ctx.keep_temp_dir(tmp_l);

                let logical = &left.logical_path;
                let item_l = Item::new(path_l, logical.clone());
                let dir_pair = ItemPair::removed(item_l);
                let node = DiffNode::new("remove", "zip_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
