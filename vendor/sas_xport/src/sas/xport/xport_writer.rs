use super::xport_constants;
use super::{
    Result, XportError, XportFileVersion, XportMetadata, XportWriterOptions,
    XportWriterOptionsInternal, XportWriterWithMetadata,
};
use crate::sas::xport::xport_writer_state::XportWriterState;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Entry point for writing SAS® transport (XPORT) files. This type provides
/// constructors only and is not instantiated directly.
#[derive(Debug)]
pub struct XportWriter;

impl XportWriter {
    /// Creates a writer backed by a buffered file. The library headers
    /// (V5 or V8, per `metadata.file_version()`) are written immediately.
    ///
    /// Because `BufWriter<File>` implements both `Write` and `Seek`, the
    /// returned writer supports record-count backpatching via the
    /// `set_count_and_*` methods on [`XportWriterWithSchema`](super::XportWriterWithSchema).
    ///
    /// To configure encoding or truncation policies, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    #[inline]
    pub fn from_file(
        file: File,
        metadata: XportMetadata,
    ) -> Result<XportWriterWithMetadata<BufWriter<File>>> {
        Self::from_writer(BufWriter::new(file), metadata)
    }

    /// Creates a writer from any `Write` implementor. The library headers
    /// are written immediately.
    ///
    /// If the writer is unbuffered (e.g., a raw [`File`]), consider wrapping
    /// it in a [`BufWriter`] for better performance. [`from_file`](Self::from_file)
    /// does this automatically.
    ///
    /// To configure encoding or truncation policies, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    #[inline]
    pub fn from_writer<W: Write>(
        writer: W,
        metadata: XportMetadata,
    ) -> Result<XportWriterWithMetadata<W>> {
        Self::from_writer_with_options(writer, metadata, XportWriterOptionsInternal::default())
    }

    /// Creates a writer backed by a buffered file at the given path. The
    /// library headers are written immediately.
    ///
    /// To configure encoding or truncation policies, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// Returns an error if the file cannot be created or if writing the
    /// library headers fails.
    #[inline]
    pub fn from_path<P: AsRef<Path>>(
        path: P,
        metadata: XportMetadata,
    ) -> Result<XportWriterWithMetadata<BufWriter<File>>> {
        let file = File::create(path.as_ref())
            .map_err(|e| XportError::io("Failed to create the file", e))?;
        Self::from_file(file, metadata)
    }

    /// Returns an option builder for writing a SAS® transport file with
    /// custom settings.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let writer = XportWriter::options()
    ///     .encoding(encoding_rs::WINDOWS_1252)
    ///     .from_path("out.xpt", metadata)?;
    /// ```
    #[inline]
    #[must_use]
    pub fn options() -> XportWriterOptions {
        XportWriterOptions::default()
    }

    /// Creates a writer from any `Write` implementor using the given options.
    /// The library headers are written immediately.
    pub(crate) fn from_writer_with_options<W: Write>(
        writer: W,
        metadata: XportMetadata,
        options: XportWriterOptionsInternal,
    ) -> Result<XportWriterWithMetadata<W>> {
        let mut state = XportWriterState::new(options, writer);
        let library_header = match metadata.file_version() {
            XportFileVersion::V5 => xport_constants::LIBRARY_HEADER_V5,
            XportFileVersion::V8 => xport_constants::LIBRARY_HEADER_V8,
        };
        state.write(library_header, "Failed to write the library header")?;
        state.write_str(metadata.symbol1(), 8, "Failed to write the first symbol")?;
        state.write_str(metadata.symbol2(), 8, "Failed to write the second symbol")?;
        state.write_str(metadata.library(), 8, "Failed to write the library")?;
        state.write_str(
            metadata.sas_version(),
            8,
            "Failed to write the file SAS version",
        )?;
        state.write_str(
            metadata.operating_system(),
            8,
            "Failed to write the file operating system",
        )?;
        state.write_padding(b' ', 24, "Failed to write 24 bytes of padding")?;
        state.write_date_time(
            metadata.created(),
            "Failed to write the file creation date/time",
        )?;
        state.write_date_time(
            metadata.modified(),
            "Failed to write the file last modified date/time",
        )?;
        state.write_padding(b' ', 64, "Failed to write 64 bytes of padding")?;

        Ok(XportWriterWithMetadata::new(state, metadata))
    }
}
