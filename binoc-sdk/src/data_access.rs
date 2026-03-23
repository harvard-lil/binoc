use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::types::{ArtifactDescriptor, ArtifactFormat, ArtifactSubject};
use crate::{BinocError, BinocResult, DataAccess, ItemRef};

/// In-process DataAccess backed by the local filesystem, temp directories,
/// and a filesystem-backed artifact store under `data_root/.artifacts/`.
///
/// Three construction modes:
///
/// - `new()` — owns a session temp dir as data_root (used by the controller).
/// - `for_plugin(data_root, workspace)` — shares the host's data_root for
///   artifact access, plus a pre-allocated workspace for expansion. Used by
///   the `export_plugin!` macro across the C ABI.
/// - `with_data_root(data_root)` — shares an existing data_root for artifact
///   access only (no expansion workspace). Used for extract-only access.
pub struct LocalDataAccess {
    _session_dir: Option<tempfile::TempDir>,
    data_root: PathBuf,
    external_root: Option<PathBuf>,
    workspace_counter: AtomicU32,
    workspaces: Mutex<Vec<tempfile::TempDir>>,
    provide_dir: Mutex<Option<tempfile::TempDir>>,
}

fn artifacts_dir(data_root: &Path) -> PathBuf {
    data_root.join(".artifacts")
}

fn safe_name(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' {
                (b as char).to_string()
            } else {
                format!("%{b:02x}")
            }
        })
        .collect()
}

fn subject_dir_name(subject: ArtifactSubject) -> &'static str {
    match subject {
        ArtifactSubject::Left => "left",
        ArtifactSubject::Right => "right",
        ArtifactSubject::Pair => "pair",
    }
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

    fn publish_artifact(
        &self,
        format: &ArtifactFormat,
        subject: ArtifactSubject,
        producer: &str,
        data: &[u8],
    ) -> BinocResult<ArtifactDescriptor> {
        let dir = artifacts_dir(&self.data_root)
            .join(safe_name(&format.package))
            .join(safe_name(&format.name))
            .join(format!("v{}", format.version))
            .join(subject_dir_name(subject));
        std::fs::create_dir_all(&dir).map_err(BinocError::Io)?;
        let handle = dir.join(safe_name(producer)).to_string_lossy().to_string();
        std::fs::write(&handle, data).map_err(BinocError::Io)?;
        Ok(ArtifactDescriptor {
            format: format.clone(),
            subject,
            producer: producer.to_string(),
            handle,
        })
    }

    fn get_artifact(&self, descriptor: &ArtifactDescriptor) -> BinocResult<Option<Vec<u8>>> {
        let path = PathBuf::from(&descriptor.handle);
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
    fn publish_and_get_artifact_round_trip() {
        let da = LocalDataAccess::new();
        let fmt = ArtifactFormat::new("binoc", "tabular", 1);
        let desc = da
            .publish_artifact(&fmt, ArtifactSubject::Left, "binoc.csv", b"hello world")
            .unwrap();
        assert_eq!(desc.format, fmt);
        assert_eq!(desc.subject, ArtifactSubject::Left);
        assert_eq!(desc.producer, "binoc.csv");
        let loaded = da.get_artifact(&desc).unwrap();
        assert_eq!(loaded, Some(b"hello world".to_vec()));
    }

    #[test]
    fn get_artifact_missing_returns_none() {
        let da = LocalDataAccess::new();
        let desc = ArtifactDescriptor {
            format: ArtifactFormat::new("nonexistent", "thing", 1),
            subject: ArtifactSubject::Pair,
            producer: "test".into(),
            handle: "/tmp/does-not-exist-binoc-test".into(),
        };
        assert_eq!(da.get_artifact(&desc).unwrap(), None);
    }

    #[test]
    fn cross_instance_artifact_visibility() {
        let da = LocalDataAccess::new();
        let fmt = ArtifactFormat::new("binoc", "tabular", 1);
        let desc = da
            .publish_artifact(&fmt, ArtifactSubject::Right, "binoc.csv", b"shared-value")
            .unwrap();
        let data_root = da.data_root().unwrap();

        let plugin_da = LocalDataAccess::with_data_root(data_root);
        let loaded = plugin_da.get_artifact(&desc).unwrap();
        assert_eq!(loaded, Some(b"shared-value".to_vec()));
    }

    #[test]
    fn for_plugin_shares_artifacts() {
        let da = LocalDataAccess::new();
        let data_root = da.data_root().unwrap();
        let ws = da.workspace().unwrap();

        let plugin_da = LocalDataAccess::for_plugin(data_root, ws);
        let fmt = ArtifactFormat::new("myplugin", "schema", 1);
        let desc = plugin_da
            .publish_artifact(&fmt, ArtifactSubject::Pair, "myplugin", b"plugin-data")
            .unwrap();

        let loaded = da.get_artifact(&desc).unwrap();
        assert_eq!(loaded, Some(b"plugin-data".to_vec()));
    }

    #[test]
    fn data_root_returns_valid_path() {
        let da = LocalDataAccess::new();
        let root = da.data_root().unwrap();
        assert!(root.exists());
    }
}
