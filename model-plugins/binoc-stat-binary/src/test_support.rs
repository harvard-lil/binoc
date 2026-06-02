//! Test-vector helpers for `binoc-stat-binary`.
//!
//! The committed fixture source is a small CSV-like text file in a staging
//! directory such as `.dta.d` or `.xpt.d`. The materializer turns it into a
//! real binary file so the vector exercises the reader without storing opaque
//! binary fixtures when the upstream crate exposes a writer.

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
use sas_xport::sas::xport::{XportMetadata, XportSchema, XportValue, XportVariable, XportWriter};
use sas_xport::sas::SasVariableType;

pub struct DtaMaterializer;
pub struct XptMaterializer;

impl VectorMaterializer for DtaMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".dta.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let fixture = read_csv_fixture(staging_dir);
        write_dta(out_path, &fixture.headers, &fixture.rows);
    }
}

impl VectorMaterializer for XptMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".xpt.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let fixture = read_csv_fixture(staging_dir);
        write_xpt(out_path, &fixture.headers, &fixture.rows);
    }
}

struct CsvFixture {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn read_csv_fixture(staging_dir: &Path) -> CsvFixture {
    let csv_path = staging_dir.join("table.csv");
    let csv = std::fs::read_to_string(&csv_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", csv_path.display()));
    let mut lines = csv.lines();
    let header_line = lines
        .next()
        .unwrap_or_else(|| panic!("{} must contain a header row", csv_path.display()));

    CsvFixture {
        headers: header_line.split(',').map(ToOwned::to_owned).collect(),
        rows: lines
            .map(|line| line.split(',').map(ToOwned::to_owned).collect())
            .collect(),
    }
}

fn write_dta(out_path: &Path, headers: &[String], rows: &[Vec<String>]) {
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
            Variable::builder(VariableType::FixedString(64), name.as_str())
                .label(format!("{name} label")),
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

fn write_xpt(out_path: &Path, headers: &[String], rows: &[Vec<String>]) {
    if out_path.exists() {
        std::fs::remove_file(out_path)
            .unwrap_or_else(|e| panic!("remove_file {}: {e}", out_path.display()));
    }

    let mut schema_builder = XportSchema::builder();
    let mut schema = schema_builder.dataset_name("BINOCTST");
    for (index, header) in headers.iter().enumerate() {
        let width = rows
            .iter()
            .filter_map(|row| row.get(index))
            .map(String::len)
            .max()
            .unwrap_or(1)
            .max(header.len());
        let mut variable = XportVariable::builder();
        variable
            .short_name(header.as_str())
            .value_type(SasVariableType::Character)
            .value_length(width.try_into().expect("xpt field width fits in u16"));
        schema = schema.add_variable(variable);
    }
    let schema = schema
        .try_build()
        .unwrap_or_else(|e| panic!("build xpt schema: {e}"));

    let file = std::fs::File::create(out_path)
        .unwrap_or_else(|e| panic!("create {}: {e}", out_path.display()));
    let writer = XportWriter::from_file(file, XportMetadata::builder().build())
        .unwrap_or_else(|e| panic!("open xpt writer {}: {e}", out_path.display()));
    let mut writer = writer
        .write_schema(schema)
        .unwrap_or_else(|e| panic!("write xpt schema {}: {e}", out_path.display()));

    for row in rows {
        let values: Vec<XportValue<'_>> = row
            .iter()
            .map(|value| XportValue::from(value.as_str()))
            .collect();
        writer
            .write_record(&values)
            .unwrap_or_else(|e| panic!("write xpt record {}: {e}", out_path.display()));
    }

    writer
        .finish()
        .unwrap_or_else(|e| panic!("finish xpt {}: {e}", out_path.display()));
}
