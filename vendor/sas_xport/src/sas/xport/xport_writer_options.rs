use super::{Result, XportError, XportMetadata, XportWriter, XportWriterWithMetadata};
use crate::sas::SasVariableType;
use encoding_rs::Encoding;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::TruncationPolicy;

/// Options to control the behavior of writing a SAS® transport file.
#[derive(Clone, Debug)]
pub(crate) struct XportWriterOptionsInternal {
    encoding: &'static Encoding,
    character_truncation_policy: TruncationPolicy,
    numeric_truncation_policy: TruncationPolicy,
}

impl XportWriterOptionsInternal {
    /// Gets the encoding used for writing character values.
    #[must_use]
    pub(crate) fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// Gets the truncation policy for the given variable type.
    #[must_use]
    pub(crate) fn truncation_policy(&self, variable_type: SasVariableType) -> TruncationPolicy {
        match variable_type {
            SasVariableType::Character => self.character_truncation_policy,
            SasVariableType::Numeric => self.numeric_truncation_policy,
        }
    }
}

impl Default for XportWriterOptionsInternal {
    fn default() -> Self {
        XportWriterOptions::default().build()
    }
}

/// A builder for configuring `XportWriterOptions`.
#[derive(Clone, Debug)]
pub struct XportWriterOptions {
    encoding: &'static Encoding,
    character_truncation_policy: TruncationPolicy,
    numeric_truncation_policy: TruncationPolicy,
}

impl Default for XportWriterOptions {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl XportWriterOptions {
    #[must_use]
    fn new() -> Self {
        Self {
            encoding: encoding_rs::UTF_8,
            character_truncation_policy: TruncationPolicy::Silent,
            numeric_truncation_policy: TruncationPolicy::Silent,
        }
    }

    /// Sets the encoding used for writing character values.
    #[inline]
    pub fn encoding(&mut self, encoding: &'static Encoding) -> &mut Self {
        self.encoding = encoding;
        self
    }

    /// Sets the truncation policy for the given variable type.
    #[inline]
    pub fn truncation_policy(
        &mut self,
        variable_type: SasVariableType,
        policy: TruncationPolicy,
    ) -> &mut Self {
        match variable_type {
            SasVariableType::Character => self.character_truncation_policy = policy,
            SasVariableType::Numeric => self.numeric_truncation_policy = policy,
        }
        self
    }

    /// Builds an `XportWriterOptions` from the current configuration.
    #[must_use]
    pub(crate) fn build(&self) -> XportWriterOptionsInternal {
        XportWriterOptionsInternal {
            encoding: self.encoding,
            character_truncation_policy: self.character_truncation_policy,
            numeric_truncation_policy: self.numeric_truncation_policy,
        }
    }

    /// Creates a writer backed by a buffered file using the configured
    /// options. The library headers are written immediately.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    //noinspection RsSelfConvention
    #[inline]
    pub fn from_file(
        &self,
        file: File,
        metadata: XportMetadata,
    ) -> Result<XportWriterWithMetadata<BufWriter<File>>> {
        self.from_writer(BufWriter::new(file), metadata)
    }

    /// Creates a writer from any `Write` implementor using the configured
    /// options. The library headers are written immediately.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    //noinspection RsSelfConvention
    #[inline]
    pub fn from_writer<W: Write>(
        &self,
        writer: W,
        metadata: XportMetadata,
    ) -> Result<XportWriterWithMetadata<W>> {
        let options = self.build();
        XportWriter::from_writer_with_options(writer, metadata, options)
    }

    /// Creates a writer backed by a buffered file at the given path using
    /// the configured options. The library headers are written immediately.
    ///
    /// # Errors
    /// Returns an error if the file cannot be created or if writing the
    /// library headers fails.
    //noinspection RsSelfConvention
    pub fn from_path<P: AsRef<Path>>(
        &self,
        path: P,
        metadata: XportMetadata,
    ) -> Result<XportWriterWithMetadata<BufWriter<File>>> {
        let file = File::create(path.as_ref())
            .map_err(|e| XportError::io("Failed to create the file", e))?;
        self.from_file(file, metadata)
    }
}

#[cfg(feature = "tokio")]
impl XportWriterOptions {
    /// Creates a writer backed by a buffered async file using the configured
    /// options. The library headers are written immediately.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    //noinspection RsSelfConvention
    #[inline]
    pub async fn from_tokio_file(
        &self,
        file: tokio::fs::File,
        metadata: XportMetadata,
    ) -> Result<super::AsyncXportWriterWithMetadata<tokio::io::BufWriter<tokio::fs::File>>> {
        self.from_tokio_writer(tokio::io::BufWriter::new(file), metadata)
            .await
    }

    /// Creates a writer from any `AsyncWrite` implementor using the
    /// configured options. The library headers are written immediately.
    ///
    /// # Errors
    /// Returns an error if writing the library headers fails.
    //noinspection RsSelfConvention
    #[inline]
    pub async fn from_tokio_writer<W: tokio::io::AsyncWrite + Unpin>(
        &self,
        writer: W,
        metadata: XportMetadata,
    ) -> Result<super::AsyncXportWriterWithMetadata<W>> {
        let options = self.build();
        super::AsyncXportWriter::from_writer_with_options(writer, metadata, options).await
    }

    /// Creates a writer backed by a buffered async file at the given path
    /// using the configured options. The library headers are written
    /// immediately.
    ///
    /// # Errors
    /// Returns an error if the file cannot be created or if writing the
    /// library headers fails.
    //noinspection RsSelfConvention
    pub async fn from_tokio_path<P: AsRef<Path>>(
        &self,
        path: P,
        metadata: XportMetadata,
    ) -> Result<super::AsyncXportWriterWithMetadata<tokio::io::BufWriter<tokio::fs::File>>> {
        let file = tokio::fs::File::create(path.as_ref())
            .await
            .map_err(|e| XportError::io("Failed to create the file", e))?;
        self.from_tokio_file(file, metadata).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_encoding_is_utf8() {
        let options = XportWriterOptionsInternal::default();
        assert_eq!(options.encoding(), encoding_rs::UTF_8);
    }

    #[test]
    fn default_character_truncation_policy_is_silent() {
        let options = XportWriterOptionsInternal::default();
        assert_eq!(
            options.truncation_policy(SasVariableType::Character),
            TruncationPolicy::Silent,
        );
    }

    #[test]
    fn default_numeric_truncation_policy_is_silent() {
        let options = XportWriterOptionsInternal::default();
        assert_eq!(
            options.truncation_policy(SasVariableType::Numeric),
            TruncationPolicy::Silent,
        );
    }

    #[test]
    fn builder_default_matches_default() {
        let from_default = XportWriterOptionsInternal::default();
        let from_builder = XportWriterOptions::default().build();
        assert_eq!(from_default.encoding(), from_builder.encoding());
        assert_eq!(
            from_default.truncation_policy(SasVariableType::Character),
            from_builder.truncation_policy(SasVariableType::Character),
        );
        assert_eq!(
            from_default.truncation_policy(SasVariableType::Numeric),
            from_builder.truncation_policy(SasVariableType::Numeric),
        );
    }

    #[test]
    fn encoding() {
        let options = XportWriterOptions::default()
            .encoding(encoding_rs::WINDOWS_1252)
            .build();
        assert_eq!(options.encoding(), encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn set_character_truncation_policy() {
        let options = XportWriterOptions::default()
            .truncation_policy(SasVariableType::Character, TruncationPolicy::Report)
            .build();
        assert_eq!(
            options.truncation_policy(SasVariableType::Character),
            TruncationPolicy::Report,
        );
        assert_eq!(
            options.truncation_policy(SasVariableType::Numeric),
            TruncationPolicy::Silent,
        );
    }

    #[test]
    fn set_numeric_truncation_policy() {
        let options = XportWriterOptions::default()
            .truncation_policy(SasVariableType::Numeric, TruncationPolicy::Report)
            .build();
        assert_eq!(
            options.truncation_policy(SasVariableType::Numeric),
            TruncationPolicy::Report,
        );
        assert_eq!(
            options.truncation_policy(SasVariableType::Character),
            TruncationPolicy::Silent,
        );
    }

    #[test]
    fn set_both_truncation_policies() {
        let options = XportWriterOptions::default()
            .truncation_policy(SasVariableType::Character, TruncationPolicy::Report)
            .truncation_policy(SasVariableType::Numeric, TruncationPolicy::Report)
            .build();
        assert_eq!(
            options.truncation_policy(SasVariableType::Character),
            TruncationPolicy::Report,
        );
        assert_eq!(
            options.truncation_policy(SasVariableType::Numeric),
            TruncationPolicy::Report,
        );
    }

    #[test]
    fn build_does_not_consume_builder() {
        let mut builder = XportWriterOptions::default();
        builder.encoding(encoding_rs::WINDOWS_1252);
        let first = builder.build();
        let second = builder.build();
        assert_eq!(first.encoding(), second.encoding());
    }
}
