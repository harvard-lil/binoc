//! C-ABI stable protocol for native renderer plugins.
//!
//! Plugins compiled as separate cdylibs expose `#[no_mangle] extern "C"`
//! functions. The host loads them via `libloading` and calls them with
//! JSON-serialized requests/responses, avoiding Rust ABI compatibility
//! requirements.
//!
//! As of CFM-27b, renderers are the only graduated stable ABI family. Rule
//! families remain in-process until their trait shapes and vocabularies satisfy
//! the graduation signal recorded in the tiered plugin surface ADR.

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
    /// Complete projected changesets. Renderer-facing vocabularies inside the
    /// IR remain open strings: actions, item types, tags, source evidence,
    /// detail-block kinds, global-claim verbs, and edit verbs must flow through
    /// the renderer ABI unchanged.
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
        vec![
            $($crate::Renderer::descriptor(
                &<$out as ::std::default::Default>::default(),
            )),*
        ]
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

#[cfg(test)]
mod tests {
    use std::ffi::{c_char, CStr, CString};

    use serde_json::json;

    use crate::{
        correspondence::Edit, BinocResult, Changeset, DiffNode, Renderer, RendererDescriptor, Side,
        Source,
    };

    #[derive(Default)]
    struct EchoRenderer;

    impl Renderer for EchoRenderer {
        fn descriptor(&self) -> RendererDescriptor {
            RendererDescriptor::new("test.echo", "echo")
        }

        fn render(
            &self,
            changesets: &[Changeset],
            config: &serde_json::Value,
        ) -> BinocResult<String> {
            let root = changesets
                .first()
                .and_then(|changeset| changeset.root.as_ref())
                .expect("test request has a root node");
            let source = root.sources.first().expect("test request has a source");
            let edits = root
                .details
                .get("edits")
                .and_then(|value| value.as_array())
                .expect("test request has edits");
            let edit_verb = edits
                .first()
                .and_then(|edit| edit.get("verb"))
                .and_then(|verb| verb.as_str())
                .expect("test request edit has verb");

            Ok(json!({
                "action": root.action,
                "item_type": root.item_type,
                "tag": root.tags.iter().next(),
                "source_evidence": source.evidence,
                "source_action": source.action,
                "edit_verb": edit_verb,
                "config_seen": config["mode"],
            })
            .to_string())
        }
    }

    crate::export_plugin! {
        module: abi_test_plugin,
        renderers: [EchoRenderer],
    }

    unsafe fn take_owned_abi_string(ptr: *mut c_char) -> String {
        assert!(!ptr.is_null());
        let value = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .expect("ABI string is UTF-8")
            .to_string();
        unsafe { _binoc_free_string(ptr) };
        value
    }

    #[test]
    fn renderer_abi_preserves_open_ir_vocabulary() {
        let description_json = unsafe { take_owned_abi_string(_binoc_plugin_describe()) };
        let description: crate::plugin_abi::PluginDescription =
            serde_json::from_str(&description_json).expect("plugin description");
        assert_eq!(description.sdk_version, crate::SDK_VERSION);
        assert_eq!(description.renderers.len(), 1);
        assert_eq!(description.renderers[0].name, "test.echo");

        let edit = Edit::new(
            "third_party.frobnicate",
            json!({ "mode": "unknown-to-host" }),
        );
        let node = DiffNode::new(
            "third_party.rebalance",
            "third_party.dataset",
            "current/data.bin",
        )
        .with_tag("third_party.semantic")
        .with_source(
            Source::new("previous/data.bin", Side::From)
                .with_evidence("third_party.pair.bespoke")
                .with_action("third_party.source_action"),
        )
        .with_detail("edits", json!([edit]));
        let request = crate::plugin_abi::RenderRequest {
            changesets: vec![Changeset::new("left", "right", Some(node))],
            config: json!({ "mode": "parity" }),
        };
        let request_json = serde_json::to_string(&request).expect("request JSON");
        let request_cstring = CString::new(request_json).expect("request has no nul");

        let response_json =
            unsafe { take_owned_abi_string(_binoc_renderer_render(0, request_cstring.as_ptr())) };
        let response: crate::plugin_abi::RenderResponse =
            serde_json::from_str(&response_json).expect("render response");
        let crate::plugin_abi::RenderResponse::Ok { output } = response else {
            panic!("renderer ABI returned an error");
        };
        let output: serde_json::Value = serde_json::from_str(&output).expect("renderer output");

        assert_eq!(output["action"], "third_party.rebalance");
        assert_eq!(output["item_type"], "third_party.dataset");
        assert_eq!(output["tag"], "third_party.semantic");
        assert_eq!(output["source_evidence"], "third_party.pair.bespoke");
        assert_eq!(output["source_action"], "third_party.source_action");
        assert_eq!(output["edit_verb"], "third_party.frobnicate");
        assert_eq!(output["config_seen"], "parity");
    }
}
