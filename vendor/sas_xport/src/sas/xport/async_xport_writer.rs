use super::xport_constants;
use super::{
    Result, XportError, XportFileVersion, XportMetadata, XportWriterOptions,
    XportWriterOptionsInternal,
};
use crate::sas::xport::async_xport_writer_state::AsyncXportWriterState;
use crate::sas::xport::async_xport_writer_with_metadata::AsyncXportWriterWithMetadata;
use std::path::Path;
use tokio::io::{AsyncWrite, BufWriter};

/// Entry point for writing SAS® transport (XPORT) files asynchronously.
/// This type provides constructors only and is not instantiated directly.
#[derive(Debug)]
pub struct AsyncXportWriter;

impl AsyncXportWriter {
    /// Creates a writer backed by a buffered async file. The library headers
    /// (V5 or V8, per `metadata.file_version()`) are written immediately.
    ///
    /// To configure encoding or truncation policies, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    #[inline]
    pub async fn from_file(
        file: tokio::fs::File,
        metadata: XportMetadata,
    ) -> Result<AsyncXportWriterWithMetadata<BufWriter<tokio::fs::File>>> {
        Self::from_writer(BufWriter::new(file), metadata).await
    }

    /// Creates a writer from any `AsyncWrite` implementor. The library headers
    /// are written immediately.
    ///
    /// If the writer is unbuffered (e.g., a raw [`tokio::fs::File`]), consider
    /// wrapping it in a [`tokio::io::BufWriter`] for better performance.
    /// [`from_file`](Self::from_file) does this automatically.
    ///
    /// To configure encoding or truncation policies, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    #[inline]
    pub async fn from_writer<W: AsyncWrite + Unpin>(
        writer: W,
        metadata: XportMetadata,
    ) -> Result<AsyncXportWriterWithMetadata<W>> {
        Self::from_writer_with_options(writer, metadata, XportWriterOptionsInternal::default())
            .await
    }

    /// Creates a writer backed by a buffered async file at the given path.
    /// The library headers are written immediately.
    ///
    /// To configure encoding or truncation policies, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// Returns an error if the file cannot be created or if writing the
    /// library headers fails.
    pub async fn from_path<P: AsRef<Path>>(
        path: P,
        metadata: XportMetadata,
    ) -> Result<AsyncXportWriterWithMetadata<BufWriter<tokio::fs::File>>> {
        let file = tokio::fs::File::create(path.as_ref())
            .await
            .map_err(|e| XportError::io("Failed to create the file", e))?;
        Self::from_file(file, metadata).await
    }

    /// Returns an option builder for writing a SAS® transport file
    /// asynchronously with custom settings.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let writer = AsyncXportWriter::options()
    ///     .encoding(encoding_rs::WINDOWS_1252)
    ///     .from_tokio_path("out.xpt", metadata).await?;
    /// ```
    #[inline]
    #[must_use]
    pub fn options() -> XportWriterOptions {
        XportWriterOptions::default()
    }

    /// Creates a writer from any `AsyncWrite` implementor using the given
    /// options. The library headers are written immediately.
    pub(crate) async fn from_writer_with_options<W: AsyncWrite + Unpin>(
        writer: W,
        metadata: XportMetadata,
        options: XportWriterOptionsInternal,
    ) -> Result<AsyncXportWriterWithMetadata<W>> {
        let mut state = AsyncXportWriterState::new(options, writer);
        let library_header = match metadata.file_version() {
            XportFileVersion::V5 => xport_constants::LIBRARY_HEADER_V5,
            XportFileVersion::V8 => xport_constants::LIBRARY_HEADER_V8,
        };
        state
            .write(library_header, "Failed to write the library header")
            .await?;
        state
            .write_str(metadata.symbol1(), 8, "Failed to write the first symbol")
            .await?;
        state
            .write_str(metadata.symbol2(), 8, "Failed to write the second symbol")
            .await?;
        state
            .write_str(metadata.library(), 8, "Failed to write the library")
            .await?;
        state
            .write_str(
                metadata.sas_version(),
                8,
                "Failed to write the file SAS version",
            )
            .await?;
        state
            .write_str(
                metadata.operating_system(),
                8,
                "Failed to write the file operating system",
            )
            .await?;
        state
            .write_padding(b' ', 24, "Failed to write 24 bytes of padding")
            .await?;
        state
            .write_date_time(
                metadata.created(),
                "Failed to write the file creation date/time",
            )
            .await?;
        state
            .write_date_time(
                metadata.modified(),
                "Failed to write the file last modified date/time",
            )
            .await?;
        state
            .write_padding(b' ', 64, "Failed to write 64 bytes of padding")
            .await?;

        Ok(AsyncXportWriterWithMetadata::new(state, metadata))
    }
}
