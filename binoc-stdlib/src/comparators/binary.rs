use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;

/// Content-hash comparison only (BLAKE3). Catch-all fallback comparator.
/// Uses pre-computed content_hash from Item when available (e.g. when the
/// directory comparator already hashed the file), falling back to computing
/// its own hash for items without one.
pub struct BinaryComparator;

fn hash_for_item(item: &Item) -> BinocResult<String> {
    if let Some(ref hash) = item.content_hash {
        return Ok(hash.clone());
    }
    let data = std::fs::read(&item.physical_path).map_err(BinocError::Io)?;
    Ok(blake3::hash(&data).to_hex().to_string())
}

impl Comparator for BinaryComparator {
    fn name(&self) -> &str {
        "binoc.binary"
    }

    fn can_handle(&self, _pair: &ItemPair) -> bool {
        true
    }

    fn reopen_data(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<ReopenedData> {
        let left = pair
            .left
            .as_ref()
            .map(|item| std::fs::read(&item.physical_path).map_err(BinocError::Io))
            .transpose()?;
        let right = pair
            .right
            .as_ref()
            .map(|item| std::fs::read(&item.physical_path).map_err(BinocError::Io))
            .transpose()?;
        Ok(ReopenedData::Binary { left, right })
    }

    fn extract(
        &self,
        data: &ReopenedData,
        _node: &DiffNode,
        aspect: &str,
    ) -> Option<ExtractResult> {
        let ReopenedData::Binary { left, right } = data else {
            return None;
        };
        match aspect {
            "content_left" => left.as_ref().map(|b| ExtractResult::Binary(b.clone())),
            "content_right" => right.as_ref().map(|b| ExtractResult::Binary(b.clone())),
            "content" | "full" => right
                .as_ref()
                .or(left.as_ref())
                .map(|b| ExtractResult::Binary(b.clone())),
            _ => None,
        }
    }

    fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let hash_l = hash_for_item(left)?;
                let hash_r = hash_for_item(right)?;

                if hash_l == hash_r {
                    let node = DiffNode::new("identical", "file", &right.logical_path)
                        .with_detail("hash", serde_json::json!(&hash_l));
                    return Ok(CompareResult::Leaf(node));
                }

                let size_l = std::fs::metadata(&left.physical_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                let size_r = std::fs::metadata(&right.physical_path)
                    .map(|m| m.len())
                    .unwrap_or(0);

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
                let hash = hash_for_item(right)?;
                let node = DiffNode::new("add", "file", &right.logical_path)
                    .with_summary("New file")
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_right", serde_json::json!(&hash));
                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let hash = hash_for_item(left)?;
                let node = DiffNode::new("remove", "file", &left.logical_path)
                    .with_summary("File removed")
                    .with_tag("binoc.content-changed")
                    .with_detail("hash_left", serde_json::json!(&hash));
                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
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
