use std::borrow::Cow;
use std::error::Error;
use std::fmt;

/// Classifies what went wrong during an XPORT operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum XportErrorKind {
    /// An I/O operation failed (reading, writing, seeking, flushing).
    Io,

    /// A header or field value did not match the expected format.
    InvalidFormat,

    /// A string could not be decoded from the file's encoding, or a
    /// character could not be encoded into the target encoding.
    Encoding,

    /// A date/time field could not be parsed.
    InvalidDateTime,

    /// A schema or value violated a structural constraint (duplicate
    /// variable name, wrong value count, type mismatch, etc.).
    Validation,

    /// One or more variable values were truncated during writing.
    /// The record was fully written before this error was reported.
    Truncation(Vec<TruncatedVariable>),

    /// A numeric conversion overflowed (e.g., value exceeds u16 range).
    Overflow,

    /// An `f64` value cannot be represented as a SAS float.
    InvalidFloat,
}

/// Identifies the file section where an error was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum XportSection {
    /// The top-level library header (file version detection).
    LibraryHeader,
    /// File-level metadata headers.
    FileMetadata,
    /// The member header for a dataset.
    MemberHeader,
    /// The descriptor header within a dataset.
    DescriptorHeader,
    /// The member data headers (dataset name, dates, etc.).
    MemberData,
    /// The namestr header (variable count).
    NamestrHeader,
    /// A variable descriptor record.
    VariableDescriptor,
    /// Extended variable labels/names/formats (V8/V9).
    VariableExtension,
    /// The observation header (record count).
    ObservationHeader,
    /// Reading or writing a data record.
    Record,
    /// Schema building/validation.
    SchemaValidation,
}

/// Describes a single variable that was truncated during record writing.
/// Use the variable index to look up name, type, and length from the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TruncatedVariable {
    variable_index: usize,
    encoded_length: usize,
}

impl TruncatedVariable {
    /// Creates a new truncated variable descriptor.
    ///
    /// `encoded_length` is the number of bytes needed to represent the
    /// value without loss. Compare with the variable's `value_length`
    /// from the schema to see how much was lost.
    #[must_use]
    pub(crate) fn new(variable_index: usize, encoded_length: usize) -> Self {
        Self {
            variable_index,
            encoded_length,
        }
    }

    /// Returns the zero-based variable index within the schema.
    #[inline]
    #[must_use]
    pub fn variable_index(&self) -> usize {
        self.variable_index
    }

    /// Returns the number of bytes needed to represent the value
    /// without loss.
    #[inline]
    #[must_use]
    pub fn encoded_length(&self) -> usize {
        self.encoded_length
    }
}

/// Represents an error that occurred while reading or writing a SAS transport file.
#[derive(Debug)]
pub struct XportError {
    kind: XportErrorKind,
    message: Cow<'static, str>,
    section: Option<XportSection>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl XportError {
    /// Creates a new error with the given kind and message.
    #[must_use]
    pub(crate) fn of_kind(kind: XportErrorKind, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind,
            message: message.into(),
            section: None,
            source: None,
        }
    }

    /// Attaches a source error for chaining.
    #[must_use]
    pub(crate) fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Attaches the file section where this error occurred.
    #[must_use]
    pub(crate) fn in_section(mut self, section: XportSection) -> Self {
        self.section = Some(section);
        self
    }

    /// Returns the error kind.
    #[inline]
    #[must_use]
    pub fn kind(&self) -> &XportErrorKind {
        &self.kind
    }

    /// Returns the error message.
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the file section where the error occurred, if available.
    #[inline]
    #[must_use]
    pub fn section(&self) -> Option<XportSection> {
        self.section
    }

    /// Creates an I/O error with a source.
    #[must_use]
    pub(crate) fn io(
        message: impl Into<Cow<'static, str>>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::of_kind(XportErrorKind::Io, message).with_source(source)
    }

    /// Creates an encoding error whose source is a descriptive string
    /// from the decoder (not a concrete error type).
    #[must_use]
    pub(crate) fn encoding(
        message: impl Into<Cow<'static, str>>,
        source: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::of_kind(XportErrorKind::Encoding, message)
            .with_source(Self::of_kind(XportErrorKind::Encoding, source))
    }
}

impl fmt::Display for XportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(section) = &self.section {
            write!(f, " at {section:?}")?;
        }
        if let XportErrorKind::Truncation(variables) = &self.kind {
            write!(f, " (")?;
            for (i, var) in variables.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(
                    f,
                    "variable {} needed {} bytes",
                    var.variable_index(),
                    var.encoded_length()
                )?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl Error for XportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn Error + 'static))
    }
}

/// A result that returns an `XportError` when an error occurs.
pub type Result<T> = std::result::Result<T, XportError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_kind_sets_specified_kind() {
        let err = XportError::of_kind(XportErrorKind::Validation, "bad schema");
        assert_eq!(err.kind(), &XportErrorKind::Validation);
        assert_eq!(err.message(), "bad schema");
    }

    #[test]
    fn builder_attaches_section() {
        let err = XportError::of_kind(XportErrorKind::InvalidFormat, "bad header")
            .in_section(XportSection::MemberHeader);
        assert_eq!(err.section(), Some(XportSection::MemberHeader));
    }

    #[test]
    fn truncation_kind_carries_variables() {
        let vars = vec![TruncatedVariable::new(0, 12), TruncatedVariable::new(2, 6)];
        let err = XportError::of_kind(XportErrorKind::Truncation(vars), "Variables were truncated");

        let XportErrorKind::Truncation(variables) = err.kind() else {
            panic!("expected Truncation kind");
        };
        assert_eq!(variables.len(), 2);
        assert_eq!(variables[0].variable_index(), 0);
        assert_eq!(variables[0].encoded_length(), 12);
        assert_eq!(variables[1].variable_index(), 2);
        assert_eq!(variables[1].encoded_length(), 6);
    }

    #[test]
    fn display_message_only() {
        let err = XportError::of_kind(XportErrorKind::InvalidFormat, "something failed");
        assert_eq!(err.to_string(), "something failed");
    }

    #[test]
    fn display_with_section() {
        let err =
            XportError::of_kind(XportErrorKind::Io, "read failed").in_section(XportSection::Record);
        assert_eq!(err.to_string(), "read failed at Record");
    }

    #[test]
    fn display_with_truncated_variables() {
        let vars = vec![TruncatedVariable::new(0, 10), TruncatedVariable::new(3, 8)];
        let err = XportError::of_kind(XportErrorKind::Truncation(vars), "Variables were truncated");
        assert_eq!(
            err.to_string(),
            "Variables were truncated (variable 0 needed 10 bytes, variable 3 needed 8 bytes)"
        );
    }

    #[test]
    fn error_source_chaining() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke");
        let err = XportError::of_kind(XportErrorKind::Io, "write failed").with_source(io_err);
        let source = err.source().unwrap();
        assert_eq!(source.to_string(), "pipe broke");
    }
}
