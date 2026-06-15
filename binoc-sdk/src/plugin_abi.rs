//! C-ABI stable protocol for native renderer plugins.
//!
//! Plugins compiled as separate cdylibs expose `#[no_mangle] extern "C"`
//! functions. The host loads them via `libloading` and calls them with
//! JSON-serialized requests/responses, avoiding Rust ABI compatibility
//! requirements.

use serde::{Deserialize, Serialize};

use crate::traits::RendererDescriptor;

// ── Plugin description ─────────────────────────────────────────────

/// Top-level plugin description returned by `_binoc_plugin_describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescription {
    pub sdk_version: String,
    #[serde(default)]
    pub renderers: Vec<RendererDescriptor>,
}

// ── Renderer wire types ───────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderRequest {
    pub changesets: Vec<crate::ir::Changeset>,
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

// ── export_plugin! macro ───────────────────────────────────────────

/// Export a renderer plugin pack.
///
/// Generates:
///
/// - `_binoc_plugin_describe`
/// - `_binoc_free_string`
/// - `_binoc_renderer_render`
/// - an empty `#[pymodule]` when the `python` feature is active
///
/// # Example
///
/// ```ignore
/// export_plugin! {
///     module: my_plugin,
///     renderers: [MyRenderer],
/// }
/// ```
#[macro_export]
macro_rules! export_plugin {
    (@out_descs $($out:ty),*) => {{
        let mut descs = Vec::new();
        $(
            descs.push($crate::Renderer::descriptor(
                &<$out as ::std::default::Default>::default(),
            ));
        )*
        descs
    }};

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
                match $crate::Renderer::render(out.as_ref(), &req.changesets, &req.config) {
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

    (
        module: $module_name:ident,
        renderers: [$($out:ty),+ $(,)?] $(,)?
    ) => {
        #[no_mangle]
        pub extern "C" fn _binoc_plugin_describe() -> *mut ::std::ffi::c_char {
            let desc = $crate::plugin_abi::PluginDescription {
                sdk_version: $crate::SDK_VERSION.to_string(),
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
}
