use super::xport_constants;
use super::{
    Result, XportDatasetVersion, XportError, XportMetadata, XportSchema, XportVariable,
    XportWriterWithSchema,
};
use crate::sas::xport::xport_error::XportErrorKind;
use crate::sas::xport::xport_writer_state::XportWriterState;
use std::io::Write;

#[derive(Debug)]
struct XportVariableLengths<'a> {
    variable: &'a XportVariable,
    name_length: u16,
    label_length: u16,
    format_length: u16,
    input_format_length: u16,
}

/// State after the library headers have been written. Ready to accept
/// dataset schemas via [`write_schema`](Self::write_schema).
///
/// Get this from [`XportWriter::from_file`](super::XportWriter::from_file)
/// or [`XportWriter::from_writer`](super::XportWriter::from_writer).
#[derive(Debug)]
pub struct XportWriterWithMetadata<W> {
    state: XportWriterState<W>,
    metadata: XportMetadata,
}

impl<W> XportWriterWithMetadata<W> {
    /// Returns the file-level metadata.
    #[inline]
    #[must_use]
    pub fn metadata(&self) -> &XportMetadata {
        &self.metadata
    }
}

impl<W: Write> XportWriterWithMetadata<W> {
    /// Creates a new writer. The library headers must already have been
    /// written; `position` should reflect the bytes written so far.
    pub(crate) fn new(state: XportWriterState<W>, metadata: XportMetadata) -> Self {
        Self { state, metadata }
    }

    /// Writes the dataset headers for the given schema and transitions to
    /// the record-writing state.
    ///
    /// The dataset version is read from `schema.xport_dataset_version()`.
    ///
    /// # Errors
    /// Returns an error if:
    /// * The dataset version exceeds the file version
    /// * An I/O error occurs while writing headers
    pub fn write_schema(mut self, schema: XportSchema) -> Result<XportWriterWithSchema<W>> {
        let variable_padding = Self::validate_schema(&schema)?;
        self.write_member_header(&schema)?;
        self.write_descriptor_header(&schema)?;
        self.write_member_header_line_1(&schema)?;
        self.write_member_header_line_2(&schema)?;
        self.write_namestr_header(&schema)?;
        let extended_variable_lengths = self.write_variables(&schema, variable_padding)?;
        if !extended_variable_lengths.is_empty() {
            self.write_variable_extensions(&schema, &extended_variable_lengths)?;
        }
        let count_position = self.write_observation_header(&schema)?;
        let with_schema =
            XportWriterWithSchema::new(self.state, self.metadata, schema, count_position);
        Ok(with_schema)
    }

    fn validate_schema(schema: &XportSchema) -> Result<u16> {
        let min_descriptor_length: u16 = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => 88,
            XportDatasetVersion::V8 => 122,
            XportDatasetVersion::V9 => 126,
        };
        if schema.variable_descriptor_length() < min_descriptor_length {
            return Err(XportError::of_kind(
                XportErrorKind::Validation,
                format!(
                    "Variable descriptor length {} is too small for {:?}; minimum is {}",
                    schema.variable_descriptor_length(),
                    schema.xport_dataset_version(),
                    min_descriptor_length,
                ),
            ));
        }
        Ok(schema.variable_descriptor_length() - min_descriptor_length)
    }

    fn write_member_header(&mut self, schema: &XportSchema) -> Result<()> {
        let member_header = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => xport_constants::MEMBER_HEADER_PREFIX_V5,
            XportDatasetVersion::V8 | XportDatasetVersion::V9 => {
                xport_constants::MEMBER_HEADER_PREFIX_V8
            }
        };
        self.state
            .write(member_header, "Failed to write the member header prefix")?;
        self.state.write_left_padded_u16(
            schema.variable_descriptor_length(),
            4,
            b'0',
            "Failed to write the variable descriptor length",
        )?;
        self.state
            .write_padding(b' ', 2, "Failed to write trailing member data padding")?;
        Ok(())
    }

    fn write_descriptor_header(&mut self, schema: &XportSchema) -> Result<()> {
        let descriptor_header = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => xport_constants::DESCRIPTOR_HEADER_V5,
            XportDatasetVersion::V8 | XportDatasetVersion::V9 => {
                xport_constants::DESCRIPTOR_HEADER_V8
            }
        };
        self.state
            .write(descriptor_header, "Failed to write the descriptor header")
    }

    fn write_member_header_line_1(&mut self, schema: &XportSchema) -> Result<()> {
        self.state
            .write_str(schema.format(), 8, "Failed to write the dataset format")?;
        let name_length = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => 8,
            XportDatasetVersion::V8 | XportDatasetVersion::V9 => 32,
        };
        self.state.write_str(
            schema.dataset_name(),
            name_length,
            "Failed to write the dataset name",
        )?;
        self.state
            .write_str(schema.sas_data(), 8, "Failed to write SASDATA")?;
        self.state
            .write_str(schema.version(), 8, "Failed to write the dataset version")?;
        self.state.write_str(
            schema.operating_system(),
            8,
            "Failed to write the dataset operating system",
        )?;
        if schema.xport_dataset_version() == XportDatasetVersion::V5 {
            self.state
                .write_padding(b' ', 24, "Failed to write 24 bytes of padding")?;
        }
        self.state.write_date_time(
            schema.created(),
            "Failed to write the dataset creation date/time",
        )?;
        Ok(())
    }

    fn write_member_header_line_2(&mut self, schema: &XportSchema) -> Result<()> {
        self.state.write_date_time(
            schema.modified(),
            "Failed to write the dataset last modified date/time",
        )?;
        self.state
            .write_padding(b' ', 16, "Failed to write 16 bytes of padding")?;
        self.state.write_str(
            schema.dataset_label(),
            40,
            "Failed to write the dataset label",
        )?;
        self.state
            .write_str(schema.dataset_type(), 8, "Failed to write the dataset type")?;
        Ok(())
    }

    fn write_namestr_header(&mut self, schema: &XportSchema) -> Result<()> {
        let prefix = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => xport_constants::NAMESTR_HEADER_PREFIX_V5,
            XportDatasetVersion::V8 | XportDatasetVersion::V9 => {
                xport_constants::NAMESTR_HEADER_PREFIX_V8
            }
        };
        self.state
            .write(prefix, "Failed to write the namestr header prefix")?;
        let variable_count = u16::try_from(schema.variables().len()).map_err(|e| {
            XportError::of_kind(XportErrorKind::Overflow, "Too many variables to write")
                .with_source(e)
        })?;
        self.state.write_left_padded_u16(
            variable_count,
            4,
            b'0',
            "Failed to write the variable count",
        )?;
        let suffix = xport_constants::NAMESTR_HEADER_SUFFIX;
        self.state
            .write(suffix, "Failed to write the namestr header suffix")?;
        Ok(())
    }

    fn write_variables<'a>(
        &mut self,
        schema: &'a XportSchema,
        variable_padding: u16,
    ) -> Result<Vec<XportVariableLengths<'a>>> {
        let mut extended_variables = Vec::new();
        for variable in schema.variables() {
            let lengths = self.write_variable(variable, schema.xport_dataset_version())?;
            if let Some(lengths) = lengths {
                extended_variables.push(lengths);
            }
            let padding_length = usize::from(variable_padding);
            self.state.write_padding(
                b'\0',
                padding_length,
                "Failed to write the trailing padding for the variable",
            )?;
        }
        self.write_trailing_blanks()?;
        Ok(extended_variables)
    }

    fn write_variable<'a>(
        &mut self,
        variable: &'a XportVariable,
        dataset_version: XportDatasetVersion,
    ) -> Result<Option<XportVariableLengths<'a>>> {
        self.state.write_u16(
            variable.value_type().code(),
            "Failed to write the variable type",
        )?;
        self.state
            .write_u16(variable.hash(), "Failed to write the variable hash")?;
        self.state.write_u16(
            variable.value_length(),
            "Failed to write the variable length",
        )?;
        self.state
            .write_u16(variable.number(), "Failed to write the variable number")?;
        self.state.write_str(
            variable.short_name(),
            usize::from(XportVariable::MAX_SHORT_NAME_LENGTH_IN_BYTES),
            "Failed to write the variable short name",
        )?;
        self.state.write_str(
            variable.short_label(),
            usize::from(XportVariable::MAX_SHORT_LABEL_LENGTH_IN_BYTES),
            "Failed to write the variable short label",
        )?;
        self.state.write_str(
            variable.short_format(),
            usize::from(XportVariable::MAX_SHORT_FORMAT_LENGTH_IN_BYTES),
            "Failed to write the variable short format",
        )?;
        self.state.write_u16(
            variable.format_length(),
            "Failed to write the variable format width",
        )?;
        self.state.write_u16(
            variable.format_precision(),
            "Failed to write the variable format precision",
        )?;
        self.state.write_u16(
            variable.justification().code(),
            "Failed to write the variable justification",
        )?;
        self.state.write_padding(
            b'\0',
            2,
            "Failed to write the 2-byte padding within a variable",
        )?;
        self.state.write_str(
            variable.short_input_format(),
            usize::from(XportVariable::MAX_SHORT_INPUT_FORMAT_LENGTH_IN_BYTES),
            "Failed to write the variable short input format",
        )?;
        self.state.write_u16(
            variable.input_format_length(),
            "Failed to write the variable input format width",
        )?;
        self.state.write_u16(
            variable.input_format_precision(),
            "Failed to write the variable input format precision",
        )?;
        self.state
            .write_u32(variable.position(), "Failed to write the variable position")?;

        self.write_variable_extension_lengths(variable, dataset_version)
    }

    fn write_variable_extension_lengths<'a>(
        &mut self,
        variable: &'a XportVariable,
        dataset_version: XportDatasetVersion,
    ) -> Result<Option<XportVariableLengths<'a>>> {
        if dataset_version == XportDatasetVersion::V5 {
            return Ok(None);
        }
        self.state.write_str(
            variable.medium_name(),
            usize::from(XportVariable::MAX_MEDIUM_NAME_LENGTH_IN_BYTES),
            "Failed to write the variable medium name",
        )?;
        let name_length = self.state.encoded_length(
            variable.full_name(),
            "Failed to determine the full name encoded length",
        )?;
        let label_length = self.state.encoded_length(
            variable.full_label(),
            "Failed to determine the full label encoded length",
        )?;
        let format_length = if variable.full_format() == "." {
            1
        } else {
            self.state.encoded_length(
                variable.full_format(),
                "Failed to determine the full format encoded length",
            )?
        };
        let input_format_length = if variable.full_input_format() == "." {
            1
        } else {
            self.state.encoded_length(
                variable.full_input_format(),
                "Failed to determine the full input format encoded length",
            )?
        };
        self.state
            .write_u16(label_length, "Failed to write the extended label length")?;
        if dataset_version == XportDatasetVersion::V9 {
            self.state
                .write_u16(format_length, "Failed to write the extended format length")?;
            self.state.write_u16(
                input_format_length,
                "Failed to write the extended input format length",
            )?;
        }
        if Self::is_extended(
            dataset_version,
            name_length,
            label_length,
            format_length,
            input_format_length,
        ) {
            let lengths = XportVariableLengths {
                variable,
                name_length,
                label_length,
                format_length,
                input_format_length,
            };
            Ok(Some(lengths))
        } else {
            Ok(None)
        }
    }

    fn is_extended(
        dataset_version: XportDatasetVersion,
        name_length: u16,
        label_length: u16,
        format_length: u16,
        input_format_length: u16,
    ) -> bool {
        if name_length > u16::from(XportVariable::MAX_SHORT_NAME_LENGTH_IN_BYTES) {
            return true;
        }
        if label_length > u16::from(XportVariable::MAX_SHORT_LABEL_LENGTH_IN_BYTES) {
            return true;
        }
        if dataset_version != XportDatasetVersion::V9 {
            return false;
        }
        if format_length > u16::from(XportVariable::MAX_SHORT_FORMAT_LENGTH_IN_BYTES) {
            return true;
        }
        input_format_length > u16::from(XportVariable::MAX_SHORT_INPUT_FORMAT_LENGTH_IN_BYTES)
    }

    fn write_trailing_blanks(&mut self) -> Result<()> {
        let header_length = u64::try_from(xport_constants::HEADER_LENGTH).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "Failed to convert the header length to u64",
            )
            .with_source(e)
        })?;
        let remaining = self.state.position() % header_length;
        if remaining == 0 {
            return Ok(());
        }
        let padding_length = header_length - remaining;
        let padding_length = usize::try_from(padding_length).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "Failed to convert the trailing length to usize",
            )
            .with_source(e)
        })?;
        self.state
            .write_padding(b' ', padding_length, "Failed to write out trailing blanks")
    }

    fn write_variable_extensions(
        &mut self,
        schema: &XportSchema,
        extension_lengths: &[XportVariableLengths<'_>],
    ) -> Result<()> {
        let label_header_prefix = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => unreachable!("Extended variables are not supported in V5"),
            XportDatasetVersion::V8 => xport_constants::LABEL_HEADER_V8_PREFIX,
            XportDatasetVersion::V9 => xport_constants::LABEL_HEADER_V9_PREFIX,
        };
        self.state.write(
            label_header_prefix,
            "Failed to write the label header prefix",
        )?;
        let extension_count = u16::try_from(extension_lengths.len()).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "More variable extensions than fit in a 16-bit value",
            )
            .with_source(e)
        })?;
        self.state.write_right_padded_u16(
            extension_count,
            32,
            b' ',
            "Failed to write the extension count",
        )?;
        let include_formats = schema.xport_dataset_version() == XportDatasetVersion::V9;
        for extension_length in extension_lengths {
            let variable = extension_length.variable;
            self.state.write_u16(
                variable.number(),
                "Failed to write the variable number for the extension",
            )?;
            self.state.write_u16(
                extension_length.name_length,
                "Failed to write the extended variable name length",
            )?;
            self.state.write_u16(
                extension_length.label_length,
                "Failed to write the extended variable label length",
            )?;
            if include_formats {
                self.state.write_u16(
                    extension_length.format_length,
                    "Failed to write the extended variable format length",
                )?;
                self.state.write_u16(
                    extension_length.input_format_length,
                    "Failed to write the extended variable input format length",
                )?;
            }
            self.state.write_dynamic_str(
                variable.full_name(),
                "Failed to write the extended variable name",
            )?;
            self.state.write_dynamic_str(
                variable.full_label(),
                "Failed to write the extended variable label",
            )?;
            if include_formats {
                self.state.write_dynamic_str(
                    variable.full_format(),
                    "Failed to write the extended variable format",
                )?;
                self.state.write_dynamic_str(
                    variable.full_input_format(),
                    "Failed to write the extended variable input format",
                )?;
            }
        }
        self.write_trailing_blanks()?;
        Ok(())
    }

    fn write_observation_header(&mut self, schema: &XportSchema) -> Result<Option<u64>> {
        let count_position = match schema.xport_dataset_version() {
            XportDatasetVersion::V5 => {
                self.state.write(
                    xport_constants::OBSERVATION_HEADER_PREFIX_V5,
                    "Failed to write the observation header prefix",
                )?;
                self.state.write(
                    xport_constants::OBSERVATION_HEADER_SUFFIX_V5,
                    "Failed to write the observation header suffix",
                )?;
                None
            }
            XportDatasetVersion::V8 | XportDatasetVersion::V9 => {
                self.state.write(
                    xport_constants::OBSERVATION_HEADER_PREFIX_V8,
                    "Failed to write the observation header prefix",
                )?;
                let count_position = self.state.position();
                self.state.write_padding(
                    b' ',
                    15,
                    "Failed to write the 15-byte padding for the record count",
                )?;
                self.state.write(
                    xport_constants::OBSERVATION_HEADER_SUFFIX_V8,
                    "Failed to write the observation header suffix",
                )?;
                Some(count_position)
            }
        };
        Ok(count_position)
    }

    /// Finalizes the file without writing any more datasets.
    ///
    /// # Errors
    /// Returns an error if an I/O error occurs while flushing.
    pub fn finish(mut self) -> Result<()> {
        self.state.flush()
    }
}
