use crate::sas::xport::Result;
use crate::sas::xport::XportReader;
use encoding_rs::Encoding;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Options to control the behavior of reading a SAS® transport file.
#[derive(Clone, Debug)]
pub(crate) struct XportReaderOptionsInternal {
    encoding: &'static Encoding,
    fallback_encodings: Vec<&'static Encoding>,
}

impl XportReaderOptionsInternal {
    /// Gets the primary encoding of the file.
    #[must_use]
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// Gets the fallback encodings, in the order they will be attempted.
    #[must_use]
    pub fn fallback_encodings(&self) -> &[&'static Encoding] {
        &self.fallback_encodings
    }
}

impl Default for XportReaderOptionsInternal {
    fn default() -> Self {
        XportReaderOptions::default().build()
    }
}

/// A builder for configuring an `XportReaderOptions`.
#[derive(Clone, Debug)]
pub struct XportReaderOptions {
    encoding: &'static Encoding,
    fallback_encodings: Vec<&'static Encoding>,
}

impl Default for XportReaderOptions {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl XportReaderOptions {
    #[must_use]
    fn new() -> Self {
        Self {
            encoding: encoding_rs::UTF_8,
            fallback_encodings: Vec::new(),
        }
    }

    /// Sets the primary encoding of the file.
    #[inline]
    pub fn encoding(&mut self, encoding: &'static Encoding) -> &mut Self {
        self.encoding = encoding;
        self
    }

    /// Adds a fallback encoding to try when the primary encoding fails.
    /// Fallback encodings are attempted in the order they are added.
    #[inline]
    pub fn add_fallback_encoding(&mut self, encoding: &'static Encoding) -> &mut Self {
        self.fallback_encodings.push(encoding);
        self
    }

    /// Builds an `XportReaderOptions` from the current configuration.
    ///
    /// Fallback encodings that duplicate the primary encoding or that appear
    /// more than once are silently removed to avoid redundant decode attempts.
    #[must_use]
    pub(crate) fn build(&self) -> XportReaderOptionsInternal {
        let mut seen = HashSet::with_capacity(self.fallback_encodings.len() + 1);
        seen.insert(self.encoding);
        let mut fallback_encodings = Vec::with_capacity(self.fallback_encodings.len());
        for encoding in &self.fallback_encodings {
            if seen.insert(*encoding) {
                fallback_encodings.push(*encoding);
            }
        }
        XportReaderOptionsInternal {
            encoding: self.encoding,
            fallback_encodings,
        }
    }

    /// Opens a SAS® transport file using the configured options. The file
    /// will be buffered using the default buffer size.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    //noinspection RsSelfConvention
    #[inline]
    pub fn from_file(&self, file: File) -> Result<XportReader<BufReader<File>>> {
        self.from_reader(BufReader::new(file))
    }

    /// Opens a SAS® transport file from the given reader using the
    /// configured options.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    //noinspection RsSelfConvention
    #[inline]
    pub fn from_reader<R: BufRead>(&self, reader: R) -> Result<XportReader<R>> {
        let options = self.build();
        XportReader::from_reader_with_options(reader, &options)
    }

    /// Opens a SAS® transport file at the given path using the configured
    /// options. The file will be buffered using the default buffer size.
    ///
    /// # Errors
    /// An error is returned if:
    /// * The file cannot be opened
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    //noinspection RsSelfConvention
    pub fn from_path<P: AsRef<Path>>(&self, path: P) -> Result<XportReader<BufReader<File>>> {
        let file = File::open(path.as_ref())
            .map_err(|e| super::XportError::io("Failed to open the file", e))?;
        self.from_file(file)
    }
}

#[cfg(feature = "tokio")]
impl XportReaderOptions {
    /// Opens a SAS® transport file using the configured options. The file
    /// will be buffered using the default buffer size.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    //noinspection RsSelfConvention
    #[inline]
    pub async fn from_tokio_file(
        &self,
        file: tokio::fs::File,
    ) -> Result<super::AsyncXportReader<tokio::io::BufReader<tokio::fs::File>>> {
        self.from_tokio_reader(tokio::io::BufReader::new(file))
            .await
    }

    /// Opens a SAS® transport file from the given reader using the
    /// configured options.
    ///
    /// # Errors
    /// An error is returned if:
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    //noinspection RsSelfConvention
    #[inline]
    pub async fn from_tokio_reader<R: tokio::io::AsyncBufRead + Unpin>(
        &self,
        reader: R,
    ) -> Result<super::AsyncXportReader<R>> {
        let options = self.build();
        super::AsyncXportReader::from_reader_with_options(reader, &options).await
    }

    /// Opens a SAS® transport file at the given path using the configured
    /// options. The file will be buffered using the default buffer size.
    ///
    /// # Errors
    /// An error is returned if:
    /// * The file cannot be opened
    /// * An I/O error occurs while reading the file
    /// * An encoding error occurs while reading metadata or the schema
    //noinspection RsSelfConvention
    pub async fn from_tokio_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<super::AsyncXportReader<tokio::io::BufReader<tokio::fs::File>>> {
        let file = tokio::fs::File::open(path.as_ref())
            .await
            .map_err(|e| super::XportError::io("Failed to open the file", e))?;
        self.from_tokio_file(file).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_encoding_is_utf8() {
        let options = XportReaderOptionsInternal::default();
        assert_eq!(options.encoding(), encoding_rs::UTF_8);
    }

    #[test]
    fn test_default_fallback_encodings_is_empty() {
        let options = XportReaderOptionsInternal::default();
        assert!(options.fallback_encodings().is_empty());
    }

    #[test]
    fn test_builder_default_matches_default() {
        let from_default = XportReaderOptionsInternal::default();
        let from_builder = XportReaderOptions::default().build();
        assert_eq!(from_default.encoding(), from_builder.encoding());
        assert_eq!(
            from_default.fallback_encodings(),
            from_builder.fallback_encodings()
        );
    }

    #[test]
    fn test_set_encoding() {
        let options = XportReaderOptions::default()
            .encoding(encoding_rs::WINDOWS_1252)
            .build();
        assert_eq!(options.encoding(), encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn test_add_fallback_encoding() {
        let options = XportReaderOptions::default()
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .build();
        assert_eq!(options.fallback_encodings(), &[encoding_rs::WINDOWS_1252]);
    }

    #[test]
    fn test_multiple_fallback_encodings_preserve_order() {
        let options = XportReaderOptions::default()
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .add_fallback_encoding(encoding_rs::SHIFT_JIS)
            .build();
        assert_eq!(
            options.fallback_encodings(),
            &[encoding_rs::WINDOWS_1252, encoding_rs::SHIFT_JIS]
        );
    }

    #[test]
    fn test_build_does_not_consume_builder() {
        let mut builder = XportReaderOptions::default();
        builder.encoding(encoding_rs::WINDOWS_1252);
        let first = builder.build();
        let second = builder.build();
        assert_eq!(first.encoding(), second.encoding());
    }

    #[test]
    fn test_duplicate_of_primary_is_dropped() {
        let options = XportReaderOptions::default()
            .encoding(encoding_rs::WINDOWS_1252)
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .build();
        assert!(options.fallback_encodings().is_empty());
    }

    #[test]
    fn test_duplicate_of_default_primary_is_dropped() {
        let options = XportReaderOptions::default()
            .add_fallback_encoding(encoding_rs::UTF_8)
            .build();
        assert!(options.fallback_encodings().is_empty());
    }

    #[test]
    fn test_duplicate_fallbacks_are_deduplicated() {
        let options = XportReaderOptions::default()
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .build();
        assert_eq!(options.fallback_encodings(), &[encoding_rs::WINDOWS_1252]);
    }

    #[test]
    fn test_distinct_fallback_is_preserved() {
        let options = XportReaderOptions::default()
            .encoding(encoding_rs::UTF_8)
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .build();
        assert_eq!(options.fallback_encodings(), &[encoding_rs::WINDOWS_1252]);
    }
}
