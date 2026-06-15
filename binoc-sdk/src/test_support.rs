//! Test helpers for recording data access in plugin and rule tests.
//!
//! Behind the `test-support` feature.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::traits::*;
use crate::types::{ArtifactDescriptor, ArtifactFormat, ArtifactSubject, ItemRef};

/// A single recorded DataAccess method call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum DataAccessOp {
    #[serde(rename = "read_bytes")]
    ReadBytes { handle: String, result_size: usize },
    #[serde(rename = "open_read")]
    OpenRead { handle: String },
    #[serde(rename = "local_path")]
    LocalPath { handle: String },
    #[serde(rename = "provide")]
    Provide {
        logical_path: String,
        content_size: usize,
    },
    #[serde(rename = "workspace")]
    Workspace,
    #[serde(rename = "register_local")]
    RegisterLocal { logical: String },
    #[serde(rename = "publish_artifact")]
    PublishArtifact {
        format: ArtifactFormat,
        subject: String,
        producer: String,
        size: usize,
    },
    #[serde(rename = "get_artifact")]
    GetArtifact { format: ArtifactFormat, found: bool },
    #[serde(rename = "data_root")]
    DataRoot,
}

/// Wraps a `DataAccess` impl and records every call.
pub struct RecordingDataAccess<D: DataAccess> {
    inner: D,
    log: Mutex<Vec<DataAccessOp>>,
}

impl<D: DataAccess> RecordingDataAccess<D> {
    pub fn new(inner: D) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
        }
    }

    pub fn take_log(&self) -> Vec<DataAccessOp> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    fn push(&self, op: DataAccessOp) {
        self.log.lock().unwrap().push(op);
    }
}

impl<D: DataAccess> DataAccess for RecordingDataAccess<D> {
    fn read_bytes(&self, item: &ItemRef) -> BinocResult<Vec<u8>> {
        let result = self.inner.read_bytes(item)?;
        self.push(DataAccessOp::ReadBytes {
            handle: item.logical_path.clone(),
            result_size: result.len(),
        });
        Ok(result)
    }

    fn open_read(&self, item: &ItemRef) -> BinocResult<Box<dyn std::io::Read + Send>> {
        self.push(DataAccessOp::OpenRead {
            handle: item.logical_path.clone(),
        });
        self.inner.open_read(item)
    }

    fn local_path(&self, item: &ItemRef) -> BinocResult<PathBuf> {
        self.push(DataAccessOp::LocalPath {
            handle: item.logical_path.clone(),
        });
        self.inner.local_path(item)
    }

    fn provide(&self, logical_path: &str, content: &[u8]) -> BinocResult<ItemRef> {
        self.push(DataAccessOp::Provide {
            logical_path: logical_path.to_string(),
            content_size: content.len(),
        });
        self.inner.provide(logical_path, content)
    }

    fn workspace(&self) -> BinocResult<PathBuf> {
        self.push(DataAccessOp::Workspace);
        self.inner.workspace()
    }

    fn register_local(&self, physical: &Path, logical: &str) -> BinocResult<ItemRef> {
        self.push(DataAccessOp::RegisterLocal {
            logical: logical.to_string(),
        });
        self.inner.register_local(physical, logical)
    }

    fn publish_artifact(
        &self,
        format: &ArtifactFormat,
        subject: ArtifactSubject,
        producer: &str,
        data: &[u8],
    ) -> BinocResult<ArtifactDescriptor> {
        let result = self
            .inner
            .publish_artifact(format, subject, producer, data)?;
        self.push(DataAccessOp::PublishArtifact {
            format: format.clone(),
            subject: format!("{subject:?}"),
            producer: producer.to_string(),
            size: data.len(),
        });
        Ok(result)
    }

    fn get_artifact(&self, descriptor: &ArtifactDescriptor) -> BinocResult<Option<Vec<u8>>> {
        let result = self.inner.get_artifact(descriptor)?;
        self.push(DataAccessOp::GetArtifact {
            format: descriptor.format.clone(),
            found: result.is_some(),
        });
        Ok(result)
    }

    fn data_root(&self) -> BinocResult<PathBuf> {
        self.push(DataAccessOp::DataRoot);
        self.inner.data_root()
    }
}
