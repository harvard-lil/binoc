//! Test helpers for verifying ABI isolation and recording data flow.
//!
//! Behind the `test-support` feature. Provides:
//!
//! - [`AbiComparator`] / [`AbiTransformer`] — wrappers that force every call
//!   through the JSON wire types, simulating the C ABI serialization boundary
//!   without dlopen. Also record data access ops made by the plugin.
//!
//! - [`RecordingDataAccess`] — wraps any `DataAccess` and logs every method
//!   call for golden-file assertions.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::data_access::LocalDataAccess;
use crate::ir::DiffNode;
use crate::plugin_abi::*;
use crate::traits::*;
use crate::types::*;
use crate::types::{ArtifactDescriptor, ArtifactSubject};

// ── Helpers ──────────────────────────────────────────────────────────

/// Re-attach transient fields (`source_items`, `artifacts`) to nodes in
/// a `TransformResult` after ABI round-tripping strips them.
fn restore_transient_fields(
    result: &mut TransformResult,
    source_items: &Option<ItemPair>,
    artifacts: &[ArtifactDescriptor],
) {
    match result {
        TransformResult::Replace(ref mut node) => {
            node.source_items = source_items.clone();
            node.artifacts = artifacts.to_vec();
        }
        TransformResult::ReplaceMany(ref mut nodes) => {
            for node in nodes.iter_mut() {
                node.source_items = source_items.clone();
                node.artifacts = artifacts.to_vec();
            }
        }
        _ => {}
    }
}

// ── Recording DataAccess ──────────────────────────────────────────────

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

// ── ABI call recording ────────────────────────────────────────────────

/// A single recorded ABI-level call with the data ops the plugin made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiCall {
    #[serde(skip)]
    pub seq: u64,
    pub method: String,
    pub plugin: String,
    pub data_ops: Vec<DataAccessOp>,
}

/// Format an `AbiCall` log as pretty-printed JSON for snapshot comparison.
pub fn format_abi_log(calls: &[AbiCall]) -> String {
    serde_json::to_string_pretty(calls).unwrap()
}

/// Trait for collecting ABI logs from wrapped plugins via `Arc<dyn Comparator>`
/// or `Arc<dyn Transformer>`. Implemented by `AbiComparator` and `AbiTransformer`.
pub trait AbiLogCollector: Send + Sync {
    fn take_abi_log(&self) -> Vec<AbiCall>;
}

impl<C: Comparator> AbiLogCollector for AbiComparator<C> {
    fn take_abi_log(&self) -> Vec<AbiCall> {
        self.take_log()
    }
}

impl<T: Transformer> AbiLogCollector for AbiTransformer<T> {
    fn take_abi_log(&self) -> Vec<AbiCall> {
        self.take_log()
    }
}

// ── AbiComparator ─────────────────────────────────────────────────────

/// Wraps a `Comparator` and forces every call through the JSON wire format,
/// exactly simulating the serialization boundary of the C ABI.
/// Records every call and the DataAccess ops the plugin makes.
pub struct AbiComparator<C: Comparator> {
    inner: C,
    log: Mutex<Vec<AbiCall>>,
    counter: Arc<AtomicU64>,
}

impl<C: Comparator> AbiComparator<C> {
    pub fn new(inner: C, counter: Arc<AtomicU64>) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
            counter,
        }
    }

    pub fn take_log(&self) -> Vec<AbiCall> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    fn next_seq(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }
}

impl<C: Comparator> Comparator for AbiComparator<C> {
    fn descriptor(&self) -> ComparatorDescriptor {
        self.inner.descriptor()
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        let ws = data.workspace()?;
        let data_root = data.data_root()?;

        let request = CompareRequest {
            pair: pair.clone(),
            data_root: data_root.to_string_lossy().to_string(),
            workspace: ws.to_string_lossy().to_string(),
        };
        let request_json = serde_json::to_string(&request)
            .map_err(|e| BinocError::Other(format!("ABI serialize CompareRequest: {e}")))?;

        let req2: CompareRequest = serde_json::from_str(&request_json)
            .map_err(|e| BinocError::Other(format!("ABI deserialize CompareRequest: {e}")))?;

        let plugin_data = RecordingDataAccess::new(LocalDataAccess::for_plugin(
            PathBuf::from(&req2.data_root),
            PathBuf::from(&req2.workspace),
        ));

        let result = self.inner.compare(&req2.pair, &plugin_data);

        let data_ops = plugin_data.take_log();

        let response = match &result {
            Ok(r) => {
                let artifacts = match r {
                    CompareResult::Leaf(n) | CompareResult::Expand(n, _) => n.artifacts.clone(),
                    _ => Vec::new(),
                };
                CompareResponse::Ok {
                    result: Box::new(
                        serde_json::from_value(serde_json::to_value(r).unwrap()).unwrap(),
                    ),
                    artifacts,
                }
            }
            Err(e) => CompareResponse::Error {
                message: e.to_string(),
            },
        };
        let response_json = serde_json::to_string(&response)
            .map_err(|e| BinocError::Other(format!("ABI serialize CompareResponse: {e}")))?;

        self.log.lock().unwrap().push(AbiCall {
            seq: self.next_seq(),
            method: "compare".into(),
            plugin: self.inner.descriptor().name.clone(),
            data_ops,
        });

        let resp2: CompareResponse = serde_json::from_str(&response_json)
            .map_err(|e| BinocError::Other(format!("ABI deserialize CompareResponse: {e}")))?;
        match resp2 {
            CompareResponse::Ok {
                mut result,
                artifacts,
            } => {
                match result.as_mut() {
                    CompareResult::Leaf(n) | CompareResult::Expand(n, _) => {
                        n.artifacts = artifacts;
                    }
                    _ => {}
                }
                Ok(*result)
            }
            CompareResponse::Error { message } => Err(BinocError::Comparator {
                comparator: self.inner.descriptor().name.clone(),
                message,
            }),
        }
    }

    fn reopen(
        &self,
        pair: &ItemPair,
        child_path: &str,
        data: &dyn DataAccess,
    ) -> BinocResult<ItemPair> {
        let ws = data.workspace()?;
        let data_root = data.data_root()?;

        let request = ReopenRequest {
            pair: pair.clone(),
            child_path: child_path.to_string(),
            data_root: data_root.to_string_lossy().to_string(),
            workspace: ws.to_string_lossy().to_string(),
        };
        let request_json = serde_json::to_string(&request)
            .map_err(|e| BinocError::Other(format!("ABI serialize ReopenRequest: {e}")))?;

        let req2: ReopenRequest = serde_json::from_str(&request_json)
            .map_err(|e| BinocError::Other(format!("ABI deserialize ReopenRequest: {e}")))?;

        let plugin_data = RecordingDataAccess::new(LocalDataAccess::for_plugin(
            PathBuf::from(&req2.data_root),
            PathBuf::from(&req2.workspace),
        ));

        let result = self
            .inner
            .reopen(&req2.pair, &req2.child_path, &plugin_data);

        let data_ops = plugin_data.take_log();

        let response = match &result {
            Ok(p) => ReopenResponse::Ok { pair: p.clone() },
            Err(e) => ReopenResponse::Error {
                message: e.to_string(),
            },
        };
        let response_json = serde_json::to_string(&response)
            .map_err(|e| BinocError::Other(format!("ABI serialize ReopenResponse: {e}")))?;

        self.log.lock().unwrap().push(AbiCall {
            seq: self.next_seq(),
            method: "reopen".into(),
            plugin: self.inner.descriptor().name.clone(),
            data_ops,
        });

        let resp2: ReopenResponse = serde_json::from_str(&response_json)
            .map_err(|e| BinocError::Other(format!("ABI deserialize ReopenResponse: {e}")))?;
        match resp2 {
            ReopenResponse::Ok { pair } => Ok(pair),
            ReopenResponse::Error { message } => Err(BinocError::Extract(message)),
        }
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let data_root = data.data_root().ok()?;
        let source_items = node.source_items.clone();
        let artifacts = node.artifacts.clone();

        let request = ExtractRequest {
            node: node.clone(),
            aspect: aspect.to_string(),
            data_root: data_root.to_string_lossy().to_string(),
            source_items,
            artifacts,
        };
        let request_json = serde_json::to_string(&request).ok()?;

        let req2: ExtractRequest = serde_json::from_str(&request_json).ok()?;
        let plugin_data = RecordingDataAccess::new(LocalDataAccess::with_data_root(PathBuf::from(
            &req2.data_root,
        )));

        let mut extract_node = req2.node;
        extract_node.source_items = req2.source_items;
        extract_node.artifacts = req2.artifacts;
        let result = self
            .inner
            .extract(&extract_node, &req2.aspect, &plugin_data);

        let data_ops = plugin_data.take_log();

        let response = match &result {
            Some(ExtractResult::Text(t)) => ExtractResponse::Text { content: t.clone() },
            Some(ExtractResult::Binary(b)) => ExtractResponse::Binary { content: b.clone() },
            None => ExtractResponse::None,
        };
        let response_json = serde_json::to_string(&response).ok()?;

        self.log.lock().unwrap().push(AbiCall {
            seq: self.next_seq(),
            method: "extract".into(),
            plugin: self.inner.descriptor().name.clone(),
            data_ops,
        });

        let resp2: ExtractResponse = serde_json::from_str(&response_json).ok()?;
        match resp2 {
            ExtractResponse::Text { content } => Some(ExtractResult::Text(content)),
            ExtractResponse::Binary { content } => Some(ExtractResult::Binary(content)),
            ExtractResponse::None | ExtractResponse::Error { .. } => None,
        }
    }
}

// ── AbiTransformer ────────────────────────────────────────────────────

/// Wraps a `Transformer` and forces every call through the JSON wire format.
/// Records every call and the DataAccess ops the plugin makes.
pub struct AbiTransformer<T: Transformer> {
    inner: T,
    log: Mutex<Vec<AbiCall>>,
    counter: Arc<AtomicU64>,
}

impl<T: Transformer> AbiTransformer<T> {
    pub fn new(inner: T, counter: Arc<AtomicU64>) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
            counter,
        }
    }

    pub fn take_log(&self) -> Vec<AbiCall> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    fn next_seq(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }
}

impl<T: Transformer> Transformer for AbiTransformer<T> {
    fn descriptor(&self) -> TransformerDescriptor {
        self.inner.descriptor()
    }

    fn transform(&self, node: DiffNode, data: &dyn DataAccess) -> TransformResult {
        let data_root = match data.data_root() {
            Ok(p) => p,
            Err(_) => return TransformResult::Unchanged,
        };
        let source_items = node.source_items.clone();
        let artifacts = node.artifacts.clone();

        let request = TransformRequest {
            node,
            data_root: data_root.to_string_lossy().to_string(),
            source_items: source_items.clone(),
            artifacts: artifacts.clone(),
        };
        let request_json = match serde_json::to_string(&request) {
            Ok(j) => j,
            Err(_) => return TransformResult::Unchanged,
        };

        let req2: TransformRequest = match serde_json::from_str(&request_json) {
            Ok(r) => r,
            Err(_) => return TransformResult::Unchanged,
        };

        let plugin_data = RecordingDataAccess::new(LocalDataAccess::with_data_root(PathBuf::from(
            &req2.data_root,
        )));

        let mut transform_node = req2.node;
        transform_node.source_items = req2.source_items;
        transform_node.artifacts = req2.artifacts;
        let result = self.inner.transform(transform_node, &plugin_data);

        let data_ops = plugin_data.take_log();

        let response = match &result {
            TransformResult::Unchanged => TransformResponse::Unchanged,
            TransformResult::Replace(n) => TransformResponse::Replace {
                node: Box::new(
                    serde_json::from_value(serde_json::to_value(n.as_ref()).unwrap()).unwrap(),
                ),
            },
            TransformResult::ReplaceMany(ns) => TransformResponse::ReplaceMany {
                nodes: serde_json::from_value(serde_json::to_value(ns).unwrap()).unwrap(),
            },
            TransformResult::Remove => TransformResponse::Remove,
        };
        let response_json = match serde_json::to_string(&response) {
            Ok(j) => j,
            Err(_) => return result,
        };

        self.log.lock().unwrap().push(AbiCall {
            seq: self.next_seq(),
            method: "transform".into(),
            plugin: self.inner.descriptor().name.clone(),
            data_ops,
        });

        let resp2: TransformResponse = match serde_json::from_str(&response_json) {
            Ok(r) => r,
            Err(_) => return result,
        };
        let mut result = match resp2.into_result() {
            Ok(r) => r,
            Err(_) => TransformResult::Unchanged,
        };
        restore_transient_fields(&mut result, &source_items, &artifacts);
        result
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let data_root = data.data_root().ok()?;
        let source_items = node.source_items.clone();
        let artifacts = node.artifacts.clone();

        let request = ExtractRequest {
            node: node.clone(),
            aspect: aspect.to_string(),
            data_root: data_root.to_string_lossy().to_string(),
            source_items,
            artifacts,
        };
        let request_json = serde_json::to_string(&request).ok()?;

        let req2: ExtractRequest = serde_json::from_str(&request_json).ok()?;
        let plugin_data = RecordingDataAccess::new(LocalDataAccess::with_data_root(PathBuf::from(
            &req2.data_root,
        )));

        let mut extract_node = req2.node;
        extract_node.source_items = req2.source_items;
        extract_node.artifacts = req2.artifacts;
        let result = self
            .inner
            .extract(&extract_node, &req2.aspect, &plugin_data);

        let data_ops = plugin_data.take_log();

        let response = match &result {
            Some(ExtractResult::Text(t)) => ExtractResponse::Text { content: t.clone() },
            Some(ExtractResult::Binary(b)) => ExtractResponse::Binary { content: b.clone() },
            None => ExtractResponse::None,
        };
        let response_json = serde_json::to_string(&response).ok()?;

        self.log.lock().unwrap().push(AbiCall {
            seq: self.next_seq(),
            method: "extract".into(),
            plugin: self.inner.descriptor().name.clone(),
            data_ops,
        });

        let resp2: ExtractResponse = serde_json::from_str(&response_json).ok()?;
        match resp2 {
            ExtractResponse::Text { content } => Some(ExtractResult::Text(content)),
            ExtractResponse::Binary { content } => Some(ExtractResult::Binary(content)),
            ExtractResponse::None | ExtractResponse::Error { .. } => None,
        }
    }
}
