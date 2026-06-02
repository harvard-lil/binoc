# sas_xport

A Rust library for reading and writing SAS® Transport (XPORT) files, supporting both V5 and V8 formats (V8 is also used in SAS® V9+).

## Features

- Read file-level metadata (SAS version, OS, creation date)
- Read dataset schemas with full variable definitions (name, type, length, label, format)
- Three record-reading APIs:
  - **Iterator API** (`records()`) — fully owned values, simple to use
  - **Borrowing API** (`next_record()`) — returns values that borrow from an internal buffer, avoiding per-record string allocations
  - **Lazy API** (`next_lazy_record()`) — returns a `LazyXportRecord` that decodes values on demand, avoiding both the `Vec<XportValue>` allocation and any decoding work for fields you don't access
- Write XPORT files with the `XportWriter` typestate API
- Optional async support via the [`tokio` feature](#tokio--async-io)
- Optional date/time conversions via the [`chrono` feature](#chrono--datetime-conversions)

## Usage

### Using the record iterator

```rust
use std::fs::File;
use sas_xport::sas::xport::{XportReader, Result};

pub fn read_xport_file(file: File) -> Result<()> {
    let reader = XportReader::from_file(file)?;
    let metadata = reader.metadata().clone();
    let Some(mut dataset) = reader.next_dataset()? else {
        println!("File contains no datasets");
        return Ok(());
    };
    loop {
        let schema = dataset.schema();
        println!("Dataset: {} (SAS® {})", schema.dataset_name(), metadata.sas_version());
        println!("Variable count: {}", schema.variables().len());

        for record in dataset.records() {
            let _record = record?;
        }
        println!("Record count: {}", dataset.record_number());
        let Some(next) = dataset.next_dataset()? else {
            break;
        };
        dataset = next;
    }
    Ok(())
}
```

### Using `next_record` for borrowing access

```rust
use std::fs::File;
use sas_xport::sas::xport::{XportReader, Result};

pub fn read_xport_file(file: File) -> Result<()> {
    let reader = XportReader::from_file(file)?;
    let Some(mut dataset) = reader.next_dataset()? else {
        println!("File contains no datasets");
        return Ok(());
    };
    loop {
        println!("Dataset: {}", dataset.schema().dataset_name());

        while let Some(record) = dataset.next_record()? {
            // Values in `record` borrow from the dataset's internal
            // buffer and are invalidated on the next call.
        }
        println!("Record count: {}", dataset.record_number());
        let Some(next) = dataset.next_dataset()? else {
            break;
        };
        dataset = next;
    }
    Ok(())
}
```

### Using `next_lazy_record` for lazy decoding

```rust
use std::fs::File;
use sas_xport::sas::xport::{XportReader, Result};

pub fn read_xport_file(file: File) -> Result<()> {
    let reader = XportReader::from_file(file)?;
    let Some(mut dataset) = reader.next_dataset()? else {
        println!("File contains no datasets");
        return Ok(());
    };
    loop {
        let schema = dataset.schema();
        let name_index = schema.variable_ordinal("NAME").unwrap();
        println!("Dataset: {}", schema.dataset_name());

        while let Some(record) = dataset.next_lazy_record()? {
            // Values are decoded on demand — only pay for the
            // fields you access.
            let name = record.get(name_index).unwrap()?;
            println!("Name: {name:?}");
        }
        println!("Record count: {}", dataset.record_number());
        let Some(next) = dataset.next_dataset()? else {
            break;
        };
        dataset = next;
    }
    Ok(())
}
```

### Writing records

```rust
use std::fs::File;
use sas_xport::sas::SasVariableType;
use sas_xport::sas::xport::{
    Result, XportMetadata, XportSchema, XportValue, XportVariable, XportWriter,
};

pub fn write_xport_file(file: File) -> Result<()> {
    let metadata = XportMetadata::builder().build();

    let mut study_id = XportVariable::builder();
    study_id
        .short_name("STUDYID")
        .value_type(SasVariableType::Character)
        .value_length(20);

    let mut age = XportVariable::builder();
    age.short_name("AGE")
        .value_type(SasVariableType::Numeric)
        .value_length(8);

    let schema = XportSchema::builder()
        .dataset_name("DM")
        .variable(study_id)
        .variable(age)
        .try_build()?;

    let writer = XportWriter::from_file(file, metadata)?;
    let mut writer = writer.write_schema(schema)?;
    writer.write_record(&[XportValue::from("STUDY-001"), XportValue::from(35.0)])?;
    writer.write_record(&[XportValue::from("STUDY-001"), XportValue::from(42.5)])?;
    writer.finish()?; // Discards the returned inner writer
    Ok(())
}
```

## Feature Flags

Both optional features are disabled by default. Enable them in your `Cargo.toml` as needed:

```toml
[dependencies]
sas_xport = { version = "0.2", features = ["tokio", "chrono"] }
```

### `tokio` — Async I/O

Enables async reader and writer types built on [Tokio](https://tokio.rs/):

- `AsyncXportReader` — async counterpart of `XportReader`, with `from_file()`, `from_reader()`, and `next_dataset()`
- `AsyncXportDataset` — read records asynchronously with `next_record()`
- `AsyncXportWriter` / `AsyncXportWriterWithMetadata` / `AsyncXportWriterWithSchema` — async typestate writer matching the sync API

```rust
use tokio::fs::File;
use sas_xport::sas::xport::{AsyncXportReader, Result};

async fn read_async(file: File) -> Result<()> {
    let reader = AsyncXportReader::from_file(file).await?;
    let Some(mut dataset) = reader.next_dataset().await? else {
        return Ok(());
    };
    while let Some(record) = dataset.next_record().await? {
        // process record
    }
    Ok(())
}
```

### `chrono` — Date/Time Conversions

Enables conversions between `SasDateTime` and [`chrono::DateTime<Local>`](https://docs.rs/chrono/latest/chrono/):

- `SasDateTime::now()` — creates a `SasDateTime` set to the current local time
- `SasDateTime::to_chrono_date_time(base_year)` — converts to `DateTime<Local>`, using `base_year` (e.g., 1900 or 2000) to resolve the two-digit SAS year
- `impl From<DateTime<Local>> for SasDateTime` — direct conversion from chrono into SAS format

```rust
use chrono::Local;
use sas_xport::sas::SasDateTime;

let now = SasDateTime::now();
let from_chrono: SasDateTime = Local::now().into();
let back = now.to_chrono_date_time(2000);
```

## About

In my recent line of work, I became very familiar with the SAS® Transport (XPORT) file format. I took a personal interest in Rust about 5 years ago and decided I needed an engaging side project to help me learn it better. That's how this project was born.

Rust still has a young ecosystem, and a great deal of statistical software requires working with SAS® XPORT files, especially in the clinical industry. I wanted to provide a pure Rust implementation. My goal was to make a flexible reader and writer that operated at breakneck speeds. I modeled it after other wonderful libraries like [BurntSushi/csv](https://github.com/BurntSushi/rust-csv) and [tafia/quick-xml](https://github.com/tafia/quick-xml).

This is a passion project that I maintain on my own time. I care deeply about its quality and want it to be genuinely useful, but I also want to keep it fun and sustainable. To that end:

- **Bug reports** are always welcome. Please file issues for anything that isn't working correctly.
- **Feature requests** are best expressed as pull requests. I'm much more likely to engage with a well-crafted PR than a request for new work.
- **Timelines** are my own. I'll get to things when I can, and I may close issues or PRs that don't align with the project's direction — nothing personal.

If you find this library valuable, the best way to support it is to contribute or share it with others.

## Benchmarks

The benchmark suite measures record-reading throughput using a synthetic 100 thousand-record XPT V5 file with 20 variables (8 numeric, 12 character). The test file is auto-generated on the first run.

Three benchmarks are included:

| Benchmark | Description |
|---|---|
| `read_all_zerocopy` | `next_record()` API reading from disk |
| `read_all_iterator` | `records()` iterator reading from disk |
| `read_all_cursor` | `next_record()` from an in-memory cursor (isolates parsing from I/O) |

### Running benchmarks

Run all benchmarks:

```sh
cargo bench -p sas_xport
```

Run a specific benchmark by name:

```sh
cargo bench -p sas_xport -- read_all_cursor
```

After the first run, Criterion saves baseline results in `target/criterion/`. Subsequent runs compare against the baseline and report regressions or improvements. To update the baseline:

```sh
cargo bench -p sas_xport -- --save-baseline main
```

HTML reports are generated in `target/criterion/report/index.html`.

## Profiling

One-time setup: `cargo install --locked samply`

### Wall-clock summary

Generates a synthetic file with the writer and reads it back, printing throughput for each phase:

```sh
cargo run --example profile --profile profiling -p sas_xport
cargo run --example profile --profile profiling -p sas_xport -- --records 500000 --phase write
```

### Function-level profiling

Uses `samply` to sample both the write and read phases, then reports the top functions from this crate by inclusive and self-time percentage:

```sh
# Linux / macOS
./profile.sh
./profile.sh --records 500000 --top 20

# Windows (PowerShell)
.\profile.ps1
.\profile.ps1 -Records 500000 -Top 20
```

## Code Coverage

One-time setup: `cargo install cargo-llvm-cov`

Generate a coverage summary:

```sh
cargo llvm-cov --package sas_xport --all-features
```

Generate an HTML report:

```sh
cargo llvm-cov --package sas_xport --all-features --html
open target/llvm-cov/html/index.html
```