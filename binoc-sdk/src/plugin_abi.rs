//! C-ABI stable protocol for native plugins.
//!
//! Plugins compiled as separate cdylibs expose a set of `#[no_mangle] extern "C"`
//! functions. The host loads them via `libloading` and calls them with JSON-serialized
//! requests/responses, avoiding any Rust ABI compatibility requirements.
//!
//! Plugin authors never use this module directly — the [`export_plugin!`] macro
//! generates the entry points, and the host's native plugin loader consumes them.

use serde::{Deserialize, Serialize};

use crate::ir::DiffNode;
use crate::traits::{ComparatorDescriptor, RendererDescriptor, TransformerDescriptor};
use crate::types::{CompareResult, ItemPair, TransformResult};

// ── Plugin description ─────────────────────────────────────────────

/// Top-level plugin description returned by `_binoc_plugin_describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescription {
    pub sdk_version: String,
    #[serde(default)]
    pub comparators: Vec<ComparatorDescriptor>,
    #[serde(default)]
    pub transformers: Vec<TransformerDescriptor>,
    #[serde(default)]
    pub renderers: Vec<RendererDescriptor>,
}

// ── Comparator wire types ──────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CompareRequest {
    pub pair: ItemPair,
    pub data_root: String,
    pub workspace: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum CompareResponse {
    #[serde(rename = "ok")]
    Ok { result: Box<CompareResult> },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReopenRequest {
    pub pair: ItemPair,
    pub child_path: String,
    pub data_root: String,
    pub workspace: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ReopenResponse {
    #[serde(rename = "ok")]
    Ok { pair: ItemPair },
    #[serde(rename = "error")]
    Error { message: String },
}

// ── Transformer wire types ─────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct TransformRequest {
    pub node: DiffNode,
    pub data_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_items: Option<ItemPair>,
}

/// Serializable version of TransformResult for the C ABI.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum TransformResponse {
    #[serde(rename = "unchanged")]
    Unchanged,
    #[serde(rename = "replace")]
    Replace { node: Box<DiffNode> },
    #[serde(rename = "replace_many")]
    ReplaceMany { nodes: Vec<DiffNode> },
    #[serde(rename = "remove")]
    Remove,
    #[serde(rename = "error")]
    Error { message: String },
}

impl TransformResponse {
    pub fn into_result(self) -> Result<TransformResult, String> {
        match self {
            Self::Unchanged => Ok(TransformResult::Unchanged),
            Self::Replace { node } => Ok(TransformResult::Replace(node)),
            Self::ReplaceMany { nodes } => Ok(TransformResult::ReplaceMany(nodes)),
            Self::Remove => Ok(TransformResult::Remove),
            Self::Error { message } => Err(message),
        }
    }
}

// ── Renderer wire types ───────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderRequest {
    pub migrations: Vec<crate::ir::Migration>,
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum RenderResponse {
    #[serde(rename = "ok")]
    Ok { output: String },
    #[serde(rename = "error")]
    Error { message: String },
}

// ── Extract wire types ─────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractRequest {
    pub node: DiffNode,
    pub aspect: String,
    pub data_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_items: Option<ItemPair>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ExtractResponse {
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "binary")]
    Binary { content: Vec<u8> },
    #[serde(rename = "none")]
    None,
    #[serde(rename = "error")]
    Error { message: String },
}

// ── export_plugin! macro ───────────────────────────────────────────

/// Export a plugin pack with any combination of comparators, transformers,
/// and renderers.
///
/// Generates C ABI entry points conditionally based on declared types:
/// - `_binoc_plugin_describe` (always)
/// - `_binoc_free_string` (always)
/// - `_binoc_comparator_compare`, `_binoc_comparator_reopen`,
///   `_binoc_comparator_extract` (if comparators declared)
/// - `_binoc_transformer_transform`, `_binoc_transformer_extract`
///   (if transformers declared)
/// - `_binoc_renderer_render` (if renderers declared)
/// - Empty `#[pymodule]` when `python` feature active
///
/// # Example
///
/// ```ignore
/// export_plugin! {
///     module: my_plugin,
///     comparators: [MyComparator],
///     transformers: [MyTransformer],
/// }
/// ```
#[macro_export]
macro_rules! export_plugin {
    // Internal: collect comparator descriptors
    (@comp_descs $($comp:ty),*) => {{
        let mut descs = Vec::new();
        $(
            descs.push($crate::Comparator::descriptor(
                &<$comp as ::std::default::Default>::default(),
            ));
        )*
        descs
    }};

    // Internal: collect transformer descriptors
    (@trans_descs $($trans:ty),*) => {{
        let mut descs = Vec::new();
        $(
            descs.push($crate::Transformer::descriptor(
                &<$trans as ::std::default::Default>::default(),
            ));
        )*
        descs
    }};

    // Internal: collect renderer descriptors
    (@out_descs $($out:ty),*) => {{
        let mut descs = Vec::new();
        $(
            descs.push($crate::Renderer::descriptor(
                &<$out as ::std::default::Default>::default(),
            ));
        )*
        descs
    }};

    // Internal: comparator entry points
    (@comparator_fns $($comp:ty),+) => {
        #[no_mangle]
        pub unsafe extern "C" fn _binoc_comparator_compare(
            index: u32,
            request: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            let response = ::std::panic::catch_unwind(|| {
                let request_str = ::std::ffi::CStr::from_ptr(request)
                    .to_str()
                    .expect("binoc SDK: valid UTF-8 request");
                let req: $crate::plugin_abi::CompareRequest =
                    $crate::_reexport::serde_json::from_str(request_str)
                        .expect("binoc SDK: deserialize CompareRequest");
                let data = $crate::LocalDataAccess::for_plugin(
                    ::std::path::PathBuf::from(&req.data_root),
                    ::std::path::PathBuf::from(&req.workspace),
                );
                let comparators: Vec<Box<dyn $crate::Comparator>> =
                    vec![$(Box::new(<$comp as ::std::default::Default>::default())),+];
                let comp = &comparators[index as usize];
                match $crate::Comparator::compare(comp.as_ref(), &req.pair, &data) {
                    Ok(result) => $crate::plugin_abi::CompareResponse::Ok {
                        result: Box::new(result),
                    },
                    Err(e) => $crate::plugin_abi::CompareResponse::Error {
                        message: e.to_string(),
                    },
                }
            });
            let response = match response {
                Ok(r) => r,
                Err(_) => $crate::plugin_abi::CompareResponse::Error {
                    message: "plugin panicked".to_string(),
                },
            };
            let json = $crate::_reexport::serde_json::to_string(&response)
                .expect("binoc SDK: serialize compare response");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_comparator_reopen(
            index: u32,
            request: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            let response = ::std::panic::catch_unwind(|| {
                let request_str = ::std::ffi::CStr::from_ptr(request)
                    .to_str()
                    .expect("binoc SDK: valid UTF-8 request");
                let req: $crate::plugin_abi::ReopenRequest =
                    $crate::_reexport::serde_json::from_str(request_str)
                        .expect("binoc SDK: deserialize ReopenRequest");
                let data = $crate::LocalDataAccess::for_plugin(
                    ::std::path::PathBuf::from(&req.data_root),
                    ::std::path::PathBuf::from(&req.workspace),
                );
                let comparators: Vec<Box<dyn $crate::Comparator>> =
                    vec![$(Box::new(<$comp as ::std::default::Default>::default())),+];
                let comp = &comparators[index as usize];
                match $crate::Comparator::reopen(comp.as_ref(), &req.pair, &req.child_path, &data) {
                    Ok(pair) => $crate::plugin_abi::ReopenResponse::Ok { pair },
                    Err(e) => $crate::plugin_abi::ReopenResponse::Error {
                        message: e.to_string(),
                    },
                }
            });
            let response = match response {
                Ok(r) => r,
                Err(_) => $crate::plugin_abi::ReopenResponse::Error {
                    message: "plugin panicked".to_string(),
                },
            };
            let json = $crate::_reexport::serde_json::to_string(&response)
                .expect("binoc SDK: serialize reopen response");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_comparator_extract(
            index: u32,
            request: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            let response = ::std::panic::catch_unwind(|| {
                let request_str = ::std::ffi::CStr::from_ptr(request)
                    .to_str()
                    .expect("binoc SDK: valid UTF-8 request");
                let req: $crate::plugin_abi::ExtractRequest =
                    $crate::_reexport::serde_json::from_str(request_str)
                        .expect("binoc SDK: deserialize ExtractRequest");
                let data = $crate::LocalDataAccess::with_data_root(
                    ::std::path::PathBuf::from(&req.data_root),
                );
                let mut node = req.node;
                node.source_items = req.source_items;
                let comparators: Vec<Box<dyn $crate::Comparator>> =
                    vec![$(Box::new(<$comp as ::std::default::Default>::default())),+];
                let comp = &comparators[index as usize];
                match $crate::Comparator::extract(comp.as_ref(), &node, &req.aspect, &data) {
                    Some($crate::ExtractResult::Text(t)) => {
                        $crate::plugin_abi::ExtractResponse::Text { content: t }
                    }
                    Some($crate::ExtractResult::Binary(b)) => {
                        $crate::plugin_abi::ExtractResponse::Binary { content: b }
                    }
                    None => $crate::plugin_abi::ExtractResponse::None,
                }
            });
            let response = match response {
                Ok(r) => r,
                Err(_) => $crate::plugin_abi::ExtractResponse::Error {
                    message: "plugin panicked".to_string(),
                },
            };
            let json = $crate::_reexport::serde_json::to_string(&response)
                .expect("binoc SDK: serialize extract response");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }
    };

    // Internal: transformer entry points
    (@transformer_fns $($trans:ty),+) => {
        #[no_mangle]
        pub unsafe extern "C" fn _binoc_transformer_transform(
            index: u32,
            request: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            let response = ::std::panic::catch_unwind(|| {
                let request_str = ::std::ffi::CStr::from_ptr(request)
                    .to_str()
                    .expect("binoc SDK: valid UTF-8 request");
                let req: $crate::plugin_abi::TransformRequest =
                    $crate::_reexport::serde_json::from_str(request_str)
                        .expect("binoc SDK: deserialize TransformRequest");
                let data = $crate::LocalDataAccess::with_data_root(
                    ::std::path::PathBuf::from(&req.data_root),
                );
                let mut node = req.node;
                node.source_items = req.source_items;
                let transformers: Vec<Box<dyn $crate::Transformer>> =
                    vec![$(Box::new(<$trans as ::std::default::Default>::default())),+];
                let trans = &transformers[index as usize];
                match $crate::Transformer::transform(trans.as_ref(), node, &data) {
                    $crate::TransformResult::Unchanged => {
                        $crate::plugin_abi::TransformResponse::Unchanged
                    }
                    $crate::TransformResult::Replace(node) => {
                        $crate::plugin_abi::TransformResponse::Replace { node }
                    }
                    $crate::TransformResult::ReplaceMany(nodes) => {
                        $crate::plugin_abi::TransformResponse::ReplaceMany { nodes }
                    }
                    $crate::TransformResult::Remove => {
                        $crate::plugin_abi::TransformResponse::Remove
                    }
                    _ => $crate::plugin_abi::TransformResponse::Unchanged,
                }
            });
            let response = match response {
                Ok(r) => r,
                Err(_) => $crate::plugin_abi::TransformResponse::Error {
                    message: "plugin panicked".to_string(),
                },
            };
            let json = $crate::_reexport::serde_json::to_string(&response)
                .expect("binoc SDK: serialize transform response");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_transformer_extract(
            index: u32,
            request: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            let response = ::std::panic::catch_unwind(|| {
                let request_str = ::std::ffi::CStr::from_ptr(request)
                    .to_str()
                    .expect("binoc SDK: valid UTF-8 request");
                let req: $crate::plugin_abi::ExtractRequest =
                    $crate::_reexport::serde_json::from_str(request_str)
                        .expect("binoc SDK: deserialize ExtractRequest");
                let data = $crate::LocalDataAccess::with_data_root(
                    ::std::path::PathBuf::from(&req.data_root),
                );
                let mut node = req.node;
                node.source_items = req.source_items;
                let transformers: Vec<Box<dyn $crate::Transformer>> =
                    vec![$(Box::new(<$trans as ::std::default::Default>::default())),+];
                let trans = &transformers[index as usize];
                match $crate::Transformer::extract(trans.as_ref(), &node, &req.aspect, &data) {
                    Some($crate::ExtractResult::Text(t)) => {
                        $crate::plugin_abi::ExtractResponse::Text { content: t }
                    }
                    Some($crate::ExtractResult::Binary(b)) => {
                        $crate::plugin_abi::ExtractResponse::Binary { content: b }
                    }
                    None => $crate::plugin_abi::ExtractResponse::None,
                }
            });
            let response = match response {
                Ok(r) => r,
                Err(_) => $crate::plugin_abi::ExtractResponse::Error {
                    message: "plugin panicked".to_string(),
                },
            };
            let json = $crate::_reexport::serde_json::to_string(&response)
                .expect("binoc SDK: serialize extract response");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }
    };

    // Internal: renderer entry points
    (@renderer_fns $($out:ty),+) => {
        #[no_mangle]
        pub unsafe extern "C" fn _binoc_renderer_render(
            index: u32,
            request: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            let response = ::std::panic::catch_unwind(|| {
                let request_str = ::std::ffi::CStr::from_ptr(request)
                    .to_str()
                    .expect("binoc SDK: valid UTF-8 request");
                let req: $crate::plugin_abi::RenderRequest =
                    $crate::_reexport::serde_json::from_str(request_str)
                        .expect("binoc SDK: deserialize RenderRequest");
                let renderers: Vec<Box<dyn $crate::Renderer>> =
                    vec![$(Box::new(<$out as ::std::default::Default>::default())),+];
                let out = &renderers[index as usize];
                match $crate::Renderer::render(out.as_ref(), &req.migrations, &req.config) {
                    Ok(output) => $crate::plugin_abi::RenderResponse::Ok { output },
                    Err(e) => $crate::plugin_abi::RenderResponse::Error {
                        message: e.to_string(),
                    },
                }
            });
            let response = match response {
                Ok(r) => r,
                Err(_) => $crate::plugin_abi::RenderResponse::Error {
                    message: "plugin panicked".to_string(),
                },
            };
            let json = $crate::_reexport::serde_json::to_string(&response)
                .expect("binoc SDK: serialize render response");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }
    };

    // ── Public entry: comparators only ─────────────────────────────
    (
        module: $module_name:ident,
        comparators: [$($comp:ty),+ $(,)?] $(,)?
    ) => {
        #[no_mangle]
        pub extern "C" fn _binoc_plugin_describe() -> *mut ::std::ffi::c_char {
            let desc = $crate::plugin_abi::PluginDescription {
                sdk_version: $crate::SDK_VERSION.to_string(),
                comparators: $crate::export_plugin!(@comp_descs $($comp),+),
                transformers: vec![],
                renderers: vec![],
            };
            let json = $crate::_reexport::serde_json::to_string(&desc)
                .expect("binoc SDK: serialize plugin description");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_free_string(s: *mut ::std::ffi::c_char) {
            if !s.is_null() {
                drop(::std::ffi::CString::from_raw(s));
            }
        }

        $crate::export_plugin!(@comparator_fns $($comp),+);

        #[cfg(feature = "python")]
        #[::pyo3::pymodule]
        fn $module_name(_m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
            Ok(())
        }
    };

    // ── Public entry: transformers only ────────────────────────────
    (
        module: $module_name:ident,
        transformers: [$($trans:ty),+ $(,)?] $(,)?
    ) => {
        #[no_mangle]
        pub extern "C" fn _binoc_plugin_describe() -> *mut ::std::ffi::c_char {
            let desc = $crate::plugin_abi::PluginDescription {
                sdk_version: $crate::SDK_VERSION.to_string(),
                comparators: vec![],
                transformers: $crate::export_plugin!(@trans_descs $($trans),+),
                renderers: vec![],
            };
            let json = $crate::_reexport::serde_json::to_string(&desc)
                .expect("binoc SDK: serialize plugin description");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_free_string(s: *mut ::std::ffi::c_char) {
            if !s.is_null() {
                drop(::std::ffi::CString::from_raw(s));
            }
        }

        $crate::export_plugin!(@transformer_fns $($trans),+);

        #[cfg(feature = "python")]
        #[::pyo3::pymodule]
        fn $module_name(_m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
            Ok(())
        }
    };

    // ── Public entry: renderers only ──────────────────────────────
    (
        module: $module_name:ident,
        renderers: [$($out:ty),+ $(,)?] $(,)?
    ) => {
        #[no_mangle]
        pub extern "C" fn _binoc_plugin_describe() -> *mut ::std::ffi::c_char {
            let desc = $crate::plugin_abi::PluginDescription {
                sdk_version: $crate::SDK_VERSION.to_string(),
                comparators: vec![],
                transformers: vec![],
                renderers: $crate::export_plugin!(@out_descs $($out),+),
            };
            let json = $crate::_reexport::serde_json::to_string(&desc)
                .expect("binoc SDK: serialize plugin description");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_free_string(s: *mut ::std::ffi::c_char) {
            if !s.is_null() {
                drop(::std::ffi::CString::from_raw(s));
            }
        }

        $crate::export_plugin!(@renderer_fns $($out),+);

        #[cfg(feature = "python")]
        #[::pyo3::pymodule]
        fn $module_name(_m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
            Ok(())
        }
    };

    // ── Public entry: comparators + transformers ───────────────────
    (
        module: $module_name:ident,
        comparators: [$($comp:ty),+ $(,)?],
        transformers: [$($trans:ty),+ $(,)?] $(,)?
    ) => {
        #[no_mangle]
        pub extern "C" fn _binoc_plugin_describe() -> *mut ::std::ffi::c_char {
            let desc = $crate::plugin_abi::PluginDescription {
                sdk_version: $crate::SDK_VERSION.to_string(),
                comparators: $crate::export_plugin!(@comp_descs $($comp),+),
                transformers: $crate::export_plugin!(@trans_descs $($trans),+),
                renderers: vec![],
            };
            let json = $crate::_reexport::serde_json::to_string(&desc)
                .expect("binoc SDK: serialize plugin description");
            ::std::ffi::CString::new(json)
                .expect("binoc SDK: CString from JSON")
                .into_raw()
        }

        #[no_mangle]
        pub unsafe extern "C" fn _binoc_free_string(s: *mut ::std::ffi::c_char) {
            if !s.is_null() {
                drop(::std::ffi::CString::from_raw(s));
            }
        }

        $crate::export_plugin!(@comparator_fns $($comp),+);
        $crate::export_plugin!(@transformer_fns $($trans),+);

        #[cfg(feature = "python")]
        #[::pyo3::pymodule]
        fn $module_name(_m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
            Ok(())
        }
    };
}
