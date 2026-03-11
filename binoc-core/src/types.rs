use std::path::PathBuf;

use crate::ir::DiffNode;

// ── Format-neutral data types for reopen/extract ────────────────────

/// Format-neutral tabular data. Produced by CSV, Excel, Parquet comparators
/// via `reopen`; consumed by tabular transformers and extractors.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct TabularDataPair {
    pub left: Option<TabularData>,
    pub right: Option<TabularData>,
}

/// Data reopened from source files, format-neutral.
/// Produced by comparator `reopen_data`, consumed by transformer `extract`.
#[derive(Debug, Clone)]
pub enum ReopenedData {
    Tabular(TabularDataPair),
    Text {
        left: Option<String>,
        right: Option<String>,
    },
    Binary {
        left: Option<Vec<u8>>,
        right: Option<Vec<u8>>,
    },
}

/// Represents one side of a comparison — a file, directory, or virtual entry.
#[derive(Debug, Clone)]
pub struct Item {
    /// Where the item physically lives on disk.
    pub physical_path: PathBuf,
    /// Logical path within the snapshot (e.g. "archive.zip/data/file.csv").
    pub logical_path: String,
    /// Whether this item is a directory.
    pub is_dir: bool,
    /// Pre-computed content hash (BLAKE3). Set by expanding comparators
    /// (e.g. directory) so downstream comparators and the controller can
    /// use it for identity short-circuit and move/copy detection.
    pub content_hash: Option<String>,
    /// Detected MIME media type (e.g. "application/zip", "text/csv").
    /// Set by expanding comparators from content sniffing (magic bytes)
    /// with extension-based fallback. Used by the controller for
    /// media-type-based dispatch alongside extension matching.
    pub media_type: Option<String>,
}

impl Item {
    pub fn new(physical_path: impl Into<PathBuf>, logical_path: impl Into<String>) -> Self {
        let physical: PathBuf = physical_path.into();
        let is_dir = physical.is_dir();
        Self {
            physical_path: physical,
            logical_path: logical_path.into(),
            is_dir,
            content_hash: None,
            media_type: None,
        }
    }

    pub fn with_content_hash(mut self, hash: String) -> Self {
        self.content_hash = Some(hash);
        self
    }

    pub fn with_media_type(mut self, media_type: String) -> Self {
        self.media_type = Some(media_type);
        self
    }

    pub fn extension(&self) -> Option<String> {
        std::path::Path::new(&self.logical_path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
    }
}

/// A pair of items to compare. Either side may be None (add/remove).
#[derive(Debug)]
pub struct ItemPair {
    pub left: Option<Item>,
    pub right: Option<Item>,
}

impl ItemPair {
    pub fn both(left: Item, right: Item) -> Self {
        Self {
            left: Some(left),
            right: Some(right),
        }
    }

    pub fn added(right: Item) -> Self {
        Self {
            left: None,
            right: Some(right),
        }
    }

    pub fn removed(left: Item) -> Self {
        Self {
            left: Some(left),
            right: None,
        }
    }

    /// Get the logical path from whichever side is present (prefer right).
    pub fn logical_path(&self) -> &str {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .map(|i| i.logical_path.as_str())
            .unwrap_or("")
    }

    /// Get file extension from whichever side is present (prefer right).
    pub fn extension(&self) -> Option<String> {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .and_then(|i| i.extension())
    }

    /// Get detected media type from whichever side is present (prefer right).
    pub fn media_type(&self) -> Option<&str> {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .and_then(|i| i.media_type.as_deref())
    }

    /// Check if either side is a directory.
    pub fn is_dir(&self) -> bool {
        self.right.as_ref().is_some_and(|i| i.is_dir)
            || self.left.as_ref().is_some_and(|i| i.is_dir)
    }

    /// If both sides have matching content hashes, return the hash.
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

/// Result of a comparator's compare operation.
pub enum CompareResult {
    /// Items are identical — no diff node produced.
    Identical,
    /// Terminal diff — no further expansion needed.
    Leaf(DiffNode),
    /// Container node with children to recursively process.
    /// The DiffNode is the container; Vec<ItemPair> are child items to diff.
    Expand(DiffNode, Vec<ItemPair>),
}

/// Result of a transformer's transform operation.
pub enum TransformResult {
    /// Node unchanged — zero cost.
    Unchanged,
    /// Replace this node with a new one.
    Replace(DiffNode),
    /// Replace this node with multiple sibling nodes.
    ReplaceMany(Vec<DiffNode>),
    /// Remove this node entirely.
    Remove,
}

/// Scope at which a transformer operates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformScope {
    /// Transformer receives individual matched nodes; controller recurses into children.
    Node,
    /// Transformer receives the whole subtree; controller does NOT recurse.
    Subtree,
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
    fn item_new_sets_is_dir_for_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let item = Item::new(path.clone(), "logical/dir");
        assert!(item.is_dir);
        assert_eq!(item.physical_path, path);
        assert_eq!(item.logical_path, "logical/dir");
    }

    #[test]
    fn item_new_sets_is_dir_for_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, b"content").unwrap();
        let item = Item::new(&file_path, "logical/file.txt");
        assert!(!item.is_dir);
        assert_eq!(item.logical_path, "logical/file.txt");
    }

    #[test]
    fn item_extension_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "data.csv");
        assert_eq!(item.extension(), Some(".csv".into()));
    }

    #[test]
    fn item_extension_tar_gz_returns_gz() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "archive.tar.gz");
        assert_eq!(item.extension(), Some(".gz".into()));
    }

    #[test]
    fn item_extension_none_for_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Makefile");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "Makefile");
        assert_eq!(item.extension(), None);
    }

    #[test]
    fn item_pair_both_sets_fields() {
        let dir = tempfile::tempdir().unwrap();
        let left_path = dir.path().join("left.txt");
        let right_path = dir.path().join("right.txt");
        std::fs::write(&left_path, b"l").unwrap();
        std::fs::write(&right_path, b"r").unwrap();
        let left = Item::new(&left_path, "left.txt");
        let right = Item::new(&right_path, "right.txt");
        let pair = ItemPair::both(left, right);
        assert!(pair.left.is_some());
        assert!(pair.right.is_some());
    }

    #[test]
    fn item_pair_added_sets_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "new.txt");
        let pair = ItemPair::added(item);
        assert!(pair.left.is_none());
        assert!(pair.right.is_some());
    }

    #[test]
    fn item_pair_removed_sets_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.txt");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "old.txt");
        let pair = ItemPair::removed(item);
        assert!(pair.left.is_some());
        assert!(pair.right.is_none());
    }

    #[test]
    fn item_pair_logical_path_prefers_right() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.txt");
        let p2 = dir.path().join("b.txt");
        std::fs::write(&p1, b"").unwrap();
        std::fs::write(&p2, b"").unwrap();
        let left = Item::new(&p1, "left.txt");
        let right = Item::new(&p2, "right.txt");
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.logical_path(), "right.txt");
    }

    #[test]
    fn item_pair_logical_path_uses_left_when_right_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("only.txt");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "only.txt");
        let pair = ItemPair::removed(item);
        assert_eq!(pair.logical_path(), "only.txt");
    }

    #[test]
    fn item_pair_extension_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.csv");
        let p2 = dir.path().join("b.csv");
        std::fs::write(&p1, b"").unwrap();
        std::fs::write(&p2, b"").unwrap();
        let left = Item::new(&p1, "a.csv");
        let right = Item::new(&p2, "b.csv");
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.extension(), Some(".csv".into()));
    }

    #[test]
    fn item_pair_extension_one_side() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.csv");
        std::fs::write(&path, b"").unwrap();
        let item = Item::new(&path, "new.csv");
        let pair = ItemPair::added(item);
        assert_eq!(pair.extension(), Some(".csv".into()));
    }

    #[test]
    fn item_pair_is_dir_detects_on_either_side() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("sub");
        std::fs::create_dir(&subdir).unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, b"").unwrap();

        let pair_both_dirs =
            ItemPair::both(Item::new(&subdir, "sub"), Item::new(dir.path(), "root"));
        assert!(pair_both_dirs.is_dir());

        let pair_dir_left = ItemPair::removed(Item::new(&subdir, "sub"));
        assert!(pair_dir_left.is_dir());

        let pair_dir_right = ItemPair::added(Item::new(&subdir, "sub"));
        assert!(pair_dir_right.is_dir());

        let pair_both_files = ItemPair::both(
            Item::new(&file_path, "file.txt"),
            Item::new(&file_path, "file.txt"),
        );
        assert!(!pair_both_files.is_dir());
    }
}
