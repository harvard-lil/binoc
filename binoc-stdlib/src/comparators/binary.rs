use binoc_sdk::*;

/// Content-hash comparison only (BLAKE3). Catch-all fallback comparator.
pub struct BinaryComparator;

impl Comparator for BinaryComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.binary")
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                if let Some(hash) = pair.matching_content_hash() {
                    let node = DiffNode::new("identical", "file", &right.logical_path)
                        .with_detail("hash", serde_json::json!(hash));
                    return Ok(CompareResult::Leaf(node));
                }

                let hash_l = left.resolve_hash(data)?;
                let hash_r = right.resolve_hash(data)?;

                if hash_l == hash_r {
                    let node = DiffNode::new("identical", "file", &right.logical_path)
                        .with_detail("hash", serde_json::json!(&hash_l));
                    return Ok(CompareResult::Leaf(node));
                }

                let size_l = left.resolve_size(data)?;
                let size_r = right.resolve_size(data)?;

                let summary = format!(
                    "Content changed ({} → {})",
                    fmt_bytes(size_l),
                    fmt_bytes(size_r)
                );
                let node = DiffNode::new("modify", "file", &right.logical_path)
                    .with_summary(summary)
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_left", serde_json::json!(&hash_l))
                    .with_detail("hash_right", serde_json::json!(&hash_r))
                    .with_detail("size_left", serde_json::json!(size_l))
                    .with_detail("size_right", serde_json::json!(size_r));

                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => {
                let hash = right.resolve_hash(data)?;
                let node = DiffNode::new("add", "file", &right.logical_path)
                    .with_summary("New file")
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_right", serde_json::json!(&hash));
                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let hash = left.resolve_hash(data)?;
                let node = DiffNode::new("remove", "file", &left.logical_path)
                    .with_summary("File removed")
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_left", serde_json::json!(&hash));
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
