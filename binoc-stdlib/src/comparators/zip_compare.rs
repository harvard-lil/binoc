use std::io::Read;
use std::path::Path;

use binoc_sdk::*;

/// Extracts both sides to temp dirs, expands into child item pairs.
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

fn extract_side(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ItemRef> {
    let phys = data.local_path(item)?;
    let ws = data.workspace()?;
    extract_zip(&phys, &ws)?;
    data.register_local(&ws, &item.logical_path)
}

impl Comparator for ZipComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.zip")
            .with_extensions(vec![".zip".into()])
            .with_media_types(vec!["application/zip".into()])
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
            (None, None) => Err(BinocError::Extract("empty pair in zip reopen".into())),
        }
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let item_l = extract_side(left, data)?;
                let item_r = extract_side(right, data)?;

                let logical = &right.logical_path;
                let dir_pair = ItemPair::both(item_l, item_r);
                let node = DiffNode::new("modify", "zip_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, Some(right)) => {
                let item_r = extract_side(right, data)?;

                let logical = &right.logical_path;
                let dir_pair = ItemPair::added(item_r);
                let node = DiffNode::new("add", "zip_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (Some(left), None) => {
                let item_l = extract_side(left, data)?;

                let logical = &left.logical_path;
                let dir_pair = ItemPair::removed(item_l);
                let node = DiffNode::new("remove", "zip_archive", logical);
                Ok(CompareResult::Expand(node, vec![dir_pair]))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}
