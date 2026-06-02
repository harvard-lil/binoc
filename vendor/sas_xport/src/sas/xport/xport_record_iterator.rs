use super::{Result, XportDataset, XportRecord};
use std::io::BufRead;

/// An iterator over the records in an [`XportDataset`]. Each call to `next()`
/// returns a fully owned record where all character values use `Cow::Owned`.
///
/// This iterator borrows the dataset mutably. Drop it before calling
/// [`XportDataset::next_dataset`]. Capture any schema information you need
/// before calling [`XportDataset::records`], since the mutable borrow prevents
/// concurrent access to the dataset.
#[derive(Debug)]
pub struct XportRecordIterator<'d, R> {
    dataset: &'d mut XportDataset<R>,
}

impl<'d, R: BufRead> XportRecordIterator<'d, R> {
    pub(crate) fn new(dataset: &'d mut XportDataset<R>) -> Self {
        Self { dataset }
    }
}

impl<R: BufRead> Iterator for XportRecordIterator<'_, R> {
    type Item = Result<XportRecord<'static>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.dataset.next_record() {
            Ok(Some(record)) => Some(Ok(record.into_owned())),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
