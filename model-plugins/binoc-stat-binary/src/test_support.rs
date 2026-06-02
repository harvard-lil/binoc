//! Test-vector helpers for `binoc-stat-binary`.
//!
//! The committed fixture source is a small CSV-like text file in a staging
//! directory such as `.dta.d`, `.sas7bdat.d`, or `.xpt.d`. The materializer
//! turns it into a real binary file so the vector exercises the reader without
//! storing opaque binary fixtures in git.

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
pub struct Sas7bdatMaterializer;
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

impl VectorMaterializer for Sas7bdatMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".sas7bdat.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let fixture = read_csv_fixture(staging_dir);
        write_sas7bdat(out_path, &fixture.headers, &fixture.rows);
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

fn write_sas7bdat(out_path: &Path, headers: &[String], rows: &[Vec<String>]) {
    const HEADER_SIZE: usize = 1024;
    const PAGE_SIZE: usize = 1024;
    const PAGE_COUNT: u32 = 2;
    const PAGE_HEADER_SIZE: usize = 24;
    const POINTER_SIZE: usize = 12;
    const SIG_COLUMN_TEXT: u32 = 0xFFFF_FFFD;
    const SIG_COLUMN_NAME: u32 = 0xFFFF_FFFF;
    const SIG_COLUMN_ATTRS: u32 = 0xFFFF_FFFC;
    const SIG_COLUMN_SIZE: u32 = 0xF6F6_F6F6;
    const SIG_ROW_SIZE: u32 = 0xF7F7_F7F7;

    if out_path.exists() {
        std::fs::remove_file(out_path)
            .unwrap_or_else(|e| panic!("remove_file {}: {e}", out_path.display()));
    }

    let widths = column_widths(headers, rows);
    let row_length: u32 = widths.iter().copied().sum();
    let row_count: u32 = rows.len().try_into().expect("row count fits in u32");

    let name_offsets = name_offsets(headers);
    let text_blob = build_text_blob(headers);
    let subheaders = [
        build_column_text_subheader(SIG_COLUMN_TEXT, &text_blob),
        build_column_name_subheader(SIG_COLUMN_NAME, &name_offsets, headers),
        build_column_attrs_subheader(SIG_COLUMN_ATTRS, &widths),
        build_column_size_subheader(SIG_COLUMN_SIZE, headers.len()),
        build_row_size_subheader(SIG_ROW_SIZE, row_length, row_count),
    ];

    let mut metadata_page = vec![0u8; PAGE_SIZE];
    metadata_page[PAGE_HEADER_SIZE - 4..PAGE_HEADER_SIZE - 2]
        .copy_from_slice(&(subheaders.len() as u16).to_le_bytes());

    let mut cursor = PAGE_HEADER_SIZE + POINTER_SIZE * subheaders.len();
    for (index, subheader) in subheaders.iter().enumerate() {
        let pointer_start = PAGE_HEADER_SIZE + index * POINTER_SIZE;
        metadata_page[pointer_start..pointer_start + 4]
            .copy_from_slice(&(cursor as u32).to_le_bytes());
        metadata_page[pointer_start + 4..pointer_start + 8]
            .copy_from_slice(&(subheader.len() as u32).to_le_bytes());
        metadata_page[cursor..cursor + subheader.len()].copy_from_slice(subheader);
        cursor += subheader.len();
    }
    assert!(cursor <= PAGE_SIZE, "metadata page overflow");

    let mut data_page = vec![0u8; PAGE_SIZE];
    data_page[PAGE_HEADER_SIZE - 8..PAGE_HEADER_SIZE - 6].copy_from_slice(&0x0100u16.to_le_bytes());
    data_page[PAGE_HEADER_SIZE - 6..PAGE_HEADER_SIZE - 4]
        .copy_from_slice(&row_count.to_le_bytes()[..2]);

    let mut row_cursor = PAGE_HEADER_SIZE;
    for row in rows {
        let mut value_cursor = row_cursor;
        for ((value, width), header) in row.iter().zip(widths.iter()).zip(headers.iter()) {
            let width = *width as usize;
            let bytes = value.as_bytes();
            assert!(
                bytes.len() <= width,
                "value {:?} exceeds width {} for column {}",
                value,
                width,
                header
            );
            data_page[value_cursor..value_cursor + width].fill(b' ');
            data_page[value_cursor..value_cursor + bytes.len()].copy_from_slice(bytes);
            value_cursor += width;
        }
        row_cursor += row_length as usize;
    }
    assert!(row_cursor <= PAGE_SIZE, "data page overflow");

    let mut header = vec![0u8; HEADER_SIZE];
    header[..32].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC2, 0xEA, 0x81,
        0x60, 0xB3, 0x14, 0x11, 0xCF, 0xBD, 0x92, 0x08, 0x00, 0x09, 0xC7, 0x31, 0x8C, 0x18, 0x1F,
        0x10, 0x11,
    ]);
    header[35] = 0x00;
    header[37] = 0x01;
    header[39] = 0x00;
    header[70] = 20;
    header[84..92].copy_from_slice(b"SASDATA ");
    write_padded(&mut header[92..124], "BINOCTST");
    header[196..200].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    header[200..204].copy_from_slice(&(PAGE_SIZE as u32).to_le_bytes());
    header[204..208].copy_from_slice(&PAGE_COUNT.to_le_bytes());
    header[216..224].copy_from_slice(b"9.0401M3");

    let mut bytes = header;
    bytes.extend_from_slice(&metadata_page);
    bytes.extend_from_slice(&data_page);

    std::fs::write(out_path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
}

fn column_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<u32> {
    headers
        .iter()
        .enumerate()
        .map(|(index, _)| {
            rows.iter()
                .filter_map(|row| row.get(index))
                .map(|value| value.len())
                .max()
                .unwrap_or(1)
                .max(1)
                .try_into()
                .expect("column width fits in u32")
        })
        .collect()
}

fn name_offsets(headers: &[String]) -> Vec<u16> {
    let mut offsets = Vec::with_capacity(headers.len());
    let mut cursor: u16 = 2;
    for name in headers {
        offsets.push(cursor);
        cursor = cursor
            .checked_add(name.len().try_into().expect("name length fits in u16"))
            .expect("blob offset fits in u16");
    }
    offsets
}

fn build_text_blob(headers: &[String]) -> Vec<u8> {
    let mut blob = Vec::new();
    for name in headers {
        blob.extend_from_slice(name.as_bytes());
    }
    blob
}

fn build_column_text_subheader(signature: u32, text_blob: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + 2 + text_blob.len());
    bytes.extend_from_slice(&signature.to_le_bytes());
    let remainder: u16 = text_blob
        .len()
        .checked_sub(6)
        .expect("text blob must satisfy parser minimum")
        .try_into()
        .expect("remainder fits in u16");
    bytes.extend_from_slice(&remainder.to_le_bytes());
    bytes.extend_from_slice(text_blob);
    bytes
}

fn build_column_name_subheader(signature: u32, offsets: &[u16], headers: &[String]) -> Vec<u8> {
    let mut bytes = vec![0u8; 20 + 8 * offsets.len()];
    bytes[..4].copy_from_slice(&signature.to_le_bytes());
    let remainder: u16 = (bytes.len() - 12)
        .try_into()
        .expect("remainder fits in u16");
    bytes[4..6].copy_from_slice(&remainder.to_le_bytes());

    let mut cursor = 12;
    for (offset, header) in offsets.iter().zip(headers.iter()) {
        bytes[cursor..cursor + 2].copy_from_slice(&0u16.to_le_bytes());
        bytes[cursor + 2..cursor + 4].copy_from_slice(&offset.to_le_bytes());
        bytes[cursor + 4..cursor + 6].copy_from_slice(
            &u16::try_from(header.len())
                .expect("header length fits in u16")
                .to_le_bytes(),
        );
        cursor += 8;
    }
    bytes
}

fn build_column_attrs_subheader(signature: u32, widths: &[u32]) -> Vec<u8> {
    let mut bytes = vec![0u8; 20 + 12 * widths.len()];
    bytes[..4].copy_from_slice(&signature.to_le_bytes());
    let remainder: u16 = (bytes.len() - 12)
        .try_into()
        .expect("remainder fits in u16");
    bytes[4..6].copy_from_slice(&remainder.to_le_bytes());

    let mut cursor = 12;
    let mut offset = 0u32;
    for width in widths {
        bytes[cursor..cursor + 4].copy_from_slice(&offset.to_le_bytes());
        bytes[cursor + 4..cursor + 8].copy_from_slice(&width.to_le_bytes());
        bytes[cursor + 10] = 0x02;
        offset = offset.checked_add(*width).expect("row offset fits in u32");
        cursor += 12;
    }
    bytes
}

fn build_column_size_subheader(signature: u32, column_count: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; 8];
    bytes[..4].copy_from_slice(&signature.to_le_bytes());
    bytes[4..8].copy_from_slice(
        &u32::try_from(column_count)
            .expect("column count fits in u32")
            .to_le_bytes(),
    );
    bytes
}

fn build_row_size_subheader(signature: u32, row_length: u32, row_count: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 190];
    bytes[..4].copy_from_slice(&signature.to_le_bytes());
    bytes[20..24].copy_from_slice(&row_length.to_le_bytes());
    bytes[24..28].copy_from_slice(&row_count.to_le_bytes());
    bytes[60..64].copy_from_slice(&row_count.to_le_bytes());
    bytes
}

fn write_padded(slot: &mut [u8], value: &str) {
    slot.fill(b' ');
    let bytes = value.as_bytes();
    slot[..bytes.len()].copy_from_slice(bytes);
}
