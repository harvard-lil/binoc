use super::async_xport_buffer::AsyncXportBuffer;
use super::{
    AsyncXportDataset, Result, XportError, XportMetadata, XportReaderOptions,
    XportReaderOptionsInternal,
};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncBufRead, BufReader};

/// The result of opening a SAS® transport file asynchronously. A valid
/// transport file always contains metadata, but may or may not contain datasets.
#[derive(Debug)]
pub struct AsyncXportReader<R> {
    buffer: AsyncXportBuffer<R>,
    metadata: XportMetadata,
}

impl<R> AsyncXportReader<R> {
    /// Gets the file metadata.
    #[inline]
    #[must_use]
    pub fn metadata(&self) -> &XportMetadata {
        &self.metadata
    }
}

impl AsyncXportReader<BufReader<File>> {
    /// Opens a SAS® transport file. The file will be buffered using the
    /// default buffer size.
    ///
    /// To configure encoding or other options, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    #[inline]
    pub async fn from_file(file: File) -> Result<Self> {
        Self::from_reader(BufReader::new(file)).await
    }

    /// Opens a SAS® transport file at the given path. The file will be
    /// buffered using the default buffer size.
    ///
    /// To configure encoding or other options, use
    /// [`options()`](Self::options) instead.
    ///
    /// # Errors
    /// An error is returned if:
    /// * The file cannot be opened
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    pub async fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .await
            .map_err(|e| XportError::io("Failed to open the file", e))?;
        Self::from_file(file).await
    }

    /// Returns an option builder for opening a SAS® transport file with
    /// custom settings.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let reader = AsyncXportReader::options()
    ///     .encoding(encoding_rs::WINDOWS_1252)
    ///     .from_tokio_path("data.xpt").await?;
    /// ```
    #[inline]
    #[must_use]
    pub fn options() -> XportReaderOptions {
        XportReaderOptions::default()
    }
}

impl<R: AsyncBufRead + Unpin> AsyncXportReader<R> {
    /// Opens a SAS® transport file from the given reader.
    ///
    /// To configure encoding or other options, use
    /// [`options()`](AsyncXportReader::options) instead.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    #[inline]
    pub async fn from_reader(reader: R) -> Result<Self> {
        Self::from_reader_with_options(reader, &XportReaderOptionsInternal::default()).await
    }

    /// Opens a SAS® transport file from the given reader using the given
    /// options.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    pub(crate) async fn from_reader_with_options(
        reader: R,
        options: &XportReaderOptionsInternal,
    ) -> Result<Self> {
        let mut buffer = AsyncXportBuffer::from_reader(reader, options);
        let metadata = buffer.read_metadata().await?;
        let reader = AsyncXportReader { buffer, metadata };
        Ok(reader)
    }

    /// Reads the next dataset schema. If there are no more datasets, `None`
    /// is returned and the reader is consumed.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the schema
    /// * An encoding error occurs while parsing the schema
    pub async fn next_dataset(mut self) -> Result<Option<AsyncXportDataset<R>>> {
        let Some(schema) = self
            .buffer
            .read_schema(self.metadata.file_version())
            .await?
        else {
            return Ok(None);
        };
        let record_offset = self.buffer.position();
        let dataset = AsyncXportDataset::new(self.buffer, self.metadata, schema, record_offset);
        Ok(Some(dataset))
    }
}
