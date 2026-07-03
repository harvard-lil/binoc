//! ESRI shapefile *fusing* parse rule (CFM-83 multi-input claim).
//!
//! A shapefile is not one file: it is a correlated set of sibling files sharing
//! a basename stem — `roads.shp` (geometry), `roads.shx` (index), `roads.dbf`
//! (attribute table), `roads.prj` (CRS as WKT), `roads.cpg` (encoding). This rule
//! claims that member set as one logical dataset and folds it into a single
//! "Shapefile layer" node:
//!
//! - the `.shp` geometry summary becomes a `structured_document_v1` child
//!   (`<layer>/>geometry`, reusing the shipped single-input `.shp` reader), so a
//!   changed feature count / geometry type / bounding box renders as a document
//!   value change under the layer;
//! - the `.dbf` attribute table becomes a `tabular_v1` child
//!   (`<layer>/>attributes`, reusing `binoc-dbf`'s producer), so per-row/per-cell
//!   attribute edits render as an ordinary table diff under the layer;
//! - the `.prj` CRS and `.cpg` encoding ride as a `parser_metadata_v1` artifact
//!   (carried-but-unrendered until CFM-82 ships a metadata writer).
//!
//! The fused node is a pure *container* (named `item_type` "Shapefile layer", no
//! primary artifact of its own) — the same node shape a parsed SQLite/Excel
//! container emits. Geometry and attributes are leaf children so the
//! leaf-shaped structured-document and tabular writers diff them; CFM-81's
//! per-artifact composition then renders "geometry changed + attributes edited"
//! coherently under one layer.
//!
//! ## Decline cleanly
//!
//! The rule is offered every same-stem sibling group whose anchor is a `.shp`.
//! If the `.shp` bytes do not validate as shapefile geometry it returns an empty
//! [`ParseOutput`] — a **decline** — releasing the members so a standalone `.dbf`
//! that merely shares a stem with a non-shapefile `.shp` still parses alone as a
//! `tabular_v1` table. The `.shp` reader is the single source of truth for "is
//! this a shapefile"; there is no separate grouping registry to disagree with it.

use binoc_sdk::*;

use crate::shapefile::read_shp_summary;

/// Multi-input shapefile claim: `.shp` (required anchor) + `.shx`/`.dbf`/`.prj`/
/// `.cpg` (optional members), correlated by shared stem.
#[derive(Default)]
pub struct ShapefileFuseRule;

/// Member slot order. Index 0 is the anchor (`.shp`); the rest are the
/// `extra_members` in this exact order, so `ParseGroup::member(N)` is stable.
const SLOT_SHX: usize = 1;
const SLOT_DBF: usize = 2;
const SLOT_PRJ: usize = 3;
const SLOT_CPG: usize = 4;

fn leaf(extensions: Vec<String>) -> NodeMatch {
    NodeMatch {
        is_dir: Some(false),
        extensions,
        media_types: Vec::new(),
    }
}

impl ParseRule for ShapefileFuseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-shapefile.fuse".into(),
            input: leaf(vec![".shp".into()]),
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    /// Required by the trait but never driven by the engine for a multi-input
    /// rule (the engine always calls [`parse_group`](Self::parse_group)). A bare
    /// `.shp` with no sidecars is not a fused layer, so this declines; the
    /// single-input `.shp` geometry parser serves that case.
    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        self.parse_group(&ParseGroup::single(item.clone()), data)
    }

    fn extra_members(&self) -> Vec<MemberMatch> {
        // Order here defines the SLOT_* indices above.
        vec![
            MemberMatch::optional(leaf(vec![".shx".into()])),
            MemberMatch::optional(leaf(vec![".dbf".into()])),
            MemberMatch::optional(leaf(vec![".prj".into()])),
            MemberMatch::optional(leaf(vec![".cpg".into()])),
        ]
    }

    fn parse_group(&self, group: &ParseGroup, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        // Fusion only earns its keep when a *content-bearing* sidecar is present:
        // a `.dbf` (attributes), `.prj` (CRS), or `.cpg` (encoding). A bare `.shp`
        // — or `.shp` + `.shx` only — carries no signal beyond geometry, so it
        // declines here and the single-input geometry parser serves it as a plain
        // `structured_document` leaf (no extra "Shapefile layer" wrapper). The
        // `.shx` is a pure offset index with nothing to diff, so it never alone
        // promotes a `.shp` to a fused layer.
        let has_content_sidecar = group.member(SLOT_DBF).is_some()
            || group.member(SLOT_PRJ).is_some()
            || group.member(SLOT_CPG).is_some();
        if !has_content_sidecar {
            return Ok(ParseOutput::default());
        }

        // The `.shp` reader is the authority on "is this a shapefile". If the
        // anchor bytes do not validate, decline the whole group; a standalone
        // `.dbf` sharing the stem then falls through to `binoc-dbf`.
        let shp_bytes = data.read_bytes(&group.anchor)?;
        let summary = match read_shp_summary(&shp_bytes) {
            Ok(summary) => summary,
            Err(_) => return Ok(ParseOutput::default()),
        };

        // The fused node is a pure container: empty primary bytes, named
        // item_type. Geometry and attributes hang off it as leaf children so the
        // leaf-shaped writers diff them.
        let mut output = ParseOutput {
            projection: ProjectionHint::default().item_type("Shapefile layer"),
            ..Default::default()
        };

        // Geometry summary -> `structured_document_v1` child, identical in shape
        // to the single-input `.shp` parser's output so the structured-document
        // writer renders feature_count / geometry_type / bbox changes unchanged.
        let geometry_value = serde_json::to_value(&summary)
            .map_err(|e| BinocError::Other(format!("serialize shapefile summary: {e}")))?;
        let geometry_bytes = serde_json::to_vec(&StructuredDocument {
            value: geometry_value,
            format: "shapefile".into(),
            source: serde_json::json!({ "shp_byte_len": shp_bytes.len() }),
        })
        .map_err(|e| BinocError::Other(format!("serialize geometry document: {e}")))?;
        let geometry_path = decompose_child(&group.anchor.logical_path, "geometry");
        output.children.push(ParsedChild {
            item: ItemRef {
                logical_path: geometry_path.clone(),
                is_dir: false,
                content_hash: Some(blake3::hash(&geometry_bytes).to_hex().to_string()),
                size: Some(geometry_bytes.len() as u64),
                media_type: Some("application/vnd.binoc.structured-document+json".into()),
                projection_hint: ProjectionHint::default().item_type("Geometry"),
                handle: geometry_path,
            },
            artifacts: vec![ParsedArtifact {
                format: structured_document_v1(),
                bytes: geometry_bytes,
            }],
        });

        // The `.dbf` attribute table becomes a `tabular_v1` child, produced by
        // the exact same reader a standalone `.dbf` uses.
        if let Some(dbf) = group.member(SLOT_DBF) {
            let dbf_bytes = data.read_bytes(dbf)?;
            let table = binoc_dbf::read_dbf_tabular(&dbf_bytes)?;
            let table_bytes = serde_json::to_vec(&table)
                .map_err(|e| BinocError::Other(format!("serialize dbf tabular: {e}")))?;
            let child_path = decompose_child(&group.anchor.logical_path, "attributes");
            output.children.push(ParsedChild {
                item: ItemRef {
                    logical_path: child_path.clone(),
                    is_dir: false,
                    content_hash: Some(blake3::hash(&table_bytes).to_hex().to_string()),
                    size: Some(table_bytes.len() as u64),
                    media_type: Some("application/vnd.binoc.tabular+json".into()),
                    projection_hint: ProjectionHint::default().item_type("Attribute table"),
                    handle: child_path,
                },
                artifacts: vec![ParsedArtifact {
                    format: tabular_v1(),
                    bytes: table_bytes,
                }],
            });
        }

        // CRS (`.prj`, WKT) and encoding (`.cpg`) ride as a parser_metadata_v1
        // artifact. Carried-but-unrendered until CFM-82's metadata writer ships;
        // emitting it now means fusion does not have to be revisited then.
        let mut metadata = serde_json::Map::new();
        if let Some(prj) = group.member(SLOT_PRJ) {
            let wkt = String::from_utf8_lossy(&data.read_bytes(prj)?)
                .trim()
                .to_string();
            metadata.insert("crs_wkt".into(), serde_json::Value::String(wkt));
        }
        if let Some(cpg) = group.member(SLOT_CPG) {
            let encoding = String::from_utf8_lossy(&data.read_bytes(cpg)?)
                .trim()
                .to_string();
            metadata.insert("encoding".into(), serde_json::Value::String(encoding));
        }
        // Record which sidecars were present (provenance), so a future consumer
        // can tell a `.shx`-less layer from a complete one.
        metadata.insert(
            "members".into(),
            serde_json::json!({
                "shx": group.member(SLOT_SHX).is_some(),
                "dbf": group.member(SLOT_DBF).is_some(),
                "prj": group.member(SLOT_PRJ).is_some(),
                "cpg": group.member(SLOT_CPG).is_some(),
            }),
        );
        let parser_metadata = ParserMetadata::new("shapefile", serde_json::Value::Object(metadata));
        let metadata_bytes = serde_json::to_vec(&parser_metadata)
            .map_err(|e| BinocError::Other(format!("serialize parser metadata: {e}")))?;
        output.artifacts.push(ParsedArtifact {
            format: parser_metadata_v1(),
            bytes: metadata_bytes,
        });

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use binoc_sdk::LocalDataAccess;
    use shapefile::{Point, ShapeWriter};

    fn write_points(points: &[(f64, f64)]) -> Vec<u8> {
        let shapes: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        let mut shp = Cursor::new(Vec::new());
        let mut shx = Cursor::new(Vec::new());
        ShapeWriter::with_shx(&mut shp, &mut shx)
            .write_shapes(&shapes)
            .unwrap();
        shp.into_inner()
    }

    #[test]
    fn declines_size_one_group() {
        let data = LocalDataAccess::new();
        let shp = data
            .provide("roads.shp", &write_points(&[(0.0, 0.0)]))
            .unwrap();
        // A lone .shp (no sidecars) is the single-input parser's job; fuse declines.
        let out = ShapefileFuseRule
            .parse_group(&ParseGroup::single(shp), &data)
            .unwrap();
        assert!(out.bytes.is_empty() && out.children.is_empty());
    }

    #[test]
    fn declines_invalid_shp_releasing_the_group() {
        let data = LocalDataAccess::new();
        let shp = data.provide("notes.shp", b"not a shapefile").unwrap();
        let dbf = data.provide("notes.dbf", b"also not a dbf").unwrap();
        // Group of two, but the anchor is not valid shapefile geometry -> decline,
        // so the .dbf is released to the single-input dbf parser.
        let group = ParseGroup {
            anchor: shp.clone(),
            members: vec![Some(shp), None, Some(dbf), None, None],
        };
        let out = ShapefileFuseRule.parse_group(&group, &data).unwrap();
        assert!(out.bytes.is_empty() && out.children.is_empty());
    }

    #[test]
    fn fuses_shp_and_dbf_into_layer_with_children() {
        let data = LocalDataAccess::new();
        let shp = data
            .provide("roads.shp", &write_points(&[(0.0, 0.0), (1.0, 1.0)]))
            .unwrap();
        // A real tiny .dbf via the dbf test materializer's producer path.
        let dbf_bytes = build_tiny_dbf();
        let dbf = data.provide("roads.dbf", &dbf_bytes).unwrap();
        let prj = data.provide("roads.prj", b"GEOGCS[\"WGS 84\"]").unwrap();
        let group = ParseGroup {
            anchor: shp.clone(),
            members: vec![Some(shp), None, Some(dbf), Some(prj), None],
        };
        let out = ShapefileFuseRule.parse_group(&group, &data).unwrap();

        // Container node: no primary bytes, named item_type, two leaf children.
        assert!(out.bytes.is_empty());
        assert_eq!(out.projection.item_type.as_deref(), Some("Shapefile layer"));
        let child_paths: Vec<&str> = out
            .children
            .iter()
            .map(|c| c.item.logical_path.as_str())
            .collect();
        assert!(child_paths.contains(&"roads.shp/>geometry"));
        assert!(child_paths.contains(&"roads.shp/>attributes"));
        // CRS rides as a parser_metadata_v1 artifact (carried, unrendered).
        assert!(out
            .artifacts
            .iter()
            .any(|a| a.format == parser_metadata_v1()));
    }

    fn build_tiny_dbf() -> Vec<u8> {
        use dbase::{FieldName, FieldValue, Record, TableWriterBuilder};
        use std::convert::TryFrom;
        let builder =
            TableWriterBuilder::new().add_character_field(FieldName::try_from("name").unwrap(), 8);
        let mut buf = Cursor::new(Vec::new());
        let mut writer = builder.build_with_dest(&mut buf);
        let mut record = Record::default();
        record.insert("name".into(), FieldValue::Character(Some("a".into())));
        writer.write_record(&record).unwrap();
        writer.finalize().unwrap();
        drop(writer);
        buf.into_inner()
    }
}
