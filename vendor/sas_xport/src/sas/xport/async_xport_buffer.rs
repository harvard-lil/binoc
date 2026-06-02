use super::decoder::Decoder;
use super::find_record_outcome::FindRecordOutcome;
use super::{
    Result, XportDatasetVersion, XportError, XportFileVersion, XportMetadata, XportMetadataBuilder,
    XportSchema, XportSchemaBuilder, XportVariableBuilder,
};
use super::{XportReaderOptionsInternal, converter};
use crate::sas::xport::xport_buffer_state::{
    MemberHeaderCheck, RemainingSectionV8, XportBufferState,
};
use crate::sas::xport::xport_constants::{HEADER_LENGTH, variable_error};
use crate::sas::xport::xport_error::XportErrorKind;
use std::io::SeekFrom;
use tokio::io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

#[derive(Debug)]
pub(crate) struct AsyncXportBuffer<R> {
    state: XportBufferState,
    reader: R,
    /// Bytes consumed from the next dataset's member header during
    /// end-of-dataset detection. Fed back into `read_member_header`
    /// on the next `read_schema` call.
    carryover: Option<Vec<u8>>,
}

impl<R> AsyncXportBuffer<R> {
    #[must_use]
    pub fn decoder(&self) -> &Decoder {
        self.state.decoder()
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.state.position()
    }

    pub fn set_carryover(&mut self, carryover: Vec<u8>) {
        self.carryover = Some(carryover);
    }
}

impl<R: AsyncBufRead + Unpin> AsyncXportBuffer<R> {
    /// Creates an `AsyncXportBuffer` from the given options.
    #[must_use]
    pub fn from_reader(reader: R, options: &XportReaderOptionsInternal) -> Self {
        let state = XportBufferState::from_options(options);
        Self {
            state,
            reader,
            carryover: None,
        }
    }

    /// Reads exactly `buf.len()` bytes, consuming from carryover first, then
    /// from the reader. If carryover has excess bytes after filling `buf`,
    /// the remainder is saved back for the next call.
    async fn read_exact_with_carryover(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let used = XportBufferState::apply_carryover(&mut self.carryover, buf);
        if used < buf.len() {
            self.reader.read_exact(&mut buf[used..]).await.map(|_| ())?;
        }
        Ok(())
    }

    pub async fn read_metadata(&mut self) -> Result<XportMetadata> {
        let mut header = [0u8; HEADER_LENGTH];
        let mut builder = XportMetadata::builder();
        let file_version = self.read_library_header(&mut header).await?;
        builder.xport_file_version(file_version);
        self.read_real_headers(&mut builder, &mut header).await?;
        Ok(builder.into())
    }

    async fn read_library_header(&mut self, header: &mut [u8]) -> Result<XportFileVersion> {
        self.reader
            .read_exact(header)
            .await
            .map_err(|e| XportError::io("Failed to determine the XPORT version. The library header record could not be read", e))?;
        self.state.process_library_header(header)
    }

    async fn read_real_headers(
        &mut self,
        builder: &mut XportMetadataBuilder,
        header: &mut [u8],
    ) -> Result<()> {
        self.read_real_header1(builder, header).await?;
        self.read_real_header2(builder, header).await?;
        Ok(())
    }

    async fn read_real_header1(
        &mut self,
        builder: &mut XportMetadataBuilder,
        header: &mut [u8],
    ) -> Result<()> {
        self.reader
            .read_exact(header)
            .await
            .map_err(|e| XportError::io("Failed to read the first metadata header record", e))?;
        self.state.process_real_header1(builder, header)
    }

    async fn read_real_header2(
        &mut self,
        builder: &mut XportMetadataBuilder,
        header: &mut [u8],
    ) -> Result<()> {
        self.reader
            .read_exact(header)
            .await
            .map_err(|e| XportError::io("Failed to read the second metadata header record", e))?;
        self.state.process_real_header2(builder, header)
    }

    async fn read_member_header(
        &mut self,
        file_version: XportFileVersion,
        header: &mut [u8],
    ) -> Result<Option<u16>> {
        let result = self.read_exact_with_carryover(header).await;
        if let Err(error) = result {
            return match error.kind() {
                std::io::ErrorKind::UnexpectedEof => Ok(None),
                _ => Err(XportError::io("Failed to read the member header", error)),
            };
        }
        self.state.process_member_header(file_version, header)
    }

    async fn read_descriptor_header(
        &mut self,
        file_version: XportFileVersion,
        header: &mut [u8],
    ) -> Result<()> {
        self.read_exact_with_carryover(header)
            .await
            .map_err(|e| XportError::io("Failed to read the member descriptor header", e))?;
        self.state.process_descriptor_header(file_version, header)
    }

    async fn read_member_line1(
        &mut self,
        file_version: XportFileVersion,
        builder: &mut XportSchemaBuilder,
        header: &mut [u8],
    ) -> Result<()> {
        self.read_exact_with_carryover(header)
            .await
            .map_err(|e| XportError::io("Failed to read the first member data header", e))?;
        self.state
            .process_member_line1(file_version, builder, header)
    }

    async fn read_member_line2(
        &mut self,
        builder: &mut XportSchemaBuilder,
        header: &mut [u8],
    ) -> Result<()> {
        self.read_exact_with_carryover(header)
            .await
            .map_err(|e| XportError::io("Failed to read the second member data header", e))?;
        self.state.process_member_line2(builder, header)
    }

    async fn read_variable_header(
        &mut self,
        file_version: XportFileVersion,
        header: &mut [u8],
    ) -> Result<u16> {
        self.read_exact_with_carryover(header)
            .await
            .map_err(|e| XportError::io("Failed to read the namestr header", e))?;
        self.state.process_variable_header(file_version, header)
    }

    async fn read_variable(
        &mut self,
        file_version: XportFileVersion,
        variable_index: u16,
        buffer: &mut [u8],
    ) -> Result<XportVariableBuilder> {
        self.read_exact_with_carryover(buffer).await.map_err(|e| {
            XportError::io(
                variable_error("Failed read the variable header", variable_index),
                e,
            )
        })?;
        self.state
            .process_variable(file_version, variable_index, buffer)
    }

    async fn read_observation_header_v8(&mut self, header: &mut [u8]) -> Result<Option<u64>> {
        self.reader
            .read_exact(header)
            .await
            .map_err(|e| XportError::io("Failed to read the observation header", e))?;

        self.state.advance_position(header.len());
        self.state.validate_observation_header_v8(header)
    }

    pub async fn read_schema(
        &mut self,
        file_version: XportFileVersion,
    ) -> Result<Option<XportSchema>> {
        let mut header = [0u8; HEADER_LENGTH];
        let descriptor_length = self.read_member_header(file_version, &mut header).await?;
        let Some(descriptor_length) = descriptor_length else {
            return Ok(None);
        };
        let mut builder = XportSchema::builder();
        builder.variable_descriptor_length(descriptor_length);
        self.read_descriptor_header(file_version, &mut header)
            .await?;
        self.read_member_line1(file_version, &mut builder, &mut header)
            .await?;
        self.read_member_line2(&mut builder, &mut header).await?;
        let variable_count = self.read_variable_header(file_version, &mut header).await?;
        let mut variable_builders = self
            .read_variables(file_version, descriptor_length, variable_count)
            .await?;
        let (dataset_version, record_count) = self
            .read_remaining_sections(file_version, &mut variable_builders, &mut header)
            .await?;
        for variable_builder in variable_builders {
            builder.add_variable(variable_builder);
        }
        builder
            .xport_dataset_version(dataset_version)
            .record_count(record_count);

        let schema = builder.try_into().map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Validation,
                "Multiple variables had the same name",
            )
            .with_source(e)
        })?;
        Ok(Some(schema))
    }

    async fn read_variables(
        &mut self,
        file_version: XportFileVersion,
        descriptor_length: u16,
        variable_count: u16,
    ) -> Result<Vec<XportVariableBuilder>> {
        let mut variable_builders = Vec::with_capacity(variable_count as usize);
        let mut buffer = vec![0u8; descriptor_length as usize];
        for variable_index in 0..variable_count {
            let variable_builder = self
                .read_variable(file_version, variable_index, &mut buffer)
                .await?;
            variable_builders.push(variable_builder);
        }

        let trailing = u32::from(variable_count) * u32::from(descriptor_length);
        self.skip_trailing(trailing)
            .await
            .map_err(|e| XportError::io("Failed to skip trailing content after variables", e))?;

        Ok(variable_builders)
    }

    async fn read_remaining_sections(
        &mut self,
        file_version: XportFileVersion,
        variable_builders: &mut [XportVariableBuilder],
        header: &mut [u8],
    ) -> Result<(XportDatasetVersion, Option<u64>)> {
        self.reader.read_exact(header).await.map_err(|e| {
            XportError::io("Failed to read the observation or extended label header", e)
        })?;
        self.state.advance_position(header.len());

        match file_version {
            XportFileVersion::V5 => {
                let record_count = XportBufferState::process_observation_header_v5(header)?;
                Ok((XportDatasetVersion::V5, record_count))
            }
            XportFileVersion::V8 => {
                self.process_remaining_sections_v8(header, variable_builders)
                    .await
            }
        }
    }

    async fn process_remaining_sections_v8(
        &mut self,
        header: &mut [u8],
        variable_builders: &mut [XportVariableBuilder],
    ) -> Result<(XportDatasetVersion, Option<u64>)> {
        match XportBufferState::classify_remaining_section_v8(header)? {
            RemainingSectionV8::ObservationV8 => {
                let record_count = self.state.parse_observation_record_count_v8(header)?;
                Ok((XportDatasetVersion::V8, record_count))
            }
            RemainingSectionV8::LabelV8 => {
                let record_count = self
                    .read_extended_values(header, variable_builders, false)
                    .await?;
                Ok((XportDatasetVersion::V8, record_count))
            }
            RemainingSectionV8::LabelV9 => {
                let record_count = self
                    .read_extended_values(header, variable_builders, true)
                    .await?;
                Ok((XportDatasetVersion::V9, record_count))
            }
        }
    }

    async fn read_extended_values(
        &mut self,
        header: &mut [u8],
        variable_builders: &mut [XportVariableBuilder],
        include_formats: bool,
    ) -> Result<Option<u64>> {
        let extension_count = self.state.read_extension_count(header)?;
        // header length = <variable number> + <name length> + <label length> + [<format length> + <informat length>]
        let extension_header_length: usize = if include_formats { 10 } else { 6 };
        let mut extension_header = vec![0u8; extension_header_length];
        let mut extension_length = 0usize;
        for _extension_index in 0..extension_count {
            self.reader
                .read_exact(&mut extension_header)
                .await
                .map_err(|e| XportError::io("Failed to read the extended variable header", e))?;
            extension_length += extension_header.len();
            self.state.advance_position(extension_header.len());
            let variable_number = XportBufferState::find_variable_number(&extension_header)?;
            let variable_builder =
                XportBufferState::find_variable_builder(variable_number, variable_builders)?;
            extension_length += self
                .build_variable(
                    &extension_header,
                    include_formats,
                    variable_number,
                    variable_builder,
                )
                .await?;
        }

        let skip_amount = XportBufferState::extension_skip_amount(extension_length)?;
        self.skip_trailing(skip_amount).await.map_err(|e| {
            XportError::io(
                "Failed to skip trailing content after variable extensions",
                e,
            )
        })?;

        self.read_observation_header_v8(header).await
    }

    async fn build_variable(
        &mut self,
        extension_header: &[u8],
        include_formats: bool,
        variable_number: usize,
        variable_builder: &mut XportVariableBuilder,
    ) -> Result<usize> {
        let extension =
            XportBufferState::read_variable_extension_lengths(extension_header, include_formats);
        let extension_buffer_length = extension.total_length();
        let mut extension_buffer = vec![0u8; extension_buffer_length];
        self.reader
            .read_exact(&mut extension_buffer)
            .await
            .map_err(|e| {
                XportError::io(
                    format!("Failed to read the extended values for variable {variable_number}"),
                    e,
                )
            })?;
        self.state.process_variable_extension(
            include_formats,
            variable_number,
            variable_builder,
            &extension,
            &extension_buffer,
        )
    }

    pub async fn find_record(
        &mut self,
        file_version: XportFileVersion,
        offset: usize,
        record_buffer: &mut [u8],
    ) -> Result<FindRecordOutcome> {
        // Consume data from the file until we fill the current buffer. Rust doesn't guarantee we will
        // fill the buffer in a single call and, in fact, we've encountered this. If we encounter EOF,
        // read will return 0, and we know we can stop processing. As we consume bytes, we will try to
        // detect if we cross over an 80-byte boundary and check whether we are at the start of a new
        // member header (new dataset schema section).
        let remainder = offset % HEADER_LENGTH;
        let remaining = if remainder == 0 {
            0
        } else {
            HEADER_LENGTH - remainder
        };
        let mut read_sum = 0;
        let mut member_checked = false;
        while read_sum < record_buffer.len() {
            let read = self
                .reader
                .read(&mut record_buffer[read_sum..])
                .await
                .map_err(|e| XportError::io("Failed to read the next record", e))?;
            if read == 0 {
                return Ok(FindRecordOutcome::EndOfDataset);
            }
            read_sum += read;
            if !member_checked && read_sum > remaining {
                if converter::all_blank(&record_buffer[..remaining]) {
                    let outcome = self
                        .check_if_member_header(file_version, record_buffer, remaining, read_sum)
                        .await?;
                    if let Some(carryover) = outcome {
                        self.state.advance_position(remaining);
                        return Ok(FindRecordOutcome::EndOfDatasetWithCarryover(carryover));
                    }
                }
                member_checked = true;
            }
        }
        self.state.advance_position(record_buffer.len());
        Ok(FindRecordOutcome::Record)
    }

    /// Checks whether the bytes after the 80-byte boundary are the start of
    /// a new member header. Returns `Some(consumed_bytes)` if a member header
    /// was confirmed, where the Vec contains all bytes consumed from the
    /// member header (both from `record_buffer` and any extra reads). Returns
    /// `None` if this is not a member header.
    async fn check_if_member_header(
        &mut self,
        file_version: XportFileVersion,
        record_buffer: &[u8],
        start: usize,
        end: usize,
    ) -> Result<Option<Vec<u8>>> {
        let check =
            XportBufferState::check_member_header_prefix(file_version, record_buffer, start, end);
        let bytes_needed = match check {
            MemberHeaderCheck::None => return Ok(None),
            MemberHeaderCheck::Full(carryover) => return Ok(Some(carryover)),
            MemberHeaderCheck::Partial { bytes_needed } => bytes_needed,
        };
        // Read extra bytes to confirm the partial match.
        let mut extra = [0u8; HEADER_LENGTH];
        let mut extra_read = 0;
        while extra_read < bytes_needed {
            let read = self
                .reader
                .read(&mut extra[extra_read..bytes_needed])
                .await
                .map_err(|e| {
                    XportError::io(
                        "Failed when trying to read the rest of the next member header",
                        e,
                    )
                })?;
            if read == 0 {
                return Ok(None);
            }
            extra_read += read;
        }
        Ok(XportBufferState::verify_member_header_extra(
            file_version,
            record_buffer,
            start,
            end,
            &extra,
            extra_read,
        ))
    }

    async fn skip_trailing(&mut self, length: u32) -> std::io::Result<()> {
        #[allow(clippy::cast_possible_truncation)]
        let remaining = length % HEADER_LENGTH as u32;
        if remaining == 0 {
            return Ok(());
        }
        #[allow(clippy::cast_possible_truncation)]
        let buffer_size = HEADER_LENGTH as u32 - remaining;
        let mut discard = [0u8; HEADER_LENGTH];
        self.reader
            .read_exact(&mut discard[..buffer_size as usize])
            .await?;
        self.state.advance_position(buffer_size as usize);
        Ok(())
    }
}

impl<R: AsyncBufRead + AsyncSeek + Unpin> AsyncXportBuffer<R> {
    pub async fn seek_from_start(&mut self, offset: u64) -> Result<u64> {
        self.reader
            .seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| XportError::io("Failed to seek the record at the given index", e))
    }
}
