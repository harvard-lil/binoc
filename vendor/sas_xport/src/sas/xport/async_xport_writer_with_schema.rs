use super::xport_constants;
use super::{Result, XportError, XportMetadata, XportSchema, XportValue};
use crate::sas::xport::async_xport_writer_state::AsyncXportWriterState;
use crate::sas::xport::async_xport_writer_with_metadata::AsyncXportWriterWithMetadata;
use crate::sas::xport::converter::{numeric_encoded_length, validate_values};
use crate::sas::xport::truncation_policy::TruncationPolicy;
use crate::sas::xport::xport_error::{TruncatedVariable, XportErrorKind, XportSection};
use crate::sas::{SasFloat64, SasVariableType};
use tokio::io::{AsyncSeek, AsyncWrite};

/// State after a dataset schema has been written. Records can be written
/// via [`write_record`](Self::write_record). Transition to the next dataset
/// with [`next_dataset`](Self::next_dataset), or finalize with
/// [`finish`](Self::finish).
///
/// Get this from [`AsyncXportWriterWithMetadata::write_schema`].
#[derive(Debug)]
pub struct AsyncXportWriterWithSchema<W: AsyncWrite + Unpin> {
    state: Option<AsyncXportWriterState<W>>,
    metadata: Option<XportMetadata>,
    schema: XportSchema,
    /// Number of records written to the current dataset.
    record_count: u64,
    /// Byte offset where the observation header's record-count field begins.
    /// `Some` for V8/V9 (the 15-character field after the prefix), `None` for V5.
    observation_count_offset: Option<u64>,
}

impl<W: AsyncWrite + Unpin> AsyncXportWriterWithSchema<W> {
    /// Returns the file-level metadata.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    #[must_use]
    pub fn metadata(&self) -> &XportMetadata {
        self.metadata.as_ref().expect("metadata taken after finish")
    }

    /// Returns the current dataset's schema.
    #[inline]
    #[must_use]
    pub fn schema(&self) -> &XportSchema {
        &self.schema
    }

    /// Returns the number of records written to the current dataset.
    #[inline]
    #[must_use]
    pub fn record_count(&self) -> u64 {
        self.record_count
    }
}

impl<W: AsyncWrite + Unpin> AsyncXportWriterWithSchema<W> {
    /// Creates a new schema-writing state. Called by
    /// `AsyncXportWriterWithMetadata::write_schema`.
    pub(crate) fn new(
        state: AsyncXportWriterState<W>,
        metadata: XportMetadata,
        schema: XportSchema,
        observation_count_offset: Option<u64>,
    ) -> Self {
        Self {
            state: Some(state),
            metadata: Some(metadata),
            schema,
            record_count: 0,
            observation_count_offset,
        }
    }

    /// Writes a single record to the current dataset. The slice must contain
    /// exactly one value per variable, in schema order. Each value's variant
    /// must match the corresponding variable's `SasVariableType`.
    ///
    /// # Errors
    /// Returns an error if:
    /// * The number of values does not match the variable count
    /// * A value's variant does not match the variable's type
    /// * An I/O error occurs
    pub async fn write_record(&mut self, values: &[XportValue<'_>]) -> Result<()> {
        validate_values(self.schema.variables(), values)?;
        let character_policy = self
            .state_mut()
            .options()
            .truncation_policy(SasVariableType::Character);
        let numeric_policy = self
            .state_mut()
            .options()
            .truncation_policy(SasVariableType::Numeric);
        let mut truncated_variables: Vec<TruncatedVariable> = Vec::new();
        for (index, value) in values.iter().enumerate() {
            let length = usize::from(self.schema.variables()[index].value_length()); // Validated above
            let encoded_length = match value {
                XportValue::Character(value) => {
                    let encoded_length = self
                        .state_mut()
                        .write_str(value.as_ref(), length, "Failed to write a Character value")
                        .await?;
                    if character_policy == TruncationPolicy::Report {
                        encoded_length
                    } else {
                        None
                    }
                }
                XportValue::Number(value) => {
                    let float = SasFloat64::try_from(*value).map_err(|()| {
                        XportError::of_kind(
                            XportErrorKind::InvalidFloat,
                            "Encountered an f64 that cannot be stored",
                        )
                    })?;
                    let bytes = float.to_be_bytes();
                    self.state_mut()
                        .write(&bytes[..length], "Failed to write a Numeric value")
                        .await?;
                    let encoded = numeric_encoded_length(bytes);
                    if numeric_policy == TruncationPolicy::Report && encoded > length {
                        Some(encoded)
                    } else {
                        None
                    }
                }
            };
            if let Some(encoded_length) = encoded_length {
                truncated_variables.push(TruncatedVariable::new(index, encoded_length));
            }
        }
        self.record_count += 1;
        if !truncated_variables.is_empty() {
            return Err(XportError::of_kind(
                XportErrorKind::Truncation(truncated_variables),
                "One or more variables were truncated while writing the record",
            )
            .in_section(XportSection::Record));
        }
        Ok(())
    }

    /// Pads the record area to an 80-byte boundary and transitions back
    /// to the metadata state, ready for the next dataset schema.
    ///
    /// Does **not** set the record count in the observation header. For
    /// seekable writers, use
    /// [`set_count_and_next_dataset`](Self::set_count_and_next_dataset).
    ///
    /// # Errors
    /// Returns an error if an I/O error occurs during padding.
    #[allow(clippy::missing_panics_doc)]
    pub async fn next_dataset(mut self) -> Result<AsyncXportWriterWithMetadata<W>> {
        self.pad_to_boundary().await?;
        let state = self.state.take().expect("state taken after finish");
        let metadata = self.metadata.take().expect("metadata taken after finish");
        Ok(AsyncXportWriterWithMetadata::new(state, metadata))
    }

    /// Pads the record area to an 80-byte boundary, flushes the writer,
    /// and returns the inner writer.
    ///
    /// Does **not** set the record count in the observation header. For
    /// seekable writers, use
    /// [`set_count_and_finish`](Self::set_count_and_finish).
    ///
    /// # Errors
    /// Returns an error if an I/O error occurs during padding or flushing.
    #[allow(clippy::missing_panics_doc)]
    pub async fn finish(mut self) -> Result<W> {
        self.pad_to_boundary().await?;
        let mut state = self.state.take().expect("state taken after finish");
        state.flush().await?;
        Ok(state.into_writer())
    }

    async fn pad_to_boundary(&mut self) -> Result<()> {
        let header_length = u64::try_from(xport_constants::HEADER_LENGTH).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "Failed to convert the header length to u64",
            )
            .with_source(e)
        })?;
        let remaining = self.state_mut().position() % header_length;
        if remaining == 0 {
            return Ok(());
        }
        let padding_length = header_length - remaining;
        let padding_length = usize::try_from(padding_length).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "Failed to convert the padding length to usize",
            )
            .with_source(e)
        })?;
        self.state_mut()
            .write_padding(
                b' ',
                padding_length,
                "Failed to pad the record area to an 80-byte boundary",
            )
            .await
    }

    fn state_mut(&mut self) -> &mut AsyncXportWriterState<W> {
        self.state.as_mut().expect("state taken after finish")
    }
}

impl<W: AsyncWrite + AsyncSeek + Unpin> AsyncXportWriterWithSchema<W> {
    /// Sets the record count in the V8/V9 observation header, pads the
    /// record area, and transitions to the metadata state for the next
    /// dataset.
    ///
    /// For V5 datasets, this behaves identically to [`next_dataset`](Self::next_dataset)
    /// because V5 has no record-count field.
    ///
    /// # Errors
    /// Returns an error if an I/O error occurs during seeking, writing,
    /// or padding.
    pub async fn set_count_and_next_dataset(mut self) -> Result<AsyncXportWriterWithMetadata<W>> {
        self.seek_and_set_record_count().await?;
        self.next_dataset().await
    }

    /// Sets the record count in the V8/V9 observation header, pads the
    /// record area, flushes the writer, and returns the inner writer.
    ///
    /// For V5 datasets, this behaves identically to [`finish`](Self::finish).
    ///
    /// # Errors
    /// Returns an error if an I/O error occurs during seeking, writing,
    /// padding, or flushing.
    pub async fn set_count_and_finish(mut self) -> Result<W> {
        self.seek_and_set_record_count().await?;
        self.finish().await
    }

    async fn seek_and_set_record_count(&mut self) -> Result<()> {
        if let Some(offset) = self.observation_count_offset {
            let record_count = self.record_count;
            self.state_mut()
                .write_record_count(offset, record_count)
                .await?;
        }
        Ok(())
    }
}
