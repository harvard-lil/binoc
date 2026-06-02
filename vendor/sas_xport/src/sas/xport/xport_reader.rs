use super::xport_buffer::XportBuffer;
use super::{
    Result, XportDataset, XportError, XportMetadata, XportReaderOptions, XportReaderOptionsInternal,
};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// The result of opening a SAS® transport file. A valid transport file always
/// contains metadata, but may or may not contain datasets.
#[derive(Debug)]
pub struct XportReader<R> {
    buffer: XportBuffer<R>,
    metadata: XportMetadata,
}

impl<R> XportReader<R> {
    /// Gets the file metadata.
    #[inline]
    #[must_use]
    pub fn metadata(&self) -> &XportMetadata {
        &self.metadata
    }
}

impl XportReader<BufReader<File>> {
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
    pub fn from_file(file: File) -> Result<Self> {
        Self::from_reader(BufReader::new(file))
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
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file =
            File::open(path.as_ref()).map_err(|e| XportError::io("Failed to open the file", e))?;
        Self::from_file(file)
    }

    /// Returns an option builder for opening a SAS® transport file with
    /// custom settings.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let reader = XportReader::options()
    ///     .encoding(encoding_rs::WINDOWS_1252)
    ///     .from_path("data.xpt")?;
    /// ```
    #[inline]
    #[must_use]
    pub fn options() -> XportReaderOptions {
        XportReaderOptions::default()
    }
}

impl<R: BufRead> XportReader<R> {
    /// Opens a SAS® transport file from the given reader.
    ///
    /// To configure encoding or other options, use
    /// [`options()`](XportReader::options) instead.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    #[inline]
    pub fn from_reader(reader: R) -> Result<Self> {
        Self::from_reader_with_options(reader, &XportReaderOptionsInternal::default())
    }

    /// Opens a SAS® transport file from the given reader using the given
    /// options.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    pub(crate) fn from_reader_with_options(
        reader: R,
        options: &XportReaderOptionsInternal,
    ) -> Result<Self> {
        let mut buffer = XportBuffer::from_reader(reader, options);
        let metadata = buffer.read_metadata()?;
        let reader = XportReader { buffer, metadata };
        Ok(reader)
    }

    /// Reads the next dataset schema. If there are no more datasets, `None`
    /// is returned and the reader is consumed.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the schema
    /// * An encoding error occurs while parsing the schema
    pub fn next_dataset(mut self) -> Result<Option<XportDataset<R>>> {
        let Some(schema) = self.buffer.read_schema(self.metadata.file_version())? else {
            return Ok(None);
        };
        let record_offset = self.buffer.position();
        let dataset = XportDataset::new(self.buffer, self.metadata, schema, record_offset);
        Ok(Some(dataset))
    }
}
