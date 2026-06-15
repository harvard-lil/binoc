//! Test-vector helpers for `binoc-binformats`. Enabled by the `test-support`
//! feature. Provides [`BinFormatsMaterializer`], a [`VectorMaterializer`] that
//! builds the binary vector artifacts (CBOR, MessagePack, BSON) from a committed
//! `source.json`.
//!
//! Staging layout (one `.<fmt>.d` directory per source file):
//!
//! ```text
//! data.cbor.d/
//!   source.json     # the value tree, as JSON
//! ```
//!
//! `source.json` is read into a `serde_json::Value` and re-encoded into the
//! binary format named by the output path's extension. Plist and Ion vectors
//! commit their text artifact directly and need no materializer.

use std::path::Path;

use binoc_stdlib::test_vectors::VectorMaterializer;

/// Builds binary serialization artifacts from `.<fmt>.d/source.json` dirs.
pub struct BinFormatsMaterializer;

impl VectorMaterializer for BinFormatsMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".cbor.d", ".msgpack.d", ".bson.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let source_path = staging_dir.join("source.json");
        let text = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", source_path.display()));
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("parse {}: {e}", source_path.display()));

        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_else(|| panic!("no extension on {}", out_path.display()));

        let bytes = match ext {
            "cbor" => {
                let mut buf = Vec::new();
                ciborium::ser::into_writer(&value, &mut buf)
                    .unwrap_or_else(|e| panic!("encode CBOR {}: {e}", out_path.display()));
                buf
            }
            "msgpack" => rmp_serde::to_vec(&value)
                .unwrap_or_else(|e| panic!("encode MessagePack {}: {e}", out_path.display())),
            "bson" => {
                let doc = bson::serialize_to_document(&value)
                    .unwrap_or_else(|e| panic!("to BSON document {}: {e}", out_path.display()));
                doc.to_vec()
                    .unwrap_or_else(|e| panic!("encode BSON {}: {e}", out_path.display()))
            }
            other => panic!(
                "unsupported binformats extension {other:?} for {}",
                out_path.display()
            ),
        };

        std::fs::write(out_path, bytes)
            .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
    }
}
