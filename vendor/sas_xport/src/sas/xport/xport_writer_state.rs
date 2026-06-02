use crate::sas::SasDateTime;
use crate::sas::xport::converter::{
    self, format_record_count, padding, prepare_date_time, prepare_string,
};
use crate::sas::xport::xport_error::XportErrorKind;
use crate::sas::xport::xport_writer_options::XportWriterOptionsInternal;
use crate::sas::xport::{Result, XportError};
use std::io::{Seek, SeekFrom, Write};

#[derive(Debug)]
pub(crate) struct XportWriterState<W> {
    options: XportWriterOptionsInternal,
    writer: W,
    buffer: Vec<u8>,
    position: u64,
}

impl<W> XportWriterState<W> {
    #[must_use]
    pub fn new(options: XportWriterOptionsInternal, writer: W) -> Self {
        Self {
            options,
            writer,
            buffer: Vec::new(),
            position: 0,
        }
    }

    #[must_use]
    pub fn options(&self) -> &XportWriterOptionsInternal {
        &self.options
    }

    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    #[must_use]
    pub fn into_writer(self) -> W {
        self.writer
    }
}

impl<W: Write> XportWriterState<W> {
    pub fn write(&mut self, bytes: &[u8], error_message: &'static str) -> Result<()> {
        self.writer
            .write_all(bytes)
            .map_err(|e| XportError::io(error_message, e))?;
        self.position += bytes.len() as u64;
        Ok(())
    }

    /// Encodes `value` into exactly `byte_length` bytes and writes it.
    ///
    /// Returns `None` if the value fit without truncation, or
    /// `Some(encoded_length)` — the full encoded byte length — if it
    /// was truncated.
    pub fn write_str(
        &mut self,
        value: &str,
        byte_length: usize,
        error_message: &'static str,
    ) -> Result<Option<usize>> {
        let truncated = prepare_string(
            self.options.encoding(),
            value,
            byte_length,
            error_message,
            &mut self.buffer,
        )?;
        self.flush_buffer(error_message)?;
        if !truncated {
            return Ok(None);
        }
        let actual = if self.options.encoding() == encoding_rs::UTF_8 {
            value.len()
        } else {
            usize::from(converter::encoded_length(
                self.options.encoding(),
                value,
                &mut self.buffer,
                error_message,
            )?)
        };
        Ok(Some(actual))
    }

    pub fn write_dynamic_str(&mut self, value: &str, error_message: &'static str) -> Result<()> {
        self.buffer.clear();
        let encoding = self.options.encoding();
        let mut encoder = encoding.new_encoder();
        let (result, _bytes_written) =
            encoder.encode_from_utf8_to_vec_without_replacement(value, &mut self.buffer, true);
        if let encoding_rs::EncoderResult::Unmappable(ch) = result {
            return Err(XportError::of_kind(
                XportErrorKind::Encoding,
                format!(
                    "{}. Character '{}' cannot be encoded in {}.",
                    error_message,
                    ch,
                    encoding.name(),
                ),
            ));
        }
        self.flush_buffer(error_message)
    }

    pub fn write_padding(
        &mut self,
        value: u8,
        byte_length: usize,
        error_message: &'static str,
    ) -> Result<()> {
        padding(byte_length, value, &mut self.buffer);
        self.flush_buffer(error_message)
    }

    pub fn write_date_time(
        &mut self,
        value: SasDateTime,
        error_message: &'static str,
    ) -> Result<()> {
        prepare_date_time(value, &mut self.buffer);
        self.flush_buffer(error_message)
    }

    pub fn encoded_length(&mut self, value: &str, error_message: &'static str) -> Result<u16> {
        converter::encoded_length(
            self.options.encoding(),
            value,
            &mut self.buffer,
            error_message,
        )
    }

    pub fn write_left_padded_u16(
        &mut self,
        value: u16,
        byte_length: usize,
        padding: u8,
        error_message: &'static str,
    ) -> Result<()> {
        converter::prepare_left_padded_u16(
            value,
            byte_length,
            padding,
            error_message,
            &mut self.buffer,
        )?;
        self.flush_buffer(error_message)
    }

    pub fn write_right_padded_u16(
        &mut self,
        value: u16,
        byte_length: usize,
        padding: u8,
        error_message: &'static str,
    ) -> Result<()> {
        converter::prepare_right_padded_u16(
            value,
            byte_length,
            padding,
            error_message,
            &mut self.buffer,
        )?;
        self.flush_buffer(error_message)
    }

    pub fn write_u16(&mut self, value: u16, error_message: &'static str) -> Result<()> {
        let bytes = value.to_be_bytes();
        self.write(&bytes, error_message)
    }

    pub fn write_u32(&mut self, value: u32, error_message: &'static str) -> Result<()> {
        let bytes = value.to_be_bytes();
        self.write(&bytes, error_message)
    }

    fn flush_buffer(&mut self, error_message: &'static str) -> Result<()> {
        self.writer
            .write_all(&self.buffer)
            .map_err(|e| XportError::io(error_message, e))?;
        self.position +=
            u64::try_from(self.buffer.len()).map_err(|e| XportError::io(error_message, e))?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .map_err(|e| XportError::io("Failed to flush the writer", e))
    }
}

impl<W: Write + Seek> XportWriterState<W> {
    /// Seeks to `offset`, writes the record count as a right-justified
    /// 15-character ASCII string, then seeks back to the original position.
    pub fn write_record_count(&mut self, offset: u64, record_count: u64) -> Result<()> {
        let current = self
            .writer
            .stream_position()
            .map_err(|e| XportError::io("Failed to get the current position", e))?;
        self.writer
            .seek(SeekFrom::Start(offset))
            .map_err(|e| XportError::io("Failed to seek to the record count offset", e))?;
        self.write_record_count_direct(record_count)?;
        self.writer
            .seek(SeekFrom::Start(current))
            .map_err(|e| XportError::io("Failed to seek back to the original position", e))?;
        Ok(())
    }

    fn write_record_count_direct(&mut self, record_count: u64) -> Result<()> {
        // Write the count directly without updating self.position, since we
        // seek back to the original position afterward.
        let buffer = format_record_count(record_count)?;
        self.writer
            .write_all(&buffer)
            .map_err(|e| XportError::io("Failed to write the record count", e))
    }
}
