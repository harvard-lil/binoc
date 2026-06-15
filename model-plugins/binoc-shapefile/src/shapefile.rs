//! ESRI shapefile geometry (`.shp`) parse rule.
//!
//! Reads the geometry stream of a `.shp` file and emits a compact
//! `structured_document_v1` summary tagged `format: "shapefile"`: the feature
//! count, the file's single geometry type, and the overall bounding box. The
//! generic structured-document writer then diffs that summary, so a changed
//! feature count or an expanded bounding box surfaces as a clean
//! `document.value_change` at a named path (`$.feature_count`, `$.bbox.max_x`,
//! ...).
//!
//! ## Why a summary, not a row-per-feature table
//!
//! A `.shp` is geometry only — there is no per-feature key in the geometry
//! stream (attributes live in the sibling `.dbf`). A `tabular_v1` row-per-feature
//! artifact would therefore have no stable row identity, so any geometry edit
//! would degrade to opaque row add/remove churn rather than a meaningful diff.
//! A small fixed-shape summary tracks the signal that matters for a changelog —
//! "the layer gained N features / its extent grew" — and diffs it cleanly.
//!
//! ## Single-input by design
//!
//! This rule reads only the `.shp` bytes it is handed; it does not reach for the
//! sibling `.shx`/`.dbf`/`.prj`. The CRS (from `.prj`) is therefore not reported
//! here. Fusing the shapefile member set into one logical dataset (and surfacing
//! CRS) needs multi-input support, which binoc does not have today; see
//! `DESIGN.md` in this crate for the investigation and proposal.

use std::io::Cursor;

use binoc_sdk::*;
use shapefile::{ShapeReader, ShapeType};

#[derive(Default)]
pub struct ShapefileParseRule;

impl ParseRule for ShapefileParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-shapefile.parse.shp".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".shp".into()],
                media_types: Vec::new(),
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let summary = read_shp_summary(&bytes)?;
        let value = serde_json::to_value(&summary)
            .map_err(|e| BinocError::Other(format!("serialize shapefile summary: {e}")))?;
        serde_json::to_vec(&StructuredDocument {
            value,
            format: "shapefile".into(),
            source: serde_json::json!({ "byte_len": bytes.len() }),
        })
        .map(ParseOutput::from)
        .map_err(|e| BinocError::Other(format!("serialize structured document artifact: {e}")))
    }
}

/// The compact geometry summary that becomes the structured-document value tree.
///
/// Field order is the JSON object key order in the artifact; the
/// structured-document writer reports changes by path (`$.feature_count`,
/// `$.geometry_type`, `$.bbox.min_x`, ...), so these names are the diff surface.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ShapefileSummary {
    /// Number of features (shape records) in the `.shp`.
    pub(crate) feature_count: usize,
    /// The single geometry type of the file (shapefiles cannot mix types).
    pub(crate) geometry_type: &'static str,
    /// Overall 2-D bounding box from the `.shp` header.
    pub(crate) bbox: BBox,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct BBox {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

/// Read a `.shp` byte buffer into a [`ShapefileSummary`].
///
/// Uses [`ShapeReader`], which reads geometry from the `.shp` stream alone and
/// requires neither the `.shx` index nor the `.dbf` attribute table. The
/// geometry type and bounding box come from the file header (no full scan
/// needed for those); the feature count comes from counting the shape records.
pub(crate) fn read_shp_summary(bytes: &[u8]) -> BinocResult<ShapefileSummary> {
    let mut reader = ShapeReader::new(Cursor::new(bytes))
        .map_err(|e| BinocError::Other(format!("open shapefile geometry: {e}")))?;

    let header = *reader.header();
    let geometry_type = shape_type_name(header.shape_type);
    let bbox = BBox {
        min_x: header.bbox.min.x,
        min_y: header.bbox.min.y,
        max_x: header.bbox.max.x,
        max_y: header.bbox.max.y,
    };

    // Count records by iterating the geometry stream. `read()` materializes the
    // shapes; we only need the count, but the files this targets are small
    // enough that a count-by-iterate is fine and avoids depending on the `.shx`.
    let mut feature_count = 0usize;
    for shape in reader.iter_shapes() {
        shape.map_err(|e| BinocError::Other(format!("read shapefile shape: {e}")))?;
        feature_count += 1;
    }

    Ok(ShapefileSummary {
        feature_count,
        geometry_type,
        bbox,
    })
}

/// A stable, human-readable name for a [`ShapeType`].
///
/// Matched explicitly (rather than `Debug`) so the artifact's `geometry_type`
/// string is a deliberate, stable part of the diff surface. The `M`/`Z` suffixes
/// distinguish measured / 3-D variants, which are genuine type changes worth
/// surfacing.
fn shape_type_name(shape_type: ShapeType) -> &'static str {
    match shape_type {
        ShapeType::NullShape => "null",
        ShapeType::Point => "point",
        ShapeType::PointM => "point_m",
        ShapeType::PointZ => "point_z",
        ShapeType::Polyline => "polyline",
        ShapeType::PolylineM => "polyline_m",
        ShapeType::PolylineZ => "polyline_z",
        ShapeType::Polygon => "polygon",
        ShapeType::PolygonM => "polygon_m",
        ShapeType::PolygonZ => "polygon_z",
        ShapeType::Multipoint => "multipoint",
        ShapeType::MultipointM => "multipoint_m",
        ShapeType::MultipointZ => "multipoint_z",
        ShapeType::Multipatch => "multipatch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapefile::{Point, Polygon, PolygonRing, ShapeWriter};

    /// Write a tiny in-memory `.shp` (two square polygons), read it back, and
    /// assert the summary's count, geometry type, and bounding box.
    #[test]
    fn summarizes_polygon_geometry() {
        let shp = write_two_square_polygons();
        let summary = read_shp_summary(&shp).unwrap();

        assert_eq!(summary.feature_count, 2);
        assert_eq!(summary.geometry_type, "polygon");
        // Square A spans (0,0)-(1,1); square B spans (2,2)-(3,3); overall box is
        // (0,0)-(3,3).
        assert_eq!(summary.bbox.min_x, 0.0);
        assert_eq!(summary.bbox.min_y, 0.0);
        assert_eq!(summary.bbox.max_x, 3.0);
        assert_eq!(summary.bbox.max_y, 3.0);
    }

    /// Build a `.shp` byte buffer containing two axis-aligned square polygons.
    fn write_two_square_polygons() -> Vec<u8> {
        let square = |x: f64, y: f64| {
            // Outer ring, clockwise, explicitly closed (first point repeated).
            Polygon::new(PolygonRing::Outer(vec![
                Point::new(x, y),
                Point::new(x, y + 1.0),
                Point::new(x + 1.0, y + 1.0),
                Point::new(x + 1.0, y),
                Point::new(x, y),
            ]))
        };
        let shapes = vec![square(0.0, 0.0), square(2.0, 2.0)];

        let mut shp = Cursor::new(Vec::new());
        let mut shx = Cursor::new(Vec::new());
        let writer = ShapeWriter::with_shx(&mut shp, &mut shx);
        writer.write_shapes(&shapes).unwrap();
        shp.into_inner()
    }
}
