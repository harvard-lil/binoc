use std::borrow::Cow;

/// Represents a value read from a SAS® transport file record. The lifetime
/// parameter `'a` ties `Character` values to the record buffer they borrow from.
#[derive(Clone, Debug)]
pub enum XportValue<'a> {
    /// A textual value borrowed from the record buffer.
    Character(Cow<'a, str>),
    /// A numeric value. Missing values are represented as `NaN`.
    Number(f64),
}

impl XportValue<'_> {
    /// Converts any borrowed character data into an owned value, making it
    /// independent of the original record buffer.
    #[must_use]
    pub fn into_owned(self) -> XportValue<'static> {
        match self {
            Self::Character(cow) => XportValue::Character(Cow::Owned(cow.into_owned())),
            Self::Number(n) => XportValue::Number(n),
        }
    }
}

impl<'a> From<&'a str> for XportValue<'a> {
    #[inline]
    fn from(value: &'a str) -> Self {
        Self::Character(Cow::Borrowed(value))
    }
}

impl From<String> for XportValue<'static> {
    #[inline]
    fn from(value: String) -> Self {
        Self::Character(Cow::Owned(value))
    }
}

impl From<f64> for XportValue<'static> {
    #[inline]
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}
