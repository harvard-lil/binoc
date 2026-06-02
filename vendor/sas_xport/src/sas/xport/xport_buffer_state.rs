use crate::sas::xport::cursor::Cursor;
use crate::sas::xport::decoder::Decoder;
use crate::sas::xport::xport_constants::{
    DESCRIPTOR_HEADER_V5, DESCRIPTOR_HEADER_V8, HEADER_LENGTH, LABEL_HEADER_V8_PREFIX,
    LABEL_HEADER_V9_PREFIX, LIBRARY_HEADER_V5, LIBRARY_HEADER_V8, MEMBER_HEADER_PREFIX_V5,
    MEMBER_HEADER_PREFIX_V8, NAMESTR_HEADER_PREFIX_V5, NAMESTR_HEADER_PREFIX_V8,
    OBSERVATION_HEADER_PREFIX_V5, OBSERVATION_HEADER_PREFIX_V8, OBSERVATION_HEADER_SUFFIX_V5,
    variable_error,
};
use crate::sas::xport::xport_error::XportErrorKind;
use crate::sas::xport::xport_variable_extension_lengths::XportVariableExtensionLengths;
use crate::sas::xport::{
    Result, XportError, XportFileVersion, XportMetadataBuilder, XportReaderOptionsInternal,
    XportSchemaBuilder, XportVariable, XportVariableBuilder, converter,
};
use crate::sas::{SasJustification, SasVariableType};

#[derive(Debug)]
pub(crate) struct XportBufferState {
    decoder: Decoder,
    metadata_decoder: Decoder,
    ascii_decoder: Decoder,
    position: usize,
}

impl XportBufferState {
    /// Creates an `XportBufferState` from the given options.
    #[must_use]
    pub fn from_options(options: &XportReaderOptionsInternal) -> Self {
        Self {
            decoder: Decoder::from_options(options),
            metadata_decoder: Decoder::metadata_from_options(options),
            ascii_decoder: Decoder::ascii(),
            position: 0,
        }
    }

    #[must_use]
    pub fn decoder(&self) -> &Decoder {
        &self.decoder
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    pub fn advance_position(&mut self, amount: usize) {
        self.position += amount;
    }

    pub fn process_library_header(&mut self, header: &[u8]) -> Result<XportFileVersion> {
        self.position += header.len();
        if header == LIBRARY_HEADER_V5 {
            Ok(XportFileVersion::V5)
        } else if header == LIBRARY_HEADER_V8 {
            Ok(XportFileVersion::V8)
        } else {
            Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Failed to determine the XPORT version. The library header record was unrecognized",
            ))
        }
    }

    pub fn process_real_header1(
        &mut self,
        builder: &mut XportMetadataBuilder,
        header: &[u8],
    ) -> Result<()> {
        self.position += header.len();
        let mut cursor = Cursor::new(header);
        let symbol1 = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding("Failed to read symbol1 from the metadata header record", e)
            })?;
        builder.symbol1(symbol1);
        let symbol2 = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding("Failed to read symbol2 from the metadata header record", e)
            })?;
        builder.symbol2(symbol2);
        let library = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding(
                    "Failed to read the library from the metadata header record",
                    e,
                )
            })?;
        builder.library(library);
        let sas_version = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding(
                    "Failed to read the version from the metadata header record",
                    e,
                )
            })?;
        builder.sas_version(sas_version);
        let operating_system =
            converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder).map_err(
                |e| {
                    XportError::encoding(
                        "Failed to read the operating system from the metadata header record",
                        e,
                    )
                },
            )?;
        builder.operating_system(operating_system);
        cursor.position(64);
        let created = converter::read_date_time(cursor.read(16)).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::InvalidDateTime,
                "Failed to read the creation date/time from the metadata header record",
            )
            .with_source(e)
        })?;
        builder.created(created);

        Ok(())
    }

    pub fn process_real_header2(
        &mut self,
        builder: &mut XportMetadataBuilder,
        header: &[u8],
    ) -> Result<()> {
        self.position += header.len();
        let modified = converter::read_date_time(&header[..16]).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::InvalidDateTime,
                "Failed to read the modification date/time from the metadata header record",
            )
            .with_source(e)
        })?;
        builder.modified(modified);
        Ok(())
    }

    pub fn process_member_header(
        &mut self,
        file_version: XportFileVersion,
        header: &[u8],
    ) -> Result<Option<u16>> {
        self.position += header.len();
        let prefix = match file_version {
            XportFileVersion::V5 => MEMBER_HEADER_PREFIX_V5,
            XportFileVersion::V8 => MEMBER_HEADER_PREFIX_V8,
        };
        if !header.starts_with(prefix) {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid member header",
            ));
        }
        let length_str =
            converter::read_string(&header[74..78], &self.ascii_decoder).map_err(|e| {
                XportError::encoding("Failed to read the variable descriptor length", e)
            })?;
        let length = length_str.parse().map_err(|e| {
            XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "The variable descriptor length was not a valid number",
            )
            .with_source(e)
        })?;
        Ok(Some(length))
    }

    pub fn process_descriptor_header(
        &mut self,
        file_version: XportFileVersion,
        header: &[u8],
    ) -> Result<()> {
        self.position += header.len();
        let expected_header = match file_version {
            XportFileVersion::V5 => DESCRIPTOR_HEADER_V5,
            XportFileVersion::V8 => DESCRIPTOR_HEADER_V8,
        };
        if header != expected_header {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid descriptor header",
            ));
        }
        Ok(())
    }

    pub fn process_member_line1(
        &mut self,
        file_version: XportFileVersion,
        builder: &mut XportSchemaBuilder,
        header: &[u8],
    ) -> Result<()> {
        self.position += header.len();

        let mut cursor = Cursor::new(header);
        let format = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding("Failed to read the format from the member data header", e)
            })?;
        builder.format(format);
        let name_length = if file_version == XportFileVersion::V8 {
            32
        } else {
            8
        };
        let dataset_name =
            converter::read_trimmed_string(cursor.read(name_length), &self.metadata_decoder)
                .map_err(|e| {
                    XportError::encoding(
                        "Failed to read the dataset name from the member data header",
                        e,
                    )
                })?;
        builder.dataset_name(dataset_name);
        let sas_data = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding("Failed to read SASDATA from the member data header", e)
            })?;
        builder.sas_data(sas_data);
        let sas_version = converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding("Failed to read the version from the member data header", e)
            })?;
        builder.version(sas_version);
        let operating_system =
            converter::read_trimmed_string(cursor.read(8), &self.metadata_decoder).map_err(
                |e| {
                    XportError::encoding(
                        "Failed to read the operating system from the member data header",
                        e,
                    )
                },
            )?;
        builder.operating_system(operating_system);
        cursor.position(64);
        let created = converter::read_date_time(cursor.read(16)).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::InvalidDateTime,
                "Encountered an invalid creation date/time while parsing the member data header",
            )
            .with_source(e)
        })?;
        builder.created(created);

        Ok(())
    }

    pub fn process_member_line2(
        &mut self,
        builder: &mut XportSchemaBuilder,
        header: &[u8],
    ) -> Result<()> {
        self.position += header.len();

        let modified = converter::read_date_time(&header[..16])
            .map_err(|e| XportError::of_kind(XportErrorKind::InvalidDateTime, "Encountered an invalid modification date/time while parsing the member data header").with_source(e))?;
        builder.modified(modified);
        let dataset_label = converter::read_trimmed_string(&header[32..72], &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding(
                    "Failed to read the dataset label from the member data header",
                    e,
                )
            })?;
        builder.dataset_label(dataset_label);
        let dataset_type = converter::read_trimmed_string(&header[72..80], &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding(
                    "Failed to read the dataset type from the member data header",
                    e,
                )
            })?;
        builder.dataset_type(dataset_type);

        Ok(())
    }

    pub fn process_variable_header(
        &mut self,
        file_version: XportFileVersion,
        header: &[u8],
    ) -> Result<u16> {
        self.position += header.len();

        let prefix = match file_version {
            XportFileVersion::V5 => NAMESTR_HEADER_PREFIX_V5,
            XportFileVersion::V8 => NAMESTR_HEADER_PREFIX_V8,
        };
        if !header.starts_with(prefix) {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid namestr header",
            ));
        }
        let count_str =
            converter::read_string(&header[54..58], &self.ascii_decoder).map_err(|e| {
                XportError::encoding(
                    "Failed to read the variable count from the namestr header",
                    e,
                )
            })?;
        let count = count_str.parse().map_err(|e| {
            XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "The variable count was not a valid number in the namestr header",
            )
            .with_source(e)
        })?;
        Ok(count)
    }

    pub fn process_variable(
        &mut self,
        file_version: XportFileVersion,
        variable_index: u16,
        buffer: &[u8],
    ) -> Result<XportVariableBuilder> {
        self.position += buffer.len();

        let mut variable_builder = XportVariable::builder();
        let value_type = SasVariableType::try_from_u16(converter::read_u16(&buffer[0..2]))
            .ok_or_else(|| {
                XportError::of_kind(
                    XportErrorKind::InvalidFormat,
                    variable_error("Invalid variable type code", variable_index),
                )
            })?;
        variable_builder.value_type(value_type);
        variable_builder.hash(converter::read_u16(&buffer[2..4]));
        variable_builder.value_length(converter::read_u16(&buffer[4..6]));
        variable_builder.number(converter::read_u16(&buffer[6..8]));
        let short_name = converter::read_trimmed_string(&buffer[8..16], &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding(
                    variable_error("Could not read the short name", variable_index),
                    e,
                )
            })?;
        variable_builder.short_name(short_name);
        let short_label = converter::read_trimmed_string(&buffer[16..56], &self.metadata_decoder)
            .map_err(|e| {
            XportError::encoding(
                variable_error("Could not read the short label", variable_index),
                e,
            )
        })?;
        variable_builder.short_label(short_label);
        let short_format = converter::read_trimmed_string(&buffer[56..64], &self.metadata_decoder)
            .map_err(|e| {
                XportError::encoding(
                    variable_error("Could not read the short format", variable_index),
                    e,
                )
            })?;
        variable_builder.short_format(short_format);
        variable_builder.format_length(converter::read_u16(&buffer[64..66]));
        variable_builder.format_precision(converter::read_u16(&buffer[66..68]));
        let justification = SasJustification::try_from_u16(converter::read_u16(&buffer[68..70]))
            .ok_or_else(|| {
                XportError::of_kind(
                    XportErrorKind::InvalidFormat,
                    variable_error("Invalid justification code", variable_index),
                )
            })?;
        variable_builder.justification(justification);
        let short_input_format =
            converter::read_trimmed_string(&buffer[72..80], &self.metadata_decoder).map_err(
                |e| {
                    XportError::encoding(
                        variable_error("Could not read the short input format", variable_index),
                        e,
                    )
                },
            )?;
        variable_builder.short_input_format(short_input_format);
        variable_builder.input_format_length(converter::read_u16(&buffer[80..82]));
        variable_builder.input_format_precision(converter::read_u16(&buffer[82..84]));
        variable_builder.position(converter::read_u32(&buffer[84..88]));

        if file_version == XportFileVersion::V8 {
            let medium_name_end_index =
                88 + XportVariable::MAX_MEDIUM_NAME_LENGTH_IN_BYTES as usize;
            let medium_name = converter::read_trimmed_string(
                &buffer[88..medium_name_end_index],
                &self.metadata_decoder,
            )
            .map_err(|e| {
                XportError::encoding(
                    variable_error("Could not read the medium name", variable_index),
                    e,
                )
            })?;
            variable_builder.medium_name(medium_name);
        }

        Ok(variable_builder)
    }

    pub fn read_extension_count(&mut self, header: &[u8]) -> Result<u32> {
        let suffix = &header[LABEL_HEADER_V8_PREFIX.len()..];
        let count = converter::read_trimmed_string(suffix, &self.ascii_decoder)
            .map_err(|e| XportError::encoding("Failed to read the extended variable count", e))?;
        if count.is_empty() {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "The extended variable count was missing from the label header",
            ));
        }
        count.parse().map_err(|e| {
            XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "The extended variable count was not a valid number",
            )
            .with_source(e)
        })
    }

    pub fn starts_with_observation_header_v8(header: &[u8]) -> bool {
        header.starts_with(OBSERVATION_HEADER_PREFIX_V8)
    }

    pub fn starts_with_label_header_v8(header: &[u8]) -> bool {
        header.starts_with(LABEL_HEADER_V8_PREFIX)
    }

    pub fn starts_with_label_header_v9(header: &[u8]) -> bool {
        header.starts_with(LABEL_HEADER_V9_PREFIX)
    }

    pub fn process_observation_header_v5(header: &[u8]) -> Result<Option<u64>> {
        if !header.starts_with(OBSERVATION_HEADER_PREFIX_V5) {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid observation header",
            ));
        }
        let suffix = &header[OBSERVATION_HEADER_PREFIX_V5.len()..];
        if suffix != OBSERVATION_HEADER_SUFFIX_V5 {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid observation header suffix",
            ));
        }
        Ok(None)
    }

    pub fn validate_observation_header_v8(&mut self, header: &[u8]) -> Result<Option<u64>> {
        if !header.starts_with(OBSERVATION_HEADER_PREFIX_V8) {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid observation header",
            ));
        }
        self.parse_observation_record_count_v8(header)
    }

    pub fn parse_observation_record_count_v8(&mut self, header: &[u8]) -> Result<Option<u64>> {
        let suffix = &header[OBSERVATION_HEADER_PREFIX_V8.len()..];
        let record_count =
            converter::read_string(&suffix[..15], &self.ascii_decoder).map_err(|e| {
                XportError::encoding(
                    "Failed to read the record count from the observation header",
                    e,
                )
            })?;
        let record_count = record_count.trim_start_matches(' ');
        if record_count.is_empty() {
            return Ok(None);
        }
        Ok(record_count.parse().ok())
    }

    pub fn find_variable_number(extension_header: &[u8]) -> Result<usize> {
        // Base-1 offset
        let variable_number = converter::read_u16(&extension_header[0..2]) as usize;
        if variable_number == 0 {
            return Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "An extension variable number was not 1-based",
            ));
        }
        // Convert to Base-0
        Ok(variable_number - 1)
    }

    pub fn find_variable_builder(
        variable_number: usize,
        variable_builders: &mut [XportVariableBuilder],
    ) -> Result<&mut XportVariableBuilder> {
        variable_builders.get_mut(variable_number).ok_or_else(|| {
            XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "An extension variable number was out-of-range",
            )
        })
    }

    pub fn read_variable_extension_lengths(
        header: &[u8],
        include_formats: bool,
    ) -> XportVariableExtensionLengths {
        let name_length = converter::read_u16(&header[2..4]);
        let label_length = converter::read_u16(&header[4..6]);
        let (format_length, input_format_length) = if include_formats {
            let format_length = converter::read_u16(&header[6..8]);
            let input_format_length = converter::read_u16(&header[8..10]);
            (format_length, input_format_length)
        } else {
            (0, 0)
        };
        XportVariableExtensionLengths::new(
            name_length,
            label_length,
            format_length,
            input_format_length,
        )
    }

    pub fn process_variable_extension(
        &mut self,
        include_formats: bool,
        variable_number: usize,
        variable_builder: &mut XportVariableBuilder,
        extension: &XportVariableExtensionLengths,
        extension_buffer: &[u8],
    ) -> Result<usize> {
        self.position += extension_buffer.len();

        let mut cursor = Cursor::new(extension_buffer);
        let long_name =
            converter::read_trimmed_string(cursor.read(extension.name()), &self.metadata_decoder)
                .map_err(|e| {
                XportError::encoding(
                    format!("Failed to read the extended name for variable {variable_number}"),
                    e,
                )
            })?;
        variable_builder.long_name(long_name);
        let long_label =
            converter::read_trimmed_string(cursor.read(extension.label()), &self.metadata_decoder)
                .map_err(|e| {
                    XportError::encoding(
                        format!("Failed to read the extended label for variable {variable_number}"),
                        e,
                    )
                })?;
        variable_builder.long_label(long_label);
        if include_formats {
            let long_format = converter::read_trimmed_string(
                cursor.read(extension.format()),
                &self.metadata_decoder,
            )
            .map_err(|e| {
                XportError::encoding(
                    format!("Failed to read the extended format for variable {variable_number}"),
                    e,
                )
            })?;
            variable_builder.long_format(long_format);
            let long_input_format = converter::read_trimmed_string(
                cursor.read(extension.input_format()),
                &self.metadata_decoder,
            )
            .map_err(|e| {
                XportError::encoding(
                    format!(
                        "Failed to read the extended input format for variable {variable_number}"
                    ),
                    e,
                )
            })?;
            variable_builder.long_input_format(long_input_format);
        }
        Ok(extension_buffer.len())
    }

    #[must_use]
    pub fn member_header_prefix(file_version: XportFileVersion) -> &'static [u8] {
        match file_version {
            XportFileVersion::V5 => MEMBER_HEADER_PREFIX_V5,
            XportFileVersion::V8 => MEMBER_HEADER_PREFIX_V8,
        }
    }

    /// Converts an extension length to a `u32` for skip-trailing arithmetic,
    /// returning an error if the value overflows.
    pub fn extension_skip_amount(extension_length: usize) -> Result<u32> {
        u32::try_from(extension_length).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "Overflow occurred while determining the extension lengths",
            )
            .with_source(e)
        })
    }

    /// Applies carryover bytes to the front of `buf`. Returns the number of
    /// bytes consumed from carryover. If the carryover is larger than `buf`,
    /// the excess is saved back into `carryover` for the next call.
    pub fn apply_carryover(carryover: &mut Option<Vec<u8>>, buf: &mut [u8]) -> usize {
        if let Some(data) = carryover.take() {
            let used = data.len().min(buf.len());
            buf[..used].copy_from_slice(&data[..used]);
            if data.len() > buf.len() {
                *carryover = Some(data[buf.len()..].to_vec());
            }
            used
        } else {
            0
        }
    }

    /// Classifies a V8/V9 remaining-section header. Returns `Ok` with the
    /// section type, or `Err` if the header is unrecognized.
    pub fn classify_remaining_section_v8(header: &[u8]) -> Result<RemainingSectionV8> {
        if Self::starts_with_observation_header_v8(header) {
            Ok(RemainingSectionV8::ObservationV8)
        } else if Self::starts_with_label_header_v8(header) {
            Ok(RemainingSectionV8::LabelV8)
        } else if Self::starts_with_label_header_v9(header) {
            Ok(RemainingSectionV8::LabelV9)
        } else {
            Err(XportError::of_kind(
                XportErrorKind::InvalidFormat,
                "Encountered an invalid observation or extended label header",
            ))
        }
    }

    /// Checks whether the bytes at `record_buffer[start..end]` begin with
    /// a member header prefix. Returns the result of the prefix check,
    /// which may indicate that additional bytes need to be read to confirm.
    #[must_use]
    pub fn check_member_header_prefix(
        file_version: XportFileVersion,
        record_buffer: &[u8],
        start: usize,
        end: usize,
    ) -> MemberHeaderCheck {
        let member_header = Self::member_header_prefix(file_version);
        let bytes_in_buffer = usize::min(end - start, member_header.len());
        if !record_buffer[start..end].starts_with(&member_header[..bytes_in_buffer]) {
            return MemberHeaderCheck::None;
        }
        if bytes_in_buffer == member_header.len() {
            let carryover = record_buffer[start..end].to_vec();
            return MemberHeaderCheck::Full(carryover);
        }
        let bytes_already_read = end - start;
        MemberHeaderCheck::Partial {
            bytes_needed: HEADER_LENGTH - bytes_already_read,
        }
    }

    /// After reading extra bytes to confirm a partial member header match,
    /// verifies whether the combined data forms a valid member header.
    /// Returns `Some(carryover)` if confirmed, `None` otherwise.
    #[must_use]
    pub fn verify_member_header_extra(
        file_version: XportFileVersion,
        record_buffer: &[u8],
        start: usize,
        end: usize,
        extra: &[u8],
        extra_read: usize,
    ) -> Option<Vec<u8>> {
        let member_header = Self::member_header_prefix(file_version);
        let bytes_already_read = end - start;
        let bytes_in_buffer = usize::min(bytes_already_read, member_header.len());
        let prefix_bytes_remaining = member_header.len().saturating_sub(bytes_already_read);
        let is_member_header = prefix_bytes_remaining == 0
            || extra[..extra_read].starts_with(&member_header[bytes_in_buffer..]);
        if is_member_header {
            let carryover_len = bytes_already_read + extra_read;
            let mut carryover = Vec::with_capacity(carryover_len);
            carryover.extend_from_slice(&record_buffer[start..end]);
            carryover.extend_from_slice(&extra[..extra_read]);
            Some(carryover)
        } else {
            None
        }
    }
}

/// Classifies the header at the start of remaining sections in a V8/V9 dataset.
#[derive(Debug)]
pub(crate) enum RemainingSectionV8 {
    /// The header is a V8 observation header.
    ObservationV8,
    /// The header is a V8 extended label header (no format extensions).
    LabelV8,
    /// The header is a V9 extended label header (includes format extensions).
    LabelV9,
}

/// Result of checking whether bytes at an 80-byte boundary match
/// a member header prefix.
#[derive(Debug)]
pub(crate) enum MemberHeaderCheck {
    /// The bytes do not match the member header prefix.
    None,
    /// The full prefix was found in the record buffer. Contains the
    /// carryover bytes (everything from the boundary onwards).
    Full(Vec<u8>),
    /// A partial prefix was found. The caller must read
    /// `bytes_needed` more bytes from I/O and then call
    /// `verify_member_header_extra`.
    Partial { bytes_needed: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sas::xport::XportReaderOptions;
    use crate::sas::xport::XportSchema;
    use crate::sas::xport::xport_constants::HEADER_LENGTH;

    /// Creates a default `XportBufferState` with UTF-8 encoding for use in tests.
    fn make_state() -> XportBufferState {
        XportBufferState::from_options(&XportReaderOptions::default().build())
    }

    /// Builds a valid 80-byte member header for the given version, with the
    /// descriptor length encoded as a zero-padded 4-character ASCII string at
    /// bytes 74..78 (e.g., "0140"). This matches the SAS spec format.
    fn make_member_header(
        file_version: XportFileVersion,
        descriptor_length: u16,
    ) -> [u8; HEADER_LENGTH] {
        let prefix = match file_version {
            XportFileVersion::V5 => MEMBER_HEADER_PREFIX_V5,
            XportFileVersion::V8 => MEMBER_HEADER_PREFIX_V8,
        };
        let mut header = [b' '; HEADER_LENGTH];
        header[..prefix.len()].copy_from_slice(prefix);
        let length_str = format!("{descriptor_length:04}");
        header[74..78].copy_from_slice(length_str.as_bytes());
        header
    }

    /// Builds a valid 80-byte namestr header for the given version, with the
    /// variable count encoded as a zero-padded 4-character ASCII string at
    /// bytes 54..58 (e.g., "0018"). This matches the SAS spec format.
    fn make_namestr_header(
        file_version: XportFileVersion,
        variable_count: u16,
    ) -> [u8; HEADER_LENGTH] {
        let prefix = match file_version {
            XportFileVersion::V5 => NAMESTR_HEADER_PREFIX_V5,
            XportFileVersion::V8 => NAMESTR_HEADER_PREFIX_V8,
        };
        let mut header = [b' '; HEADER_LENGTH];
        header[..prefix.len()].copy_from_slice(prefix);
        let count_str = format!("{variable_count:04}");
        header[54..58].copy_from_slice(count_str.as_bytes());
        header
    }

    /// Builds a valid 80-byte observation header for V8, with the record count
    /// encoded as a 15-character right-justified ASCII string after the prefix.
    fn make_observation_header_v8(record_count: Option<u64>) -> [u8; HEADER_LENGTH] {
        let mut header = [b' '; HEADER_LENGTH];
        header[..OBSERVATION_HEADER_PREFIX_V8.len()].copy_from_slice(OBSERVATION_HEADER_PREFIX_V8);
        if let Some(count) = record_count {
            let count_str = format!("{count:>15}");
            let start = OBSERVATION_HEADER_PREFIX_V8.len();
            header[start..start + 15].copy_from_slice(count_str.as_bytes());
        }
        header
    }

    // -----------------------------------------------------------------------
    // process_library_header
    // -----------------------------------------------------------------------

    #[test]
    fn library_header_recognizes_v5() {
        // The exact V5 library header should be recognized and return V5.
        let mut state = make_state();
        let result = state.process_library_header(LIBRARY_HEADER_V5);
        assert_eq!(XportFileVersion::V5, result.unwrap());
    }

    #[test]
    fn library_header_recognizes_v8() {
        // The exact V8 library header should be recognized and return V8.
        let mut state = make_state();
        let result = state.process_library_header(LIBRARY_HEADER_V8);
        assert_eq!(XportFileVersion::V8, result.unwrap());
    }

    #[test]
    fn library_header_rejects_unrecognized() {
        // A header that doesn't match either V5 or V8 should be rejected.
        let mut state = make_state();
        let garbage = [b'X'; HEADER_LENGTH];
        assert!(state.process_library_header(&garbage).is_err());
    }

    #[test]
    fn library_header_advances_position() {
        // Processing a header should advance the position by the header length.
        let mut state = make_state();
        assert_eq!(0, state.position());
        let _ = state.process_library_header(LIBRARY_HEADER_V5);
        assert_eq!(HEADER_LENGTH, state.position());
    }

    // -----------------------------------------------------------------------
    // process_member_header
    // -----------------------------------------------------------------------

    #[test]
    fn member_header_v5_parses_descriptor_length() {
        // A valid V5 member header with the standard descriptor length of 140.
        let mut state = make_state();
        let header = make_member_header(XportFileVersion::V5, 140);
        let result = state
            .process_member_header(XportFileVersion::V5, &header)
            .unwrap();
        assert_eq!(Some(140), result);
    }

    #[test]
    fn member_header_v5_parses_vax_vms_descriptor_length() {
        // Per the SAS TS-140 spec, VAX/VMS systems use a 136-byte descriptor
        // instead of the standard 140. The parser should accept both.
        let mut state = make_state();
        let header = make_member_header(XportFileVersion::V5, 136);
        let result = state
            .process_member_header(XportFileVersion::V5, &header)
            .unwrap();
        assert_eq!(Some(136), result);
    }

    #[test]
    fn member_header_v8_parses_descriptor_length() {
        // A valid V8 member header with descriptor length 140.
        let mut state = make_state();
        let header = make_member_header(XportFileVersion::V8, 140);
        let result = state
            .process_member_header(XportFileVersion::V8, &header)
            .unwrap();
        assert_eq!(Some(140), result);
    }

    #[test]
    fn member_header_rejects_wrong_prefix() {
        // A header that doesn't start with the expected member prefix should fail.
        let mut state = make_state();
        let garbage = [b'X'; HEADER_LENGTH];
        assert!(
            state
                .process_member_header(XportFileVersion::V5, &garbage)
                .is_err()
        );
    }

    #[test]
    fn member_header_rejects_non_numeric_length() {
        // The descriptor length field contains non-numeric characters.
        let mut state = make_state();
        let mut header = make_member_header(XportFileVersion::V5, 140);
        header[74..78].copy_from_slice(b"ABCD");
        assert!(
            state
                .process_member_header(XportFileVersion::V5, &header)
                .is_err()
        );
    }

    // -----------------------------------------------------------------------
    // process_descriptor_header
    // -----------------------------------------------------------------------

    #[test]
    fn descriptor_header_v5_accepts_valid() {
        // The exact V5 descriptor header should be accepted.
        let mut state = make_state();
        assert!(
            state
                .process_descriptor_header(XportFileVersion::V5, DESCRIPTOR_HEADER_V5)
                .is_ok()
        );
    }

    #[test]
    fn descriptor_header_v8_accepts_valid() {
        // The exact V8 descriptor header should be accepted.
        let mut state = make_state();
        assert!(
            state
                .process_descriptor_header(XportFileVersion::V8, DESCRIPTOR_HEADER_V8)
                .is_ok()
        );
    }

    #[test]
    fn descriptor_header_rejects_mismatch() {
        // Passing a V5 header when V8 is expected should fail (exact match required).
        let mut state = make_state();
        assert!(
            state
                .process_descriptor_header(XportFileVersion::V8, DESCRIPTOR_HEADER_V5)
                .is_err()
        );
    }

    // -----------------------------------------------------------------------
    // process_variable_header (namestr header)
    // -----------------------------------------------------------------------

    #[test]
    fn variable_header_v5_parses_count() {
        // A valid V5 namestr header with variable count 18 at bytes 54..58.
        let mut state = make_state();
        let header = make_namestr_header(XportFileVersion::V5, 18);
        let count = state
            .process_variable_header(XportFileVersion::V5, &header)
            .unwrap();
        assert_eq!(18, count);
    }

    #[test]
    fn variable_header_v8_parses_count() {
        // A valid V8 namestr header with variable count 49.
        let mut state = make_state();
        let header = make_namestr_header(XportFileVersion::V8, 49);
        let count = state
            .process_variable_header(XportFileVersion::V8, &header)
            .unwrap();
        assert_eq!(49, count);
    }

    #[test]
    fn variable_header_rejects_wrong_prefix() {
        // A header with an unrecognized prefix should fail.
        let mut state = make_state();
        let garbage = [b'X'; HEADER_LENGTH];
        assert!(
            state
                .process_variable_header(XportFileVersion::V5, &garbage)
                .is_err()
        );
    }

    #[test]
    fn variable_header_rejects_non_numeric_count() {
        // The variable count field contains non-numeric characters.
        let mut state = make_state();
        let mut header = make_namestr_header(XportFileVersion::V5, 10);
        header[54..58].copy_from_slice(b"XXXX");
        assert!(
            state
                .process_variable_header(XportFileVersion::V5, &header)
                .is_err()
        );
    }

    // -----------------------------------------------------------------------
    // process_observation_header_v5
    // -----------------------------------------------------------------------

    #[test]
    fn observation_header_v5_accepts_valid() {
        // The exact V5 observation header should be accepted with no record count.
        let mut header = [b' '; HEADER_LENGTH];
        header[..OBSERVATION_HEADER_PREFIX_V5.len()].copy_from_slice(OBSERVATION_HEADER_PREFIX_V5);
        header[OBSERVATION_HEADER_PREFIX_V5.len()..].copy_from_slice(OBSERVATION_HEADER_SUFFIX_V5);
        let result = XportBufferState::process_observation_header_v5(&header);
        assert_eq!(None, result.unwrap());
    }

    #[test]
    fn observation_header_v5_rejects_invalid() {
        // Any deviation from the exact V5 observation header should fail.
        let garbage = [b'X'; HEADER_LENGTH];
        assert!(XportBufferState::process_observation_header_v5(&garbage).is_err());
    }

    // -----------------------------------------------------------------------
    // validate_observation_header_v8 / parse_observation_record_count_v8
    // -----------------------------------------------------------------------

    #[test]
    fn observation_header_v8_validates_and_parses_count() {
        // A valid V8 observation header with record count 1000.
        let mut state = make_state();
        let header = make_observation_header_v8(Some(1000));
        let result = state.validate_observation_header_v8(&header).unwrap();
        assert_eq!(Some(1000), result);
    }

    #[test]
    fn observation_header_v8_blank_count_returns_none() {
        // A valid V8 observation header with no record count (all spaces).
        // Some files omit the count; the parser should return None rather than error.
        let mut state = make_state();
        let header = make_observation_header_v8(None);
        let result = state.validate_observation_header_v8(&header).unwrap();
        assert_eq!(None, result);
    }

    #[test]
    fn observation_header_v8_rejects_invalid_prefix() {
        // A header with the wrong prefix should fail validation.
        let mut state = make_state();
        let garbage = [b'X'; HEADER_LENGTH];
        assert!(state.validate_observation_header_v8(&garbage).is_err());
    }

    #[test]
    fn parse_observation_record_count_v8_skips_prefix_validation() {
        // parse_observation_record_count_v8 trusts the prefix and only parses
        // the count. This is used when the caller has already verified the prefix.
        let mut state = make_state();
        let header = make_observation_header_v8(Some(42));
        let result = state.parse_observation_record_count_v8(&header).unwrap();
        assert_eq!(Some(42), result);
    }

    // -----------------------------------------------------------------------
    // read_extension_count
    // -----------------------------------------------------------------------

    #[test]
    fn extension_count_parses_valid_number() {
        // A LABELV8 header with extension count "5" after the prefix.
        let mut state = make_state();
        let mut header = [b' '; HEADER_LENGTH];
        header[..LABEL_HEADER_V8_PREFIX.len()].copy_from_slice(LABEL_HEADER_V8_PREFIX);
        let start = LABEL_HEADER_V8_PREFIX.len();
        header[start..=start].copy_from_slice(b"5");
        let count = state.read_extension_count(&header).unwrap();
        assert_eq!(5, count);
    }

    #[test]
    fn extension_count_rejects_blank() {
        // A LABELV8 header with an all-blank suffix should be treated as an error,
        // since a present LABELV8 header must contain a valid count.
        let mut state = make_state();
        let mut header = [b' '; HEADER_LENGTH];
        header[..LABEL_HEADER_V8_PREFIX.len()].copy_from_slice(LABEL_HEADER_V8_PREFIX);
        assert!(state.read_extension_count(&header).is_err());
    }

    #[test]
    fn extension_count_rejects_non_numeric() {
        // A LABELV8 header with non-numeric text in the count field.
        let mut state = make_state();
        let mut header = [b' '; HEADER_LENGTH];
        header[..LABEL_HEADER_V8_PREFIX.len()].copy_from_slice(LABEL_HEADER_V8_PREFIX);
        let start = LABEL_HEADER_V8_PREFIX.len();
        header[start..start + 3].copy_from_slice(b"abc");
        assert!(state.read_extension_count(&header).is_err());
    }

    // -----------------------------------------------------------------------
    // find_variable_number
    // -----------------------------------------------------------------------

    #[test]
    fn find_variable_number_converts_from_base_1() {
        // Variable numbers in the file are 1-based; the result should be 0-based.
        let header = 1u16.to_be_bytes();
        assert_eq!(0, XportBufferState::find_variable_number(&header).unwrap());
    }

    #[test]
    fn find_variable_number_handles_large_index() {
        // Variable number 100 (1-based) should return 99 (0-based).
        let header = 100u16.to_be_bytes();
        assert_eq!(99, XportBufferState::find_variable_number(&header).unwrap());
    }

    #[test]
    fn find_variable_number_rejects_zero() {
        // Variable number 0 is invalid since the encoding is 1-based.
        let header = 0u16.to_be_bytes();
        assert!(XportBufferState::find_variable_number(&header).is_err());
    }

    // -----------------------------------------------------------------------
    // starts_with helpers
    // -----------------------------------------------------------------------

    #[test]
    fn starts_with_observation_header_v8_matches() {
        // A header starting with the V8 observation prefix should match.
        let mut header = [b' '; HEADER_LENGTH];
        header[..OBSERVATION_HEADER_PREFIX_V8.len()].copy_from_slice(OBSERVATION_HEADER_PREFIX_V8);
        assert!(XportBufferState::starts_with_observation_header_v8(&header));
    }

    #[test]
    fn starts_with_observation_header_v8_rejects_mismatch() {
        // A header that does not start with the V8 observation prefix should not match.
        let header = [b'X'; HEADER_LENGTH];
        assert!(!XportBufferState::starts_with_observation_header_v8(
            &header
        ));
    }

    #[test]
    fn starts_with_label_header_v8_matches() {
        // A header starting with the LABELV8 prefix should match.
        let mut header = [b' '; HEADER_LENGTH];
        header[..LABEL_HEADER_V8_PREFIX.len()].copy_from_slice(LABEL_HEADER_V8_PREFIX);
        assert!(XportBufferState::starts_with_label_header_v8(&header));
    }

    #[test]
    fn starts_with_label_header_v9_matches() {
        // A header starting with the LABELV9 prefix should match.
        let mut header = [b' '; HEADER_LENGTH];
        header[..LABEL_HEADER_V9_PREFIX.len()].copy_from_slice(LABEL_HEADER_V9_PREFIX);
        assert!(XportBufferState::starts_with_label_header_v9(&header));
    }

    #[test]
    fn starts_with_label_header_v8_does_not_match_v9() {
        // LABELV9 prefix should not match the LABELV8 check. These headers
        // are distinct because V9 includes format/informat extensions that V8 does not.
        let mut header = [b' '; HEADER_LENGTH];
        header[..LABEL_HEADER_V9_PREFIX.len()].copy_from_slice(LABEL_HEADER_V9_PREFIX);
        assert!(!XportBufferState::starts_with_label_header_v8(&header));
    }

    // -----------------------------------------------------------------------
    // member_header_prefix
    // -----------------------------------------------------------------------

    #[test]
    fn member_header_prefix_v5() {
        assert_eq!(
            MEMBER_HEADER_PREFIX_V5,
            XportBufferState::member_header_prefix(XportFileVersion::V5)
        );
    }

    #[test]
    fn member_header_prefix_v8() {
        assert_eq!(
            MEMBER_HEADER_PREFIX_V8,
            XportBufferState::member_header_prefix(XportFileVersion::V8)
        );
    }

    // -----------------------------------------------------------------------
    // Helpers for building schema and variable buffers
    // -----------------------------------------------------------------------

    /// Writes `text` into `buf` at `offset`, space-padding to `width`.
    fn write_ascii(buf: &mut [u8], offset: usize, text: &str, width: usize) {
        let bytes = text.as_bytes();
        let len = bytes.len().min(width);
        buf[offset..offset + len].copy_from_slice(&bytes[..len]);
        // Remaining bytes stay as the initial fill (spaces)
    }

    /// Writes a u16 in big-endian at the given offset.
    fn write_u16_be(buf: &mut [u8], offset: usize, value: u16) {
        buf[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    /// Writes a u32 in big-endian at the given offset.
    fn write_u32_be(buf: &mut [u8], offset: usize, value: u32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    /// Writes a `SasDateTime` in the 16-byte format `DDMMMYY:HH:mm:ss`.
    fn write_datetime(buf: &mut [u8], offset: usize, dt: crate::sas::SasDateTime) {
        let formatted = dt.to_string();
        buf[offset..offset + 16].copy_from_slice(formatted.as_bytes());
    }

    /// The default timestamp used in test buffers: 01JAN00:00:00:00.
    fn test_datetime() -> crate::sas::SasDateTime {
        crate::sas::SasDateTime::new()
    }

    /// Builds a valid 80-byte member line 1 header for V5 (8-byte dataset name).
    /// Layout: `format`(0..8), `name`(8..16), `sas_data`(16..24), `version`(24..32),
    ///         `OS`(32..40), [gap 40..64], `created`(64..80)
    fn make_member_line1_v5(
        format: &str,
        dataset_name: &str,
        sas_data: &str,
        version: &str,
        os: &str,
        created: crate::sas::SasDateTime,
    ) -> [u8; HEADER_LENGTH] {
        let mut buf = [b' '; HEADER_LENGTH];
        write_ascii(&mut buf, 0, format, 8);
        write_ascii(&mut buf, 8, dataset_name, 8);
        write_ascii(&mut buf, 16, sas_data, 8);
        write_ascii(&mut buf, 24, version, 8);
        write_ascii(&mut buf, 32, os, 8);
        write_datetime(&mut buf, 64, created);
        buf
    }

    /// Builds a valid 80-byte member line 1 header for V8 (32-byte dataset name).
    /// Layout: `format`(0..8), `name`(8..40), `sas_data`(40..48), `version`(48..56),
    ///         `OS`(56..64), `created`(64..80)
    fn make_member_line1_v8(
        format: &str,
        dataset_name: &str,
        sas_data: &str,
        version: &str,
        os: &str,
        created: crate::sas::SasDateTime,
    ) -> [u8; HEADER_LENGTH] {
        let mut buf = [b' '; HEADER_LENGTH];
        write_ascii(&mut buf, 0, format, 8);
        write_ascii(&mut buf, 8, dataset_name, 32);
        write_ascii(&mut buf, 40, sas_data, 8);
        write_ascii(&mut buf, 48, version, 8);
        write_ascii(&mut buf, 56, os, 8);
        write_datetime(&mut buf, 64, created);
        buf
    }

    /// Builds a valid 80-byte member line 2 header.
    /// Layout: `modified`(0..16), [gap 16..32], `label`(32..72), `type`(72..80)
    fn make_member_line2(
        modified: crate::sas::SasDateTime,
        dataset_label: &str,
        dataset_type: &str,
    ) -> [u8; HEADER_LENGTH] {
        let mut buf = [b' '; HEADER_LENGTH];
        write_datetime(&mut buf, 0, modified);
        write_ascii(&mut buf, 32, dataset_label, 40);
        write_ascii(&mut buf, 72, dataset_type, 8);
        buf
    }

    /// Builds a 140-byte NAMESTR (variable descriptor) buffer from an `XportVariable`.
    /// This is the standard V5 layout; V8 adds a 32-byte medium name at offset 88.
    fn make_variable_descriptor(variable: &XportVariable, include_medium_name: bool) -> Vec<u8> {
        let length = 140;
        let mut buf = vec![b' '; length];
        write_u16_be(&mut buf, 0, variable.value_type().code());
        write_u16_be(&mut buf, 2, variable.hash());
        write_u16_be(&mut buf, 4, variable.value_length());
        write_u16_be(&mut buf, 6, variable.number());
        write_ascii(&mut buf, 8, variable.short_name(), 8);
        write_ascii(&mut buf, 16, variable.short_label(), 40);
        write_ascii(&mut buf, 56, variable.short_format(), 8);
        write_u16_be(&mut buf, 64, variable.format_length());
        write_u16_be(&mut buf, 66, variable.format_precision());
        write_u16_be(&mut buf, 68, u16::from(variable.justification()));
        // bytes 70..72 are unused
        buf[70] = 0;
        buf[71] = 0;
        write_ascii(&mut buf, 72, variable.short_input_format(), 8);
        write_u16_be(&mut buf, 80, variable.input_format_length());
        write_u16_be(&mut buf, 82, variable.input_format_precision());
        write_u32_be(&mut buf, 84, variable.position());
        if include_medium_name {
            write_ascii(&mut buf, 88, variable.medium_name(), 32);
        }
        buf
    }

    /// Builds a LABELV8/V9 extension header for one variable.
    /// V8 layout (6 bytes): `variable_number`(2) + `name_len`(2) + `label_len`(2)
    /// V9 layout (10 bytes): `variable_number`(2) + `name_len`(2) + `label_len`(2)
    ///                        + `format_len`(2) + `informat_len`(2)
    fn make_extension_header(
        variable_number: u16,
        name: &str,
        label: &str,
        format: Option<&str>,
        input_format: Option<&str>,
    ) -> Vec<u8> {
        let include_formats = format.is_some();
        let len = if include_formats { 10 } else { 6 };
        let mut buf = vec![0u8; len];
        write_u16_be(&mut buf, 0, variable_number);
        #[allow(clippy::cast_possible_truncation)] // Test data is always small
        {
            write_u16_be(&mut buf, 2, name.len() as u16);
            write_u16_be(&mut buf, 4, label.len() as u16);
            if include_formats {
                write_u16_be(&mut buf, 6, format.unwrap().len() as u16);
                write_u16_be(&mut buf, 8, input_format.unwrap().len() as u16);
            }
        }
        buf
    }

    /// Builds the extension data buffer (name + label + optional format + informat).
    fn make_extension_buffer(
        name: &str,
        label: &str,
        format: Option<&str>,
        input_format: Option<&str>,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(label.as_bytes());
        if let Some(f) = format {
            buf.extend_from_slice(f.as_bytes());
        }
        if let Some(f) = input_format {
            buf.extend_from_slice(f.as_bytes());
        }
        buf
    }

    // -----------------------------------------------------------------------
    // process_member_line1 / process_member_line2
    // -----------------------------------------------------------------------

    #[test]
    fn member_line1_v5_parses_all_fields() {
        // A V5 member line 1 header with all fields populated.
        // V5 uses an 8-byte dataset name field.
        let mut state = make_state();
        let created = test_datetime();
        let header = make_member_line1_v5("SAS", "AE", "SASDATA", "9.0401M5", "X64_SR12", created);
        let mut builder = XportSchema::builder();
        state
            .process_member_line1(XportFileVersion::V5, &mut builder, &header)
            .unwrap();
        let schema = builder.try_build().unwrap();
        assert_eq!("SAS", schema.format());
        assert_eq!("AE", schema.dataset_name());
        assert_eq!("SASDATA", schema.sas_data());
        assert_eq!("9.0401M5", schema.version());
        assert_eq!("X64_SR12", schema.operating_system());
        assert_eq!(created, schema.created());
    }

    #[test]
    fn member_line1_v8_parses_long_dataset_name() {
        // V8 uses a 32-byte dataset name field, allowing longer names.
        let mut state = make_state();
        let created = test_datetime();
        let header =
            make_member_line1_v8("SAS", "LONG_DATASET_NAME", "SASDATA", "9.1", "WIN", created);
        let mut builder = XportSchema::builder();
        state
            .process_member_line1(XportFileVersion::V8, &mut builder, &header)
            .unwrap();
        let schema = builder.try_build().unwrap();
        assert_eq!("LONG_DATASET_NAME", schema.dataset_name());
    }

    #[test]
    fn member_line2_parses_all_fields() {
        // Member line 2 contains the modified timestamp, dataset label, and type.
        let mut state = make_state();
        let modified = test_datetime();
        let header = make_member_line2(modified, "Adverse Events", "Data");
        let mut builder = XportSchema::builder();
        state.process_member_line2(&mut builder, &header).unwrap();
        let schema = builder.try_build().unwrap();
        assert_eq!(modified, schema.modified());
        assert_eq!("Adverse Events", schema.dataset_label());
        assert_eq!("Data", schema.dataset_type());
    }

    #[test]
    fn member_line2_handles_blank_label_and_type() {
        // Both label and type can be blank (all spaces).
        let mut state = make_state();
        let modified = test_datetime();
        let header = make_member_line2(modified, "", "");
        let mut builder = XportSchema::builder();
        state.process_member_line2(&mut builder, &header).unwrap();
        let schema = builder.try_build().unwrap();
        assert_eq!("", schema.dataset_label());
        assert_eq!("", schema.dataset_type());
    }

    // -----------------------------------------------------------------------
    // process_variable
    // -----------------------------------------------------------------------

    #[test]
    fn process_variable_v5_round_trips_character_variable() {
        // Build a character variable, serialize it to bytes, parse it back,
        // and verify the result matches the original. Each value appears once.
        let mut state = make_state();
        let expected = XportVariable::builder()
            .value_type(SasVariableType::Character)
            .hash(0)
            .value_length(20)
            .number(1)
            .short_name("STUDYID")
            .short_label("Study Identifier")
            .short_format("")
            .format_length(0)
            .format_precision(0)
            .justification(SasJustification::Left)
            .short_input_format("")
            .input_format_length(0)
            .input_format_precision(0)
            .position(0)
            .build();
        let buf = make_variable_descriptor(&expected, false);
        let actual = state
            .process_variable(XportFileVersion::V5, 0, &buf)
            .unwrap()
            .build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn process_variable_v5_round_trips_numeric_with_format() {
        // A numeric variable with a format, precision, and right justification.
        let mut state = make_state();
        let expected = XportVariable::builder()
            .value_type(SasVariableType::Numeric)
            .value_length(8)
            .number(5)
            .short_name("AESTDY")
            .short_label("Study Day of AE Start")
            .short_format("BEST")
            .format_length(12)
            .format_precision(2)
            .justification(SasJustification::Right)
            .position(80)
            .build();
        let buf = make_variable_descriptor(&expected, false);
        let actual = state
            .process_variable(XportFileVersion::V5, 4, &buf)
            .unwrap()
            .build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn process_variable_v8_round_trips_with_medium_name() {
        // V8 variable descriptors include a 32-byte medium name at offset 88.
        let mut state = make_state();
        let expected = XportVariable::builder()
            .value_type(SasVariableType::Character)
            .value_length(8)
            .number(1)
            .short_name("varnameg")
            .medium_name("varnamegreaterthan8")
            .build();
        let buf = make_variable_descriptor(&expected, true);
        let actual = state
            .process_variable(XportFileVersion::V8, 0, &buf)
            .unwrap()
            .build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn process_variable_rejects_invalid_type_code() {
        // Variable type code 99 is not a valid SasVariableType (only 1 and 2 are).
        let mut state = make_state();
        let mut buf = vec![b' '; 140];
        write_u16_be(&mut buf, 0, 99); // Invalid type code
        assert!(
            state
                .process_variable(XportFileVersion::V5, 0, &buf)
                .is_err()
        );
    }

    #[test]
    fn process_variable_rejects_invalid_justification() {
        // Justification code 5 is not valid (only 0=Left and 1=Right).
        let mut state = make_state();
        let variable = XportVariable::builder()
            .value_type(SasVariableType::Character)
            .value_length(8)
            .number(1)
            .short_name("TEST")
            .build();
        let mut buf = make_variable_descriptor(&variable, false);
        write_u16_be(&mut buf, 68, 5); // Invalid justification
        assert!(
            state
                .process_variable(XportFileVersion::V5, 0, &buf)
                .is_err()
        );
    }

    // -----------------------------------------------------------------------
    // read_variable_extension_lengths / process_variable_extension
    // -----------------------------------------------------------------------

    #[test]
    fn extension_lengths_v8_reads_name_and_label() {
        // LABELV8 extension headers have 6 bytes: var_number(2) + name_len(2) + label_len(2).
        let header = make_extension_header(1, "long_variable_name", "A long label", None, None);
        let lengths = XportBufferState::read_variable_extension_lengths(&header, false);
        assert_eq!(18, lengths.name());
        assert_eq!(12, lengths.label());
        assert_eq!(0, lengths.format());
        assert_eq!(0, lengths.input_format());
        assert_eq!(30, lengths.total_length());
    }

    #[test]
    fn extension_lengths_v9_reads_all_fields() {
        // LABELV9 extension headers have 10 bytes, adding format and informat lengths.
        let header = make_extension_header(
            1,
            "long_name",
            "long_label",
            Some("DOLLAR32.2"),
            Some("BEST12."),
        );
        let lengths = XportBufferState::read_variable_extension_lengths(&header, true);
        assert_eq!(9, lengths.name());
        assert_eq!(10, lengths.label());
        assert_eq!(10, lengths.format());
        assert_eq!(7, lengths.input_format());
        assert_eq!(36, lengths.total_length());
    }

    #[test]
    fn process_variable_extension_v8_sets_long_name_and_label() {
        // A V8 extension should populate the long name and long label on the builder.
        // V8 does not include format or informat extensions.
        let mut state = make_state();
        let name = "long_variable_name";
        let label = "A descriptive label";
        let ext_header = make_extension_header(1, name, label, None, None);
        let ext_buffer = make_extension_buffer(name, label, None, None);
        let extension = XportBufferState::read_variable_extension_lengths(&ext_header, false);
        let mut builder = XportVariable::builder();
        let bytes_read = state
            .process_variable_extension(false, 0, &mut builder, &extension, &ext_buffer)
            .unwrap();
        let variable = builder.build();
        assert_eq!(name, variable.long_name());
        assert_eq!(label, variable.long_label());
        assert_eq!("", variable.long_format());
        assert_eq!("", variable.long_input_format());
        assert_eq!(ext_buffer.len(), bytes_read);
    }

    #[test]
    fn process_variable_extension_v9_sets_all_extended_fields() {
        // A V9 extension should populate long name, label, format, and informat.
        let mut state = make_state();
        let name = "varnamegreaterthan8";
        let label = "This variable has an incredibly long variable label";
        let format = "TESTFMTNAMETHATISLONGERTHANEIGHT.";
        let informat = "BEST12.";
        let ext_header = make_extension_header(1, name, label, Some(format), Some(informat));
        let ext_buffer = make_extension_buffer(name, label, Some(format), Some(informat));
        let extension = XportBufferState::read_variable_extension_lengths(&ext_header, true);
        let mut builder = XportVariable::builder();
        let bytes_read = state
            .process_variable_extension(true, 0, &mut builder, &extension, &ext_buffer)
            .unwrap();
        let variable = builder.build();
        assert_eq!(name, variable.long_name());
        assert_eq!(label, variable.long_label());
        assert_eq!(format, variable.long_format());
        assert_eq!(informat, variable.long_input_format());
        assert_eq!(ext_buffer.len(), bytes_read);
    }
}
