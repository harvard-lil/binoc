use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString};

use binoc_core::config::{DatasetConfig, PluginRegistry};
use binoc_core::controller::Controller;
use binoc_core::ir;
use binoc_core::output;
use binoc_core::traits::{self, BinocError, BinocResult, CompareContext};
use binoc_core::types::{
    CompareResult, ExtractResult, Item, ItemPair, ReopenedData, TransformResult,
};

use binoc_stdlib::outputters::markdown as md_outputter;

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
// PyDiffNode — wraps binoc_core::ir::DiffNode
// ═══════════════════════════════════════════════════════════════════════════

#[pyclass(name = "DiffNode")]
#[derive(Clone)]
pub struct PyDiffNode {
    inner: ir::DiffNode,
}

#[pymethods]
impl PyDiffNode {
    #[new]
    #[pyo3(signature = (kind, item_type, path, *, source_path=None, summary=None, tags=None, details=None, annotations=None, children=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        kind: String,
        item_type: String,
        path: String,
        source_path: Option<String>,
        summary: Option<String>,
        tags: Option<Bound<'_, PyAny>>,
        details: Option<Bound<'_, PyDict>>,
        annotations: Option<Bound<'_, PyDict>>,
        children: Option<Vec<PyDiffNode>>,
    ) -> PyResult<Self> {
        let mut node = ir::DiffNode::new(kind, item_type, path);
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

    #[getter]
    fn kind(&self) -> &str {
        &self.inner.kind
    }

    #[getter]
    fn item_type(&self) -> &str {
        &self.inner.item_type
    }

    #[getter]
    fn path(&self) -> &str {
        &self.inner.path
    }

    #[getter]
    fn source_path(&self) -> Option<&str> {
        self.inner.source_path.as_deref()
    }

    #[getter]
    fn summary(&self) -> Option<&str> {
        self.inner.summary.as_deref()
    }

    #[getter]
    fn tags(&self) -> Vec<String> {
        self.inner.tags.iter().cloned().collect()
    }

    #[getter]
    fn children(&self) -> Vec<PyDiffNode> {
        self.inner
            .children
            .iter()
            .map(|c| PyDiffNode { inner: c.clone() })
            .collect()
    }

    #[getter]
    fn details<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        json_map_to_py(py, &self.inner.details)
    }

    #[getter]
    fn annotations<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        json_map_to_py(py, &self.inner.annotations)
    }

    fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    fn all_tags(&self) -> Vec<String> {
        self.inner.all_tags().into_iter().collect()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("kind", &self.inner.kind)?;
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

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn with_summary(&self, summary: String) -> Self {
        Self {
            inner: self.inner.clone().with_summary(summary),
        }
    }

    fn with_tag(&self, tag: String) -> Self {
        Self {
            inner: self.inner.clone().with_tag(tag),
        }
    }

    fn with_source_path(&self, source: String) -> Self {
        Self {
            inner: self.inner.clone().with_source_path(source),
        }
    }

    fn with_children(&self, children: Vec<PyDiffNode>) -> Self {
        let children: Vec<ir::DiffNode> = children.into_iter().map(|c| c.inner).collect();
        Self {
            inner: self.inner.clone().with_children(children),
        }
    }

    fn with_detail(&self, key: String, value: Bound<'_, PyAny>) -> PyResult<Self> {
        let json_val = py_to_json(&value)?;
        Ok(Self {
            inner: self.inner.clone().with_detail(key, json_val),
        })
    }

    fn find_node(&self, selector: &str) -> Option<PyDiffNode> {
        find_node_recursive(&self.inner, selector).map(|n| PyDiffNode { inner: n.clone() })
    }

    fn __repr__(&self) -> String {
        format!(
            "DiffNode(kind={:?}, item_type={:?}, path={:?})",
            self.inner.kind, self.inner.item_type, self.inner.path
        )
    }

    fn __str__(&self) -> String {
        format!(
            "{} {} at {}",
            self.inner.kind, self.inner.item_type, self.inner.path
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

fn find_node_recursive<'a>(node: &'a ir::DiffNode, selector: &str) -> Option<&'a ir::DiffNode> {
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
    children: Vec<ir::DiffNode>,
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
// PyMigration — wraps binoc_core::ir::Migration
// ═══════════════════════════════════════════════════════════════════════════

#[pyclass(name = "Migration")]
#[derive(Clone)]
pub struct PyMigration {
    inner: ir::Migration,
}

#[pymethods]
impl PyMigration {
    #[new]
    #[pyo3(signature = (from_snapshot, to_snapshot, root=None))]
    fn new(from_snapshot: String, to_snapshot: String, root: Option<PyDiffNode>) -> Self {
        Self {
            inner: ir::Migration::new(from_snapshot, to_snapshot, root.map(|n| n.inner)),
        }
    }

    #[getter]
    #[allow(clippy::wrong_self_convention)]
    fn from_snapshot(&self) -> &str {
        &self.inner.from_snapshot
    }

    #[getter]
    fn to_snapshot(&self) -> &str {
        &self.inner.to_snapshot
    }

    #[getter]
    fn root(&self) -> Option<PyDiffNode> {
        self.inner
            .root
            .as_ref()
            .map(|r| PyDiffNode { inner: r.clone() })
    }

    #[getter]
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.metadata {
            dict.set_item(k, v)?;
        }
        Ok(dict)
    }

    #[getter]
    fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    fn find_node(&self, selector: &str) -> Option<PyDiffNode> {
        self.inner.root.as_ref().and_then(|root| {
            find_node_recursive(root, selector).map(|n| PyDiffNode { inner: n.clone() })
        })
    }

    fn to_json(&self) -> PyResult<String> {
        output::to_json(&self.inner).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

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
        let inner: ir::Migration =
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
            "Migration(from={:?}, to={:?}, nodes={})",
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
// PyItemPair — for Python plugin comparators
// ═══════════════════════════════════════════════════════════════════════════

#[pyclass(name = "ItemPair")]
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
            left_physical: pair
                .left
                .as_ref()
                .map(|i| i.physical_path.to_string_lossy().to_string()),
            right_physical: pair
                .right
                .as_ref()
                .map(|i| i.physical_path.to_string_lossy().to_string()),
            left_logical: pair.left.as_ref().map(|i| i.logical_path.clone()),
            right_logical: pair.right.as_ref().map(|i| i.logical_path.clone()),
        }
    }

    fn to_rust(&self) -> ItemPair {
        match (&self.left_physical, &self.right_physical) {
            (Some(l), Some(r)) => ItemPair::both(
                Item::new(l.as_str(), self.left_logical.as_deref().unwrap_or("")),
                Item::new(r.as_str(), self.right_logical.as_deref().unwrap_or("")),
            ),
            (None, Some(r)) => ItemPair::added(Item::new(
                r.as_str(),
                self.right_logical.as_deref().unwrap_or(""),
            )),
            (Some(l), None) => ItemPair::removed(Item::new(
                l.as_str(),
                self.left_logical.as_deref().unwrap_or(""),
            )),
            (None, None) => ItemPair::both(Item::new("", ""), Item::new("", "")),
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

#[pyclass(name = "Identical")]
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

#[pyclass(name = "Leaf")]
#[derive(Clone)]
pub struct PyLeaf {
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

#[pyclass(name = "Expand")]
#[derive(Clone)]
pub struct PyExpand {
    #[pyo3(get)]
    node: PyDiffNode,
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

#[pyclass(name = "Unchanged")]
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

#[pyclass(name = "Replace")]
#[derive(Clone)]
pub struct PyReplace {
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

#[pyclass(name = "ReplaceMany")]
#[derive(Clone)]
pub struct PyReplaceMany {
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

#[pyclass(name = "Remove")]
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
// Python plugin bridges — wrap Python objects as Rust trait objects
// ═══════════════════════════════════════════════════════════════════════════

struct PyComparatorBridge {
    py_obj: Py<PyAny>,
    name: String,
    extensions: Vec<String>,
}

// Safety: PyComparatorBridge may be moved or shared across threads (rayon workers) because:
// - Every use of `py_obj` is inside `Python::attach(|py| { ... })`, which acquires the GIL
//   for the duration of the closure. No thread touches the Python object without the GIL.
// - The other fields (`name`, `extensions`) are immutable and only read; no shared mutable state.
unsafe impl Send for PyComparatorBridge {}
unsafe impl Sync for PyComparatorBridge {}

impl traits::Comparator for PyComparatorBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn can_handle(&self, pair: &ItemPair) -> bool {
        if !self.extensions.is_empty() && !pair.is_dir() {
            if let Some(ext) = pair.extension() {
                if self.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext)) {
                    return true;
                }
            }
        }
        Python::attach(|py| {
            let py_pair = PyItemPair::from_rust(pair);
            self.py_obj
                .call_method1(py, "can_handle", (py_pair,))
                .and_then(|r| r.extract::<bool>(py))
                .unwrap_or(false)
        })
    }

    fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
        Python::attach(|py| {
            let py_pair = PyItemPair::from_rust(pair);
            let result = self
                .py_obj
                .call_method1(py, "compare", (py_pair,))
                .map_err(|e| BinocError::Comparator {
                    comparator: self.name.clone(),
                    message: e.to_string(),
                })?;

            convert_py_compare_result(py, &result)
        })
    }

    fn extract(
        &self,
        _data: &ReopenedData,
        _diff: &ir::DiffNode,
        _selector: &str,
    ) -> Option<ExtractResult> {
        None
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
            message: format!("compare() must return Identical, Leaf, or Expand, got {type_name}",),
        })
    }
}

struct PyTransformerBridge {
    py_obj: Py<PyAny>,
    name: String,
    match_types_val: Vec<String>,
    match_tags_val: Vec<String>,
    match_kinds_val: Vec<String>,
}

// Safety: PyTransformerBridge may be moved or shared across threads (controller/transform phase)
// because:
// - Every use of `py_obj` is inside `Python::attach(|py| { ... })`, which acquires the GIL
//   for the duration of the closure. No thread touches the Python object without the GIL.
// - The other fields (`name`, `match_*_val`) are immutable and only read; no shared mutable state.
unsafe impl Send for PyTransformerBridge {}
unsafe impl Sync for PyTransformerBridge {}

impl traits::Transformer for PyTransformerBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn can_handle(&self, node: &ir::DiffNode) -> bool {
        if !self.match_types_val.is_empty()
            && self.match_types_val.iter().any(|t| t == &node.item_type)
        {
            return true;
        }
        if !self.match_tags_val.is_empty()
            && self.match_tags_val.iter().any(|t| node.tags.contains(t))
        {
            return true;
        }
        if !self.match_kinds_val.is_empty() && self.match_kinds_val.iter().any(|k| k == &node.kind)
        {
            return true;
        }
        Python::attach(|py| {
            let py_node = PyDiffNode {
                inner: node.clone(),
            };
            self.py_obj
                .call_method1(py, "can_handle", (py_node,))
                .and_then(|r| r.extract::<bool>(py))
                .unwrap_or(false)
        })
    }

    fn transform(&self, node: ir::DiffNode, _ctx: &CompareContext) -> TransformResult {
        Python::attach(|py| {
            let py_node = PyDiffNode {
                inner: node.clone(),
            };
            let result = match self.py_obj.call_method1(py, "transform", (py_node,)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Python transformer {} error: {}", self.name, e);
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
        Some(TransformResult::Replace(replace.node.inner))
    } else if let Ok(replace_many) = bound.extract::<PyReplaceMany>() {
        let nodes: Vec<ir::DiffNode> = replace_many.nodes.into_iter().map(|n| n.inner).collect();
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
    Ok(PyComparatorBridge {
        py_obj: obj.clone().unbind(),
        name,
        extensions,
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
    let match_kinds: Vec<String> = obj
        .getattr("match_kinds")
        .and_then(|v| v.extract())
        .unwrap_or_default();
    Ok(PyTransformerBridge {
        py_obj: obj.clone().unbind(),
        name,
        match_types_val: match_types,
        match_tags_val: match_tags,
        match_kinds_val: match_kinds,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// PyConfig — dataset configuration
// ═══════════════════════════════════════════════════════════════════════════

#[pyclass(name = "Config")]
pub struct PyConfig {
    dataset_config: DatasetConfig,
    extra_comparators: Vec<Py<PyAny>>,
    extra_transformers: Vec<Py<PyAny>>,
}

#[pymethods]
impl PyConfig {
    #[staticmethod]
    fn default() -> Self {
        Self {
            dataset_config: DatasetConfig::default_config(),
            extra_comparators: Vec::new(),
            extra_transformers: Vec::new(),
        }
    }

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

    fn add_comparator(&mut self, comparator: Bound<'_, PyAny>) -> PyResult<()> {
        self.extra_comparators.push(comparator.unbind());
        Ok(())
    }

    fn add_transformer(&mut self, transformer: Bound<'_, PyAny>) -> PyResult<()> {
        self.extra_transformers.push(transformer.unbind());
        Ok(())
    }

    #[getter]
    fn comparators(&self) -> Vec<String> {
        self.dataset_config.comparators.clone()
    }

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

fn build_controller(py: Python<'_>, config: &PyConfig) -> PyResult<Controller> {
    let registry = binoc_stdlib::default_registry();
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

#[pyfunction]
#[pyo3(signature = (snapshot_a, snapshot_b, *, config=None))]
fn diff(
    py: Python<'_>,
    snapshot_a: &str,
    snapshot_b: &str,
    config: Option<&PyConfig>,
) -> PyResult<PyMigration> {
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

    let controller = build_controller(py, config)?;

    let migration = py
        .detach(|| controller.diff(snapshot_a, snapshot_b))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    Ok(PyMigration { inner: migration })
}

#[pyfunction]
fn to_json(migration: &PyMigration) -> PyResult<String> {
    migration.to_json()
}

#[pyfunction]
#[pyo3(signature = (migration, node_path, aspect="content", *, snapshot_a=None, snapshot_b=None, config=None))]
fn extract(
    py: Python<'_>,
    migration: &PyMigration,
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

    let controller = build_controller(py, config)?;

    let snap_a = snapshot_a
        .map(|s| s.to_string())
        .unwrap_or_else(|| migration.inner.from_snapshot.clone());
    let snap_b = snapshot_b
        .map(|s| s.to_string())
        .unwrap_or_else(|| migration.inner.to_snapshot.clone());

    let result = py
        .detach(|| controller.extract(&migration.inner, node_path, aspect, &snap_a, &snap_b))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    match result {
        ExtractResult::Text(text) => Ok(text),
        ExtractResult::Binary(bytes) => Ok(String::from_utf8_lossy(&bytes).to_string()),
    }
}

#[pyfunction]
#[pyo3(signature = (migrations, *, config=None))]
fn to_markdown(migrations: Vec<PyMigration>, config: Option<&PyConfig>) -> String {
    let md_config: md_outputter::MarkdownOutputterConfig = config
        .map(|c| {
            let val = c.dataset_config.output.get_for_outputter("binoc.markdown");
            serde_json::from_value(val).unwrap_or_default()
        })
        .unwrap_or_default();

    let rust_migrations: Vec<ir::Migration> = migrations.into_iter().map(|m| m.inner).collect();
    md_outputter::render_markdown(&rust_migrations, &md_config)
}

// ═══════════════════════════════════════════════════════════════════════════
// Plugin registry — exposes PluginRegistry to Python for entry-point
// discovery. Rust plugins register native trait objects (no per-file
// Python bridge); Python plugins go through PyComparatorBridge as before.
// ═══════════════════════════════════════════════════════════════════════════

#[pyclass(name = "PluginRegistry")]
pub struct PyPluginRegistry {
    pub inner: PluginRegistry,
}

#[pymethods]
impl PyPluginRegistry {
    /// Create a registry pre-populated with the standard library plugins.
    #[staticmethod]
    fn default() -> Self {
        Self {
            inner: binoc_stdlib::default_registry(),
        }
    }

    /// Register a Python comparator into the registry under `name`.
    fn register_comparator(
        &mut self,
        py: Python<'_>,
        name: String,
        obj: Py<PyAny>,
    ) -> PyResult<()> {
        let bridge = create_comparator_bridge(py, obj.bind(py))?;
        self.inner.register_comparator(name, Arc::new(bridge));
        Ok(())
    }

    /// Register a Python transformer into the registry under `name`.
    fn register_transformer(
        &mut self,
        py: Python<'_>,
        name: String,
        obj: Py<PyAny>,
    ) -> PyResult<()> {
        let bridge = create_transformer_bridge(py, obj.bind(py))?;
        self.inner.register_transformer(name, Arc::new(bridge));
        Ok(())
    }

    /// List all registered comparator names.
    fn list_comparators(&self) -> Vec<String> {
        self.inner.comparator_names()
    }

    /// List all registered transformer names.
    fn list_transformers(&self) -> Vec<String> {
        self.inner.transformer_names()
    }

    /// List all registered outputter names.
    fn list_outputters(&self) -> Vec<String> {
        self.inner.outputter_names()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI entry point — delegates to the Rust CLI with a pre-populated registry
// ═══════════════════════════════════════════════════════════════════════════

#[pyfunction]
fn run_cli(registry: &mut PyPluginRegistry, args: Vec<String>) -> PyResult<()> {
    let inner = std::mem::take(&mut registry.inner);
    binoc_cli::run(inner, args).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Module definition
// ═══════════════════════════════════════════════════════════════════════════

#[pymodule]
fn _binoc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Core types
    m.add_class::<PyDiffNode>()?;
    m.add_class::<PyMigration>()?;
    m.add_class::<PyItemPair>()?;
    m.add_class::<PyConfig>()?;
    m.add_class::<PyPluginRegistry>()?;

    // Compare result types
    m.add_class::<PyIdentical>()?;
    m.add_class::<PyLeaf>()?;
    m.add_class::<PyExpand>()?;

    // Transform result types
    m.add_class::<PyUnchanged>()?;
    m.add_class::<PyReplace>()?;
    m.add_class::<PyReplaceMany>()?;
    m.add_class::<PyRemove>()?;

    // Top-level functions
    m.add_function(wrap_pyfunction!(diff, m)?)?;
    m.add_function(wrap_pyfunction!(to_json, m)?)?;
    m.add_function(wrap_pyfunction!(to_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(run_cli, m)?)?;

    Ok(())
}
