use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::types::{ArtifactDescriptor, ArtifactFormat, ArtifactSubject};
use crate::{BinocError, BinocResult, DataAccess, ItemRef};

/// In-process DataAccess backed by the local filesystem, temp directories,
/// and a filesystem-backed artifact store under `data_root/.artifacts/`.
///
/// Construction modes:
///
/// - [`Self::new`] — unrestricted paths (tests, ad-hoc tooling).
/// - [`Self::new_for_diff`] — paths must stay under the two snapshot trees or
///   session workspace (used by the controller).
/// - [`Self::for_plugin`] — shares the host's `data_root` for artifact access,
///   plus a pre-allocated workspace for expansion (C ABI plugins).
/// - [`Self::with_data_root`] — shares an existing `data_root` for artifact
///   reads only (no expansion workspace).
pub struct LocalDataAccess {
    _session_dir: Option<tempfile::TempDir>,
    data_root: PathBuf,
    external_root: Option<PathBuf>,
    workspace_counter: AtomicU32,
    workspaces: Mutex<Vec<tempfile::TempDir>>,
    provide_dir: Mutex<Option<tempfile::TempDir>>,
    path_policy: PathPolicy,
}

enum PathPolicy {
    Unrestricted,
    Restricted {
        snapshot_a: PathBuf,
        snapshot_b: PathBuf,
        extra_allowed: Mutex<Vec<PathBuf>>,
    },
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

/// True when `path` is `root` or a descendant (component-wise).
fn path_is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

fn item_ref_from_physical(physical: &Path, logical: &str) -> ItemRef {
    ItemRef {
        logical_path: logical.to_string(),
        is_dir: physical.is_dir(),
        content_hash: None,
        size: None,
        media_type: None,
        projection_hint: Default::default(),
        handle: physical.to_string_lossy().to_string(),
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
            path_policy: PathPolicy::Unrestricted,
        }
    }

    /// Session-backed access with path confinement: filesystem reads and
    /// `register_local` targets must lie under the snapshot roots, the session
    /// `data_root`, or a workspace / provide directory created by this instance.
    pub fn new_for_diff(snapshot_a: &Path, snapshot_b: &Path) -> BinocResult<Self> {
        let session = tempfile::tempdir().map_err(BinocError::Io)?;
        let data_root = session.path().to_path_buf();
        let snap_a = std::fs::canonicalize(snapshot_a).map_err(BinocError::Io)?;
        let snap_b = std::fs::canonicalize(snapshot_b).map_err(BinocError::Io)?;
        let data_root_canon = std::fs::canonicalize(&data_root).map_err(BinocError::Io)?;
        Ok(Self {
            _session_dir: Some(session),
            data_root,
            external_root: None,
            workspace_counter: AtomicU32::new(0),
            workspaces: Mutex::new(Vec::new()),
            provide_dir: Mutex::new(None),
            path_policy: PathPolicy::Restricted {
                snapshot_a: snap_a,
                snapshot_b: snap_b,
                extra_allowed: Mutex::new(vec![data_root_canon]),
            },
        })
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
            path_policy: PathPolicy::Unrestricted,
        }
    }

    /// Create a LocalDataAccess that can only read from an existing data_root
    /// cache. No workspace for expansion. Used during extract-only access.
    pub fn with_data_root(data_root: PathBuf) -> Self {
        Self {
            _session_dir: None,
            data_root,
            external_root: None,
            workspace_counter: AtomicU32::new(0),
            workspaces: Mutex::new(Vec::new()),
            provide_dir: Mutex::new(None),
            path_policy: PathPolicy::Unrestricted,
        }
    }

    fn record_allowed_if_restricted(&self, path: &Path) -> BinocResult<()> {
        if let PathPolicy::Restricted { extra_allowed, .. } = &self.path_policy {
            let c = std::fs::canonicalize(path).map_err(BinocError::Io)?;
            extra_allowed.lock().unwrap().push(c);
        }
        Ok(())
    }

    fn enforce_path_policy_resolved(&self, resolved: &Path) -> BinocResult<()> {
        match &self.path_policy {
            PathPolicy::Unrestricted => Ok(()),
            PathPolicy::Restricted {
                snapshot_a,
                snapshot_b,
                extra_allowed,
            } => {
                if path_is_within(resolved, snapshot_a) || path_is_within(resolved, snapshot_b) {
                    return Ok(());
                }
                let roots = extra_allowed.lock().unwrap();
                for root in roots.iter() {
                    if path_is_within(resolved, root) {
                        return Ok(());
                    }
                }
                Err(BinocError::PathPolicy(format!(
                    "path must stay under snapshot directories or session workspace: {}",
                    resolved.display()
                )))
            }
        }
    }

    /// Enforce policy for a path that must already exist on disk (e.g. `register_local`).
    fn enforce_path_policy(&self, physical: &Path) -> BinocResult<()> {
        let resolved = std::fs::canonicalize(physical).map_err(BinocError::Io)?;
        self.enforce_path_policy_resolved(&resolved)
    }

    /// Enforce policy before reading; allows missing leaf paths under an allowed directory.
    fn enforce_policy_for_read_path(&self, path: &Path) -> BinocResult<()> {
        match &self.path_policy {
            PathPolicy::Unrestricted => Ok(()),
            PathPolicy::Restricted { .. } => {
                if let Ok(c) = std::fs::canonicalize(path) {
                    return self.enforce_path_policy_resolved(&c);
                }
                let mut probe: Option<&Path> = Some(path);
                while let Some(p) = probe {
                    if p.as_os_str().is_empty() {
                        break;
                    }
                    if p.exists() {
                        let base = std::fs::canonicalize(p).map_err(BinocError::Io)?;
                        self.enforce_path_policy_resolved(&base)?;
                        return Ok(());
                    }
                    probe = p.parent();
                }
                Err(BinocError::PathPolicy(format!(
                    "cannot resolve path under session: {}",
                    path.display()
                )))
            }
        }
    }

    fn ensure_provide_dir(&self) -> BinocResult<PathBuf> {
        if let Some(root) = &self.external_root {
            let d = root.join("_provide");
            std::fs::create_dir_all(&d).map_err(BinocError::Io)?;
            self.record_allowed_if_restricted(&d)?;
            return Ok(d);
        }
        let mut guard = self.provide_dir.lock().unwrap();
        if guard.is_none() {
            let dir = tempfile::tempdir().map_err(BinocError::Io)?;
            self.record_allowed_if_restricted(dir.path())?;
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
        let p = Path::new(&item.handle);
        self.enforce_policy_for_read_path(p)?;
        std::fs::read(p).map_err(BinocError::Io)
    }

    fn open_read(&self, item: &ItemRef) -> BinocResult<Box<dyn std::io::Read + Send>> {
        let p = Path::new(&item.handle);
        self.enforce_policy_for_read_path(p)?;
        let file = std::fs::File::open(p).map_err(BinocError::Io)?;
        Ok(Box::new(file))
    }

    fn local_path(&self, item: &ItemRef) -> BinocResult<PathBuf> {
        let p = PathBuf::from(&item.handle);
        self.enforce_policy_for_read_path(&p)?;
        Ok(p)
    }

    fn provide(&self, logical_path: &str, content: &[u8]) -> BinocResult<ItemRef> {
        let dir = self.ensure_provide_dir()?;
        let safe_name = logical_path.replace(['/', '\\'], "_");
        let file_path = dir.join(&safe_name);
        std::fs::write(&file_path, content).map_err(BinocError::Io)?;
        self.enforce_path_policy(&file_path)?;
        Ok(item_ref_from_physical(&file_path, logical_path))
    }

    fn workspace(&self) -> BinocResult<PathBuf> {
        if let Some(root) = &self.external_root {
            let n = self.workspace_counter.fetch_add(1, Ordering::Relaxed);
            let subdir = root.join(format!("ws-{n}"));
            std::fs::create_dir_all(&subdir).map_err(BinocError::Io)?;
            self.record_allowed_if_restricted(&subdir)?;
            return Ok(subdir);
        }
        let dir = tempfile::tempdir().map_err(BinocError::Io)?;
        let path = dir.path().to_path_buf();
        self.record_allowed_if_restricted(&path)?;
        self.workspaces.lock().unwrap().push(dir);
        Ok(path)
    }

    fn register_local(&self, physical: &Path, logical: &str) -> BinocResult<ItemRef> {
        self.enforce_path_policy(physical)?;
        Ok(item_ref_from_physical(physical, logical))
    }

    fn publish_artifact(
        &self,
        format: &ArtifactFormat,
        subject: ArtifactSubject,
        producer: &str,
        data: &[u8],
    ) -> BinocResult<ArtifactDescriptor> {
        let id: u64 = rand::random();
        let dir = artifacts_dir(&self.data_root)
            .join(safe_name(&format.package))
            .join(safe_name(&format.name))
            .join(format!("v{}", format.version))
            .join(subject_dir_name(subject));
        std::fs::create_dir_all(&dir).map_err(BinocError::Io)?;
        let filename = format!("{}-{id:016x}", safe_name(producer));
        let handle = dir.join(filename).to_string_lossy().to_string();
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
        self.enforce_policy_for_read_path(&path)?;
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

    #[test]
    fn restricted_rejects_register_outside_snapshots() {
        let tmp_a = tempfile::tempdir().unwrap();
        let tmp_b = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x.txt"), b"x").unwrap();

        let da = LocalDataAccess::new_for_diff(tmp_a.path(), tmp_b.path()).unwrap();
        let p = outside.path().join("x.txt");
        let err = da.register_local(&p, "x.txt").unwrap_err();
        assert!(matches!(err, BinocError::PathPolicy(_)));
    }

    #[test]
    fn restricted_allows_register_under_snapshot() {
        let tmp_a = tempfile::tempdir().unwrap();
        let tmp_b = tempfile::tempdir().unwrap();
        let f = tmp_a.path().join("f.txt");
        std::fs::write(&f, b"ok").unwrap();

        let da = LocalDataAccess::new_for_diff(tmp_a.path(), tmp_b.path()).unwrap();
        da.register_local(&f, "f.txt").unwrap();
    }
}
