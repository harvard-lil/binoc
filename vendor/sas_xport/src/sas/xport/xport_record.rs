use std::ops::{Deref, Index};

use super::XportValue;

/// A record read from a SAS transport file.
#[derive(Clone, Debug)]
pub struct XportRecord<'a>(Vec<XportValue<'a>>);

impl<'a> XportRecord<'a> {
    /// Creates an empty record pre-allocated for the given number of variables.
    #[inline]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    /// Appends a value to the record.
    #[inline]
    pub(crate) fn push(&mut self, value: XportValue<'a>) {
        self.0.push(value);
    }

    /// Returns the number of values in the record.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the record contains no values.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the values in the record.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, XportValue<'a>> {
        self.0.iter()
    }

    /// Converts all borrowed character data into owned values, making the
    /// record independent of the original record buffer.
    #[must_use]
    pub fn into_owned(self) -> XportRecord<'static> {
        let owned = self.0.into_iter().map(XportValue::into_owned).collect();
        XportRecord(owned)
    }
}

impl<'a> Deref for XportRecord<'a> {
    type Target = [XportValue<'a>];

    #[inline]
    fn deref(&self) -> &[XportValue<'a>] {
        &self.0
    }
}

impl<'a, I: std::slice::SliceIndex<[XportValue<'a>]>> Index<I> for XportRecord<'a> {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        &self.0[index]
    }
}

impl<'a> IntoIterator for XportRecord<'a> {
    type Item = XportValue<'a>;
    type IntoIter = std::vec::IntoIter<XportValue<'a>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, 'b> IntoIterator for &'b XportRecord<'a> {
    type Item = &'b XportValue<'a>;
    type IntoIter = std::slice::Iter<'b, XportValue<'a>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
