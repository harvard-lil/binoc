//! shapefile correspondence rule pack for Binoc.
//!
//! Parses the geometry stream of an ESRI shapefile (`.shp`) into the generic
//! `structured_document_v1` artifact tagged `format: "shapefile"`, so the stdlib
//! structured-document writer diffs a compact geometry summary (feature count,
//! geometry type, bounding box) without the engine knowing anything geospatial.
//!
//! This is a **single-input** `.shp` parser. binoc cannot yet fuse the
//! `.shp`/`.shx`/`.dbf`/`.prj` member set into one logical dataset (that needs
//! multi-input rule support); see `DESIGN.md` in this crate for the
//! investigation and the proposed grouping-expand design. The sibling `.dbf`
//! attribute table is already parsed independently by `binoc-dbf`.

mod shapefile;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use shapefile::ShapefileParseRule;

/// Register this pack's parse rules into an engine config.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(ShapefileParseRule)));
}

#[cfg(feature = "test-support")]
pub mod test_support;
