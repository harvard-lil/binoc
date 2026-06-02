use crate::sas::sas_float_64::SasFloat64;
use crate::sas::xport::converter::{self, read_trimmed_string};
use crate::sas::xport::cursor::Cursor;
use crate::sas::xport::decoder::Decoder;
use crate::sas::xport::find_record_outcome::{FindRecordOutcome, NarrowRecordAction};
use crate::sas::xport::Result;
use crate::sas::xport::{
    XportError, XportMetadata, XportRecord, XportSchema, XportValue, XportVariable,
};
use crate::sas::SasVariableType;

#[derive(Debug)]
pub(crate) struct XportDatasetState {
    metadata: XportMetadata,
    schema: XportSchema,
    record_offset: usize,
    record_number: usize,
    record_length: usize,
    blank_row_count: usize,
    /// Saved non-blank record data from the narrow-record blank-row
    /// disambiguation path. When a non-blank record is found after one
    /// or more blank records, the non-blank data is saved here instead
    /// of seeking backwards, and a blank record is returned. On the next
    /// call, this saved record is returned directly.
    pending_record: Option<Vec<u8>>,
    record_buffer: Vec<u8>,
    complete: bool,
}

impl XportDatasetState {
    #[must_use]
    pub fn new(
        metadata: XportMetadata,
        schema: XportSchema,
        record_offset: usize,
        record_length: usize,
    ) -> Self {
        Self {
            metadata,
            schema,
            record_offset,
            record_number: 0,
            record_length,
            blank_row_count: 0,
            pending_record: None,
            record_buffer: vec![b' '; record_length],
            complete: false,
        }
    }

    /// Returns a reference to the internal record buffer.
    #[must_use]
    pub fn record_buffer(&self) -> &[u8] {
        &self.record_buffer
    }

    /// Returns a mutable reference to the internal record buffer.
    #[must_use]
    pub fn record_buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.record_buffer
    }

    #[must_use]
    pub fn metadata(&self) -> &XportMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn schema(&self) -> &XportSchema {
        &self.schema
    }

    #[must_use]
    pub fn record_number(&self) -> usize {
        self.record_number
    }

    pub fn increment_record_number(&mut self) {
        self.record_number += 1;
    }

    pub fn set_record_number(&mut self, record_number: usize) {
        self.record_number = record_number;
    }

    #[must_use]
    pub fn complete(&self) -> bool {
        self.complete
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn record_length(&self) -> usize {
        self.record_length
    }

    pub fn clear_blank_row_count(&mut self) {
        self.blank_row_count = 0;
    }

    pub fn clear_pending_record(&mut self) {
        self.pending_record = None;
    }

    /// Handles the end-of-dataset variants of `FindRecordOutcome`.
    /// Sets the dataset to complete and returns any carryover bytes
    /// that should be passed to the buffer.
    pub fn handle_end_of_dataset(&mut self, outcome: FindRecordOutcome) -> Option<Vec<u8>> {
        self.complete = true;
        match outcome {
            FindRecordOutcome::EndOfDatasetWithCarryover(carryover) => Some(carryover),
            _ => None,
        }
    }

    #[must_use]
    pub fn into_metadata(self) -> XportMetadata {
        self.metadata
    }

    /// Extracts the values for each of the variables in the dataset schema. The order of
    /// the values corresponds with the order the variables appear in the schema.
    pub fn extract_record(&self, decode: &Decoder) -> Result<XportRecord<'_>> {
        let variables = self.schema.variables();
        let mut record = XportRecord::with_capacity(variables.len());
        let mut cursor = Cursor::new(&self.record_buffer); // We don't trust the variable positions in the file
        for variable in variables {
            let value = Self::extract_value(variable, &mut cursor, decode)?;
            record.push(value);
        }
        Ok(record)
    }

    /// Gets the value from the next position in the buffer.
    fn extract_value<'a>(
        variable: &XportVariable,
        cursor: &mut Cursor<'a>,
        decoder: &Decoder,
    ) -> Result<XportValue<'a>> {
        let variable_name = variable.full_name();
        let value_length = variable.value_length() as usize;
        match variable.value_type() {
            SasVariableType::Character => {
                Self::extract_text(decoder, variable_name, cursor.read(value_length))
            }
            // Should we check if a numeric value has a length > 8?
            SasVariableType::Numeric => Ok(Self::extract_number(cursor.read(value_length))),
        }
    }

    /// Reads text from the given buffer using the specified encoding.
    pub(crate) fn extract_text<'a>(
        decoder: &Decoder,
        variable_name: &str,
        buffer: &'a [u8],
    ) -> Result<XportValue<'a>> {
        read_trimmed_string(buffer, decoder)
            .map_err(|e| {
                let message = format!("Failed to read the value for {variable_name}.");
                XportError::encoding(message, e)
            })
            .map(XportValue::Character)
    }

    /// The `SASFloat64` class expects all numbers to be 8-bytes (64-bits);
    /// however, SAS allows people to define numeric values that are smaller
    /// at the cost of precision. The representation for IBM floats is identical
    /// between 32-bit and 64-bit except that the mantissa (fractional part) is
    /// truncated. Padding the value out to 8-bytes with zeros shouldn't change the value.
    #[must_use]
    pub(crate) fn extract_number(buffer: &[u8]) -> XportValue<'_> {
        let mut bytes = [0u8; XportVariable::DEFAULT_NUMERIC_LENGTH as usize];
        bytes[..buffer.len()].copy_from_slice(buffer);
        let sas = SasFloat64::from_be_bytes(bytes);
        XportValue::Number(sas.into())
    }

    /// Handles the pre-loop checks for narrow-record disambiguation.
    /// Returns `Some(outcome)` if the caller should return immediately
    /// (pending record or buffered blank row), or `None` if the caller
    /// should enter the `find_wide_record` loop.
    pub fn check_narrow_preloop(&mut self) -> Option<FindRecordOutcome> {
        if let Some(pending) = self.pending_record.take() {
            self.record_buffer[..pending.len()].copy_from_slice(&pending);
            return Some(FindRecordOutcome::Record);
        }
        if self.blank_row_count > 0 {
            self.blank_row_count -= 1;
            self.record_buffer.fill(b' ');
            return Some(FindRecordOutcome::Record);
        }
        None
    }

    /// After `find_wide_record` returned `Record`, classifies the buffer
    /// contents for narrow-record disambiguation. If the row is blank it
    /// increments the blank counter and tells the caller to continue. If
    /// a non-blank row follows blank rows, it saves the non-blank data,
    /// fills the buffer with spaces, and tells the caller to return a
    /// blank record.
    pub fn classify_narrow_record(&mut self) -> NarrowRecordAction {
        if converter::all_blank(&self.record_buffer) {
            self.blank_row_count += 1;
            NarrowRecordAction::Continue
        } else if self.blank_row_count > 0 {
            self.pending_record = Some(self.record_buffer.clone());
            self.blank_row_count -= 1;
            self.record_buffer.fill(b' ');
            NarrowRecordAction::ReturnRecord
        } else {
            NarrowRecordAction::ReturnRecord
        }
    }

    #[must_use]
    pub fn compute_record_offset(&self) -> usize {
        self.record_offset + ((self.record_number + self.blank_row_count) * self.record_length)
    }

    #[must_use]
    pub fn compute_file_position(&self, record_index: usize) -> usize {
        self.record_offset + (self.record_length * record_index)
    }
}
