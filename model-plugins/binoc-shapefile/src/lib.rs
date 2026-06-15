//! shapefile correspondence rule pack for Binoc.
//!
//! Parses the geometry stream of an ESRI shapefile (`.shp`) into the generic
//! `structured_document_v1` artifact tagged `format: "shapefile"`, so the stdlib
//! structured-document writer diffs a compact geometry summary (feature count,
//! geometry type, bounding box) without the engine knowing anything geospatial.
//!
//! Two parse rules ship here (CFM-83):
//!
//! - [`ShapefileFuseRule`] — a **multi-input** claim over the
//!   `.shp`/`.shx`/`.dbf`/`.prj`/`.cpg` member set (correlated by shared stem).
//!   It folds the set into one "Shapefile layer" node: geometry summary as the
//!   primary artifact, the `.dbf` as a `tabular_v1` child, and CRS/encoding as a
//!   `parser_metadata_v1` artifact. It declines cleanly when the group is not a
//!   real shapefile, so a standalone `.dbf` sharing a stem still parses alone.
//! - [`ShapefileParseRule`] — the original **single-input** `.shp` geometry
//!   parser, still valid for a bare `.shp` with no sidecars and reused by the
//!   fusing rule as the geometry reader.

mod fusion;
mod shapefile;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use fusion::ShapefileFuseRule;
pub use shapefile::ShapefileParseRule;

/// Register this pack's parse rules into an engine config.
///
/// Both the fusing (multi-input) and single-input `.shp` parsers are registered.
/// Arity-descending precedence in the engine offers a same-stem sibling group to
/// the size-5 fusing claim first; a bare `.shp` (or a declined group) falls
/// through to the size-1 geometry parser.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(ShapefileParseRule)));
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(ShapefileFuseRule)));
}

#[cfg(feature = "test-support")]
pub mod test_support;
