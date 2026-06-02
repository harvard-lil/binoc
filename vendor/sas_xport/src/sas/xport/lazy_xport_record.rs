use crate::sas::SasVariableType;
use crate::sas::xport::decoder::Decoder;
use crate::sas::xport::xport_dataset_state::XportDatasetState;
use crate::sas::xport::{Result, XportRecord, XportValue, XportVariable};

/// A zero-allocation view over a raw record buffer. Values are decoded
/// on demand rather than eagerly, avoiding the per-record
/// `Vec<XportValue>` allocation that [`XportRecord`] requires.
///
/// The lifetime `'a` ties this record to the dataset that produced it.
/// Drop the record before calling any mutating method on the dataset.
#[derive(Debug)]
pub struct LazyXportRecord<'a> {
    buffer: &'a [u8],
    variables: &'a [XportVariable],
    decoder: &'a Decoder,
}

impl<'a> LazyXportRecord<'a> {
    #[inline]
    pub(crate) fn new(
        buffer: &'a [u8],
        variables: &'a [XportVariable],
        decoder: &'a Decoder,
    ) -> Self {
        Self {
            buffer,
            variables,
            decoder,
        }
    }

    /// Returns the number of variables in the record.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.variables.len()
    }

    /// Returns `true` if the record has no variables.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    /// Decodes the value at the given variable index. Returns `None`
    /// if the index is out of bounds, or `Some(Err)` if the value
    /// cannot be decoded.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Result<XportValue<'a>>> {
        let variable = self.variables.get(index)?;
        let offset = variable.record_offset() as usize;
        let length = variable.value_length() as usize;
        let bytes = &self.buffer[offset..offset + length];
        Some(Self::decode(variable, bytes, self.decoder))
    }

    /// Returns an iterator that decodes each value sequentially.
    /// This is more efficient than calling [`get`](Self::get) in a
    /// loop because it walks the buffer without recomputing offsets.
    #[inline]
    #[must_use]
    pub fn iter(&self) -> LazyXportRecordIter<'a, '_> {
        LazyXportRecordIter {
            record: self,
            index: 0,
        }
    }

    /// Eagerly decodes all values into an [`XportRecord`].
    ///
    /// # Errors
    /// Returns `Err` if any character value cannot be decoded.
    pub fn into_record(self) -> Result<XportRecord<'a>> {
        let mut record = XportRecord::with_capacity(self.variables.len());
        for variable in self.variables {
            let offset = variable.record_offset() as usize;
            let length = variable.value_length() as usize;
            let bytes = &self.buffer[offset..offset + length];
            record.push(Self::decode(variable, bytes, self.decoder)?);
        }
        Ok(record)
    }

    fn decode(
        variable: &XportVariable,
        bytes: &'a [u8],
        decoder: &Decoder,
    ) -> Result<XportValue<'a>> {
        match variable.value_type() {
            SasVariableType::Character => {
                XportDatasetState::extract_text(decoder, variable.full_name(), bytes)
            }
            SasVariableType::Numeric => Ok(XportDatasetState::extract_number(bytes)),
        }
    }
}

impl<'a, 'rec> IntoIterator for &'rec LazyXportRecord<'a> {
    type Item = Result<XportValue<'a>>;
    type IntoIter = LazyXportRecordIter<'a, 'rec>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over the values in a [`LazyXportRecord`], decoding each
/// value on demand as the iterator advances.
#[derive(Debug)]
pub struct LazyXportRecordIter<'a, 'rec> {
    record: &'rec LazyXportRecord<'a>,
    index: usize,
}

impl<'a> Iterator for LazyXportRecordIter<'a, '_> {
    type Item = Result<XportValue<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        let variable = self.record.variables.get(self.index)?;
        let offset = variable.record_offset() as usize;
        let length = variable.value_length() as usize;
        let bytes = &self.record.buffer[offset..offset + length];
        self.index += 1;
        Some(LazyXportRecord::decode(
            variable,
            bytes,
            self.record.decoder,
        ))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.record.variables.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for LazyXportRecordIter<'_, '_> {}
