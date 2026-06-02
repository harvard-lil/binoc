use super::SasError;
use super::sas_float_64::SasFloat64;

/// Represents a missing value in the dataset. Missing values are typically
/// only used for numeric values, as using them with textual values would lead
/// to ambiguities. For example, the character `.` is the most common missing
/// value indicator, which is valid text.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SasMissingValue {
    code: u8,
}

impl SasMissingValue {
    /// The period (`.`) missing value
    pub const PERIOD: Self = Self { code: b'.' };
    /// The underscore (`_`) missing value
    pub const UNDERSCORE: Self = Self { code: b'_' };

    /// Returns the numeric representation of the missing value.
    #[inline]
    #[must_use]
    pub const fn code(self) -> u8 {
        self.code
    }

    /// Creates a missing value from the given code.
    ///
    /// This accepts any `u8` value, even those outside the 28 standard SAS
    /// missing value codes. Use [`try_from_u8`](Self::try_from_u8) for
    /// validated construction, or [`is_standard`](Self::is_standard) to check
    /// if a code is valid.
    #[inline]
    #[must_use]
    pub const fn from_u8(code: u8) -> Self {
        Self { code }
    }

    /// Attempts to create a missing value from a standard SAS code.
    ///
    /// Returns `Some` only for the 28 valid SAS missing value codes:
    /// - `.` (period) - the primary/system missing value
    /// - `_` (underscore)
    /// - `A` through `Z`
    #[inline]
    #[must_use]
    pub const fn try_from_u8(code: u8) -> Option<Self> {
        if Self::is_standard_code(code) {
            Some(Self { code })
        } else {
            None
        }
    }

    /// Returns `true` if this missing value uses one of the 28 standard SAS codes.
    ///
    /// The standard codes are: `.`, `_`, and `A` through `Z`.
    #[inline]
    #[must_use]
    pub const fn is_standard(self) -> bool {
        Self::is_standard_code(self.code)
    }

    /// Returns `true` if the given code is one of the 28 standard SAS missing value codes.
    const fn is_standard_code(code: u8) -> bool {
        code == b'.' || code == b'_' || (code >= b'A' && code <= b'Z')
    }

    /// Attempts to convert the given [`SasFloat64`] to a missing value.
    #[inline]
    #[must_use]
    pub const fn try_from_sas_float64(value: SasFloat64) -> Option<Self> {
        let bytes = value.to_be_bytes();
        if bytes[0] != 0
            && bytes[1] == 0
            && bytes[2] == 0
            && bytes[3] == 0
            && bytes[4] == 0
            && bytes[5] == 0
            && bytes[6] == 0
            && bytes[7] == 0
        {
            Some(Self::from_u8(bytes[0]))
        } else {
            None
        }
    }

    /// Attempts to convert the given `f64` to a missing value.
    ///
    /// SAS® uses the fact that IEEE-754 floating point values can have multiple NaN
    /// representations. They store the 8-bit missing value representation as a NaN sequence
    /// inside the IEEE floating point value. This method attempts to extract that encoded
    /// sequence.
    ///
    /// If the extracted code is not a standard SAS missing value code, this returns
    /// [`Self::PERIOD`] as a fallback.
    #[inline]
    #[must_use]
    pub fn try_from_f64(value: f64) -> Option<Self> {
        if !value.is_nan() {
            return None;
        }
        let bytes = value.to_be_bytes();
        let code = !bytes[2];
        let missing_value = if Self::is_standard_code(code) {
            Self::from_u8(code)
        } else {
            Self::PERIOD
        };
        Some(missing_value)
    }
}

impl Default for SasMissingValue {
    /// Returns [`Self::PERIOD`].
    #[inline]
    fn default() -> Self {
        Self::PERIOD
    }
}

impl From<SasMissingValue> for f64 {
    /// Creates an `f64` from a [`SasMissingValue`].
    fn from(value: SasMissingValue) -> Self {
        let code = value.code();
        let mut bytes = [0u8; 8];
        bytes[0] = 0xFFu8;
        bytes[1] = 0xFFu8;
        bytes[2] = !code;
        f64::from_be_bytes(bytes)
    }
}

impl From<SasMissingValue> for u8 {
    fn from(value: SasMissingValue) -> Self {
        value.code()
    }
}

impl TryFrom<SasFloat64> for SasMissingValue {
    type Error = SasError;

    #[inline]
    fn try_from(value: SasFloat64) -> Result<Self, Self::Error> {
        Self::try_from_sas_float64(value)
            .ok_or_else(|| SasError::new("Value is not a missing value"))
    }
}

impl TryFrom<f64> for SasMissingValue {
    type Error = SasError;

    #[inline]
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::try_from_f64(value).ok_or_else(|| SasError::new("Value is not NaN"))
    }
}

#[cfg(test)]
mod tests {
    use super::SasMissingValue;
    use crate::sas::sas_float_64::SasFloat64;

    #[test]
    fn default_is_period() {
        assert_eq!(SasMissingValue::PERIOD, SasMissingValue::default());
    }

    #[test]
    fn period_has_correct_code() {
        assert_eq!(b'.', SasMissingValue::PERIOD.code());
    }

    #[test]
    fn underscore_has_correct_code() {
        assert_eq!(b'_', SasMissingValue::UNDERSCORE.code());
    }

    #[test]
    fn from_u8_creates_missing_value() {
        let missing = SasMissingValue::from_u8(b'A');
        assert_eq!(b'A', missing.code());
    }

    #[test]
    fn try_from_sas_float64_with_missing_value() {
        // A SAS missing value has non-zero first byte and remaining bytes zero
        let sas_float = SasFloat64::from_be_bytes([b'.', 0, 0, 0, 0, 0, 0, 0]);
        let missing = SasMissingValue::try_from_sas_float64(sas_float).unwrap();
        assert_eq!(SasMissingValue::PERIOD, missing);
    }

    #[test]
    fn try_from_sas_float64_with_non_missing_value() {
        // A regular value (e.g., 1.0 in IBM format) should return None
        let sas_float = SasFloat64::from_be_bytes([0x41, 0x10, 0, 0, 0, 0, 0, 0]);
        assert!(SasMissingValue::try_from_sas_float64(sas_float).is_none());
    }

    #[test]
    fn round_trip_double_try_from_f64() {
        let missing = SasMissingValue::try_from_f64(f64::NAN).unwrap();
        let ieee = f64::from(missing);
        assert!(f64::is_nan(ieee));
    }

    #[test]
    fn round_trip_double_try_from_sas_float64() {
        let ieee = f64::from(SasMissingValue::PERIOD);
        assert!(f64::is_nan(ieee));
        let round_tripped = SasMissingValue::try_from_f64(ieee).unwrap();
        assert_eq!(SasMissingValue::PERIOD, round_tripped);
    }

    #[test]
    fn try_from_trait_sas_float64_valid() {
        let sas_float = SasFloat64::from_be_bytes([b'.', 0, 0, 0, 0, 0, 0, 0]);
        let missing: SasMissingValue = sas_float.try_into().unwrap();
        assert_eq!(SasMissingValue::PERIOD, missing);
    }

    #[test]
    fn try_from_trait_sas_float64_invalid() {
        let sas_float = SasFloat64::from_be_bytes([0x41, 0x10, 0, 0, 0, 0, 0, 0]);
        let result: Result<SasMissingValue, _> = sas_float.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn try_from_trait_f64_valid() {
        let missing: SasMissingValue = f64::NAN.try_into().unwrap();
        assert!(f64::from(missing).is_nan());
    }

    #[test]
    fn try_from_trait_f64_invalid() {
        let result: Result<SasMissingValue, _> = 1.0f64.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn into_f64_produces_nan() {
        let ieee = f64::from(SasMissingValue::PERIOD);
        assert!(ieee.is_nan());
    }

    #[test]
    fn into_u8_returns_code() {
        assert_eq!(
            SasMissingValue::PERIOD.code(),
            u8::from(SasMissingValue::PERIOD)
        );
        assert_eq!(
            SasMissingValue::UNDERSCORE.code(),
            u8::from(SasMissingValue::UNDERSCORE)
        );
    }

    #[test]
    fn try_from_u8_accepts_period() {
        let missing = SasMissingValue::try_from_u8(b'.').unwrap();
        assert_eq!(SasMissingValue::PERIOD, missing);
    }

    #[test]
    fn try_from_u8_accepts_underscore() {
        let missing = SasMissingValue::try_from_u8(b'_').unwrap();
        assert_eq!(SasMissingValue::UNDERSCORE, missing);
    }

    #[test]
    fn try_from_u8_accepts_all_letters() {
        for code in b'A'..=b'Z' {
            let missing = SasMissingValue::try_from_u8(code);
            assert!(missing.is_some(), "Expected Some for code {}", code as char);
            assert_eq!(code, missing.unwrap().code());
        }
    }

    #[test]
    fn try_from_u8_rejects_invalid_codes() {
        // Test some invalid codes
        let invalid_codes = [0u8, b'0', b'9', b'a', b'z', b' ', b'@', 0xFF];
        for code in invalid_codes {
            assert!(
                SasMissingValue::try_from_u8(code).is_none(),
                "Expected None for code {:02X}",
                code
            );
        }
    }

    #[test]
    fn is_standard_true_for_valid_codes() {
        assert!(SasMissingValue::PERIOD.is_standard());
        assert!(SasMissingValue::UNDERSCORE.is_standard());
        assert!(SasMissingValue::from_u8(b'A').is_standard());
        assert!(SasMissingValue::from_u8(b'Z').is_standard());
    }

    #[test]
    fn is_standard_false_for_invalid_codes() {
        assert!(!SasMissingValue::from_u8(0).is_standard());
        assert!(!SasMissingValue::from_u8(b'a').is_standard());
        assert!(!SasMissingValue::from_u8(0xFF).is_standard());
    }

    #[test]
    fn try_from_f64_falls_back_to_period_for_nonstandard_nan() {
        // Create a NaN with a non-standard payload
        // IEEE NaN format: [0xFF, 0xFF, payload, ...]
        // The code is extracted as !bytes[2], so bytes[2] = !code
        // For code 0x00, bytes[2] = 0xFF
        let nonstandard_nan = f64::from_be_bytes([0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0]);
        assert!(nonstandard_nan.is_nan());

        let missing = SasMissingValue::try_from_f64(nonstandard_nan).unwrap();
        assert_eq!(SasMissingValue::PERIOD, missing);
    }

    #[test]
    fn try_from_f64_preserves_standard_codes() {
        // Test that standard codes round-trip correctly
        for code in std::iter::once(b'.')
            .chain(std::iter::once(b'_'))
            .chain(b'A'..=b'Z')
        {
            let original = SasMissingValue::from_u8(code);
            let ieee = f64::from(original);
            let recovered = SasMissingValue::try_from_f64(ieee).unwrap();
            assert_eq!(
                original, recovered,
                "Code {} did not round-trip",
                code as char
            );
        }
    }
}
