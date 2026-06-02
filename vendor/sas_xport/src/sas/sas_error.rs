use std::borrow::Cow;
use std::error::Error;
use std::fmt;

/// Represents an error that occurred while processing SAS® data.
#[derive(Debug)]
pub struct SasError {
    message: Cow<'static, str>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl SasError {
    /// Creates a new error with the given message. Use this method
    /// when there are no internal errors.
    ///
    /// Accepts `&'static str`, `String`, or `Cow<'static, str>`.
    /// Static strings avoid allocation; dynamic strings are stored owned.
    #[inline]
    #[must_use]
    pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Creates a new error with the given message, wrapping the given source.
    /// Use this method when wrapping an internal error.
    ///
    /// Accepts `&'static str`, `String`, or `Cow<'static, str>`.
    /// Static strings avoid allocation; dynamic strings are stored owned.
    #[inline]
    #[must_use]
    pub fn wrap(
        message: impl Into<Cow<'static, str>>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Returns the error message.
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for SasError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn Error + 'static))
    }
}
