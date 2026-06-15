//! Test-vector helpers for `binoc-shapefile`. Enabled by the `test-support`
//! feature. Provides [`ShapefileMaterializer`], a [`VectorMaterializer`] that
//! builds a real `.shp` geometry file from a committed `geometry.json`.
//!
//! Staging layout (one `.shp.d` directory per source file):
//!
//! ```text
//! data.shp.d/
//!   geometry.json
//! ```
//!
//! `geometry.json` has the shape:
//!
//! ```json
//! {
//!   "type": "polygon",
//!   "features": [
//!     [[0, 0], [0, 1], [1, 1], [1, 0], [0, 0]],
//!     [[2, 2], [2, 3], [3, 3], [3, 2], [2, 2]]
//!   ]
//! }
//! ```
//!
//! - `type` is `"point"` or `"polygon"` (the two geometry kinds these vectors
//!   exercise).
//! - For `"point"`, each feature is a single `[x, y]` coordinate pair.
//! - For `"polygon"`, each feature is an outer ring: a list of `[x, y]` pairs,
//!   conventionally closed (first point repeated as last).
//!
//! Only a `.shp` (and its `.shx` index, discarded) is produced — the geometry
//! parser reads `.shp` alone, so the vectors need no `.dbf`/`.prj`.

use std::io::Cursor;
use std::path::Path;

use binoc_stdlib::test_vectors::VectorMaterializer;
use serde::Deserialize;
use shapefile::{Point, Polygon, PolygonRing, ShapeWriter};

/// Builds `.shp` geometry files from `.shp.d/geometry.json` dirs.
pub struct ShapefileMaterializer;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum GeometrySpec {
    Point { features: Vec<[f64; 2]> },
    Polygon { features: Vec<Vec<[f64; 2]>> },
}

impl VectorMaterializer for ShapefileMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".shp.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let geometry_path = staging_dir.join("geometry.json");
        let text = std::fs::read_to_string(&geometry_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", geometry_path.display()));
        let spec: GeometrySpec = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("parse {}: {e}", geometry_path.display()));

        let mut shp = Cursor::new(Vec::new());
        let mut shx = Cursor::new(Vec::new());
        let writer = ShapeWriter::with_shx(&mut shp, &mut shx);

        match spec {
            GeometrySpec::Point { features } => {
                let shapes: Vec<Point> = features.iter().map(|[x, y]| Point::new(*x, *y)).collect();
                writer
                    .write_shapes(&shapes)
                    .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
            }
            GeometrySpec::Polygon { features } => {
                let shapes: Vec<Polygon> = features
                    .iter()
                    .map(|ring| {
                        let points: Vec<Point> =
                            ring.iter().map(|[x, y]| Point::new(*x, *y)).collect();
                        Polygon::new(PolygonRing::Outer(points))
                    })
                    .collect();
                writer
                    .write_shapes(&shapes)
                    .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
            }
        }

        std::fs::write(out_path, shp.into_inner())
            .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
    }
}
