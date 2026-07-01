//! The ABI canary crossing test.
//!
//! Builds this crate's `cdylib`, loads it over `libloading` (exactly as
//! `binoc-python`'s native-plugin loader does), and asserts that plugin
//! description and a render round-trip across the `extern "C"` + JSON boundary.
//! If the renderer ABI or its wire contract drifts, this fails to load or
//! round-trip — the property the fat in-process build would otherwise give up.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::process::Command;

use binoc_sdk::plugin_abi::{PluginDescription, RenderRequest, RenderResponse};
use binoc_sdk::{Changeset, DiffNode, SDK_VERSION};

type DescribeFn = unsafe extern "C" fn() -> *mut c_char;
type RenderFn = unsafe extern "C" fn(u32, *const c_char) -> *mut c_char;
type FreeFn = unsafe extern "C" fn(*mut c_char);

/// Build the canary `cdylib` and return the path to the built artifact.
///
/// The nested `cargo build` guarantees the `cdylib` exists (a plain
/// `cargo test` links the crate's rlib but may not emit the `cdylib`); it is a
/// no-op when the artifact is already current. The artifact lives in the
/// profile directory two levels up from the test binary
/// (`<target>/<profile>/deps/<test>` → `<target>/<profile>/`).
fn build_and_locate_cdylib() -> PathBuf {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "binoc-abi-canary"])
        .status()
        .expect("run cargo build for the canary cdylib");
    assert!(
        status.success(),
        "failed to build the binoc-abi-canary cdylib"
    );

    let mut dir = std::env::current_exe().expect("locate the test executable");
    dir.pop(); // drop the test binary file name
    if dir.ends_with("deps") {
        dir.pop(); // drop `deps`, landing in the profile directory
    }

    let file_name = format!(
        "{}binoc_abi_canary{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let path = dir.join(&file_name);
    assert!(
        path.exists(),
        "canary cdylib not found at {} (looked for {file_name})",
        path.display()
    );
    path
}

/// Take ownership of a C string produced by the plugin and free it via the
/// plugin's own `_binoc_free_string`, mirroring the host loader's contract.
unsafe fn take_owned(ptr: *mut c_char, free_fn: FreeFn) -> String {
    assert!(!ptr.is_null(), "plugin returned a null string");
    let value = CStr::from_ptr(ptr)
        .to_str()
        .expect("plugin string is valid UTF-8")
        .to_string();
    free_fn(ptr);
    value
}

#[test]
fn native_renderer_crosses_the_c_abi() {
    let path = build_and_locate_cdylib();

    unsafe {
        let lib = libloading::Library::new(&path).expect("dlopen the canary cdylib");

        let describe_fn: DescribeFn = *lib
            .get::<DescribeFn>(b"_binoc_plugin_describe")
            .expect("_binoc_plugin_describe symbol");
        let render_fn: RenderFn = *lib
            .get::<RenderFn>(b"_binoc_renderer_render")
            .expect("_binoc_renderer_render symbol");
        let free_fn: FreeFn = *lib
            .get::<FreeFn>(b"_binoc_free_string")
            .expect("_binoc_free_string symbol");

        // ── Description crosses, and the plugin agrees with the host SDK. ──
        let description_json = take_owned(describe_fn(), free_fn);
        let description: PluginDescription =
            serde_json::from_str(&description_json).expect("parse PluginDescription");
        assert_eq!(
            description.sdk_version, SDK_VERSION,
            "plugin was built against a different binoc-sdk than the host"
        );
        assert_eq!(description.renderers.len(), 1);
        assert_eq!(description.renderers[0].name, "canary.echo");

        // ── A render round-trips over the wire. ──
        let node = DiffNode::new("modify", "file", "data.bin");
        let request = RenderRequest {
            changesets: vec![Changeset::new("left", "right", Some(node))],
            config: serde_json::json!({ "mode": "canary" }),
        };
        let request_json = serde_json::to_string(&request).expect("serialize RenderRequest");
        let request_c = CString::new(request_json).expect("request has no interior nul");

        let response_json = take_owned(render_fn(0, request_c.as_ptr()), free_fn);
        let response: RenderResponse =
            serde_json::from_str(&response_json).expect("parse RenderResponse");

        match response {
            RenderResponse::Ok { output } => {
                let echoed: serde_json::Value =
                    serde_json::from_str(&output).expect("renderer output is JSON");
                assert_eq!(echoed["root_action"], "modify");
                assert_eq!(echoed["changesets"], 1);
                assert_eq!(echoed["mode"], "canary");
            }
            RenderResponse::Error { message } => panic!("renderer returned an error: {message}"),
        }
    }
}
