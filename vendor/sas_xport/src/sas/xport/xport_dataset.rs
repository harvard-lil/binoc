use super::find_record_outcome::{FindRecordOutcome, NarrowRecordAction};
use super::lazy_xport_record::LazyXportRecord;
use super::xport_buffer::XportBuffer;
use super::xport_constants::HEADER_LENGTH;
use super::xport_error::XportErrorKind;
use super::xport_record_iterator::XportRecordIterator;
use super::{Result, XportError, XportMetadata, XportRecord, XportSchema};
use crate::sas::xport::xport_dataset_state::XportDatasetState;
use std::io::{BufRead, Seek};

/// Represents a dataset within a SAS® transport file.
#[derive(Debug)]
pub struct XportDataset<R> {
    buffer: XportBuffer<R>,
    advance_fn: fn(&mut Self) -> Result<FindRecordOutcome>,
    state: XportDatasetState,
}

impl<R> XportDataset<R> {
    /// Gets the file metadata.
    #[inline]
    #[must_use]
    pub fn metadata(&self) -> &XportMetadata {
        self.state.metadata()
    }

    /// Gets the schema.
    #[inline]
    #[must_use]
    pub fn schema(&self) -> &XportSchema {
        self.state.schema()
    }

    /// Gets the number of records that have been read from the dataset.
    /// After reading all records, this value represents the total number
    /// of records that were in the dataset.
    #[inline]
    #[must_use]
    pub fn record_number(&self) -> usize {
        self.state.record_number()
    }
}

impl<R: BufRead> XportDataset<R> {
    pub(crate) fn new(
        buffer: XportBuffer<R>,
        metadata: XportMetadata,
        schema: XportSchema,
        record_offset: usize,
    ) -> Self {
        // We can avoid a lot of checks if the record size is greater than 80.
        let record_length = schema.compute_record_length();
        let advance_fn = if record_length < HEADER_LENGTH {
            Self::find_narrow_record
        } else {
            Self::find_wide_record
        };
        let state = XportDatasetState::new(metadata, schema, record_offset, record_length);
        Self {
            buffer,
            advance_fn,
            state,
        }
    }

    /// Reads the next record from the dataset. If no more records are available,
    /// `None` is returned.
    ///
    /// # Errors
    /// Will return Err if:
    /// * An I/O error occurs while trying to read the file
    /// * An encoding error occurs preventing a string from being read
    pub fn next_record(&mut self) -> Result<Option<XportRecord<'_>>> {
        // When attempting to read the next record, there are several things we might encounter:
        // 1) An actual record.
        // 2) Zero or more blanks (ASCII spaces) followed by:
        //    a) EOF
        //    b) or the start of the next dataset schema.
        // 3) An abrupt EOF without trailing spaces.
        //
        // If the size of a record is less than a header size (80 chars), and we see all blanks, we will
        // need to look ahead to see if we encounter the EOF or the start of the next schema. If we see
        // any non-blanks along the way, then the blanks we saw earlier must have been part of a valid
        // record, and we back up the file pointer, parse the blank record, and return it. If we find the
        // EOF, we return None to indicate we are done. If we encounter the start of the next schema,
        // we back up the file pointer and return None.
        //
        // If the size of the record is greater than or equal than a header size (80 chars), we see if
        // the remaining characters in the current 80-char block are all blanks. If we see all blanks, and
        // we're at the EOF, we return None. If we see the start of the next schema header, we might need
        // to grab some extra data to see if the full schema header. If it's not, then we will potentially
        // need to back up the file pointer.
        //
        // NOTE: Backing up the file pointer could potentially be expensive. Most of the time, backing up
        // should be efficient because seeking within the BufReader should just update the index. However,
        // if the buffer is refilled as part of looking forward, it might require an actual OS call.
        if self.state.complete() {
            return Ok(None);
        }
        let outcome = (self.advance_fn)(self)?;
        if !matches!(outcome, FindRecordOutcome::Record) {
            if let Some(carryover) = self.state.handle_end_of_dataset(outcome) {
                self.buffer.set_carryover(carryover);
            }
            return Ok(None);
        }
        self.state.increment_record_number();
        let record = self.state.extract_record(self.buffer.decoder())?;
        Ok(Some(record))
    }

    /// Reads the next record as a [`LazyXportRecord`] that decodes values
    /// on demand. This avoids the per-record `Vec<XportValue>` allocation
    /// that [`next_record`](Self::next_record) performs.
    ///
    /// The returned record borrows the dataset's internal buffer, schema,
    /// and decoder. Drop it before calling any other `&mut self` method
    /// on this dataset.
    ///
    /// # Errors
    /// Will return Err if:
    /// * An I/O error occurs while trying to read the file
    pub fn next_lazy_record(&mut self) -> Result<Option<LazyXportRecord<'_>>> {
        if self.state.complete() {
            return Ok(None);
        }
        let outcome = (self.advance_fn)(self)?;
        if !matches!(outcome, FindRecordOutcome::Record) {
            if let Some(carryover) = self.state.handle_end_of_dataset(outcome) {
                self.buffer.set_carryover(carryover);
            }
            return Ok(None);
        }
        self.state.increment_record_number();
        let variables = self.state.schema().variables();
        let decoder = self.buffer.decoder();
        Ok(Some(LazyXportRecord::new(
            self.state.record_buffer(),
            variables,
            decoder,
        )))
    }

    /// Returns an iterator over the records in this dataset. Each record
    /// contains fully owned values. The iterator borrows this dataset
    /// mutably; drop it before calling [`Self::next_dataset`].
    ///
    /// For zero-copy access, use [`Self::next_record`] directly instead.
    #[inline]
    #[must_use]
    pub fn records(&mut self) -> XportRecordIterator<'_, R> {
        XportRecordIterator::new(self)
    }

    fn find_narrow_record(&mut self) -> Result<FindRecordOutcome> {
        if let Some(outcome) = self.state.check_narrow_preloop() {
            return Ok(outcome);
        }
        loop {
            let outcome = self.find_wide_record()?;
            if !matches!(outcome, FindRecordOutcome::Record) {
                return Ok(outcome);
            }
            match self.state.classify_narrow_record() {
                NarrowRecordAction::ReturnRecord => return Ok(FindRecordOutcome::Record),
                NarrowRecordAction::Continue => {}
            }
        }
    }

    fn find_wide_record(&mut self) -> Result<FindRecordOutcome> {
        let offset = self.state.compute_record_offset();
        let file_version = self.state.metadata().file_version();
        self.buffer
            .find_record(file_version, offset, self.state.record_buffer_mut())
    }

    /// Consumes the rest of the current dataset without parsing any
    /// of the records, making it more efficient when skipping to the end of the dataset
    /// and none of the records are needed.
    ///
    /// # Errors
    /// Will return `Err` if an I/O error occurs while advancing to the end of the dataset.
    pub fn skip_to_end(&mut self) -> Result<()> {
        while !self.state.complete() {
            let outcome = (self.advance_fn)(self)?;
            if matches!(outcome, FindRecordOutcome::Record) {
                self.state.increment_record_number();
            } else if let Some(carryover) = self.state.handle_end_of_dataset(outcome) {
                self.buffer.set_carryover(carryover);
            }
        }
        Ok(())
    }

    /// Reads the next dataset. It is an error if there are unread records in the current dataset.
    /// Ensure every record in the current dataset is read by either calling `next_record` until
    /// `Ok(None)` is returned or by calling `skip_to_end`.
    ///
    /// # Errors
    /// Return an `Err` if:
    /// * There are unread records in the current dataset.
    /// * An I/O error occurs while advancing to the next dataset.
    /// * An I/O error occurs while reading the schema of the next dataset.
    /// * An encoding error occurs while trying to parse the schema.
    pub fn next_dataset(mut self) -> Result<Option<Self>> {
        // Before we read the next dataset, we make sure the file position is correct.
        // We'll either be immediately after the metadata or reading the previous dataset.
        // If we've previously read a dataset, we make sure we've completely consumed it.
        // We know we must have read the metadata previously.
        if !self.state.complete() {
            return Err(XportError::of_kind(
                XportErrorKind::Validation,
                "Cannot read the next dataset. Advance to the end of the current dataset",
            ));
        }
        let metadata = self.state.into_metadata();
        let schema = self.buffer.read_schema(metadata.file_version())?;
        if let Some(schema) = schema {
            let record_offset = self.buffer.position();
            let dataset = Self::new(self.buffer, metadata, schema, record_offset);
            Ok(Some(dataset))
        } else {
            Ok(None)
        }
    }
}

impl<R: BufRead + Seek> XportDataset<R> {
    /// Positions the reader directly before the record at the specified
    /// 0-based index, such that calling next will return that record. No checks
    /// are performed to guarantee the given index falls within the current
    /// dataset. If the XPORT file only contains a single dataset, requesting an
    /// index beyond the end of the file will result in an error. If the XPORT
    /// file contains multiple datasets, requesting an index beyond the end of
    /// the current dataset will not result in an error but result in undefined
    /// behavior (most likely causing all future reads to return garbage data).
    ///
    /// # Errors
    /// `Err` is returned if
    /// * An I/O error occurs while seeking to the given position.
    /// * The offset is off the end of the file.
    /// * The new record number cannot be represented as a 64-bit unsigned integer.
    pub fn seek(&mut self, index: u64) -> Result<()> {
        if index == self.state.record_number() as u64 {
            return Ok(());
        }
        let new_record_number = usize::try_from(index).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "The new record number cannot be represented for the given index",
            )
            .with_source(e)
        })?;
        let position = self.state.compute_file_position(new_record_number);
        self.buffer.seek_from_start(position as u64)?;
        self.state.clear_blank_row_count();
        self.state.clear_pending_record();
        self.state.set_record_number(new_record_number);
        Ok(())
    }
}
