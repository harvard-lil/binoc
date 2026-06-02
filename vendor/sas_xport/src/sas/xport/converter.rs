use super::xport_error::{XportError, XportErrorKind};
use super::xport_value::XportValue;
use super::xport_variable::XportVariable;
use crate::sas::{SasDateTime, SasVariableType};
use encoding_rs::Encoding;
use std::borrow::Cow;
use std::io::Write as _;

use super::decoder::Decoder;

/// Parses a date/time in the format DDMMMYY:HH:mm:ss from a byte buffer.
pub fn read_date_time(buffer: &[u8]) -> Result<SasDateTime, XportError> {
    encoding_rs::UTF_8
        .decode_without_bom_handling_and_without_replacement(buffer)
        .ok_or_else(|| {
            XportError::of_kind(
                XportErrorKind::InvalidDateTime,
                "The date/time contained non-ASCII characters",
            )
        })?
        .parse()
        .map_err(|e| {
            XportError::of_kind(XportErrorKind::InvalidDateTime, "Failed to parse date/time")
                .with_source(e)
        })
}

pub fn read_u16(buffer: &[u8]) -> u16 {
    u16::from_be_bytes(buffer.try_into().unwrap())
}

pub fn read_u32(buffer: &[u8]) -> u32 {
    u32::from_be_bytes(buffer.try_into().unwrap())
}

pub fn read_string<'a>(
    buffer: &'a [u8],
    decoder: &Decoder,
) -> Result<Cow<'a, str>, Cow<'static, str>> {
    decoder.decode(buffer)
}

pub fn read_trimmed_string<'a>(
    buffer: &'a [u8],
    decoder: &Decoder,
) -> Result<Cow<'a, str>, Cow<'static, str>> {
    let trimmed = trim_end_ascii(buffer);
    read_string(trimmed, decoder)
}

/// Encodes `value` using `encoding` into exactly `byte_length` bytes,
/// writing the result into `buf`.
///
/// `buf` is cleared and resized to `byte_length`. The encoded bytes are
/// written up to the last complete character boundary that fits. Any
/// remaining bytes are filled with ASCII spaces (0x20).
///
/// Returns `true` if the encoded value was truncated to fit.
///
/// # Errors
/// Returns an error if `value` contains characters that cannot be
/// represented in `encoding`.
pub fn prepare_string(
    encoding: &'static Encoding,
    value: &str,
    byte_length: usize,
    error_message: &str,
    buffer: &mut Vec<u8>,
) -> Result<bool, XportError> {
    buffer.clear();
    buffer.resize(byte_length, b' ');
    encode_into_slice(encoding, value, buffer, error_message)
}

/// Encodes `value` into a pre-allocated `buffer` that is already filled with
/// spaces (0x20). The buffer length determines the field width.
///
/// Returns `true` if the encoded value was truncated to fit.
pub fn encode_into_slice(
    encoding: &'static Encoding,
    value: &str,
    buffer: &mut [u8],
    error_message: &str,
) -> Result<bool, XportError> {
    if encoding == encoding_rs::UTF_8 {
        let copy_len = floor_char_boundary(value, buffer.len());
        buffer[..copy_len].copy_from_slice(&value.as_bytes()[..copy_len]);
        Ok(copy_len < value.len())
    } else {
        let mut encoder = encoding.new_encoder();
        let (result, bytes_read, _bytes_written) =
            encoder.encode_from_utf8_without_replacement(value, buffer, true);
        match result {
            encoding_rs::EncoderResult::Unmappable(ch) => Err(XportError::of_kind(
                XportErrorKind::Encoding,
                format!(
                    "{}. Character '{}' cannot be encoded in {}.",
                    error_message,
                    ch,
                    encoding.name(),
                ),
            )),
            encoding_rs::EncoderResult::OutputFull => Ok(true),
            encoding_rs::EncoderResult::InputEmpty => Ok(bytes_read < value.len()),
        }
    }
}

/// Returns the largest byte index `<= byte_length` that lies on a UTF-8
/// character boundary in `value`. Equivalent to the nightly-only
/// `str::floor_char_boundary`.
fn floor_char_boundary(value: &str, byte_length: usize) -> usize {
    if byte_length >= value.len() {
        return value.len();
    }
    let mut i = byte_length;
    while i > 0 && !value.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Returns the minimum number of bytes needed to represent the numeric
/// value without loss (i.e., the index of the last non-zero byte + 1).
/// Returns `0` for an all-zero value.
pub fn numeric_encoded_length(bytes: [u8; 8]) -> usize {
    for i in (0..8).rev() {
        if bytes[i] != 0 {
            return i + 1;
        }
    }
    0
}

/// Validates that the number of values matches the number of variables and
/// that each value's variant matches the corresponding variable's type.
pub fn validate_values(
    variables: &[XportVariable],
    values: &[XportValue<'_>],
) -> Result<(), XportError> {
    if variables.len() != values.len() {
        return Err(XportError::of_kind(
            XportErrorKind::Validation,
            "The wrong number of values were provided",
        ));
    }
    for (variable, value) in variables.iter().zip(values) {
        let mismatch = match variable.value_type() {
            SasVariableType::Character => matches!(value, XportValue::Number(_)),
            SasVariableType::Numeric => matches!(value, XportValue::Character(_)),
        };
        if mismatch {
            return Err(XportError::of_kind(
                XportErrorKind::Validation,
                "Encountered a variable/value mismatch",
            ));
        }
    }
    Ok(())
}

pub fn padding(byte_length: usize, value: u8, buffer: &mut Vec<u8>) {
    buffer.clear();
    buffer.resize(byte_length, value);
}

/// Writes a `SasDateTime` in the 16-byte format `DDMMMYY:HH:mm:ss` into
/// `buffer`, replacing its contents. No intermediate string is allocated.
pub fn prepare_date_time(date_time: SasDateTime, buffer: &mut Vec<u8>) {
    fn push_two_digits(buf: &mut Vec<u8>, value: u8) {
        buf.push(b'0' + value / 10);
        buf.push(b'0' + value % 10);
    }

    buffer.clear();
    buffer.reserve(16);

    push_two_digits(buffer, date_time.day());
    buffer.extend_from_slice(date_time.month().abbreviation().as_bytes());
    push_two_digits(buffer, date_time.year());
    buffer.push(b':');
    push_two_digits(buffer, date_time.hour());
    buffer.push(b':');
    push_two_digits(buffer, date_time.minute());
    buffer.push(b':');
    push_two_digits(buffer, date_time.second());
}

/// Returns `true` if every byte in `buffer` is a space (0x20) or null (0x00).
///
/// Scans in `usize`-width chunks for throughput, falling back to
/// byte-at-a-time for the remainder.
pub fn all_blank(buffer: &[u8]) -> bool {
    // For each byte b, (b ^ 0x20) maps space→0x00 and null stays 0x00.
    // Any other byte will have at least one non-zero bit after the XOR,
    // and that bit survives the AND with the original byte — so
    // `(b ^ 0x20) & b == 0` iff b is space or null.
    let spaces = usize::from_ne_bytes([0x20; size_of::<usize>()]);
    let (chunks, remainder) = buffer.as_chunks::<{ size_of::<usize>() }>();
    for &chunk in chunks {
        let w = usize::from_ne_bytes(chunk);
        if (w ^ spaces) & w != 0 {
            return false;
        }
    }
    for &b in remainder {
        if (b ^ 0x20) & b != 0 {
            return false;
        }
    }
    true
}

/// Trims trailing spaces (0x20) and null bytes (0x00) from `buffer`.
///
/// Scans backwards in `usize`-width chunks, then byte-at-a-time within
/// the remainder.
pub fn trim_end_ascii(buffer: &[u8]) -> &[u8] {
    let spaces = usize::from_ne_bytes([0x20; size_of::<usize>()]);
    let (remainder, chunks) = buffer.as_rchunks::<{ size_of::<usize>() }>();

    // Phase 1: scan backwards in usize-width chunks from the end.
    let mut trimmed_chunks = chunks.len();
    for &chunk in chunks.iter().rev() {
        let w = usize::from_ne_bytes(chunk);
        if (w ^ spaces) & w != 0 {
            break;
        }
        trimmed_chunks -= 1;
    }

    let end = remainder.len() + trimmed_chunks * size_of::<usize>();

    // Phase 2: byte-at-a-time if we trimmed all chunks, or for the remainder.
    let mut end = end;
    while end > 0 {
        let b = buffer[end - 1];
        if b != b' ' && b != b'\0' {
            break;
        }
        end -= 1;
    }

    &buffer[..end]
}

/// Computes the encoded byte length of `value` using the given encoding,
/// returning the length as a `u16`. The `buffer` is used as scratch space
/// and may be resized.
pub fn encoded_length(
    encoding: &'static Encoding,
    value: &str,
    buffer: &mut Vec<u8>,
    error_message: &'static str,
) -> Result<u16, XportError> {
    let mut encoder = encoding.new_encoder();
    let max_len = encoder
        .max_buffer_length_from_utf8_without_replacement(value.len())
        .ok_or_else(|| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                format!("{error_message}. Encoded length overflow"),
            )
        })?;
    buffer.clear();
    buffer.resize(max_len, 0);
    let (result, _read, written) =
        encoder.encode_from_utf8_without_replacement(value, buffer, true);
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
    u16::try_from(written).map_err(|e| {
        XportError::of_kind(
            XportErrorKind::Overflow,
            format!("{error_message}. Encoded length exceeds the 16-bit limit"),
        )
        .with_source(e)
    })
}

/// Formats a `u16` as decimal digits into `buffer`, left-padded with
/// `pad` to fill `byte_length`. Returns an error if the formatted
/// value exceeds `byte_length`.
pub fn prepare_left_padded_u16(
    value: u16,
    byte_length: usize,
    pad: u8,
    error_message: &'static str,
    buffer: &mut Vec<u8>,
) -> Result<(), XportError> {
    let (digits, len) = format_u16(value, error_message)?;
    if len > byte_length {
        return Err(XportError::of_kind(
            XportErrorKind::Overflow,
            format!("{error_message}. Value {value} exceeds {byte_length} characters."),
        ));
    }
    buffer.clear();
    buffer.resize(byte_length - len, pad);
    buffer.extend_from_slice(&digits[..len]);
    Ok(())
}

/// Formats a `u16` as decimal digits into `buffer`, right-padded with
/// `pad` to fill `byte_length`. Returns an error if the formatted
/// value exceeds `byte_length`.
pub fn prepare_right_padded_u16(
    value: u16,
    byte_length: usize,
    pad: u8,
    error_message: &'static str,
    buffer: &mut Vec<u8>,
) -> Result<(), XportError> {
    let (digits, len) = format_u16(value, error_message)?;
    if len > byte_length {
        return Err(XportError::of_kind(
            XportErrorKind::Overflow,
            format!("{error_message}. Value {value} exceeds {byte_length} characters."),
        ));
    }
    buffer.clear();
    buffer.extend_from_slice(&digits[..len]);
    buffer.resize(byte_length, pad);
    Ok(())
}

/// Formats a `u64` as a right-justified 15-character ASCII field,
/// padded with spaces on the left.
pub fn format_record_count(record_count: u64) -> Result<[u8; 15], XportError> {
    let mut digits = [0u8; 20]; // u64 max is 20 digits
    let len = {
        let mut cursor = std::io::Cursor::new(&mut digits[..]);
        write!(cursor, "{record_count}")
            .map_err(|e| XportError::io("Failed to format the record count", e))?;
        usize::try_from(cursor.position()).map_err(|e| {
            XportError::of_kind(
                XportErrorKind::Overflow,
                "Failed to convert the record count string length to usize",
            )
            .with_source(e)
        })?
    };
    let field_length = 15;
    if len > field_length {
        return Err(XportError::of_kind(
            XportErrorKind::Overflow,
            format!("Record count {record_count} exceeds the {field_length}-character field"),
        ));
    }
    let mut buffer = [b' '; 15];
    let start = field_length - len;
    buffer[start..].copy_from_slice(&digits[..len]);
    Ok(buffer)
}

/// Formats a `u16` into a decimal digit buffer, returning the digits
/// and their length.
fn format_u16(value: u16, error_message: &'static str) -> Result<([u8; 5], usize), XportError> {
    let mut digits = [0u8; 5]; // u16 -> str can never be more than 5 characters
    let mut cursor = std::io::Cursor::new(&mut digits[..]);
    write!(cursor, "{value}").map_err(|e| XportError::io(error_message, e))?;
    let len = usize::try_from(cursor.position()).map_err(|e| {
        XportError::of_kind(
            XportErrorKind::Overflow,
            "Failed to convert a 16-bit numeric string length to usize",
        )
        .with_source(e)
    })?;
    Ok((digits, len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sas::SasMonth;

    // read_u16

    #[test]
    fn read_u16_big_endian() {
        assert_eq!(0x0102, read_u16(&[0x01, 0x02]));
    }

    #[test]
    fn read_u16_zero() {
        assert_eq!(0, read_u16(&[0x00, 0x00]));
    }

    #[test]
    fn read_u16_max() {
        assert_eq!(u16::MAX, read_u16(&[0xFF, 0xFF]));
    }

    // read_u32

    #[test]
    fn read_u32_big_endian() {
        assert_eq!(0x0102_0304, read_u32(&[0x01, 0x02, 0x03, 0x04]));
    }

    #[test]
    fn read_u32_zero() {
        assert_eq!(0, read_u32(&[0x00, 0x00, 0x00, 0x00]));
    }

    #[test]
    fn read_u32_max() {
        assert_eq!(u32::MAX, read_u32(&[0xFF, 0xFF, 0xFF, 0xFF]));
    }

    // read_date_time

    #[test]
    fn read_date_time_valid() {
        let dt = read_date_time(b"23SEP23:12:56:03").unwrap();
        assert_eq!(23, dt.day());
        assert_eq!(SasMonth::September, dt.month());
        assert_eq!(23, dt.year());
        assert_eq!(12, dt.hour());
        assert_eq!(56, dt.minute());
        assert_eq!(3, dt.second());
    }

    #[test]
    fn read_date_time_rejects_non_ascii() {
        let result = read_date_time(&[0xFF; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn read_date_time_rejects_invalid_format() {
        let result = read_date_time(b"not-a-date-time!");
        assert!(result.is_err());
    }

    // read_string / read_trimmed_string

    #[test]
    fn read_string_returns_content() {
        let decoder = Decoder::ascii();
        let result = read_string(b"HELLO", &decoder).unwrap();
        assert_eq!("HELLO", result);
    }

    #[test]
    fn read_trimmed_string_trims_trailing_spaces() {
        let decoder = Decoder::ascii();
        let result = read_trimmed_string(b"SAS     ", &decoder).unwrap();
        assert_eq!("SAS", result);
    }

    #[test]
    fn read_trimmed_string_trims_trailing_nulls() {
        let decoder = Decoder::ascii();
        let result = read_trimmed_string(b"SAS\0\0\0", &decoder).unwrap();
        assert_eq!("SAS", result);
    }

    #[test]
    fn read_trimmed_string_trims_mixed_trailing() {
        let decoder = Decoder::ascii();
        let result = read_trimmed_string(b"SAS \0 \0", &decoder).unwrap();
        assert_eq!("SAS", result);
    }

    #[test]
    fn read_trimmed_string_all_spaces_returns_empty() {
        let decoder = Decoder::ascii();
        let result = read_trimmed_string(b"        ", &decoder).unwrap();
        assert_eq!("", result);
    }

    // all_blank

    #[test]
    fn all_blank_true_for_spaces() {
        assert!(all_blank(b"        "));
    }

    #[test]
    fn all_blank_false_for_non_spaces() {
        assert!(!all_blank(b"  X     "));
    }

    #[test]
    fn all_blank_true_for_empty() {
        assert!(all_blank(b""));
    }

    #[test]
    fn all_blank_true_for_nulls() {
        assert!(all_blank(&[0x00]));
    }

    // trim_end_ascii

    #[test]
    fn trim_end_ascii_removes_trailing_spaces() {
        assert_eq!(b"ABC", trim_end_ascii(b"ABC   "));
    }

    #[test]
    fn trim_end_ascii_removes_trailing_nulls() {
        assert_eq!(b"ABC", trim_end_ascii(b"ABC\0\0"));
    }

    #[test]
    fn trim_end_ascii_preserves_leading_spaces() {
        assert_eq!(b"  ABC", trim_end_ascii(b"  ABC  "));
    }

    #[test]
    fn trim_end_ascii_all_spaces_returns_empty() {
        let result: &[u8] = trim_end_ascii(b"    ");
        assert!(result.is_empty());
    }

    #[test]
    fn trim_end_ascii_empty_returns_empty() {
        let result: &[u8] = trim_end_ascii(b"");
        assert!(result.is_empty());
    }

    #[test]
    fn trim_end_ascii_no_trailing_whitespace_unchanged() {
        assert_eq!(b"ABC", trim_end_ascii(b"ABC"));
    }

    // prepare_string

    #[test]
    fn prepare_string_exact_fit() {
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "SAS", 3, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"SAS", buf.as_slice());
    }

    #[test]
    fn prepare_string_pads_short_value() {
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "SAS", 8, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"SAS     ", buf.as_slice());
    }

    #[test]
    fn prepare_string_truncates_long_value() {
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "ABCDEFGHIJ", 8, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"ABCDEFGH", buf.as_slice());
    }

    #[test]
    fn prepare_string_empty_value_fills_spaces() {
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "", 4, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"    ", buf.as_slice());
    }

    #[test]
    fn prepare_string_truncates_at_char_boundary() {
        // 'é' is 2 bytes in UTF-8 — truncating at byte 8 would split it
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "1234567é9", 8, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"1234567 ", buf.as_slice());
    }

    #[test]
    fn prepare_string_reuses_buffer() {
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "FIRST", 8, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"FIRST   ", buf.as_slice());
        prepare_string(encoding_rs::UTF_8, "SECOND", 8, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"SECOND  ", buf.as_slice());
    }

    #[test]
    fn prepare_string_zero_length() {
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "ABC", 0, "SYMBOL1", &mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn prepare_string_utf8_three_byte_not_split() {
        // '€' is 3 bytes in UTF-8 (E2 82 AC). With 5 bytes available,
        // "AB€" would be 2 + 3 = 5, which fits exactly.
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "AB€", 5, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(&[b'A', b'B', 0xE2, 0x82, 0xAC], buf.as_slice());
    }

    #[test]
    fn prepare_string_utf8_three_byte_truncated_not_split() {
        // "AB€" is 5 bytes. With only 4 bytes available, the '€' can't
        //  fit, so we get "AB" + 2 spaces.
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "AB€", 4, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"AB  ", buf.as_slice());
    }

    #[test]
    fn prepare_string_utf8_four_byte_not_split() {
        // '𝄞' (U+1D11E) is 4 bytes in UTF-8. With only 4 bytes available,
        // "A" (1 byte) fits, but '𝄞' (4 bytes) does not, so we get "A" + 3 spaces.
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "A𝄞", 4, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"A   ", buf.as_slice());
    }

    #[test]
    fn prepare_string_utf8_four_byte_exact_fit() {
        // "A𝄞" is 1 + 4 = 5 bytes. With 5 bytes it fits exactly.
        let mut buf = Vec::new();
        prepare_string(encoding_rs::UTF_8, "A𝄞B", 5, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(&[b'A', 0xF0, 0x9D, 0x84, 0x9E], buf.as_slice());
    }

    #[test]
    fn prepare_string_shift_jis_two_byte_not_split() {
        // In Shift-JIS, '漢' (U+6F22) encodes as 2 bytes (8A BF).
        // With 3 bytes: "A" (1 byte) + '漢' (2 bytes) = 3, fits exactly.
        let mut buf = Vec::new();
        prepare_string(encoding_rs::SHIFT_JIS, "A漢", 3, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(3, buf.len());
        assert_eq!(b'A', buf[0]);
        // The remaining 2 bytes are the Shift-JIS encoding of '漢'.
    }

    #[test]
    fn prepare_string_shift_jis_two_byte_truncated_not_split() {
        // "A漢" in Shift-JIS is 3 bytes. With only 2 bytes, the 2-byte
        // character can't fit, so we get "A" + 1 space.
        let mut buf = Vec::new();
        prepare_string(encoding_rs::SHIFT_JIS, "A漢", 2, "SYMBOL1", &mut buf).unwrap();
        assert_eq!(b"A ", buf.as_slice());
    }

    #[test]
    fn prepare_string_unmappable_character_returns_error() {
        // ISO-8859-1 cannot encode '漢' (U+6F22).
        let mut buf = Vec::new();
        let result = prepare_string(encoding_rs::ISO_8859_2, "A漢B", 8, "SYMBOL1", &mut buf);
        assert!(result.is_err());
    }
}
