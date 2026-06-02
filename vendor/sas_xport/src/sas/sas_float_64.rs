use ibm_hfp::{IbmFloat64, IbmFloatError};
use std::fmt::{self, Display, Formatter};

use super::sas_missing_value::SasMissingValue;

/// Saturating IBM HFP conversion for SAS-side use.
///
/// `IbmFloat64::try_from(f64)` is strict: it errors on infinity, overflow, and
/// underflow. SAS XPORT writers want clamping at the IBM range boundary
/// instead, so this helper maps those errors to `MAX_VALUE` / `MIN_VALUE` /
/// signed zero. Only NaN propagates as an error (the `SasFloat64` caller
/// pre-filters NaN via `is_finite()`, so the helper never actually returns
/// `Err` from inside `SasFloat64::try_from` — but we keep the fallible
/// signature so the helper is reusable by future callers without surprise).
fn ieee_to_ibm_saturating(value: f64) -> Result<IbmFloat64, IbmFloatError> {
    use IbmFloatError::{
        NegativeInfinity, NegativeOverflow, NegativeUnderflow, NotANumber, PositiveInfinity,
        PositiveOverflow, PositiveUnderflow,
    };
    // Note on the underflow case: both signs collapse to +0. The IBM byte
    // pattern for -0 is [0x80, 0, 0, 0, 0, 0, 0, 0], which collides with the
    // SAS missing-value sentinel encoding (any non-zero byte 0 with bytes
    // 1..=7 all zero is read as a missing value / NaN by `From<SasFloat64>
    // for f64`). Saturating negative underflow to -0 IBM would therefore
    // round-trip as NaN instead of zero — so we drop the sign instead.
    match IbmFloat64::try_from(value) {
        Ok(ibm) => Ok(ibm),
        Err(NotANumber) => Err(NotANumber),
        Err(PositiveInfinity | PositiveOverflow) => Ok(IbmFloat64::MAX_VALUE),
        Err(NegativeInfinity | NegativeOverflow) => Ok(IbmFloat64::MIN_VALUE),
        Err(PositiveUnderflow | NegativeUnderflow) => Ok(IbmFloat64::new()),
    }
}

/// Represents a 64-bit SAS floating point value.
///
/// SAS uses IBM hexadecimal floating point format as its base representation
/// but extends it to support "missing values" - special sentinel values that
/// indicate absent or null data. SAS supports 28 distinct missing values:
/// the primary missing value (`.`), the underscore (`_`), and letters A-Z.
///
/// When a `SasFloat64` represents a missing value, the first byte contains
/// the missing value code (with the sign bit potentially set), and all
/// remaining bytes are zero.
///
/// When converting to IEEE 754 `f64`, missing values are encoded as specific
/// NaN bit patterns, allowing round-trip conversion to preserve the missing
/// value identity.
#[derive(Clone, Copy, Debug, Default)]
pub struct SasFloat64 {
    ibm: IbmFloat64,
}

impl SasFloat64 {
    /// The maximum representable value.
    pub const MAX_VALUE: Self = Self::from_ibm_float_64(IbmFloat64::MAX_VALUE);
    /// The minimum representable value (most negative).
    pub const MIN_VALUE: Self = Self::from_ibm_float_64(IbmFloat64::MIN_VALUE);

    /// Creates a new `SasFloat64` with a value of 0.0.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ibm: IbmFloat64::new(),
        }
    }

    /// Creates a `SasFloat64` from an `IbmFloat64`.
    #[inline]
    #[must_use]
    pub const fn from_ibm_float_64(ibm: IbmFloat64) -> Self {
        Self { ibm }
    }

    /// Creates a `SasFloat64` from a big-endian byte array.
    #[inline]
    #[must_use]
    pub const fn from_be_bytes(bytes: [u8; 8]) -> Self {
        Self {
            ibm: IbmFloat64::from_be_bytes(bytes),
        }
    }

    /// Creates a `SasFloat64` from a little-endian byte array.
    #[inline]
    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self {
            ibm: IbmFloat64::from_le_bytes(bytes),
        }
    }

    /// Returns the value as a big-endian byte array.
    #[inline]
    #[must_use]
    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.ibm.to_be_bytes()
    }

    /// Returns the value as a little-endian byte array.
    #[inline]
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 8] {
        self.ibm.to_le_bytes()
    }

    /// Returns `true` if the value is positive (including positive zero).
    #[inline]
    #[must_use]
    pub const fn is_sign_positive(self) -> bool {
        self.ibm.is_sign_positive()
    }

    /// Returns `true` if the value is negative (including negative zero).
    #[inline]
    #[must_use]
    pub const fn is_sign_negative(self) -> bool {
        self.ibm.is_sign_negative()
    }

    /// Returns `true` if this value represents positive or negative infinity.
    ///
    /// Note: IBM floating point does not have native infinity representation.
    /// This implementation uses the maximum representable value as infinity.
    #[inline]
    #[must_use]
    pub const fn is_infinite(self) -> bool {
        let bytes = self.ibm.to_be_bytes();
        bytes[0] & 0x7Fu8 == 0x7Fu8
            && bytes[1] == 0xFFu8
            && bytes[2] == 0xFFu8
            && bytes[3] == 0xFFu8
            && bytes[4] == 0xFFu8
            && bytes[5] == 0xFFu8
            && bytes[6] == 0xFFu8
            && bytes[7] == 0xFFu8
    }

    /// Returns `true` if this value represents a SAS missing value.
    ///
    /// In SAS, missing values are analogous to IEEE 754 NaN values.
    #[inline]
    #[must_use]
    pub const fn is_nan(self) -> bool {
        self.missing_value().is_some()
    }

    /// Returns `true` if this value is finite (not infinite and not a missing value).
    #[inline]
    #[must_use]
    pub const fn is_finite(self) -> bool {
        !self.is_infinite() && !self.is_nan()
    }

    /// Returns the missing value if this `SasFloat64` represents one, or `None` otherwise.
    ///
    /// A SAS missing value is identified by having all bytes except the first be zero.
    /// The first byte (with the sign bit masked off) contains the missing value code.
    #[inline]
    #[must_use]
    pub const fn missing_value(self) -> Option<SasMissingValue> {
        let bytes = self.ibm.to_be_bytes();
        if bytes[1] != 0
            || bytes[2] != 0
            || bytes[3] != 0
            || bytes[4] != 0
            || bytes[5] != 0
            || bytes[6] != 0
            || bytes[7] != 0
        {
            return None;
        }
        let high = bytes[0] & 0x7Fu8;
        Some(SasMissingValue::from_u8(high))
    }
}

impl Display for SasFloat64 {
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&f64::from(*self), formatter)
    }
}

impl TryFrom<f64> for SasFloat64 {
    type Error = ();

    #[inline]
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        // Fast path: normal finite numbers (the overwhelming majority of calls).
        // `IbmFloat64::try_from` is strict; the saturating helper clamps
        // overflow/underflow/infinity at the IBM range boundary. Finite inputs
        // can never produce NaN errors, so the `.map_err` is dead in practice.
        if value.is_finite() {
            return ieee_to_ibm_saturating(value)
                .map(Self::from)
                .map_err(|_| ());
        }
        // Slow path: infinite or NaN.
        if value.is_infinite() {
            let infinity = if value.is_sign_positive() {
                Self::MAX_VALUE
            } else {
                Self::MIN_VALUE
            };
            return Ok(infinity);
        }
        // NaN: check for SAS missing value encoding, else default.
        if let Some(missing_value) = SasMissingValue::try_from_f64(value) {
            return Ok(missing_value.into());
        }
        Ok(SasMissingValue::default().into())
    }
}

impl From<IbmFloat64> for SasFloat64 {
    #[inline]
    fn from(ibm: IbmFloat64) -> Self {
        Self { ibm }
    }
}

impl From<SasFloat64> for f64 {
    fn from(value: SasFloat64) -> f64 {
        let bytes = value.ibm.to_be_bytes();
        let high = bytes[0];
        if high != 0 {
            let is_nan = bytes[1..].iter().all(|&x| x == 0u8);
            if is_nan {
                let bytes = [
                    0xFFu8, 0xFFu8, !high, 0x00u8, 0x00u8, 0x00u8, 0x00u8, 0x00u8,
                ];
                return f64::from_be_bytes(bytes);
            }
            let is_infinite = bytes[1..].iter().all(|&x| x == 0xFFu8);
            if is_infinite {
                return if (high & 0x80u8) == 0 {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                };
            }
        }
        f64::from(value.ibm)
    }
}

impl From<SasFloat64> for IbmFloat64 {
    #[inline]
    fn from(value: SasFloat64) -> IbmFloat64 {
        value.ibm
    }
}

impl From<SasMissingValue> for SasFloat64 {
    #[inline]
    fn from(value: SasMissingValue) -> Self {
        Self::from_be_bytes([
            value.code(),
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sas::sas_missing_value::SasMissingValue;
    use float_cmp::assert_approx_eq;

    #[test]
    fn new_as_bytes_returns_slice() {
        let x = SasFloat64::new();
        assert_eq!([0u8; 8], x.to_be_bytes());
    }

    #[test]
    fn from_f64_zero() {
        let x = SasFloat64::try_from(0f64).unwrap();
        assert_eq!([0u8; 8], x.to_be_bytes());
    }

    #[test]
    fn round_trip_zero() {
        let x = SasFloat64::try_from(0f64).unwrap();
        let expected = [0x00u8; 8];
        let actual = x.to_be_bytes();
        assert_eq!(expected, actual);

        let float: f64 = x.into();
        assert_approx_eq!(f64, 0f64, float);
    }

    #[test]
    fn round_trip_118_625() {
        let x = SasFloat64::try_from(-118.625).unwrap();
        let expected = [
            0b1100_0010u8,
            0b0111_0110u8,
            0b1010_0000u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
        ];
        let actual = x.to_be_bytes();
        assert_eq!(expected, actual);

        assert!(x.is_sign_negative());

        let float: f64 = x.into();
        assert_approx_eq!(f64, -118.625, float);
    }

    #[test]
    fn round_trip_nan_default_ieee() {
        let from_ieee = SasFloat64::try_from(f64::NAN).unwrap();
        let from_sas = f64::from(from_ieee);
        assert!(f64::is_nan(from_sas));
    }

    #[test]
    fn round_trip_nan_period() {
        let x = SasFloat64::from(SasMissingValue::PERIOD);
        let expected = [
            SasMissingValue::PERIOD.code(),
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
        ];
        let actual = x.to_be_bytes();
        assert_eq!(expected, actual);

        assert!(
            x.missing_value()
                .is_some_and(|x| x == SasMissingValue::PERIOD)
        );

        let float: f64 = x.into();
        assert!(f64::is_nan(float));
    }

    #[test]
    fn round_trip_nan_underscore() {
        let missing_code = SasMissingValue::UNDERSCORE;
        let expected = [
            missing_code.code(),
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
            0x00u8,
        ];
        let x = SasFloat64::from_be_bytes(expected);

        assert!(x.missing_value().is_some_and(|x| x == missing_code));

        // There are many representations of NaN in IEEE-754. SAS uses this
        // by encoding the missing value code inside the IEEE floating point.
        // We should be able to convert an IBM float to an IEEE float and back
        // again, retaining the original missing value code used.
        let float: f64 = x.into();
        assert!(f64::is_nan(float));

        let y = SasFloat64::try_from(float).unwrap();
        assert!(y.missing_value().is_some_and(|x| x == missing_code));
    }

    #[test]
    fn round_trip_positive_infinity() {
        let x = SasFloat64::try_from(f64::INFINITY).unwrap();
        let expected = [
            0x7Fu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
        ];
        let actual = x.to_be_bytes();
        assert_eq!(expected, actual);

        assert!(x.is_sign_positive());
        assert!(x.is_infinite());

        let float: f64 = x.into();
        assert_approx_eq!(f64, f64::INFINITY, float);
    }

    #[test]
    fn round_trip_negative_infinity() {
        let x = SasFloat64::try_from(f64::NEG_INFINITY).unwrap();
        let expected = [0xFFu8; 8];
        let actual = x.to_be_bytes();
        assert_eq!(expected, actual);

        assert!(x.is_sign_negative());
        assert!(x.is_infinite());

        let float: f64 = x.into();
        assert_approx_eq!(f64, f64::NEG_INFINITY, float);
    }

    #[test]
    fn all_missing_values_round_trip() {
        // Test all 28 SAS missing values: period, underscore, and A-Z
        let codes: Vec<u8> = std::iter::once(b'.')
            .chain(std::iter::once(b'_'))
            .chain(b'A'..=b'Z')
            .collect();

        for code in codes {
            let missing = SasMissingValue::from_u8(code);
            let sas_float = SasFloat64::from(missing);

            // Verify it's recognized as a missing value
            assert!(
                sas_float.is_nan(),
                "Missing value {code} not recognized as NaN"
            );

            // Convert to IEEE and back
            let ieee: f64 = sas_float.into();
            assert!(
                ieee.is_nan(),
                "Missing value {code} not NaN after IEEE conversion"
            );

            let round_tripped = SasFloat64::try_from(ieee).unwrap();
            let recovered = round_tripped.missing_value();

            assert!(
                recovered.is_some(),
                "Failed to recover missing value for code {code}"
            );
            assert_eq!(
                missing,
                recovered.unwrap(),
                "Missing value mismatch for code {code}"
            );
        }
    }

    #[test]
    fn is_finite_for_normal_values() {
        let x = SasFloat64::try_from(42.0).unwrap();
        assert!(x.is_finite());
        assert!(!x.is_nan());
        assert!(!x.is_infinite());
    }

    #[test]
    fn is_not_finite_for_missing() {
        let x = SasFloat64::from(SasMissingValue::PERIOD);
        assert!(!x.is_finite());
        assert!(x.is_nan());
    }

    #[test]
    fn is_not_finite_for_infinity() {
        let x = SasFloat64::try_from(f64::INFINITY).unwrap();
        assert!(!x.is_finite());
        assert!(x.is_infinite());
    }

    #[test]
    fn from_overflow_positive_saturates_to_max() {
        let huge = 1.0e300;
        let x = SasFloat64::try_from(huge).unwrap();
        assert_eq!(SasFloat64::MAX_VALUE.to_be_bytes(), x.to_be_bytes());
    }

    #[test]
    fn from_overflow_negative_saturates_to_min() {
        let huge = -1.0e300;
        let x = SasFloat64::try_from(huge).unwrap();
        assert_eq!(SasFloat64::MIN_VALUE.to_be_bytes(), x.to_be_bytes());
    }

    #[test]
    fn from_underflow_positive_saturates_to_zero() {
        let tiny = 1.0e-310;
        let x = SasFloat64::try_from(tiny).unwrap();
        let f = f64::from(x);
        assert_eq!(0.0_f64.to_bits(), f.to_bits());
    }

    #[test]
    fn from_underflow_negative_saturates_to_zero() {
        // Negative underflow drops sign because [0x80, 0, ...] would be
        // re-read as a SAS missing value rather than -0. See the comment on
        // `ieee_to_ibm_saturating`.
        let tiny = -1.0e-310;
        let x = SasFloat64::try_from(tiny).unwrap();
        let f = f64::from(x);
        assert_eq!(0.0_f64.to_bits(), f.to_bits());
    }
}
