//! Test-vector helpers for `binoc-shapefile`. Enabled by the `test-support`
//! feature. Provides [`ShapefileMaterializer`], a [`VectorMaterializer`] that
//! builds a real `.shp` geometry file (and, for fusion vectors, its sibling
//! `.shx`/`.dbf`/`.prj`/`.cpg`) from a committed staging directory.
//!
//! Staging layout (one `.shp.d` directory per source file):
//!
//! ```text
//! roads.shp.d/
//!   geometry.json     (required)
//!   attributes.json   (optional -> roads.dbf)
//!   crs.wkt           (optional -> roads.prj)
//!   encoding.txt      (optional -> roads.cpg)
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
//! `attributes.json`, when present, drives the sibling `.dbf` (the per-feature
//! attribute table the fusing parser surfaces as a `tabular_v1` child):
//!
//! ```json
//! {
//!   "columns": [
//!     { "name": "name", "type": "character", "width": 24 },
//!     { "name": "lanes", "type": "numeric", "width": 4 }
//!   ],
//!   "rows": [["Main St", 2], ["Oak Ave", 4]]
//! }
//! ```
//!
//! `crs.wkt` (-> `.prj`) and `encoding.txt` (-> `.cpg`) are copied verbatim.
//! Geometry-only vectors that omit the optional files materialize only a `.shp`
//! and exercise the single-input geometry parser.

use std::io::Cursor;
use std::path::Path;

use binoc_stdlib::test_vectors::VectorMaterializer;
use dbase::{FieldName, FieldValue, Record, TableWriterBuilder};
use serde::Deserialize;
use serde_json::Value;
use shapefile::{Point, Polygon, PolygonRing, ShapeWriter};
use std::convert::TryFrom;

/// Builds shapefile member files from a `.shp.d/` staging dir: always a `.shp`,
/// plus a real `.shx` and the `.dbf`/`.prj`/`.cpg` sidecars when the staging dir
/// provides their source files (i.e. for fusion vectors). A geometry-only
/// staging dir yields just the `.shp`.
pub struct ShapefileMaterializer;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum GeometrySpec {
    Point { features: Vec<[f64; 2]> },
    Polygon { features: Vec<Vec<[f64; 2]>> },
}

#[derive(Deserialize)]
struct AttributeSpec {
    columns: Vec<ColumnSpec>,
    rows: Vec<Vec<Value>>,
}

#[derive(Deserialize)]
struct ColumnSpec {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "default_width")]
    width: u8,
    #[serde(default)]
    decimals: u8,
}

fn default_width() -> u8 {
    32
}

impl VectorMaterializer for ShapefileMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".shp.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        // out_path is `<stem>.shp`; siblings replace the `.shp` extension.
        let shp_stem = out_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| panic!("bad .shp out path {}", out_path.display()));
        let dir = out_path.parent().unwrap_or_else(|| Path::new("."));
        let sibling = |ext: &str| dir.join(format!("{shp_stem}.{ext}"));

        // .shp (+ real .shx) from geometry.json.
        let (shp_bytes, shx_bytes) = build_geometry(staging_dir);
        std::fs::write(out_path, shp_bytes)
            .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));

        let attributes_path = staging_dir.join("attributes.json");
        let prj_path = staging_dir.join("crs.wkt");
        let cpg_path = staging_dir.join("encoding.txt");

        // The `.shx` index only matters when this is a real fusion vector (one
        // with a content sidecar): the fusing parser folds the `.shx` member into
        // the layer. For a geometry-only vector (just geometry.json) the `.shx` is
        // discarded, exactly as before — keeping those vectors single-input so
        // they exercise the plain `.shp` geometry parser, not a fused layer.
        let is_fusion_vector = attributes_path.exists() || prj_path.exists() || cpg_path.exists();
        if is_fusion_vector {
            std::fs::write(sibling("shx"), shx_bytes)
                .unwrap_or_else(|e| panic!("write {}.shx: {e}", shp_stem));
        }

        // .dbf from attributes.json (optional).
        if attributes_path.exists() {
            let dbf_bytes = build_dbf(&attributes_path);
            std::fs::write(sibling("dbf"), dbf_bytes)
                .unwrap_or_else(|e| panic!("write {}.dbf: {e}", shp_stem));
        }

        // .prj from crs.wkt (optional, verbatim).
        if prj_path.exists() {
            std::fs::copy(&prj_path, sibling("prj"))
                .unwrap_or_else(|e| panic!("write {}.prj: {e}", shp_stem));
        }

        // .cpg from encoding.txt (optional, verbatim).
        if cpg_path.exists() {
            std::fs::copy(&cpg_path, sibling("cpg"))
                .unwrap_or_else(|e| panic!("write {}.cpg: {e}", shp_stem));
        }
    }
}

/// Build `.shp` + `.shx` byte buffers from `staging_dir/geometry.json`.
fn build_geometry(staging_dir: &Path) -> (Vec<u8>, Vec<u8>) {
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
            writer.write_shapes(&shapes).expect("write point shapes");
        }
        GeometrySpec::Polygon { features } => {
            let shapes: Vec<Polygon> = features
                .iter()
                .map(|ring| {
                    let points: Vec<Point> = ring.iter().map(|[x, y]| Point::new(*x, *y)).collect();
                    Polygon::new(PolygonRing::Outer(points))
                })
                .collect();
            writer.write_shapes(&shapes).expect("write polygon shapes");
        }
    }

    (shp.into_inner(), shx.into_inner())
}

/// Build a `.dbf` byte buffer from an `attributes.json` spec.
fn build_dbf(attributes_path: &Path) -> Vec<u8> {
    let text = std::fs::read_to_string(attributes_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", attributes_path.display()));
    let spec: AttributeSpec = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", attributes_path.display()));

    let mut builder = TableWriterBuilder::new();
    for column in &spec.columns {
        let field = FieldName::try_from(column.name.as_str())
            .unwrap_or_else(|_| panic!("bad dbf field name {}", column.name));
        builder = match column.kind.as_str() {
            "character" => builder.add_character_field(field, column.width),
            "numeric" => builder.add_numeric_field(field, column.width, column.decimals),
            "logical" => builder.add_logical_field(field),
            other => panic!("unsupported dbf column type {other}"),
        };
    }

    let mut buf = Cursor::new(Vec::new());
    let mut writer = builder.build_with_dest(&mut buf);
    for row in &spec.rows {
        let mut record = Record::default();
        for (column, cell) in spec.columns.iter().zip(row) {
            record.insert(column.name.clone(), cell_to_field(column, cell));
        }
        writer.write_record(&record).expect("write dbf record");
    }
    writer.finalize().expect("finalize dbf");
    drop(writer);
    buf.into_inner()
}

fn cell_to_field(column: &ColumnSpec, cell: &Value) -> FieldValue {
    match column.kind.as_str() {
        "character" => FieldValue::Character(cell.as_str().map(str::to_string)),
        "numeric" => FieldValue::Numeric(cell.as_f64()),
        "logical" => FieldValue::Logical(cell.as_bool()),
        other => panic!("unsupported dbf column type {other}"),
    }
}
