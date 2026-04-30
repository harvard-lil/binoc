use serde::{Deserialize, Serialize};

use crate::ir::DiffNode;

// ── Artifact types ──────────────────────────────────────────────────

/// Which side of a comparison an artifact describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactSubject {
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "right")]
    Right,
    #[serde(rename = "pair")]
    Pair,
}

/// Identifies an artifact's data format as a structured tuple of
/// (package, name, version).
///
/// - **`package`** — the package that owns and defines this format,
///   resolvable through the language's normal package system
///   (e.g. `"binoc"`, `"binoc-csv"`, `"acme-parquet"`).
/// - **`name`** — the format name within that package
///   (e.g. `"tabular"`, `"relational-schema"`).
/// - **`version`** — a single integer. Bump only for breaking schema
///   changes. Adding optional fields to an existing version is fine
///   and does not require a bump (JSON/serde naturally ignore unknown
///   fields and default missing ones).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactFormat {
    pub package: String,
    pub name: String,
    pub version: u32,
}

impl ArtifactFormat {
    pub fn new(package: impl Into<String>, name: impl Into<String>, version: u32) -> Self {
        Self {
            package: package.into(),
            name: name.into(),
            version,
        }
    }
}

impl std::fmt::Display for ArtifactFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.v{}", self.package, self.name, self.version)
    }
}

/// Descriptor for a published artifact attached to a node.
///
/// Artifacts are the unified mechanism for both private reuse and
/// cross-plugin composition. A comparator or transformer publishes
/// zero or more artifacts; downstream plugins consume them by format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    pub format: ArtifactFormat,
    pub subject: ArtifactSubject,
    pub producer: String,
    /// Opaque handle managed by the SDK's DataAccess implementation.
    /// Plugins should not create or interpret this value directly.
    pub handle: String,
}

// ── Standard artifact formats ───────────────────────────────────────

/// Standard format for tabular data artifacts.
///
/// Any comparator that parses a tabular source format (CSV, TSV, Excel,
/// Parquet, …) should publish artifacts with this format so that
/// generic tabular transformers and extractors can consume them without
/// knowing the source format.
pub fn tabular_v1() -> ArtifactFormat {
    ArtifactFormat::new("binoc", "tabular", 1)
}

// ── Format-neutral data types ───────────────────────────────────────

/// Format-neutral tabular data. Produced by CSV, Excel, Parquet comparators;
/// consumed by tabular transformers and extractors.
///
/// This is the codec type for the [`TABULAR_V1`] artifact format.
/// Serialize with `serde_json::to_vec`, deserialize with `serde_json::from_slice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabularData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl TabularData {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }

    pub fn column_values(&self, name: &str) -> Option<Vec<&str>> {
        let idx = self.column_index(name)?;
        Some(
            self.rows
                .iter()
                .map(|r| r.get(idx).map(|s| s.as_str()).unwrap_or(""))
                .collect(),
        )
    }

    pub fn to_csv(&self) -> String {
        let mut out = self.headers.join(",");
        out.push('\n');
        for row in &self.rows {
            out.push_str(&row.join(","));
            out.push('\n');
        }
        out
    }
}

/// A pair of tabular data (left/right sides of a comparison).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabularDataPair {
    pub left: Option<TabularData>,
    pub right: Option<TabularData>,
}

impl TabularDataPair {
    /// Build a `TabularDataPair` from [`tabular_v1`] artifacts on a node.
    ///
    /// Returns `None` if neither left nor right artifact is present.
    /// This is the standard way for transformers and extractors to obtain
    /// tabular data without knowing the source format.
    pub fn from_artifacts(
        node: &crate::ir::DiffNode,
        data: &dyn crate::traits::DataAccess,
    ) -> Option<Self> {
        let fmt = tabular_v1();
        let left = node
            .artifacts
            .iter()
            .find(|a| a.format == fmt && a.subject == ArtifactSubject::Left)
            .and_then(|desc| data.get_artifact(desc).ok()?)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let right = node
            .artifacts
            .iter()
            .find(|a| a.format == fmt && a.subject == ArtifactSubject::Right)
            .and_then(|desc| data.get_artifact(desc).ok()?)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        if left.is_none() && right.is_none() {
            return None;
        }
        Some(Self { left, right })
    }
}

// ── Tabular extraction ──────────────────────────────────────────────

/// Shared extraction logic for tabular data.
///
/// Given a `TabularDataPair` and an aspect name, produces the
/// corresponding `ExtractResult`. This is format-neutral — any
/// comparator or transformer that works with tabular artifacts can
/// delegate extraction here.
pub fn tabular_extract(
    pair: &TabularDataPair,
    _node: &DiffNode,
    aspect: &str,
) -> Option<ExtractResult> {
    match aspect {
        "rows_added" => {
            let right = pair.right.as_ref()?;
            let left_len = pair.left.as_ref().map_or(0, |l| l.rows.len());
            if left_len >= right.rows.len() {
                return Some(ExtractResult::Text("No rows added.\n".into()));
            }
            let added = TabularData {
                headers: right.headers.clone(),
                rows: right.rows[left_len..].to_vec(),
            };
            Some(ExtractResult::Text(added.to_csv()))
        }
        "rows_removed" => {
            let left = pair.left.as_ref()?;
            let right_len = pair.right.as_ref().map_or(0, |r| r.rows.len());
            if right_len >= left.rows.len() {
                return Some(ExtractResult::Text("No rows removed.\n".into()));
            }
            let removed = TabularData {
                headers: left.headers.clone(),
                rows: left.rows[right_len..].to_vec(),
            };
            Some(ExtractResult::Text(removed.to_csv()))
        }
        "cells_changed" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let common_cols = tabular_columns_in_common(left, right);
            let min_rows = left.rows.len().min(right.rows.len());

            let mut out = String::from("row,column,old_value,new_value\n");
            for i in 0..min_rows {
                for col in &common_cols {
                    let li = left.column_index(col)?;
                    let ri = right.column_index(col)?;
                    let lv = left.rows[i].get(li).map(|s| s.as_str()).unwrap_or("");
                    let rv = right.rows[i].get(ri).map(|s| s.as_str()).unwrap_or("");
                    if lv != rv {
                        out.push_str(&format!("{i},{col},{lv},{rv}\n"));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "columns_added" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let left_set: std::collections::BTreeSet<&str> =
                left.headers.iter().map(|s| s.as_str()).collect();
            let added: Vec<&str> = right
                .headers
                .iter()
                .filter(|h| !left_set.contains(h.as_str()))
                .map(|h| h.as_str())
                .collect();
            if added.is_empty() {
                return Some(ExtractResult::Text("No columns added.\n".into()));
            }
            let mut out = String::new();
            for col in &added {
                out.push_str(&format!("{col}\n"));
                if let Some(vals) = right.column_values(col) {
                    for val in vals {
                        out.push_str(&format!("  {val}\n"));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "columns_removed" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let right_set: std::collections::BTreeSet<&str> =
                right.headers.iter().map(|s| s.as_str()).collect();
            let removed: Vec<&str> = left
                .headers
                .iter()
                .filter(|h| !right_set.contains(h.as_str()))
                .map(|h| h.as_str())
                .collect();
            if removed.is_empty() {
                return Some(ExtractResult::Text("No columns removed.\n".into()));
            }
            let mut out = String::new();
            for col in &removed {
                out.push_str(&format!("{col}\n"));
                if let Some(vals) = left.column_values(col) {
                    for val in vals {
                        out.push_str(&format!("  {val}\n"));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "content" | "full" => {
            let mut out = String::new();
            if let Some(left) = &pair.left {
                out.push_str("--- left\n");
                out.push_str(&left.to_csv());
            }
            if let Some(right) = &pair.right {
                out.push_str("+++ right\n");
                out.push_str(&right.to_csv());
            }
            Some(ExtractResult::Text(out))
        }
        _ => None,
    }
}

fn tabular_columns_in_common(left: &TabularData, right: &TabularData) -> Vec<String> {
    let left_set: std::collections::BTreeSet<&str> =
        left.headers.iter().map(|s| s.as_str()).collect();
    right
        .headers
        .iter()
        .filter(|h| left_set.contains(h.as_str()))
        .cloned()
        .collect()
}

// ── Item types ──────────────────────────────────────────────────────

/// Metadata-only view of one side of a comparison. Carries logical identity
/// and content metadata but NOT a filesystem path — data access goes through
/// [`DataAccess`].
///
/// # Metadata invariants
///
/// `content_hash`, `size`, and `media_type` are **opportunistic hints**.
/// Producers (expanding comparators like directory/zip, or data backends)
/// populate them when doing so is cheap — typically as a byproduct of work
/// they were already performing. Consumers **must not assume presence**, but
/// **may trust presence**: when a field is set, the value accurately reflects
/// the current bytes. Use [`ItemRef::resolve_hash`] / [`ItemRef::resolve_size`]
/// to obtain a value with a transparent fall-back read.
///
/// This keeps fast paths (directory-only listings, short-circuit identical
/// detection) cheap while letting consumers that need a value — most notably
/// the move detector, which correlates leaves across container boundaries —
/// hydrate on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRef {
    pub logical_path: String,
    pub is_dir: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Opaque identifier used by DataAccess implementations to locate data.
    /// Plugin authors should not create or interpret this value directly.
    #[serde(default)]
    pub handle: String,
}

impl ItemRef {
    pub fn extension(&self) -> Option<String> {
        std::path::Path::new(&self.logical_path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
    }

    /// Return the item's BLAKE3 content hash, computing it from bytes if
    /// not already cached on this `ItemRef`. Never valid for directories.
    pub fn resolve_hash(&self, data: &dyn crate::DataAccess) -> crate::BinocResult<String> {
        if let Some(hash) = &self.content_hash {
            return Ok(hash.clone());
        }
        let bytes = data.read_bytes(self)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    /// Return the item's byte length, reading from the backend if not already
    /// cached on this `ItemRef`. Never valid for directories.
    pub fn resolve_size(&self, data: &dyn crate::DataAccess) -> crate::BinocResult<u64> {
        if let Some(size) = self.size {
            return Ok(size);
        }
        let bytes = data.read_bytes(self)?;
        Ok(bytes.len() as u64)
    }
}

/// A pair of items to compare. Either side may be None (add/remove).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemPair {
    pub left: Option<ItemRef>,
    pub right: Option<ItemRef>,
}

impl ItemPair {
    pub fn both(left: ItemRef, right: ItemRef) -> Self {
        Self {
            left: Some(left),
            right: Some(right),
        }
    }

    pub fn added(right: ItemRef) -> Self {
        Self {
            left: None,
            right: Some(right),
        }
    }

    pub fn removed(left: ItemRef) -> Self {
        Self {
            left: Some(left),
            right: None,
        }
    }

    pub fn logical_path(&self) -> &str {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .map(|i| i.logical_path.as_str())
            .unwrap_or("")
    }

    pub fn extension(&self) -> Option<String> {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .and_then(|i| i.extension())
    }

    pub fn media_type(&self) -> Option<&str> {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .and_then(|i| i.media_type.as_deref())
    }

    pub fn is_dir(&self) -> bool {
        self.right.as_ref().is_some_and(|i| i.is_dir)
            || self.left.as_ref().is_some_and(|i| i.is_dir)
    }

    pub fn matching_content_hash(&self) -> Option<&str> {
        match (&self.left, &self.right) {
            (Some(l), Some(r)) => match (&l.content_hash, &r.content_hash) {
                (Some(hl), Some(hr)) if hl == hr => Some(hl.as_str()),
                _ => None,
            },
            _ => None,
        }
    }
}

// ── Result enums ────────────────────────────────────────────────────

/// Result of a comparator's compare operation.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CompareResult {
    /// Items are identical — no diff node produced.
    Identical,
    /// Terminal diff — no further expansion needed.
    Leaf(DiffNode),
    /// Container node with children to recursively process.
    Expand(DiffNode, Vec<ItemPair>),
    /// Comparator cannot handle this item after all — try the next one.
    Skip,
}

/// Result of a transformer's transform operation.
#[non_exhaustive]
pub enum TransformResult {
    /// Node unchanged — zero cost.
    Unchanged,
    /// Replace this node with a new one.
    Replace(Box<DiffNode>),
    /// Replace this node with multiple sibling nodes.
    ReplaceMany(Vec<DiffNode>),
    /// Remove this node entirely.
    Remove,
}

/// Dispatch filter on node shape for transformer matching.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeShapeFilter {
    /// Match any node regardless of children.
    #[default]
    Any,
    /// Match only container nodes (those with children).
    Container,
    /// Match only leaf nodes (those without children).
    Leaf,
    /// Match only the tree root. Intended for tree-wide walkers
    /// (correlation detectors, roll-ups) that need to see the entire
    /// changeset at once and do their own traversal. Called exactly
    /// once per diff.
    Root,
}

/// Whether a comparator handles files, containers (directories), or both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemScope {
    /// Non-directory items only (most comparators).
    #[default]
    Files,
    /// Directories only (directory comparator).
    Containers,
    /// Both files and directories.
    Any,
}

/// Result of an extract (on-demand detail retrieval) operation.
pub enum ExtractResult {
    Text(String),
    Binary(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_item(logical: &str, is_dir: bool) -> ItemRef {
        ItemRef {
            logical_path: logical.into(),
            is_dir,
            content_hash: None,
            size: None,
            media_type: None,
            handle: String::new(),
        }
    }

    #[test]
    fn item_ref_extension() {
        let item = bare_item("data.csv", false);
        assert_eq!(item.extension(), Some(".csv".into()));
    }

    #[test]
    fn item_ref_extension_none() {
        let item = bare_item("Makefile", false);
        assert_eq!(item.extension(), None);
    }

    #[test]
    fn item_pair_logical_path_prefers_right() {
        let left = bare_item("left.txt", false);
        let right = bare_item("right.txt", false);
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.logical_path(), "right.txt");
    }

    #[test]
    fn item_pair_logical_path_falls_back_to_left() {
        let left = bare_item("only.txt", false);
        let pair = ItemPair::removed(left);
        assert_eq!(pair.logical_path(), "only.txt");
    }

    #[test]
    fn item_pair_is_dir() {
        let dir = bare_item("sub", true);
        let pair = ItemPair::added(dir);
        assert!(pair.is_dir());
    }

    #[test]
    fn item_pair_matching_hash() {
        let mut left = bare_item("f", false);
        left.content_hash = Some("abc".into());
        let mut right = bare_item("f", false);
        right.content_hash = Some("abc".into());
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.matching_content_hash(), Some("abc"));
    }
}
