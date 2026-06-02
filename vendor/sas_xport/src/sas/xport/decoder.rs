use crate::sas::xport::XportReaderOptionsInternal;
use encoding_rs::Encoding;
use std::borrow::Cow;

#[derive(Debug)]
pub(crate) struct Decoder {
    decoders: Vec<&'static Encoding>,
}

impl Decoder {
    const DEFAULT_RESULT: Cow<'static, str> = Cow::Borrowed("Failed to decode the string.");

    fn new(primary: &'static Encoding, fallback_encodings: &[&'static Encoding]) -> Self {
        let mut decoders = Vec::with_capacity(fallback_encodings.len() + 1);
        decoders.push(primary);
        decoders.extend_from_slice(fallback_encodings);
        Self { decoders }
    }

    /// Creates a decoder for record data: primary encoding and user-specified fallbacks.
    pub(crate) fn from_options(options: &XportReaderOptionsInternal) -> Self {
        Self::new(options.encoding(), options.fallback_encodings())
    }

    /// Creates a decoder for metadata fields that are expected to be ASCII but
    /// may contain extended characters. Includes the user-specified fallbacks
    /// and, if neither the primary nor any fallback is ASCII-compatible, appends
    /// UTF-8 as a safety net.
    pub(crate) fn metadata_from_options(options: &XportReaderOptionsInternal) -> Self {
        let mut fallback_encodings = Vec::from(options.fallback_encodings());
        if !options.encoding().is_ascii_compatible() {
            let has_ascii_compatible = fallback_encodings.iter().any(|e| e.is_ascii_compatible());
            if !has_ascii_compatible {
                fallback_encodings.push(encoding_rs::UTF_8);
            }
        }
        Self::new(options.encoding(), &fallback_encodings)
    }

    /// Creates a decoder that only accepts ASCII (via UTF-8), with no fallbacks.
    pub(crate) fn ascii() -> Self {
        Self::new(encoding_rs::UTF_8, &[])
    }

    pub(crate) fn decode<'b>(&self, buffer: &'b [u8]) -> Result<Cow<'b, str>, Cow<'static, str>> {
        // Fast path: when the primary encoding is UTF-8 (the common case),
        // use std::str::from_utf8 which is heavily SIMD-optimized and avoids
        // the encoding_rs function-call overhead.
        if self.decoders.first() == Some(&encoding_rs::UTF_8) {
            if let Ok(s) = std::str::from_utf8(buffer) {
                return Ok(Cow::Borrowed(s));
            }
            // Invalid UTF-8 — fall through to try fallback encodings.
            for decoder in &self.decoders[1..] {
                let result = decoder.decode_without_bom_handling_and_without_replacement(buffer);
                if let Some(decoded) = result {
                    return Ok(decoded);
                }
            }
            return Err(Self::DEFAULT_RESULT);
        }
        for decoder in &self.decoders {
            let result = decoder.decode_without_bom_handling_and_without_replacement(buffer);
            if let Some(decoded) = result {
                return Ok(decoded);
            }
        }
        Err(Self::DEFAULT_RESULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sas::xport::XportReaderOptions;

    #[test]
    fn test_ascii_decoder_decodes_ascii() {
        let decoder = Decoder::ascii();
        let result = decoder.decode(b"HELLO WORLD").unwrap();
        assert_eq!(result, "HELLO WORLD");
    }

    #[test]
    fn test_ascii_decoder_rejects_non_ascii() {
        let decoder = Decoder::ascii();
        assert!(decoder.decode(&[0x97]).is_err());
    }

    #[test]
    fn test_utf8_primary_decodes_valid_utf8() {
        let decoder = Decoder::new(encoding_rs::UTF_8, &[]);
        let result = decoder.decode("café".as_bytes()).unwrap();
        assert_eq!(result, "café");
    }

    #[test]
    fn test_utf8_primary_rejects_invalid_bytes() {
        let decoder = Decoder::new(encoding_rs::UTF_8, &[]);
        assert!(decoder.decode(&[0x97]).is_err());
    }

    /// Verifies the intended decode pattern: try the primary encoding first
    /// and fall back to the secondary encoding when the primary produces
    /// malformed sequences.
    #[test]
    fn test_fallback_on_primary_failure() {
        // An em dash (U+2014) encoded as Windows-1252: byte 0x97
        let decoder = Decoder::new(encoding_rs::UTF_8, &[encoding_rs::WINDOWS_1252]);
        let result = decoder.decode(&[0x97]).unwrap();
        assert_eq!(result, "—");
    }

    /// Verifies fallback handles smart quotes from Microsoft Office products.
    #[test]
    fn test_fallback_smart_quotes() {
        // Windows-1252 smart quotes: left double (0x93), right double (0x94)
        let decoder = Decoder::new(encoding_rs::UTF_8, &[encoding_rs::WINDOWS_1252]);
        let result = decoder
            .decode(&[0x93, 0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x94])
            .unwrap();
        assert_eq!(result, "\u{201c}Hello\u{201d}");
    }

    /// Verifies fallback with Windows-1252 for characters outside ASCII.
    #[test]
    fn test_fallback_cafe() {
        // "café" in Windows-1252: the 'é' is byte 0xE9
        let decoder = Decoder::new(encoding_rs::UTF_8, &[encoding_rs::WINDOWS_1252]);
        let result = decoder.decode(&[0x63, 0x61, 0x66, 0xE9]).unwrap();
        assert_eq!(result, "café");
    }

    /// Pure ASCII should decode cleanly with the primary encoding and never
    /// need the fallback, regardless of configuration.
    #[test]
    fn test_ascii_succeeds_without_needing_fallback() {
        let decoder = Decoder::new(encoding_rs::UTF_8, &[encoding_rs::WINDOWS_1252]);
        let result = decoder.decode(b"HELLO WORLD").unwrap();
        assert_eq!(result, "HELLO WORLD");
    }

    /// Valid UTF-8 multibyte characters should not trigger fallback.
    #[test]
    fn test_valid_utf8_does_not_trigger_fallback() {
        let decoder = Decoder::new(encoding_rs::UTF_8, &[encoding_rs::WINDOWS_1252]);
        let result = decoder.decode("café".as_bytes()).unwrap();
        assert_eq!(result, "café");
    }

    /// All decoders fail: should return an error.
    #[test]
    fn test_all_decoders_fail_returns_error() {
        // Shift_JIS won't decode a lone 0x80 either
        let decoder = Decoder::new(encoding_rs::UTF_8, &[encoding_rs::SHIFT_JIS]);
        assert!(decoder.decode(&[0xFF]).is_err());
    }

    #[test]
    fn test_multiple_fallbacks_tried_in_order() {
        // 0x97 is invalid UTF-8, invalid Shift_JIS, but valid Windows-1252 (em dash)
        let decoder = Decoder::new(
            encoding_rs::UTF_8,
            &[encoding_rs::SHIFT_JIS, encoding_rs::WINDOWS_1252],
        );
        let result = decoder.decode(&[0x97]).unwrap();
        assert_eq!(result, "—");
    }

    #[test]
    fn test_from_options_uses_primary_and_fallbacks() {
        let options = XportReaderOptions::default()
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .build();
        let decoder = Decoder::from_options(&options);
        let result = decoder.decode(&[0x97]).unwrap();
        assert_eq!(result, "—");
    }

    #[test]
    fn test_metadata_from_options_adds_utf8_for_non_ascii_primary() {
        let options = XportReaderOptions::default()
            .encoding(encoding_rs::SHIFT_JIS)
            .build();
        let decoder = Decoder::metadata_from_options(&options);
        // Pure ASCII should decode via the UTF-8 safety net
        let result = decoder.decode(b"HELLO").unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_metadata_from_options_skips_utf8_when_fallback_is_ascii_compatible() {
        let options = XportReaderOptions::default()
            .encoding(encoding_rs::SHIFT_JIS)
            .add_fallback_encoding(encoding_rs::WINDOWS_1252)
            .build();
        let decoder = Decoder::metadata_from_options(&options);
        // Windows-1252 is ASCII-compatible, so UTF-8 should not be appended.
        // The decoder should have 2 entries: SHIFT_JIS + WINDOWS_1252
        let result = decoder.decode(b"HELLO").unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_metadata_from_options_ascii_compatible_primary_no_extra_utf8() {
        let options = XportReaderOptions::default()
            .encoding(encoding_rs::UTF_8)
            .build();
        let decoder = Decoder::metadata_from_options(&options);
        let result = decoder.decode(b"HELLO").unwrap();
        assert_eq!(result, "HELLO");
    }
}
