use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::sync::Arc;

use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString};

use binoc_core::config::{DatasetConfig, PluginRegistry};
use binoc_core::controller::Controller;
use binoc_core::output;
use binoc_sdk::plugin_abi::{
    CompareRequest, CompareResponse, ExtractRequest, ExtractResponse, PluginDescription,
    RenderRequest, RenderResponse, ReopenRequest, ReopenResponse, TransformRequest,
    TransformResponse,
};
use binoc_sdk::*;

use binoc_stdlib::renderers::markdown as md_renderer;

// ═══════════════════════════════════════════════════════════════════════════
// Native plugin loader — loads Rust plugins via C ABI (libloading)
// ═══════════════════════════════════════════════════════════════════════════

type DescribeFn = unsafe extern "C" fn() -> *mut std::ffi::c_char;
type AbiFn = unsafe extern "C" fn(u32, *const std::ffi::c_char) -> *mut std::ffi::c_char;
type FreeFn = unsafe extern "C" fn(*mut std::ffi::c_char);

struct NativePlugin {
    _lib: libloading::Library,
    describe_fn: DescribeFn,
    free_fn: FreeFn,
    compare_fn: Option<AbiFn>,
    reopen_fn: Option<AbiFn>,
    comparator_extract_fn: Option<AbiFn>,
    transform_fn: Option<AbiFn>,
    transformer_extract_fn: Option<AbiFn>,
    render_fn: Option<AbiFn>,
}

unsafe impl Send for NativePlugin {}
unsafe impl Sync for NativePlugin {}

impl NativePlugin {
    fn load(path: &str) -> Result<Self, String> {
        unsafe {
            let lib = libloading::Library::new(path)
                .map_err(|e| format!("failed to load native plugin {path}: {e}"))?;

            let describe: libloading::Symbol<DescribeFn> = lib
                .get(b"_binoc_plugin_describe")
                .map_err(|e| format!("missing _binoc_plugin_describe in {path}: {e}"))?;
            let free: libloading::Symbol<FreeFn> = lib
                .get(b"_binoc_free_string")
                .map_err(|e| format!("missing _binoc_free_string in {path}: {e}"))?;

            let describe_fn = *describe;
            let free_fn = *free;

            let compare_fn = lib
                .get::<AbiFn>(b"_binoc_comparator_compare")
                .ok()
                .map(|s| *s);
            let reopen_fn = lib
                .get::<AbiFn>(b"_binoc_comparator_reopen")
                .ok()
                .map(|s| *s);
            let comparator_extract_fn = lib
                .get::<AbiFn>(b"_binoc_comparator_extract")
                .ok()
                .map(|s| *s);
            let transform_fn = lib
                .get::<AbiFn>(b"_binoc_transformer_transform")
                .ok()
                .map(|s| *s);
            let transformer_extract_fn = lib
                .get::<AbiFn>(b"_binoc_transformer_extract")
                .ok()
                .map(|s| *s);
            let render_fn = lib.get::<AbiFn>(b"_binoc_renderer_render").ok().map(|s| *s);

            Ok(Self {
                _lib: lib,
                describe_fn,
                free_fn,
                compare_fn,
                reopen_fn,
                comparator_extract_fn,
                transform_fn,
                transformer_extract_fn,
                render_fn,
            })
        }
    }

    fn describe(&self) -> Result<PluginDescription, String> {
        unsafe {
            let ptr = (self.describe_fn)();
            if ptr.is_null() {
                return Err("_binoc_plugin_describe returned null".into());
            }
            let json = CStr::from_ptr(ptr)
                .to_str()
                .map_err(|e| format!("invalid UTF-8 from describe: {e}"))?
                .to_string();
            (self.free_fn)(ptr);
            serde_json::from_str(&json).map_err(|e| format!("invalid plugin description JSON: {e}"))
        }
    }

    fn call_abi(&self, func: AbiFn, index: u32, request_json: &str) -> Result<String, String> {
        unsafe {
            let request =
                CString::new(request_json).map_err(|e| format!("null byte in request: {e}"))?;
            let ptr = func(index, request.as_ptr());
            if ptr.is_null() {
                return Err("ABI call returned null".into());
            }
            let json = CStr::from_ptr(ptr)
                .to_str()
                .map_err(|e| format!("invalid UTF-8 from ABI call: {e}"))?
                .to_string();
            (self.free_fn)(ptr);
            Ok(json)
        }
    }
}

// ── NativeComparator ───────────────────────────────────────────────

struct NativeComparator {
    plugin: Arc<NativePlugin>,
    desc: ComparatorDescriptor,
    index: u32,
}

impl Comparator for NativeComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        self.desc.clone()
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        let compare_fn = self
            .plugin
            .compare_fn
            .ok_or_else(|| BinocError::Other("plugin missing _binoc_comparator_compare".into()))?;
        let ws = data.workspace()?;
        let data_root = data.data_root()?;
        let request = CompareRequest {
            pair: pair.clone(),
            data_root: data_root.to_string_lossy().to_string(),
            workspace: ws.to_string_lossy().to_string(),
        };
        let request_json = serde_json::to_string(&request)
            .map_err(|e| BinocError::Other(format!("serialize CompareRequest: {e}")))?;
        let json = self
            .plugin
            .call_abi(compare_fn, self.index, &request_json)
            .map_err(BinocError::Other)?;
        let response: CompareResponse = serde_json::from_str(&json)
            .map_err(|e| BinocError::Other(format!("deserialize CompareResponse: {e}")))?;
        match response {
            CompareResponse::Ok { result } => Ok(*result),
            CompareResponse::Error { message } => Err(BinocError::Comparator {
                comparator: self.desc.name.clone(),
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
        let reopen_fn = self.plugin.reopen_fn.ok_or_else(|| {
            BinocError::Extract(format!("{} does not support reopen", self.desc.name))
        })?;
        let ws = data.workspace()?;
        let data_root = data.data_root()?;
        let request = ReopenRequest {
            pair: pair.clone(),
            child_path: child_path.to_string(),
            data_root: data_root.to_string_lossy().to_string(),
            workspace: ws.to_string_lossy().to_string(),
        };
        let request_json = serde_json::to_string(&request)
            .map_err(|e| BinocError::Other(format!("serialize ReopenRequest: {e}")))?;
        let json = self
            .plugin
            .call_abi(reopen_fn, self.index, &request_json)
            .map_err(BinocError::Other)?;
        let response: ReopenResponse = serde_json::from_str(&json)
            .map_err(|e| BinocError::Other(format!("deserialize ReopenResponse: {e}")))?;
        match response {
            ReopenResponse::Ok { pair } => Ok(*pair),
            ReopenResponse::Error { message } => Err(BinocError::Extract(message)),
        }
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let extract_fn = self.plugin.comparator_extract_fn?;
        let data_root = data.data_root().ok()?;
        let request = ExtractRequest {
            node: node.clone(),
            aspect: aspect.to_string(),
            data_root: data_root.to_string_lossy().to_string(),
        };
        let request_json = serde_json::to_string(&request).ok()?;
        let json = self
            .plugin
            .call_abi(extract_fn, self.index, &request_json)
            .ok()?;
        let response: ExtractResponse = serde_json::from_str(&json).ok()?;
        match response {
            ExtractResponse::Text { content } => Some(ExtractResult::Text(content)),
            ExtractResponse::Binary { content } => Some(ExtractResult::Binary(content)),
            ExtractResponse::None | ExtractResponse::Error { .. } => None,
        }
    }
}

// ── NativeTransformer ──────────────────────────────────────────────

struct NativeTransformer {
    plugin: Arc<NativePlugin>,
    desc: TransformerDescriptor,
    index: u32,
}

impl Transformer for NativeTransformer {
    fn descriptor(&self) -> TransformerDescriptor {
        self.desc.clone()
    }

    fn transform(
        &self,
        node: DiffNode,
        data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let Some(transform_fn) = self.plugin.transform_fn else {
            return TransformResult::Unchanged;
        };
        let data_root = match data.data_root() {
            Ok(p) => p,
            Err(_) => return TransformResult::Unchanged,
        };
        let request = TransformRequest {
            node,
            data_root: data_root.to_string_lossy().to_string(),
            config: config.clone(),
        };
        let request_json = match serde_json::to_string(&request) {
            Ok(j) => j,
            Err(_) => return TransformResult::Unchanged,
        };
        let json = match self
            .plugin
            .call_abi(transform_fn, self.index, &request_json)
        {
            Ok(j) => j,
            Err(_) => return TransformResult::Unchanged,
        };
        let response: TransformResponse = match serde_json::from_str(&json) {
            Ok(r) => r,
            Err(_) => return TransformResult::Unchanged,
        };
        match response.into_result() {
            Ok(r) => r,
            Err(_) => TransformResult::Unchanged,
        }
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let extract_fn = self.plugin.transformer_extract_fn?;
        let data_root = data.data_root().ok()?;
        let request = ExtractRequest {
            node: node.clone(),
            aspect: aspect.to_string(),
            data_root: data_root.to_string_lossy().to_string(),
        };
        let request_json = serde_json::to_string(&request).ok()?;
        let json = self
            .plugin
            .call_abi(extract_fn, self.index, &request_json)
            .ok()?;
        let response: ExtractResponse = serde_json::from_str(&json).ok()?;
        match response {
            ExtractResponse::Text { content } => Some(ExtractResult::Text(content)),
            ExtractResponse::Binary { content } => Some(ExtractResult::Binary(content)),
            ExtractResponse::None | ExtractResponse::Error { .. } => None,
        }
    }
}

// ── NativeRenderer ────────────────────────────────────────────────

struct NativeRenderer {
    plugin: Arc<NativePlugin>,
    desc: RendererDescriptor,
    index: u32,
}

impl Renderer for NativeRenderer {
    fn descriptor(&self) -> RendererDescriptor {
        self.desc.clone()
    }

    fn render(&self, changesets: &[Changeset], config: &serde_json::Value) -> BinocResult<String> {
        let render_fn = self
            .plugin
            .render_fn
            .ok_or_else(|| BinocError::Other("plugin missing _binoc_renderer_render".into()))?;
        let request = RenderRequest {
            changesets: changesets.to_vec(),
            config: config.clone(),
        };
        let request_json = serde_json::to_string(&request)
            .map_err(|e| BinocError::Other(format!("serialize RenderRequest: {e}")))?;
        let json = self
            .plugin
            .call_abi(render_fn, self.index, &request_json)
            .map_err(BinocError::Other)?;
        let response: RenderResponse = serde_json::from_str(&json)
            .map_err(|e| BinocError::Other(format!("deserialize RenderResponse: {e}")))?;
        match response {
            RenderResponse::Ok { output } => Ok(output),
            RenderResponse::Error { message } => Err(BinocError::Other(message)),
        }
    }
}

// ── Library resolution and loading ─────────────────────────────────

/// Given a module's `__file__`, find the native shared library.
///
/// When maturin packages a pyo3 extension, the installed layout is a
/// Python package directory containing both an `__init__.py` and the
/// `.so`/`.dylib`/`.pyd`. If `__file__` points to `__init__.py`, we
/// scan the directory for the native extension.
fn resolve_native_library(file_path: &str) -> Result<String, String> {
    if !file_path.ends_with("__init__.py") {
        return Ok(file_path.to_string());
    }
    let dir = std::path::Path::new(file_path)
        .parent()
        .ok_or("no parent directory for __init__.py")?;
    let entry = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .find(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "so" | "dylib" | "pyd"))
        })
        .ok_or_else(|| format!("no native extension found in {}", dir.display()))?;
    Ok(entry.path().to_string_lossy().to_string())
}

fn load_native_plugin_into_registry(
    module_path: &str,
    registry: &mut PluginRegistry,
) -> Result<(), String> {
    let lib_path = Python::attach(|py| -> PyResult<String> {
        let module = py.import(module_path)?;
        let file_attr = module.getattr("__file__")?;
        file_attr.extract::<String>()
    })
    .map_err(|e| format!("could not import {module_path}: {e}"))?;

    let lib_path = resolve_native_library(&lib_path)?;

    let plugin = Arc::new(NativePlugin::load(&lib_path)?);
    let description = plugin.describe()?;

    for (i, desc) in description.comparators.into_iter().enumerate() {
        let native = NativeComparator {
            plugin: Arc::clone(&plugin),
            desc,
            index: i as u32,
        };
        registry
            .register_comparator(Arc::new(native))
            .map_err(|e| e.to_string())?;
    }

    for (i, desc) in description.transformers.into_iter().enumerate() {
        let native = NativeTransformer {
            plugin: Arc::clone(&plugin),
            desc,
            index: i as u32,
        };
        registry
            .register_transformer(Arc::new(native))
            .map_err(|e| e.to_string())?;
    }

    for (i, desc) in description.renderers.into_iter().enumerate() {
        let native = NativeRenderer {
            plugin: Arc::clone(&plugin),
            desc,
            index: i as u32,
        };
        registry
            .register_renderer(Arc::new(native))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// JSON <-> Python conversion helpers
// ═══════════════════════════════════════════════════════════════════════════

fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::json!(i))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(serde_json::json!(f))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let items: PyResult<Vec<serde_json::Value>> =
            list.iter().map(|item| py_to_json(&item)).collect();
        Ok(serde_json::Value::Array(items?))
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key = k.extract::<String>()?;
            map.insert(key, py_to_json(&v)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        let s = obj.str()?.to_string();
        Ok(serde_json::Value::String(s))
    }
}

fn json_to_py<'py>(py: Python<'py>, value: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        serde_json::Value::Null => Ok(py.None().into_bound(py)),
        serde_json::Value::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.into_any())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.into_any())
            } else {
                Ok(py.None().into_bound(py))
            }
        }
        serde_json::Value::String(s) => Ok(PyString::new(py, s).into_any()),
        serde_json::Value::Array(arr) => {
            let items: PyResult<Vec<Bound<'py, PyAny>>> =
                arr.iter().map(|v| json_to_py(py, v)).collect();
            Ok(PyList::new(py, items?)?.into_any())
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

fn json_map_to_py<'py>(
    py: Python<'py>,
    map: &BTreeMap<String, serde_json::Value>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (k, v) in map {
        dict.set_item(k, json_to_py(py, v)?)?;
    }
    Ok(dict)
}

fn py_dict_to_json_map(dict: &Bound<'_, PyDict>) -> PyResult<BTreeMap<String, serde_json::Value>> {
    let mut map = BTreeMap::new();
    for (k, v) in dict.iter() {
        map.insert(k.extract::<String>()?, py_to_json(&v)?);
    }
    Ok(map)
}

// ═══════════════════════════════════════════════════════════════════════════
// PyDiffNode
// ═══════════════════════════════════════════════════════════════════════════

/// A node in the diff tree — the primary IR type.
///
/// A ``DiffNode`` records one change (or unchanged item) at one logical path.
/// Every comparator emits nodes; every transformer rewrites them. ``action``,
/// ``item_type``, and ``tags`` are open strings so plugins can introduce new
/// vocabulary without a core release.
///
/// Nodes are iterable and indexable over their children::
///
///     for child in node:
///         print(child.path, child.action)
///
///     first_child = node[0]
///     count = len(node)
#[pyclass(name = "DiffNode", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyDiffNode {
    inner: DiffNode,
}

#[pymethods]
impl PyDiffNode {
    /// Construct a ``DiffNode``.
    ///
    /// :param action: Open-string verb such as ``"add"``, ``"remove"``,
    ///     ``"modify"``, or a plugin-specific action.
    /// :param item_type: Open-string noun describing what kind of item this
    ///     is (``"file"``, ``"directory"``, ``"csv.row"``, ...).
    /// :param path: Logical path of the item within the snapshot.
    /// :param source_path: Optional prior logical path, for moves/renames.
    /// :param summary: Optional human-readable one-line summary.
    /// :param tags: Optional list or set of open-string tags (used for
    ///     renderer significance classification and transformer dispatch).
    /// :param details: Optional dict of structured JSON-serializable data.
    /// :param annotations: Optional dict of transient/presentation data.
    /// :param children: Optional list of child ``DiffNode`` s.
    #[new]
    #[pyo3(signature = (action, item_type, path, *, source_path=None, summary=None, tags=None, details=None, annotations=None, children=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        action: String,
        item_type: String,
        path: String,
        source_path: Option<String>,
        summary: Option<String>,
        tags: Option<Bound<'_, PyAny>>,
        details: Option<Bound<'_, PyDict>>,
        annotations: Option<Bound<'_, PyDict>>,
        children: Option<Vec<PyDiffNode>>,
    ) -> PyResult<Self> {
        let mut node = DiffNode::new(action, item_type, path);
        node.source_path = source_path;
        node.summary = summary;
        if let Some(tags_obj) = tags {
            if let Ok(tag_list) = tags_obj.extract::<Vec<String>>() {
                node.tags = tag_list.into_iter().collect();
            } else if let Ok(tag_set) = tags_obj.cast::<PySet>() {
                for item in tag_set.iter() {
                    node.tags.insert(item.extract::<String>()?);
                }
            } else {
                return Err(PyTypeError::new_err(
                    "tags must be a list or set of strings",
                ));
            }
        }
        if let Some(d) = details {
            node.details = py_dict_to_json_map(&d)?;
        }
        if let Some(a) = annotations {
            node.annotations = py_dict_to_json_map(&a)?;
        }
        if let Some(c) = children {
            node.children = c.into_iter().map(|n| n.inner).collect();
        }
        Ok(Self { inner: node })
    }

    /// Open-string verb describing what changed (``"add"``, ``"modify"``, ...).
    #[getter]
    fn action(&self) -> &str {
        &self.inner.action
    }
    /// Open-string noun describing the kind of item (``"file"``, ``"csv.row"``, ...).
    #[getter]
    fn item_type(&self) -> &str {
        &self.inner.item_type
    }
    /// Logical path of this item within its snapshot.
    #[getter]
    fn path(&self) -> &str {
        &self.inner.path
    }
    /// Prior logical path if this item was moved or renamed; ``None`` otherwise.
    #[getter]
    fn source_path(&self) -> Option<&str> {
        self.inner.source_path.as_deref()
    }
    /// Optional one-line human summary of the change.
    #[getter]
    fn summary(&self) -> Option<&str> {
        self.inner.summary.as_deref()
    }
    /// Open-string tags attached to this node (used for renderer significance
    /// classification and transformer dispatch).
    #[getter]
    fn tags(&self) -> Vec<String> {
        self.inner.tags.iter().cloned().collect()
    }
    /// Direct children of this node.
    #[getter]
    fn children(&self) -> Vec<PyDiffNode> {
        self.inner
            .children
            .iter()
            .map(|c| PyDiffNode { inner: c.clone() })
            .collect()
    }
    /// Structured JSON-serializable details describing the change.
    #[getter]
    fn details<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        json_map_to_py(py, &self.inner.details)
    }
    /// Transient/presentation annotations not part of the persisted IR.
    #[getter]
    fn annotations<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        json_map_to_py(py, &self.inner.annotations)
    }

    /// Total number of nodes in the subtree rooted at this node.
    fn node_count(&self) -> usize {
        self.inner.node_count()
    }
    /// Union of all tags on this node and its descendants.
    fn all_tags(&self) -> Vec<String> {
        self.inner.all_tags().into_iter().collect()
    }

    /// Serialize this node (recursively) to a plain Python ``dict``.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("action", &self.inner.action)?;
        dict.set_item("item_type", &self.inner.item_type)?;
        dict.set_item("path", &self.inner.path)?;
        dict.set_item("source_path", self.inner.source_path.as_deref())?;
        dict.set_item("summary", self.inner.summary.as_deref())?;
        dict.set_item("tags", self.tags())?;
        let children: PyResult<Vec<Bound<'py, PyDict>>> = self
            .inner
            .children
            .iter()
            .map(|c| PyDiffNode { inner: c.clone() }.to_dict(py))
            .collect();
        dict.set_item("children", PyList::new(py, children?)?)?;
        dict.set_item("details", json_map_to_py(py, &self.inner.details)?)?;
        dict.set_item("annotations", json_map_to_py(py, &self.inner.annotations)?)?;
        Ok(dict)
    }

    /// Serialize this node (recursively) to pretty-printed JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    /// Return a clone of this node with ``summary`` replaced.
    fn with_summary(&self, summary: String) -> Self {
        Self {
            inner: self.inner.clone().with_summary(summary),
        }
    }
    /// Return a clone of this node with ``tag`` added to ``tags``.
    fn with_tag(&self, tag: String) -> Self {
        Self {
            inner: self.inner.clone().with_tag(tag),
        }
    }
    /// Return a clone of this node with ``source_path`` replaced (used to
    /// record moves/renames).
    fn with_source_path(&self, source: String) -> Self {
        Self {
            inner: self.inner.clone().with_source_path(source),
        }
    }
    /// Return a clone of this node with its ``children`` replaced.
    fn with_children(&self, children: Vec<PyDiffNode>) -> Self {
        let children: Vec<DiffNode> = children.into_iter().map(|c| c.inner).collect();
        Self {
            inner: self.inner.clone().with_children(children),
        }
    }
    /// Return a clone of this node with ``details[key] = value`` set. ``value``
    /// must be JSON-serializable.
    fn with_detail(&self, key: String, value: Bound<'_, PyAny>) -> PyResult<Self> {
        let json_val = py_to_json(&value)?;
        Ok(Self {
            inner: self.inner.clone().with_detail(key, json_val),
        })
    }
    /// Recursively search this subtree for a node whose ``path`` matches
    /// ``selector``. Returns ``None`` if no match is found.
    fn find_node(&self, selector: &str) -> Option<PyDiffNode> {
        find_node_recursive(&self.inner, selector).map(|n| PyDiffNode { inner: n.clone() })
    }

    fn __repr__(&self) -> String {
        format!(
            "DiffNode(action={:?}, item_type={:?}, path={:?})",
            self.inner.action, self.inner.item_type, self.inner.path
        )
    }
    fn __str__(&self) -> String {
        format!(
            "{} {} at {}",
            self.inner.action, self.inner.item_type, self.inner.path
        )
    }
    fn __len__(&self) -> usize {
        self.inner.children.len()
    }
    fn __getitem__(&self, idx: isize) -> PyResult<PyDiffNode> {
        let len = self.inner.children.len() as isize;
        let actual = if idx < 0 { len + idx } else { idx };
        if actual < 0 || actual >= len {
            return Err(PyIndexError::new_err("index out of range"));
        }
        Ok(PyDiffNode {
            inner: self.inner.children[actual as usize].clone(),
        })
    }
    fn __iter__(&self) -> PyDiffNodeIter {
        PyDiffNodeIter {
            children: self.inner.children.clone(),
            index: 0,
        }
    }
    fn __bool__(&self) -> bool {
        true
    }
}

fn find_node_recursive<'a>(node: &'a DiffNode, selector: &str) -> Option<&'a DiffNode> {
    if node.path == selector {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_node_recursive(child, selector) {
            return Some(found);
        }
    }
    None
}

#[pyclass]
struct PyDiffNodeIter {
    children: Vec<DiffNode>,
    index: usize,
}

#[pymethods]
impl PyDiffNodeIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self) -> Option<PyDiffNode> {
        if self.index < self.children.len() {
            let node = &self.children[self.index];
            self.index += 1;
            Some(PyDiffNode {
                inner: node.clone(),
            })
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PyChangeset
// ═══════════════════════════════════════════════════════════════════════════

/// The result of :func:`diff` — a rooted diff tree plus metadata.
///
/// A ``Changeset`` records the two snapshot names it was computed from, the
/// root of the diff tree (``None`` if the snapshots are identical), and a
/// free-form ``metadata`` dict for plugin use. Serialize with
/// :meth:`to_json` / :meth:`to_dict`, or via the module-level :func:`to_json`
/// and :func:`to_markdown`.
#[pyclass(name = "Changeset", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyChangeset {
    inner: Changeset,
}

#[pymethods]
impl PyChangeset {
    /// Construct a ``Changeset``.
    ///
    /// :param from_snapshot: Name/identifier of the earlier snapshot.
    /// :param to_snapshot: Name/identifier of the later snapshot.
    /// :param root: Root of the diff tree, or ``None`` if the two snapshots
    ///     compare identical.
    #[new]
    #[pyo3(signature = (from_snapshot, to_snapshot, root=None))]
    fn new(from_snapshot: String, to_snapshot: String, root: Option<PyDiffNode>) -> Self {
        Self {
            inner: Changeset::new(from_snapshot, to_snapshot, root.map(|n| n.inner)),
        }
    }

    /// Name/identifier of the earlier snapshot this changeset was computed from.
    #[getter]
    #[allow(clippy::wrong_self_convention)]
    fn from_snapshot(&self) -> &str {
        &self.inner.from_snapshot
    }
    /// Name/identifier of the later snapshot this changeset was computed from.
    #[getter]
    fn to_snapshot(&self) -> &str {
        &self.inner.to_snapshot
    }
    /// Root of the diff tree, or ``None`` if the snapshots compare identical.
    #[getter]
    fn root(&self) -> Option<PyDiffNode> {
        self.inner
            .root
            .as_ref()
            .map(|r| PyDiffNode { inner: r.clone() })
    }
    /// Free-form metadata dict (plugin-populated).
    #[getter]
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.metadata {
            dict.set_item(k, v)?;
        }
        Ok(dict)
    }
    /// Total number of nodes in the diff tree (0 if :attr:`root` is ``None``).
    #[getter]
    fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Recursively search the diff tree for a node whose ``path`` matches
    /// ``selector``. Returns ``None`` if there is no root or no match.
    fn find_node(&self, selector: &str) -> Option<PyDiffNode> {
        self.inner.root.as_ref().and_then(|root| {
            find_node_recursive(root, selector).map(|n| PyDiffNode { inner: n.clone() })
        })
    }
    /// Serialize this changeset to canonical binoc changeset JSON.
    fn to_json(&self) -> PyResult<String> {
        output::to_json(&self.inner).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    /// Serialize this changeset to a plain Python ``dict``.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("from_snapshot", &self.inner.from_snapshot)?;
        dict.set_item("to_snapshot", &self.inner.to_snapshot)?;
        match &self.inner.root {
            Some(r) => {
                let root_dict = PyDiffNode { inner: r.clone() }.to_dict(py)?;
                dict.set_item("root", root_dict)?;
            }
            None => dict.set_item("root", py.None())?,
        }
        let meta = PyDict::new(py);
        for (k, v) in &self.inner.metadata {
            meta.set_item(k, v)?;
        }
        dict.set_item("metadata", meta)?;
        Ok(dict)
    }
    fn save(&self, path: &str) -> PyResult<()> {
        let json = self.to_json()?;
        std::fs::write(path, json).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json_str: &str) -> PyResult<Self> {
        let inner: Changeset =
            serde_json::from_str(json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }
    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        let data =
            std::fs::read_to_string(path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Self::from_json(&data)
    }

    fn __repr__(&self) -> String {
        format!(
            "Changeset(from={:?}, to={:?}, nodes={})",
            self.inner.from_snapshot,
            self.inner.to_snapshot,
            self.inner.node_count()
        )
    }
    fn __str__(&self) -> String {
        match self.inner.node_count() {
            0 => format!(
                "{} → {}: no changes",
                self.inner.from_snapshot, self.inner.to_snapshot
            ),
            n => format!(
                "{} → {}: {} change nodes",
                self.inner.from_snapshot, self.inner.to_snapshot, n
            ),
        }
    }
    fn __bool__(&self) -> bool {
        self.inner.root.is_some()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PyItemPair — Python-facing item pair (physical paths for Python plugins)
// ═══════════════════════════════════════════════════════════════════════════

#[pyclass(name = "ItemPair", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyItemPair {
    left_physical: Option<String>,
    right_physical: Option<String>,
    left_logical: Option<String>,
    right_logical: Option<String>,
}

impl PyItemPair {
    fn from_rust(pair: &ItemPair) -> Self {
        Self {
            left_physical: pair.left.as_ref().map(|i| i.handle.clone()),
            right_physical: pair.right.as_ref().map(|i| i.handle.clone()),
            left_logical: pair.left.as_ref().map(|i| i.logical_path.clone()),
            right_logical: pair.right.as_ref().map(|i| i.logical_path.clone()),
        }
    }

    fn to_rust(&self) -> ItemPair {
        let make_ref = |phys: &str, logical: &str| -> ItemRef {
            ItemRef {
                logical_path: logical.to_string(),
                is_dir: std::path::Path::new(phys).is_dir(),
                content_hash: None,
                size: None,
                media_type: None,
                handle: phys.to_string(),
            }
        };
        match (&self.left_physical, &self.right_physical) {
            (Some(l), Some(r)) => ItemPair::both(
                make_ref(l, self.left_logical.as_deref().unwrap_or("")),
                make_ref(r, self.right_logical.as_deref().unwrap_or("")),
            ),
            (None, Some(r)) => {
                ItemPair::added(make_ref(r, self.right_logical.as_deref().unwrap_or("")))
            }
            (Some(l), None) => {
                ItemPair::removed(make_ref(l, self.left_logical.as_deref().unwrap_or("")))
            }
            (None, None) => ItemPair::both(make_ref("", ""), make_ref("", "")),
        }
    }
}

#[pymethods]
impl PyItemPair {
    #[staticmethod]
    #[pyo3(signature = (left_path, right_path, left_logical="", right_logical=""))]
    fn both(
        left_path: String,
        right_path: String,
        left_logical: &str,
        right_logical: &str,
    ) -> Self {
        Self {
            left_physical: Some(left_path),
            right_physical: Some(right_path),
            left_logical: Some(left_logical.to_string()),
            right_logical: Some(right_logical.to_string()),
        }
    }
    #[staticmethod]
    #[pyo3(signature = (path, logical=""))]
    fn added(path: String, logical: &str) -> Self {
        Self {
            left_physical: None,
            right_physical: Some(path),
            left_logical: None,
            right_logical: Some(logical.to_string()),
        }
    }
    #[staticmethod]
    #[pyo3(signature = (path, logical=""))]
    fn removed(path: String, logical: &str) -> Self {
        Self {
            left_physical: Some(path),
            right_physical: None,
            left_logical: Some(logical.to_string()),
            right_logical: None,
        }
    }

    #[getter]
    fn left_path(&self) -> Option<&str> {
        self.left_physical.as_deref()
    }
    #[getter]
    fn right_path(&self) -> Option<&str> {
        self.right_physical.as_deref()
    }
    #[getter]
    fn logical_path(&self) -> &str {
        self.right_logical
            .as_deref()
            .or(self.left_logical.as_deref())
            .unwrap_or("")
    }
    #[getter]
    fn extension(&self) -> Option<String> {
        let path = self.logical_path();
        std::path::Path::new(path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
    }
    #[getter]
    fn is_dir(&self) -> bool {
        if let Some(p) = &self.right_physical {
            if std::path::Path::new(p).is_dir() {
                return true;
            }
        }
        if let Some(p) = &self.left_physical {
            if std::path::Path::new(p).is_dir() {
                return true;
            }
        }
        false
    }
    fn __repr__(&self) -> String {
        format!("ItemPair(logical_path={:?})", self.logical_path())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Compare/Transform result types for Python plugins
// ═══════════════════════════════════════════════════════════════════════════

/// Comparator result: the two items are semantically identical; no diff
/// node is produced.
#[pyclass(name = "Identical", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyIdentical;
#[pymethods]
impl PyIdentical {
    #[new]
    fn new() -> Self {
        Self
    }
    fn __repr__(&self) -> &str {
        "Identical()"
    }
}

/// Comparator result: produce this :class:`DiffNode` as a terminal leaf —
/// the controller will not recurse into its children.
#[pyclass(name = "Leaf", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyLeaf {
    /// The terminal diff node.
    #[pyo3(get)]
    node: PyDiffNode,
}
#[pymethods]
impl PyLeaf {
    #[new]
    fn new(node: PyDiffNode) -> Self {
        Self { node }
    }
    fn __repr__(&self) -> String {
        format!("Leaf({})", self.node.__repr__())
    }
}

/// Comparator result: produce this :class:`DiffNode` as a container, and
/// schedule the given children as additional item pairs for the controller
/// to dispatch.
#[pyclass(name = "Expand", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyExpand {
    /// The container diff node.
    #[pyo3(get)]
    node: PyDiffNode,
    /// Child item pairs to recurse into.
    #[pyo3(get)]
    children: Vec<PyItemPair>,
}
#[pymethods]
impl PyExpand {
    #[new]
    fn new(node: PyDiffNode, children: Vec<PyItemPair>) -> Self {
        Self { node, children }
    }
    fn __repr__(&self) -> String {
        format!(
            "Expand({}, {} children)",
            self.node.__repr__(),
            self.children.len()
        )
    }
}

/// Transformer result: do not rewrite this node.
#[pyclass(name = "Unchanged", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyUnchanged;
#[pymethods]
impl PyUnchanged {
    #[new]
    fn new() -> Self {
        Self
    }
    fn __repr__(&self) -> &str {
        "Unchanged()"
    }
}

/// Transformer result: replace the matched node with this single new node.
#[pyclass(name = "Replace", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyReplace {
    /// The replacement diff node.
    #[pyo3(get)]
    node: PyDiffNode,
}
#[pymethods]
impl PyReplace {
    #[new]
    fn new(node: PyDiffNode) -> Self {
        Self { node }
    }
    fn __repr__(&self) -> String {
        format!("Replace({})", self.node.__repr__())
    }
}

/// Transformer result: replace the matched node with zero or more new nodes.
#[pyclass(name = "ReplaceMany", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyReplaceMany {
    /// The replacement diff nodes.
    #[pyo3(get)]
    nodes: Vec<PyDiffNode>,
}
#[pymethods]
impl PyReplaceMany {
    #[new]
    fn new(nodes: Vec<PyDiffNode>) -> Self {
        Self { nodes }
    }
    fn __repr__(&self) -> String {
        format!("ReplaceMany({} nodes)", self.nodes.len())
    }
}

/// Transformer result: drop the matched node from the tree entirely.
#[pyclass(name = "Remove", module = "binoc._binoc", from_py_object)]
#[derive(Clone)]
pub struct PyRemove;
#[pymethods]
impl PyRemove {
    #[new]
    fn new() -> Self {
        Self
    }
    fn __repr__(&self) -> &str {
        "Remove()"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Python plugin bridges — wrap Python objects as Rust SDK trait objects
// ═══════════════════════════════════════════════════════════════════════════

struct PyComparatorBridge {
    py_obj: Py<PyAny>,
    desc: ComparatorDescriptor,
}

unsafe impl Send for PyComparatorBridge {}
unsafe impl Sync for PyComparatorBridge {}

impl Comparator for PyComparatorBridge {
    fn descriptor(&self) -> ComparatorDescriptor {
        self.desc.clone()
    }

    fn compare(&self, pair: &ItemPair, _data: &dyn DataAccess) -> BinocResult<CompareResult> {
        Python::attach(|py| {
            let py_pair = PyItemPair::from_rust(pair);
            let result = self
                .py_obj
                .call_method1(py, "compare", (py_pair,))
                .map_err(|e| BinocError::Comparator {
                    comparator: self.desc.name.clone(),
                    message: e.to_string(),
                })?;

            convert_py_compare_result(py, &result)
        })
    }
}

fn convert_py_compare_result(py: Python<'_>, obj: &Py<PyAny>) -> BinocResult<CompareResult> {
    let bound = obj.bind(py);
    if bound.is_instance_of::<PyIdentical>() {
        Ok(CompareResult::Identical)
    } else if let Ok(leaf) = bound.extract::<PyLeaf>() {
        Ok(CompareResult::Leaf(leaf.node.inner))
    } else if let Ok(expand) = bound.extract::<PyExpand>() {
        let children: Vec<ItemPair> = expand.children.iter().map(|c| c.to_rust()).collect();
        Ok(CompareResult::Expand(expand.node.inner, children))
    } else {
        let type_name = bound
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        Err(BinocError::Comparator {
            comparator: "python".into(),
            message: format!("compare() must return Identical, Leaf, or Expand, got {type_name}"),
        })
    }
}

struct PyTransformerBridge {
    py_obj: Py<PyAny>,
    desc: TransformerDescriptor,
}

unsafe impl Send for PyTransformerBridge {}
unsafe impl Sync for PyTransformerBridge {}

impl Transformer for PyTransformerBridge {
    fn descriptor(&self) -> TransformerDescriptor {
        self.desc.clone()
    }

    fn transform(
        &self,
        node: DiffNode,
        _data: &dyn DataAccess,
        _config: &serde_json::Value,
    ) -> TransformResult {
        Python::attach(|py| {
            let py_node = PyDiffNode {
                inner: node.clone(),
            };
            let result = match self.py_obj.call_method1(py, "transform", (py_node,)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Python transformer {} error: {}", self.desc.name, e);
                    return TransformResult::Unchanged;
                }
            };

            convert_py_transform_result(py, &result).unwrap_or(TransformResult::Unchanged)
        })
    }
}

fn convert_py_transform_result(py: Python<'_>, obj: &Py<PyAny>) -> Option<TransformResult> {
    let bound = obj.bind(py);
    if bound.is_instance_of::<PyUnchanged>() {
        Some(TransformResult::Unchanged)
    } else if let Ok(replace) = bound.extract::<PyReplace>() {
        Some(TransformResult::Replace(Box::new(replace.node.inner)))
    } else if let Ok(replace_many) = bound.extract::<PyReplaceMany>() {
        let nodes: Vec<DiffNode> = replace_many.nodes.into_iter().map(|n| n.inner).collect();
        Some(TransformResult::ReplaceMany(nodes))
    } else if bound.is_instance_of::<PyRemove>() {
        Some(TransformResult::Remove)
    } else {
        None
    }
}

fn create_comparator_bridge(
    _py: Python<'_>,
    obj: &Bound<'_, PyAny>,
) -> PyResult<PyComparatorBridge> {
    let name: String = obj
        .getattr("name")
        .and_then(|n| n.extract())
        .unwrap_or_else(|_| "python_comparator".to_string());
    let extensions: Vec<String> = obj
        .getattr("extensions")
        .and_then(|e| e.extract())
        .unwrap_or_default();
    let desc = ComparatorDescriptor::new(name).with_extensions(extensions);
    Ok(PyComparatorBridge {
        py_obj: obj.clone().unbind(),
        desc,
    })
}

fn create_transformer_bridge(
    _py: Python<'_>,
    obj: &Bound<'_, PyAny>,
) -> PyResult<PyTransformerBridge> {
    let name: String = obj
        .getattr("name")
        .and_then(|n| n.extract())
        .unwrap_or_else(|_| "python_transformer".to_string());
    let match_types: Vec<String> = obj
        .getattr("match_types")
        .and_then(|v| v.extract())
        .unwrap_or_default();
    let match_tags: Vec<String> = obj
        .getattr("match_tags")
        .and_then(|v| v.extract())
        .unwrap_or_default();
    let match_actions: Vec<String> = obj
        .getattr("match_actions")
        .and_then(|v| v.extract())
        .unwrap_or_default();
    let node_shape: NodeShapeFilter = obj
        .getattr("node_shape")
        .and_then(|v| v.extract::<String>())
        .ok()
        .and_then(|s| match s.as_str() {
            "container" => Some(NodeShapeFilter::Container),
            "leaf" => Some(NodeShapeFilter::Leaf),
            _ => None,
        })
        .unwrap_or_default();
    let desc = TransformerDescriptor::new(name)
        .with_match_types(match_types)
        .with_match_tags(match_tags)
        .with_match_actions(match_actions)
        .with_node_shape(node_shape);
    Ok(PyTransformerBridge {
        py_obj: obj.clone().unbind(),
        desc,
    })
}

struct PyRendererBridge {
    py_obj: Py<PyAny>,
    desc: RendererDescriptor,
}

unsafe impl Send for PyRendererBridge {}
unsafe impl Sync for PyRendererBridge {}

impl Renderer for PyRendererBridge {
    fn descriptor(&self) -> RendererDescriptor {
        self.desc.clone()
    }

    fn render(&self, changesets: &[Changeset], config: &serde_json::Value) -> BinocResult<String> {
        Python::attach(|py| {
            let py_changesets = PyList::new(
                py,
                changesets.iter().map(|m| {
                    PyChangeset { inner: m.clone() }
                        .into_pyobject(py)
                        .unwrap()
                        .into_any()
                }),
            )
            .map_err(|e| BinocError::Other(e.to_string()))?;

            let config_json =
                serde_json::to_string(config).map_err(|e| BinocError::Other(e.to_string()))?;
            let py_config = py
                .import("json")
                .and_then(|json_mod| json_mod.call_method1("loads", (config_json,)))
                .map_err(|e| BinocError::Other(e.to_string()))?;

            let result = self
                .py_obj
                .call_method1(py, "render", (py_changesets, py_config))
                .map_err(|e| BinocError::Other(format!("Python renderer error: {e}")))?;

            result
                .extract::<String>(py)
                .map_err(|e| BinocError::Other(format!("Python renderer must return str: {e}")))
        })
    }
}

fn create_renderer_bridge(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<PyRendererBridge> {
    let name: String = obj
        .getattr("name")
        .and_then(|n| n.extract())
        .unwrap_or_else(|_| "python_renderer".to_string());
    let file_extension: String = obj
        .getattr("file_extension")
        .and_then(|e| e.extract())
        .unwrap_or_else(|_| "txt".to_string());
    let desc = RendererDescriptor::new(name, file_extension);
    Ok(PyRendererBridge {
        py_obj: obj.clone().unbind(),
        desc,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// PyConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Dataset-level diff configuration.
///
/// A ``Config`` selects which registered comparators and transformers run for
/// a given dataset, and holds references to ad-hoc Python plugin instances
/// registered via :meth:`add_comparator` / :meth:`add_transformer` (i.e.
/// without packaging them as entry points).
#[pyclass(name = "Config", module = "binoc._binoc")]
pub struct PyConfig {
    dataset_config: DatasetConfig,
    extra_comparators: Vec<Py<PyAny>>,
    extra_transformers: Vec<Py<PyAny>>,
}

#[pymethods]
impl PyConfig {
    /// Return a fresh ``Config`` populated with the standard-library
    /// defaults (stdlib comparators and transformers, in their default
    /// order).
    #[staticmethod]
    fn default() -> Self {
        Self {
            dataset_config: DatasetConfig::default_config(),
            extra_comparators: Vec::new(),
            extra_transformers: Vec::new(),
        }
    }
    /// Load a dataset config from a TOML file on disk.
    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        let config = DatasetConfig::from_file(std::path::Path::new(path))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self {
            dataset_config: config,
            extra_comparators: Vec::new(),
            extra_transformers: Vec::new(),
        })
    }
    /// Construct a ``Config`` directly from explicit plugin-name lists.
    ///
    /// :param comparators: Names of registered comparators to run, in order.
    ///     If ``None``, the stdlib defaults are used.
    /// :param transformers: Names of registered transformers to run, in
    ///     order. If ``None``, the stdlib defaults are used.
    #[new]
    #[pyo3(signature = (*, comparators=None, transformers=None))]
    fn new(comparators: Option<Vec<String>>, transformers: Option<Vec<String>>) -> Self {
        let mut config = DatasetConfig::default_config();
        if let Some(c) = comparators {
            config.comparators = c;
        }
        if let Some(t) = transformers {
            config.transformers = t;
        }
        Self {
            dataset_config: config,
            extra_comparators: Vec::new(),
            extra_transformers: Vec::new(),
        }
    }
    /// Register an ad-hoc :class:`Comparator` instance with this config.
    ///
    /// Useful for quick scripts and tests where packaging the comparator as
    /// a distribution entry point would be overkill. The comparator is
    /// appended after any comparators resolved from the registry.
    fn add_comparator(&mut self, comparator: Bound<'_, PyAny>) -> PyResult<()> {
        self.extra_comparators.push(comparator.unbind());
        Ok(())
    }
    /// Register an ad-hoc :class:`Transformer` instance with this config.
    fn add_transformer(&mut self, transformer: Bound<'_, PyAny>) -> PyResult<()> {
        self.extra_transformers.push(transformer.unbind());
        Ok(())
    }
    /// Names of the comparators this config will run, in order.
    #[getter]
    fn comparators(&self) -> Vec<String> {
        self.dataset_config.comparators.clone()
    }
    /// Names of the transformers this config will run, in order.
    #[getter]
    fn transformers(&self) -> Vec<String> {
        self.dataset_config.transformers.clone()
    }
    fn __repr__(&self) -> String {
        format!(
            "Config(comparators={:?}, transformers={:?}, extra_comparators={}, extra_transformers={})",
            self.dataset_config.comparators,
            self.dataset_config.transformers,
            self.extra_comparators.len(),
            self.extra_transformers.len(),
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Top-level functions
// ═══════════════════════════════════════════════════════════════════════════

fn build_controller(
    py: Python<'_>,
    config: &PyConfig,
    registry: Option<&PyPluginRegistry>,
) -> PyResult<Controller> {
    let default_registry;
    let registry = match registry {
        Some(r) => &r.inner,
        None => {
            default_registry = binoc_stdlib::default_registry();
            &default_registry
        }
    };
    let resolved = registry
        .resolve(&config.dataset_config)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let mut comparators = resolved.comparators;
    let mut transformers = resolved.transformers;

    for py_comp in &config.extra_comparators {
        let bridge = create_comparator_bridge(py, py_comp.bind(py))?;
        comparators.push(Arc::new(bridge));
    }
    for py_trans in &config.extra_transformers {
        let bridge = create_transformer_bridge(py, py_trans.bind(py))?;
        transformers.push(Arc::new(bridge));
    }

    Ok(Controller::new(comparators, transformers))
}

/// Diff two snapshots and return the resulting :class:`Changeset`.
///
/// :param snapshot_a: Path to the earlier snapshot (file or directory).
/// :param snapshot_b: Path to the later snapshot (file or directory).
/// :param config: Optional :class:`Config` controlling which comparators and
///     transformers run. If ``None``, the stdlib defaults are used.
/// :param registry: Optional :class:`PluginRegistry` providing the set of
///     plugins available to resolve from ``config``. If ``None``, the
///     stdlib registry is used.
/// :returns: The resulting :class:`Changeset`.
#[pyfunction]
#[pyo3(signature = (snapshot_a, snapshot_b, *, config=None, registry=None))]
fn diff(
    py: Python<'_>,
    snapshot_a: &str,
    snapshot_b: &str,
    config: Option<&PyConfig>,
    registry: Option<&PyPluginRegistry>,
) -> PyResult<PyChangeset> {
    let default_config;
    let config = match config {
        Some(c) => c,
        None => {
            default_config = PyConfig {
                dataset_config: DatasetConfig::default_config(),
                extra_comparators: Vec::new(),
                extra_transformers: Vec::new(),
            };
            &default_config
        }
    };

    let controller = build_controller(py, config, registry)?;

    let changeset = py
        .detach(|| controller.diff(snapshot_a, snapshot_b))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    Ok(PyChangeset { inner: changeset })
}

/// Render a :class:`Changeset` as canonical binoc changeset JSON.
#[pyfunction]
fn to_json(changeset: &PyChangeset) -> PyResult<String> {
    changeset.to_json()
}

/// Extract the raw data for a single changed node, by logical path.
///
/// Given a :class:`Changeset` and a ``node_path`` selector, return the
/// requested ``aspect`` (by default ``"content"``) of that node's data as a
/// string. Snapshot paths default to the names recorded on the changeset
/// but can be overridden for relocated snapshots.
#[pyfunction]
#[pyo3(signature = (changeset, node_path, aspect="content", *, snapshot_a=None, snapshot_b=None, config=None))]
fn extract(
    py: Python<'_>,
    changeset: &PyChangeset,
    node_path: &str,
    aspect: &str,
    snapshot_a: Option<&str>,
    snapshot_b: Option<&str>,
    config: Option<&PyConfig>,
) -> PyResult<String> {
    let default_config;
    let config = match config {
        Some(c) => c,
        None => {
            default_config = PyConfig {
                dataset_config: DatasetConfig::default_config(),
                extra_comparators: Vec::new(),
                extra_transformers: Vec::new(),
            };
            &default_config
        }
    };

    let controller = build_controller(py, config, None)?;

    let snap_a = snapshot_a
        .map(|s| s.to_string())
        .unwrap_or_else(|| changeset.inner.from_snapshot.clone());
    let snap_b = snapshot_b
        .map(|s| s.to_string())
        .unwrap_or_else(|| changeset.inner.to_snapshot.clone());

    let result = py
        .detach(|| controller.extract(&changeset.inner, node_path, aspect, &snap_a, &snap_b))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    match result {
        ExtractResult::Text(text) => Ok(text),
        ExtractResult::Binary(bytes) => Ok(String::from_utf8_lossy(&bytes).to_string()),
    }
}

/// Render one or more changesets to Markdown using the stdlib renderer.
///
/// :param changesets: List of :class:`Changeset` s to render.
/// :param config: Optional :class:`Config`. The ``binoc.markdown`` section of
///     its output config controls per-renderer options (significance rules,
///     section layout, etc.).
/// :returns: The rendered Markdown string.
#[pyfunction]
#[pyo3(signature = (changesets, *, config=None))]
fn to_markdown(changesets: Vec<PyChangeset>, config: Option<&PyConfig>) -> String {
    let md_config: md_renderer::MarkdownRendererConfig = config
        .map(|c| {
            let val = c.dataset_config.output.get_for_renderer("binoc.markdown");
            serde_json::from_value(val).unwrap_or_default()
        })
        .unwrap_or_default();

    let rust_changesets: Vec<Changeset> = changesets.into_iter().map(|m| m.inner).collect();
    md_renderer::render_markdown(&rust_changesets, &md_config)
}

// ═══════════════════════════════════════════════════════════════════════════
// Plugin registry
// ═══════════════════════════════════════════════════════════════════════════

/// A mutable registry of comparator, transformer, and renderer plugins.
///
/// Test harnesses and plugin authors build a ``PluginRegistry``,
/// register plugin instances or load native ``.so`` plugins into it, and
/// pass it to :func:`diff` to control which plugins are available for
/// config resolution.
#[pyclass(name = "PluginRegistry", module = "binoc._binoc")]
pub struct PyPluginRegistry {
    pub inner: PluginRegistry,
}

#[pymethods]
impl PyPluginRegistry {
    /// Return a fresh registry preloaded with the standard-library plugins.
    #[staticmethod]
    fn default() -> Self {
        Self {
            inner: binoc_stdlib::default_registry(),
        }
    }
    /// Register a Python :class:`Comparator` instance with this registry.
    ///
    /// The comparator's own ``name`` attribute is used for dispatch; the
    /// ``_name`` argument is accepted for API symmetry and ignored.
    fn register_comparator(
        &mut self,
        py: Python<'_>,
        _name: String,
        obj: Py<PyAny>,
    ) -> PyResult<()> {
        let bridge = create_comparator_bridge(py, obj.bind(py))?;
        self.inner
            .register_comparator(Arc::new(bridge))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    /// Register a Python :class:`Transformer` instance with this registry.
    fn register_transformer(
        &mut self,
        py: Python<'_>,
        _name: String,
        obj: Py<PyAny>,
    ) -> PyResult<()> {
        let bridge = create_transformer_bridge(py, obj.bind(py))?;
        self.inner
            .register_transformer(Arc::new(bridge))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    /// Register a Python renderer instance with this registry.
    ///
    /// A Python renderer is any object with a ``name`` attribute, a
    /// ``file_extension`` attribute (defaults to ``"txt"``) and a
    /// ``render(changesets, config) -> str`` method.
    fn register_renderer(&mut self, py: Python<'_>, _name: String, obj: Py<PyAny>) -> PyResult<()> {
        let bridge = create_renderer_bridge(py, obj.bind(py))?;
        self.inner
            .register_renderer(Arc::new(bridge))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    /// Load a native Rust plugin from a shared library path.
    ///
    /// The library must expose the binoc plugin C ABI (``_binoc_plugin_describe``
    /// and related entry points). This is the same mechanism used by the
    /// entry-point-based plugin discovery in :mod:`binoc`.
    fn load_native_plugin(&mut self, module_path: String) -> PyResult<()> {
        load_native_plugin_into_registry(&module_path, &mut self.inner)
            .map_err(PyRuntimeError::new_err)
    }
    /// Return the names of all registered comparators.
    fn list_comparators(&self) -> Vec<String> {
        self.inner.comparator_names()
    }
    /// Return the names of all registered transformers.
    fn list_transformers(&self) -> Vec<String> {
        self.inner.transformer_names()
    }
    /// Return the names of all registered renderers.
    fn list_renderers(&self) -> Vec<String> {
        self.inner.renderer_names()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI entry point
// ═══════════════════════════════════════════════════════════════════════════

#[pyfunction]
fn run_cli(registry: &mut PyPluginRegistry, args: Vec<String>) -> PyResult<()> {
    let inner = std::mem::take(&mut registry.inner);
    binoc_cli::run(inner, args).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Module
// ═══════════════════════════════════════════════════════════════════════════

#[pymodule]
fn _binoc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDiffNode>()?;
    m.add_class::<PyChangeset>()?;
    m.add_class::<PyItemPair>()?;
    m.add_class::<PyConfig>()?;
    m.add_class::<PyPluginRegistry>()?;
    m.add_class::<PyIdentical>()?;
    m.add_class::<PyLeaf>()?;
    m.add_class::<PyExpand>()?;
    m.add_class::<PyUnchanged>()?;
    m.add_class::<PyReplace>()?;
    m.add_class::<PyReplaceMany>()?;
    m.add_class::<PyRemove>()?;
    m.add_function(wrap_pyfunction!(diff, m)?)?;
    m.add_function(wrap_pyfunction!(to_json, m)?)?;
    m.add_function(wrap_pyfunction!(to_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(run_cli, m)?)?;
    Ok(())
}
