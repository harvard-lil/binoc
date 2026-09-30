//! ABI canary: a native renderer exported over the plugin C ABI.
//!
//! This crate exists to keep the renderer ABI boundary *honest*. It is a real,
//! separately-compiled `cdylib` that goes through [`binoc_sdk::export_plugin!`],
//! and [`tests/abi_crossing.rs`](../tests/abi_crossing.rs) loads the built
//! artifact over `libloading` — the same path `binoc-python` uses — and asserts
//! a render round-trips.
//!
//! Why this matters: compiling plugins in-process (the fat-`binoc` wheel) means
//! the compiler no longer forces plugin-facing types to be expressible across a
//! process boundary. This crate restores that guarantee for the stable
//! (renderer) tier: if the renderer ABI drifts to something that cannot cross
//! `extern "C"` + JSON, `export_plugin!` fails to compile here, and if the wire
//! contract drifts, the crossing test fails to load or round-trip. It is also
//! the template each rule family must satisfy when it graduates into
//! `plugin_abi`. See
//! `docs/adr/2026-06-30-fat_binoc_distribution_and_abi_canary.md`.

use binoc_sdk::{BinocResult, Changeset, Renderer, RendererDescriptor};

/// A trivial renderer that echoes a few open-vocabulary IR fields back as JSON.
/// Its only job is to exercise the real `cdylib` -> `libloading` crossing.
#[derive(Default)]
pub struct EchoRenderer;

impl Renderer for EchoRenderer {
    fn descriptor(&self) -> RendererDescriptor {
        RendererDescriptor::new("canary.echo", "json")
    }

    fn render(&self, changesets: &[Changeset], config: &serde_json::Value) -> BinocResult<String> {
        let root_action = changesets
            .first()
            .and_then(|changeset| changeset.root.as_ref())
            .map(|root| root.action.clone())
            .unwrap_or_else(|| "none".to_string());
        Ok(serde_json::json!({
            "changesets": changesets.len(),
            "root_action": root_action,
            "mode": config.get("mode"),
        })
        .to_string())
    }
}

binoc_sdk::export_plugin! {
    module: binoc_abi_canary,
    renderers: [EchoRenderer],
}
