use binoc_sdk::*;

/// Content-hash comparison only (BLAKE3). Catch-all fallback comparator.
pub struct BinaryComparator;

struct ItemInfo {
    hash: String,
    size: u64,
}

fn info_for_item(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ItemInfo> {
    if let Some(ref hash) = item.content_hash {
        return Ok(ItemInfo {
            hash: hash.clone(),
            size: 0,
        });
    }
    let bytes = data.read_bytes(item)?;
    Ok(ItemInfo {
        hash: blake3::hash(&bytes).to_hex().to_string(),
        size: bytes.len() as u64,
    })
}

impl Comparator for BinaryComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.binary")
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let info_l = info_for_item(left, data)?;
                let info_r = info_for_item(right, data)?;

                if info_l.hash == info_r.hash {
                    let node = DiffNode::new("identical", "file", &right.logical_path)
                        .with_detail("hash", serde_json::json!(&info_l.hash));
                    return Ok(CompareResult::Leaf(node));
                }

                let summary = format!(
                    "Content changed ({} → {})",
                    fmt_bytes(info_l.size),
                    fmt_bytes(info_r.size)
                );
                let node = DiffNode::new("modify", "file", &right.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_left", serde_json::json!(&info_l.hash))
                    .with_detail("hash_right", serde_json::json!(&info_r.hash))
                    .with_detail("size_left", serde_json::json!(info_l.size))
                    .with_detail("size_right", serde_json::json!(info_r.size));

                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => {
                let info = info_for_item(right, data)?;
                let node = DiffNode::new("add", "file", &right.logical_path)
                    .with_summary("New file")
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_right", serde_json::json!(&info.hash));
                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let info = info_for_item(left, data)?;
                let node = DiffNode::new("remove", "file", &left.logical_path)
                    .with_summary("File removed")
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_left", serde_json::json!(&info.hash));
                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }

    fn extract(
        &self,
        _node: &DiffNode,
        _aspect: &str,
        _data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        None
    }
}

fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} bytes")
    }
}
