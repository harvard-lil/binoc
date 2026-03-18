use serde::{Deserialize, Serialize};

use crate::ir::DiffNode;

// ── Format-neutral data types ───────────────────────────────────────

/// Format-neutral tabular data. Produced by CSV, Excel, Parquet comparators;
/// consumed by tabular transformers and extractors. Convention type — not
/// part of the DataAccess cache protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ── Item types ──────────────────────────────────────────────────────

/// Metadata-only view of one side of a comparison. Carries logical identity
/// and content metadata but NOT a filesystem path — data access goes through
/// [`DataAccess`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRef {
    pub logical_path: String,
    pub is_dir: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
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

/// Scope at which a transformer operates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformScope {
    /// Transformer receives individual matched nodes; controller recurses into children.
    #[default]
    Node,
    /// Transformer receives the whole subtree; controller does NOT recurse.
    Subtree,
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

    #[test]
    fn item_ref_extension() {
        let item = ItemRef {
            logical_path: "data.csv".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: String::new(),
        };
        assert_eq!(item.extension(), Some(".csv".into()));
    }

    #[test]
    fn item_ref_extension_none() {
        let item = ItemRef {
            logical_path: "Makefile".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: String::new(),
        };
        assert_eq!(item.extension(), None);
    }

    #[test]
    fn item_pair_logical_path_prefers_right() {
        let left = ItemRef {
            logical_path: "left.txt".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: String::new(),
        };
        let right = ItemRef {
            logical_path: "right.txt".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: String::new(),
        };
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.logical_path(), "right.txt");
    }

    #[test]
    fn item_pair_logical_path_falls_back_to_left() {
        let left = ItemRef {
            logical_path: "only.txt".into(),
            is_dir: false,
            content_hash: None,
            media_type: None,
            handle: String::new(),
        };
        let pair = ItemPair::removed(left);
        assert_eq!(pair.logical_path(), "only.txt");
    }

    #[test]
    fn item_pair_is_dir() {
        let dir = ItemRef {
            logical_path: "sub".into(),
            is_dir: true,
            content_hash: None,
            media_type: None,
            handle: String::new(),
        };
        let pair = ItemPair::added(dir);
        assert!(pair.is_dir());
    }

    #[test]
    fn item_pair_matching_hash() {
        let left = ItemRef {
            logical_path: "f".into(),
            is_dir: false,
            content_hash: Some("abc".into()),
            media_type: None,
            handle: String::new(),
        };
        let right = ItemRef {
            logical_path: "f".into(),
            is_dir: false,
            content_hash: Some("abc".into()),
            media_type: None,
            handle: String::new(),
        };
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.matching_content_hash(), Some("abc"));
    }
}
