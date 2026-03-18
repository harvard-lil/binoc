use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::{BinocError, BinocResult, DataAccess, ItemRef};

/// In-process DataAccess backed by the local filesystem, temp directories,
/// and a filesystem-backed cache under `data_root/.cache/`.
///
/// Three construction modes:
///
/// - `new()` — owns a session temp dir as data_root (used by the controller).
/// - `for_plugin(data_root, workspace)` — shares the host's data_root for
///   cache access, plus a pre-allocated workspace for expansion. Used by
///   the `export_plugin!` macro across the C ABI.
/// - `with_data_root(data_root)` — shares an existing data_root for cache
///   access only (no expansion workspace). Used for extract-only access.
pub struct LocalDataAccess {
    _session_dir: Option<tempfile::TempDir>,
    data_root: PathBuf,
    external_root: Option<PathBuf>,
    workspace_counter: AtomicU32,
    workspaces: Mutex<Vec<tempfile::TempDir>>,
    provide_dir: Mutex<Option<tempfile::TempDir>>,
}

fn cache_dir(data_root: &Path) -> PathBuf {
    data_root.join(".cache")
}

fn safe_cache_key(key: &str) -> String {
    key.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' {
                (b as char).to_string()
            } else {
                format!("%{b:02x}")
            }
        })
        .collect()
}

impl LocalDataAccess {
    pub fn new() -> Self {
        let session = tempfile::tempdir().expect("failed to create session temp dir");
        let data_root = session.path().to_path_buf();
        Self {
            _session_dir: Some(session),
            data_root,
            external_root: None,
            workspace_counter: AtomicU32::new(0),
            workspaces: Mutex::new(Vec::new()),
            provide_dir: Mutex::new(None),
        }
    }

    /// Create a LocalDataAccess for a plugin running across the C ABI.
    /// Shares the host's `data_root` for cache access and uses `workspace`
    /// for expansion (provide, workspace calls).
    pub fn for_plugin(data_root: PathBuf, workspace: PathBuf) -> Self {
        Self {
            _session_dir: None,
            data_root,
            external_root: Some(workspace),
            workspace_counter: AtomicU32::new(0),
            workspaces: Mutex::new(Vec::new()),
            provide_dir: Mutex::new(None),
        }
    }

    /// Create a LocalDataAccess that can only read from an existing data_root
    /// cache. No workspace for expansion. Used during extract.
    pub fn with_data_root(data_root: PathBuf) -> Self {
        Self {
            _session_dir: None,
            data_root,
            external_root: None,
            workspace_counter: AtomicU32::new(0),
            workspaces: Mutex::new(Vec::new()),
            provide_dir: Mutex::new(None),
        }
    }

    pub fn register_local_impl(physical: &Path, logical: &str) -> BinocResult<ItemRef> {
        Ok(ItemRef {
            logical_path: logical.to_string(),
            is_dir: physical.is_dir(),
            content_hash: None,
            media_type: None,
            handle: physical.to_string_lossy().to_string(),
        })
    }

    fn ensure_provide_dir(&self) -> BinocResult<PathBuf> {
        if let Some(root) = &self.external_root {
            let d = root.join("_provide");
            std::fs::create_dir_all(&d).map_err(BinocError::Io)?;
            return Ok(d);
        }
        let mut guard = self.provide_dir.lock().unwrap();
        if guard.is_none() {
            let dir = tempfile::tempdir().map_err(BinocError::Io)?;
            *guard = Some(dir);
        }
        Ok(guard.as_ref().unwrap().path().to_path_buf())
    }
}

impl Default for LocalDataAccess {
    fn default() -> Self {
        Self::new()
    }
}

impl DataAccess for LocalDataAccess {
    fn read_bytes(&self, item: &ItemRef) -> BinocResult<Vec<u8>> {
        std::fs::read(&item.handle).map_err(BinocError::Io)
    }

    fn open_read(&self, item: &ItemRef) -> BinocResult<Box<dyn std::io::Read + Send>> {
        let file = std::fs::File::open(&item.handle).map_err(BinocError::Io)?;
        Ok(Box::new(file))
    }

    fn local_path(&self, item: &ItemRef) -> BinocResult<PathBuf> {
        Ok(PathBuf::from(&item.handle))
    }

    fn provide(&self, logical_path: &str, content: &[u8]) -> BinocResult<ItemRef> {
        let dir = self.ensure_provide_dir()?;
        let safe_name = logical_path.replace(['/', '\\'], "_");
        let file_path = dir.join(&safe_name);
        std::fs::write(&file_path, content).map_err(BinocError::Io)?;
        Self::register_local_impl(&file_path, logical_path)
    }

    fn workspace(&self) -> BinocResult<PathBuf> {
        if let Some(root) = &self.external_root {
            let n = self.workspace_counter.fetch_add(1, Ordering::Relaxed);
            let subdir = root.join(format!("ws-{n}"));
            std::fs::create_dir_all(&subdir).map_err(BinocError::Io)?;
            return Ok(subdir);
        }
        let dir = tempfile::tempdir().map_err(BinocError::Io)?;
        let path = dir.path().to_path_buf();
        self.workspaces.lock().unwrap().push(dir);
        Ok(path)
    }

    fn register_local(&self, physical: &Path, logical: &str) -> BinocResult<ItemRef> {
        Self::register_local_impl(physical, logical)
    }

    fn store(&self, key: &str, data: &[u8]) -> BinocResult<()> {
        let dir = cache_dir(&self.data_root);
        std::fs::create_dir_all(&dir).map_err(BinocError::Io)?;
        let path = dir.join(safe_cache_key(key));
        std::fs::write(path, data).map_err(BinocError::Io)
    }

    fn load(&self, key: &str) -> BinocResult<Option<Vec<u8>>> {
        let path = cache_dir(&self.data_root).join(safe_cache_key(key));
        match std::fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(BinocError::Io(e)),
        }
    }

    fn data_root(&self) -> BinocResult<PathBuf> {
        Ok(self.data_root.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_load_round_trip() {
        let da = LocalDataAccess::new();
        da.store("test-key", b"hello world").unwrap();
        let loaded = da.load("test-key").unwrap();
        assert_eq!(loaded, Some(b"hello world".to_vec()));
    }

    #[test]
    fn load_missing_returns_none() {
        let da = LocalDataAccess::new();
        assert_eq!(da.load("nonexistent").unwrap(), None);
    }

    #[test]
    fn store_load_with_special_chars_in_key() {
        let da = LocalDataAccess::new();
        da.store("tabular:path/to/file.csv", b"data").unwrap();
        let loaded = da.load("tabular:path/to/file.csv").unwrap();
        assert_eq!(loaded, Some(b"data".to_vec()));
    }

    #[test]
    fn cross_instance_cache_visibility() {
        let da = LocalDataAccess::new();
        da.store("shared-key", b"shared-value").unwrap();
        let data_root = da.data_root().unwrap();

        let plugin_da = LocalDataAccess::with_data_root(data_root);
        let loaded = plugin_da.load("shared-key").unwrap();
        assert_eq!(loaded, Some(b"shared-value".to_vec()));
    }

    #[test]
    fn for_plugin_shares_cache() {
        let da = LocalDataAccess::new();
        let data_root = da.data_root().unwrap();
        let ws = da.workspace().unwrap();

        let plugin_da = LocalDataAccess::for_plugin(data_root, ws);
        plugin_da.store("from-plugin", b"plugin-data").unwrap();

        let loaded = da.load("from-plugin").unwrap();
        assert_eq!(loaded, Some(b"plugin-data".to_vec()));
    }

    #[test]
    fn data_root_returns_valid_path() {
        let da = LocalDataAccess::new();
        let root = da.data_root().unwrap();
        assert!(root.exists());
    }
}
