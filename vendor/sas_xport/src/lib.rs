//! Provides the ability to read and write SAS® Transport (XPORT) files, including version 5
//! and 8 file formats. V8 is also used in SAS® V9+.
//!
//! [`XportReader`](sas::xport::XportReader) is the entry point for reading an XPORT
//! file, and [`XportWriter`](sas::xport::XportWriter) is the entry point for writing one.
//! An XPORT file consists of zero, one or more datasets, each with their own schema.
//! The XPORT file also provides metadata about the file itself.
//!
//! # Reading records
//!
//! There are three ways to read records from a dataset:
//!
//! - **[`records()`](sas::xport::XportDataset::records)** returns an iterator that yields
//!   fully owned values. This is the simplest approach but allocates a new `String` for each
//!   character value in every record.
//! - **[`next_record()`](sas::xport::XportDataset::next_record)** returns an
//!   [`XportRecord`](sas::xport::XportRecord) whose values borrow from the dataset's
//!   internal buffer. This avoids per-record string allocations for character data when
//!   the encoding is ASCII-compatible, making it better suited for high-throughput processing.
//! - **[`next_lazy_record()`](sas::xport::XportDataset::next_lazy_record)** returns a
//!   [`LazyXportRecord`](sas::xport::LazyXportRecord) that decodes values on demand.
//!   This avoids both the `Vec<XportValue>` allocation and any decoding work for
//!   fields you don't access, making it ideal when you only need a subset of columns.
//!
//! ## Using the record iterator
//!
//! ```rust
//! use std::fs::File;
//! use sas_xport::sas::xport::{XportReader, Result};
//!
//! pub fn read_xport_file(file: File) -> Result<()> {
//!     let reader = XportReader::from_file(file)?;
//!     let metadata = reader.metadata().clone();
//!     let Some(mut dataset) = reader.next_dataset()? else {
//!         println!("File contains no datasets");
//!         return Ok(());
//!     };
//!     loop {
//!         let schema = dataset.schema();
//!         println!("Dataset: {} (SAS® {})", schema.dataset_name(), metadata.sas_version());
//!         println!("Variable count: {}", schema.variables().len());
//!
//!         for record in dataset.records() {
//!             let _record = record?;
//!         }
//!         println!("Record count: {}", dataset.record_number());
//!         let Some(next) = dataset.next_dataset()? else {
//!             break;
//!         };
//!         dataset = next;
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Using `next_record` for borrowing access
//!
//! ```rust
//! use std::fs::File;
//! use sas_xport::sas::xport::{XportReader, Result};
//!
//! pub fn read_xport_file(file: File) -> Result<()> {
//!     let reader = XportReader::from_file(file)?;
//!     let Some(mut dataset) = reader.next_dataset()? else {
//!         println!("File contains no datasets");
//!         return Ok(());
//!     };
//!     loop {
//!         println!("Dataset: {}", dataset.schema().dataset_name());
//!
//!         while let Some(record) = dataset.next_record()? {
//!             // Values in `record` borrow from the dataset's internal
//!             // buffer and are invalidated on the next call.
//!         }
//!         println!("Record count: {}", dataset.record_number());
//!         let Some(next) = dataset.next_dataset()? else {
//!             break;
//!         };
//!         dataset = next;
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Using `next_lazy_record` for lazy decoding
//!
//! ```rust
//! use std::fs::File;
//! use sas_xport::sas::xport::{XportReader, Result};
//!
//! pub fn read_xport_file(file: File) -> Result<()> {
//!     let reader = XportReader::from_file(file)?;
//!     let Some(mut dataset) = reader.next_dataset()? else {
//!         println!("File contains no datasets");
//!         return Ok(());
//!     };
//!     loop {
//!         let schema = dataset.schema();
//!         let name_index = schema.variable_ordinal("NAME").unwrap();
//!         println!("Dataset: {}", schema.dataset_name());
//!
//!         while let Some(record) = dataset.next_lazy_record()? {
//!             // Values are decoded on demand — only pay for the
//!             // fields you access.
//!             let name = record.get(name_index).unwrap()?;
//!             println!("Name: {name:?}");
//!         }
//!         println!("Record count: {}", dataset.record_number());
//!         let Some(next) = dataset.next_dataset()? else {
//!             break;
//!         };
//!         dataset = next;
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Writing records
//!
//! [`XportWriter`](sas::xport::XportWriter) is the entry point for writing an XPORT file.
//! Build an [`XportMetadata`](sas::xport::XportMetadata) for file-level headers, then write
//! one or more datasets, each with an [`XportSchema`](sas::xport::XportSchema) and records
//! as slices of [`XportValue`](sas::xport::XportValue).
//!
//! ```rust
//! use std::fs::File;
//! use sas_xport::sas::SasVariableType;
//! use sas_xport::sas::xport::{
//!     Result, XportMetadata, XportSchema, XportValue, XportVariable, XportWriter,
//! };
//!
//! pub fn write_xport_file(file: File) -> Result<()> {
//!     let metadata = XportMetadata::builder().build();
//!
//!     let mut study_id = XportVariable::builder();
//!     study_id
//!         .short_name("STUDYID")
//!         .value_type(SasVariableType::Character)
//!         .value_length(20);
//!
//!     let mut age = XportVariable::builder();
//!     age.short_name("AGE")
//!         .value_type(SasVariableType::Numeric)
//!         .value_length(8);
//!
//!     let schema = XportSchema::builder()
//!         .dataset_name("DM")
//!         .add_variable(study_id) // Pass the builders
//!         .add_variable(age)
//!         .try_build()?;
//!
//!     let writer = XportWriter::from_file(file, metadata)?;
//!     let mut writer = writer.write_schema(schema)?;
//!     writer.write_record(&[XportValue::from("STUDY-001"), XportValue::from(35.0)])?;
//!     writer.write_record(&[XportValue::from("STUDY-001"), XportValue::from(42.5)])?;
//!     writer.finish()?; // Discards the returned inner writer
//!     Ok(())
//! }
//! ```
//!
//! # Feature Flags
//!
//! Both features are disabled by default.
//!
//! | Feature  | Description |
//! |----------|-------------|
//! | `tokio`  | Async reader/writer types: [`AsyncXportReader`](sas::xport::AsyncXportReader), [`AsyncXportDataset`](sas::xport::AsyncXportDataset), [`AsyncXportWriter`](sas::xport::AsyncXportWriter), and related typestate types. |
//! | `chrono` | Conversions between [`SasDateTime`](sas::SasDateTime) and [`chrono::DateTime<Local>`](chrono::DateTime), including [`SasDateTime::now()`](sas::SasDateTime::now) and [`SasDateTime::to_chrono_date_time()`](sas::SasDateTime::to_chrono_date_time). |

#![warn(missing_docs)]

pub mod sas;
