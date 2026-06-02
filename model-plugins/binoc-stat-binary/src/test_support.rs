//! Test-vector helpers for `binoc-stat-binary`.
//!
//! The committed fixture source is a small CSV-like text file in a `.dta.d`
//! staging directory. The materializer turns it into a real Stata file so the
//! vector exercises the binary reader without storing opaque binary fixtures.

use std::path::Path;

use binoc_stdlib::test_vectors::VectorMaterializer;
use dta::stata::dta::byte_order::ByteOrder;
use dta::stata::dta::dta_writer::DtaWriter;
use dta::stata::dta::header::Header;
use dta::stata::dta::release::Release;
use dta::stata::dta::schema::Schema;
use dta::stata::dta::value::Value;
use dta::stata::dta::variable::Variable;
use dta::stata::dta::variable_type::VariableType;

pub struct DtaMaterializer;

impl VectorMaterializer for DtaMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".dta.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let csv_path = staging_dir.join("table.csv");
        let csv = std::fs::read_to_string(&csv_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", csv_path.display()));
        let mut lines = csv.lines();
        let header_line = lines
            .next()
            .unwrap_or_else(|| panic!("{} must contain a header row", csv_path.display()));
        let headers: Vec<&str> = header_line.split(',').collect();

        let rows: Vec<Vec<&str>> = lines.map(|line| line.split(',').collect()).collect();
        write_dta(out_path, &headers, &rows);
    }
}

fn write_dta(out_path: &Path, headers: &[&str], rows: &[Vec<&str>]) {
    if out_path.exists() {
        std::fs::remove_file(out_path)
            .unwrap_or_else(|e| panic!("remove_file {}: {e}", out_path.display()));
    }

    let header = Header::builder(Release::V118, ByteOrder::LittleEndian)
        .dataset_label("binoc test vector")
        .build();
    let mut schema_builder = Schema::builder();
    for name in headers {
        schema_builder = schema_builder.add_variable(
            Variable::builder(VariableType::FixedString(64), *name).label(format!("{name} label")),
        );
    }
    let schema = schema_builder
        .build()
        .unwrap_or_else(|e| panic!("build dta schema: {e}"));

    let mut record_writer = DtaWriter::new()
        .from_path(out_path)
        .unwrap_or_else(|e| panic!("create {}: {e}", out_path.display()))
        .write_header(header)
        .unwrap_or_else(|e| panic!("write dta header {}: {e}", out_path.display()))
        .write_schema(schema)
        .unwrap_or_else(|e| panic!("write dta schema {}: {e}", out_path.display()))
        .into_record_writer()
        .unwrap_or_else(|e| panic!("start dta records {}: {e}", out_path.display()));

    for row in rows {
        let values: Vec<Value<'_>> = row.iter().map(|value| Value::string(value)).collect();
        record_writer
            .write_record(&values)
            .unwrap_or_else(|e| panic!("write dta record {}: {e}", out_path.display()));
    }

    record_writer
        .into_long_string_writer()
        .unwrap_or_else(|e| panic!("finish dta records {}: {e}", out_path.display()))
        .into_value_label_writer()
        .unwrap_or_else(|e| panic!("start dta value labels {}: {e}", out_path.display()))
        .finish()
        .unwrap_or_else(|e| panic!("finish dta {}: {e}", out_path.display()));
}
