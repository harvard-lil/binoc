use std::collections::BTreeSet;

use binoc_sdk::*;

/// File-correspondence by relative path. Expands into child item pairs for
/// each matched, added, or removed file. Pre-computes BLAKE3 hashes for all
/// child files, enabling the controller to short-circuit identical items and
/// ensuring hashes are available for move/copy detection.
pub struct DirectoryComparator;

fn list_entries(dir: &std::path::Path) -> BinocResult<Vec<std::path::PathBuf>> {
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|e| BinocError::Io(e.into()))?;
        entries.push(entry.into_path());
    }
    Ok(entries)
}

fn relative_name(entry: &std::path::Path, base: &std::path::Path) -> String {
    entry
        .strip_prefix(base)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            entry
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        })
}

fn identify(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<(String, Option<String>)> {
    let bytes = data.read_bytes(item)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let media_type = infer::get(&bytes)
        .map(|t| t.mime_type().to_string())
        .or_else(|| {
            mime_guess::from_path(&item.logical_path)
                .first()
                .map(|m| m.essence_str().to_string())
        });
    Ok((hash, media_type))
}

fn make_item_ref(
    path: &std::path::Path,
    logical: String,
    data: &dyn DataAccess,
) -> BinocResult<ItemRef> {
    let mut item = data.register_local(path, &logical)?;
    if !item.is_dir {
        if let Ok((hash, media_type)) = identify(&item, data) {
            item.content_hash = Some(hash);
            item.media_type = media_type;
        }
    }
    Ok(item)
}

impl Comparator for DirectoryComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.directory")
            .with_scope(ItemScope::Containers)
            .with_handles_identical(true)
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        child_path: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<ItemPair> {
        let child_rel = child_path
            .strip_prefix(&format!("{}/", pair.logical_path()))
            .or_else(|| {
                let lp = pair.logical_path();
                if lp.is_empty() {
                    Some(child_path)
                } else {
                    child_path.strip_prefix(lp)
                }
            })
            .unwrap_or(child_path);

        let first_component = child_rel.split('/').next().unwrap_or(child_rel);

        let make_child = |item: &ItemRef| -> BinocResult<ItemRef> {
            let phys = data.local_path(item)?;
            let child_phys = phys.join(first_component);
            let logical = if item.logical_path.is_empty() {
                first_component.to_string()
            } else {
                format!("{}/{}", item.logical_path, first_component)
            };
            data.register_local(&child_phys, &logical)
        };

        match (&pair.left, &pair.right) {
            (Some(l), Some(r)) => Ok(ItemPair::both(make_child(l)?, make_child(r)?)),
            (None, Some(r)) => Ok(ItemPair::added(make_child(r)?)),
            (Some(l), None) => Ok(ItemPair::removed(make_child(l)?)),
            (None, None) => Err(BinocError::Extract("empty pair in reopen".into())),
        }
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => self.compare_dirs(left, right, data),
            (None, Some(right)) => {
                let phys = data.local_path(right)?;
                let entries = list_entries(&phys)?;
                let children: BinocResult<Vec<ItemPair>> = entries
                    .into_iter()
                    .map(|path| {
                        let name = relative_name(&path, &phys);
                        let logical = if right.logical_path.is_empty() {
                            name
                        } else {
                            format!("{}/{}", right.logical_path, name)
                        };
                        let item = make_item_ref(&path, logical, data)?;
                        Ok(ItemPair::added(item))
                    })
                    .collect();

                let node = DiffNode::new("add", "directory", &right.logical_path);
                Ok(CompareResult::Expand(node, children?))
            }
            (Some(left), None) => {
                let phys = data.local_path(left)?;
                let entries = list_entries(&phys)?;
                let children: BinocResult<Vec<ItemPair>> = entries
                    .into_iter()
                    .map(|path| {
                        let name = relative_name(&path, &phys);
                        let logical = if left.logical_path.is_empty() {
                            name
                        } else {
                            format!("{}/{}", left.logical_path, name)
                        };
                        let item = make_item_ref(&path, logical, data)?;
                        Ok(ItemPair::removed(item))
                    })
                    .collect();

                let node = DiffNode::new("remove", "directory", &left.logical_path);
                Ok(CompareResult::Expand(node, children?))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}

impl DirectoryComparator {
    fn compare_dirs(
        &self,
        left: &ItemRef,
        right: &ItemRef,
        data: &dyn DataAccess,
    ) -> BinocResult<CompareResult> {
        let phys_l = data.local_path(left)?;
        let phys_r = data.local_path(right)?;

        let entries_l = list_entries(&phys_l)?;
        let entries_r = list_entries(&phys_r)?;

        let names_l: BTreeSet<String> = entries_l
            .iter()
            .map(|e| relative_name(e, &phys_l))
            .collect();
        let names_r: BTreeSet<String> = entries_r
            .iter()
            .map(|e| relative_name(e, &phys_r))
            .collect();

        let mut children = Vec::new();

        for name in names_l.intersection(&names_r) {
            let path_l = phys_l.join(name);
            let path_r = phys_r.join(name);
            let logical = if right.logical_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", right.logical_path, name)
            };
            let item_l = make_item_ref(&path_l, logical.clone(), data)?;
            let item_r = make_item_ref(&path_r, logical, data)?;
            children.push(ItemPair::both(item_l, item_r));
        }

        for name in names_r.difference(&names_l) {
            let path_r = phys_r.join(name);
            let logical = if right.logical_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", right.logical_path, name)
            };
            children.push(ItemPair::added(make_item_ref(&path_r, logical, data)?));
        }

        for name in names_l.difference(&names_r) {
            let path_l = phys_l.join(name);
            let logical = if left.logical_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", left.logical_path, name)
            };
            children.push(ItemPair::removed(make_item_ref(&path_l, logical, data)?));
        }

        let kind = if children.is_empty() {
            "identical"
        } else {
            "modify"
        };
        let node = DiffNode::new(kind, "directory", &right.logical_path);
        Ok(CompareResult::Expand(node, children))
    }
}
