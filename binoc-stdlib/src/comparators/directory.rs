use std::collections::BTreeSet;

use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;

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

/// Read a file's bytes, compute BLAKE3 hash and detect media type in one pass.
fn read_and_identify(
    path: &std::path::Path,
    logical_path: &str,
) -> BinocResult<(String, Option<String>)> {
    let data = std::fs::read(path).map_err(BinocError::Io)?;
    let hash = blake3::hash(&data).to_hex().to_string();
    let media_type = infer::get(&data)
        .map(|t| t.mime_type().to_string())
        .or_else(|| {
            mime_guess::from_path(logical_path)
                .first()
                .map(|m| m.essence_str().to_string())
        });
    Ok((hash, media_type))
}

fn make_item(path: std::path::PathBuf, logical: String) -> Item {
    let mut item = Item::new(&path, logical.clone());
    if !item.is_dir {
        if let Ok((hash, media_type)) = read_and_identify(&path, &logical) {
            item.content_hash = Some(hash);
            item.media_type = media_type;
        }
    }
    item
}

impl Comparator for DirectoryComparator {
    fn name(&self) -> &str {
        "binoc.directory"
    }

    fn can_handle(&self, pair: &ItemPair) -> bool {
        pair.is_dir()
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        child_path: &str,
        _ctx: &CompareContext,
    ) -> BinocResult<ItemPair> {
        // Resolve a child's physical path from the parent directory pair.
        // child_path is the full logical path; we need the relative segment.
        let parent_logical = pair
            .left
            .as_ref()
            .or(pair.right.as_ref())
            .map(|i| i.logical_path.as_str())
            .unwrap_or("");

        let relative = if parent_logical.is_empty() {
            child_path.to_string()
        } else if let Some(stripped) = child_path.strip_prefix(parent_logical) {
            stripped.trim_start_matches('/').to_string()
        } else {
            child_path.to_string()
        };

        let left = pair.left.as_ref().map(|item| {
            let phys = item.physical_path.join(&relative);
            Item::new(phys, child_path)
        });
        let right = pair.right.as_ref().map(|item| {
            let phys = item.physical_path.join(&relative);
            Item::new(phys, child_path)
        });

        match (left, right) {
            (Some(l), Some(r)) => Ok(ItemPair::both(l, r)),
            (None, Some(r)) => Ok(ItemPair::added(r)),
            (Some(l), None) => Ok(ItemPair::removed(l)),
            (None, None) => Err(BinocError::Extract("both sides missing in reopen".into())),
        }
    }

    fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => self.compare_dirs(left, right),
            (None, Some(right)) => {
                let entries = list_entries(&right.physical_path)?;
                let children: Vec<ItemPair> = entries
                    .into_iter()
                    .map(|path| {
                        let name = relative_name(&path, &right.physical_path);
                        let logical = if right.logical_path.is_empty() {
                            name
                        } else {
                            format!("{}/{}", right.logical_path, name)
                        };
                        ItemPair::added(make_item(path, logical))
                    })
                    .collect();

                let node = DiffNode::new("add", "directory", &right.logical_path);
                Ok(CompareResult::Expand(node, children))
            }
            (Some(left), None) => {
                let entries = list_entries(&left.physical_path)?;
                let children: Vec<ItemPair> = entries
                    .into_iter()
                    .map(|path| {
                        let name = relative_name(&path, &left.physical_path);
                        let logical = if left.logical_path.is_empty() {
                            name
                        } else {
                            format!("{}/{}", left.logical_path, name)
                        };
                        ItemPair::removed(make_item(path, logical))
                    })
                    .collect();

                let node = DiffNode::new("remove", "directory", &left.logical_path);
                Ok(CompareResult::Expand(node, children))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}

impl DirectoryComparator {
    fn compare_dirs(&self, left: &Item, right: &Item) -> BinocResult<CompareResult> {
        let entries_l = list_entries(&left.physical_path)?;
        let entries_r = list_entries(&right.physical_path)?;

        let names_l: BTreeSet<String> = entries_l
            .iter()
            .map(|e| relative_name(e, &left.physical_path))
            .collect();
        let names_r: BTreeSet<String> = entries_r
            .iter()
            .map(|e| relative_name(e, &right.physical_path))
            .collect();

        let mut children = Vec::new();

        for name in names_l.intersection(&names_r) {
            let path_l = left.physical_path.join(name);
            let path_r = right.physical_path.join(name);
            let logical = if right.logical_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", right.logical_path, name)
            };
            children.push(ItemPair::both(
                make_item(path_l, logical.clone()),
                make_item(path_r, logical),
            ));
        }

        for name in names_r.difference(&names_l) {
            let path_r = right.physical_path.join(name);
            let logical = if right.logical_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", right.logical_path, name)
            };
            children.push(ItemPair::added(make_item(path_r, logical)));
        }

        for name in names_l.difference(&names_r) {
            let path_l = left.physical_path.join(name);
            let logical = if left.logical_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", left.logical_path, name)
            };
            children.push(ItemPair::removed(make_item(path_l, logical)));
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
