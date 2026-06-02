/// The result of attempting to read a record from the file.
#[derive(Debug)]
pub(crate) enum FindRecordOutcome {
    /// A complete record was read into the buffer.
    Record,
    /// End of dataset reached (EOF or padding). No more records.
    EndOfDataset,
    /// The end of the dataset was reached, and we consumed bytes belonging to the next
    /// dataset's member header. The `Vec` contains those already-read bytes.
    EndOfDatasetWithCarryover(Vec<u8>),
}

/// Tells the narrow-record loop caller what to do after `find_wide_record`
/// returned `FindRecordOutcome::Record`.
#[derive(Debug)]
pub(crate) enum NarrowRecordAction {
    /// The record buffer contains a valid record — return it.
    ReturnRecord,
    /// The row was blank and has been counted — call `find_wide_record` again.
    Continue,
}
