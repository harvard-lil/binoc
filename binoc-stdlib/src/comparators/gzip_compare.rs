use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use binoc_sdk::*;

const MAX_DECOMPRESSED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Expands a single-stream gzip file into one inner item, then re-dispatches
/// that item through the normal comparator chain.
pub struct GzipComparator;

struct Decompressed {
    item: ItemRef,
}

fn strip_gzip_suffix(logical_path: &str) -> Option<String> {
    logical_path
        .to_ascii_lowercase()
        .strip_suffix(".gz")
        .map(|stripped_lower| logical_path[..stripped_lower.len()].to_string())
        .filter(|s| !s.is_empty())
}

fn safe_output_path(workspace: &Path, logical_path: &str) -> PathBuf {
    let safe_name = logical_path.replace(['/', '\\'], "_");
    workspace.join(if safe_name.is_empty() {
        "decompressed"
    } else {
        &safe_name
    })
}

fn gzip_error(message: impl Into<String>) -> BinocError {
    BinocError::Gzip(message.into())
}

fn decompress_side(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<Decompressed> {
    let inner_logical = strip_gzip_suffix(&item.logical_path).ok_or_else(|| {
        gzip_error(format!(
            "gzip item has no inner filename: {}",
            item.logical_path
        ))
    })?;

    let workspace = data.workspace()?;
    let out_path = safe_output_path(&workspace, &inner_logical);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(BinocError::Io)?;
    }

    let reader = data.open_read(item)?;
    let mut decoder = flate2::read::GzDecoder::new(reader);
    let mut out = std::fs::File::create(&out_path).map_err(BinocError::Io)?;
    let mut hasher = blake3::Hasher::new();
    let mut total = 0_u64;
    let mut buf = [0_u8; 64 * 1024];

    loop {
        let n = decoder.read(&mut buf).map_err(BinocError::Io)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or_else(|| gzip_error("decompressed size overflow"))?;
        if total > MAX_DECOMPRESSED_BYTES {
            return Err(gzip_error(format!(
                "decompressed stream exceeds {} bytes: {}",
                MAX_DECOMPRESSED_BYTES, item.logical_path
            )));
        }
        out.write_all(&buf[..n]).map_err(BinocError::Io)?;
        hasher.update(&buf[..n]);
    }
    out.flush().map_err(BinocError::Io)?;

    let mut inner = data.register_local(&out_path, &inner_logical)?;
    inner.size = Some(total);
    inner.content_hash = Some(hasher.finalize().to_hex().to_string());
    inner.media_type = media_type_for_inner(&inner_logical);

    Ok(Decompressed { item: inner })
}

fn media_type_for_inner(logical_path: &str) -> Option<String> {
    mime_guess::from_path(logical_path)
        .first()
        .map(|m| m.essence_str().to_string())
}

fn decompress_pair(pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<Option<ItemPair>> {
    match (&pair.left, &pair.right) {
        (Some(left), Some(right)) => {
            let item_l = decompress_side(left, data)?.item;
            let item_r = decompress_side(right, data)?.item;
            Ok(Some(ItemPair::both(item_l, item_r)))
        }
        (None, Some(right)) => {
            let item_r = decompress_side(right, data)?.item;
            Ok(Some(ItemPair::added(item_r)))
        }
        (Some(left), None) => {
            let item_l = decompress_side(left, data)?.item;
            Ok(Some(ItemPair::removed(item_l)))
        }
        (None, None) => Ok(None),
    }
}

impl Comparator for GzipComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.gzip")
            .with_extensions(vec![".gz".into()])
            .with_media_types(vec!["application/gzip".into()])
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        _child_path: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<ItemPair> {
        decompress_pair(pair, data)?
            .ok_or_else(|| BinocError::Extract("empty pair in gzip reopen".into()))
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        let Some(inner_pair) = decompress_pair(pair, data)? else {
            return Ok(CompareResult::Identical);
        };
        if inner_pair.matching_content_hash().is_some() {
            return Ok(CompareResult::Identical);
        }

        let (action, logical) = match (&pair.left, &pair.right) {
            (Some(_), Some(right)) => ("modify", right.logical_path.as_str()),
            (None, Some(right)) => ("add", right.logical_path.as_str()),
            (Some(left), None) => ("remove", left.logical_path.as_str()),
            (None, None) => ("identical", pair.logical_path()),
        };

        let node = DiffNode::new(action, "gzip_stream", logical);
        Ok(CompareResult::Expand(node, vec![inner_pair]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_final_gzip_suffix() {
        assert_eq!(strip_gzip_suffix("data.csv.gz"), Some("data.csv".into()));
        assert_eq!(
            strip_gzip_suffix("archive.zip/data.txt.GZ"),
            Some("archive.zip/data.txt".into())
        );
        assert_eq!(strip_gzip_suffix(".gz"), None);
    }
}
