//! Emits the JSON Schema for the changeset wire format.
//!
//! Writes a pretty-printed JSON Schema (draft 2020-12) to the path passed on
//! argv, or to `docs/reference/changeset-schema.json` by default. The
//! resulting file is consumed by `scripts/build_changeset_schema_page.py` to
//! render the human-readable reference page.
//!
//! Gated on the `schema` feature so `schemars` stays out of the production
//! dependency graph for SDK users. See
//! `docs/adr/documentation_platform_and_info_design.md` Open Question 1.

use std::path::PathBuf;

use binoc_sdk::Changeset;
use schemars::schema_for;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("docs/reference/changeset-schema.json"));

    let schema = schema_for!(Changeset);
    let mut json = serde_json::to_string_pretty(&schema)?;
    json.push('\n');

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Skip the write when the content is unchanged so `just docs-schema` has
    // stable no-op semantics (matches the Python renderer's behavior and
    // keeps file mtimes stable for downstream caches).
    let unchanged = std::fs::read_to_string(&out_path)
        .map(|existing| existing == json)
        .unwrap_or(false);
    if unchanged {
        eprintln!("{} is up to date.", out_path.display());
    } else {
        std::fs::write(&out_path, json)?;
        eprintln!("Wrote {}", out_path.display());
    }
    Ok(())
}
