use super::SasError;

/// Indicates how a variable is stored in the file.
#[repr(u16)]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SasVariableType {
    /// The variable is stored as an IBM hexadecimal floating point number.
    /// See: <https://en.wikipedia.org/wiki/IBM_hexadecimal_floating-point>.
    Numeric = 1,
    /// The value is stored as text, padded with spaces (ASCII 0x20 characters) to fill
    /// the specified fixed width of the variable.
    Character = 2,
}

impl SasVariableType {
    /// Returns the variable type encoded as a 16-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Attempts to parse a variable type from a 16-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn try_from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Numeric),
            2 => Some(Self::Character),
            _ => None,
        }
    }
}

impl From<SasVariableType> for u16 {
    fn from(value: SasVariableType) -> Self {
        value.code()
    }
}

impl TryFrom<u16> for SasVariableType {
    type Error = SasError;

    #[inline]
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_from_u16(value).ok_or_else(|| SasError::new("Invalid SAS variable type"))
    }
}

#[cfg(test)]
mod tests {
    use super::SasVariableType;

    #[test]
    fn try_from_u16_numeric() {
        assert_eq!(
            Some(SasVariableType::Numeric),
            SasVariableType::try_from_u16(1)
        );
    }

    #[test]
    fn try_from_u16_character() {
        assert_eq!(
            Some(SasVariableType::Character),
            SasVariableType::try_from_u16(2)
        );
    }

    #[test]
    fn try_from_u16_returns_none_for_zero() {
        assert!(SasVariableType::try_from_u16(0).is_none());
    }

    #[test]
    fn try_from_u16_returns_none_for_three() {
        assert!(SasVariableType::try_from_u16(3).is_none());
    }

    #[test]
    fn code_numeric() {
        assert_eq!(1u16, SasVariableType::Numeric.code());
    }

    #[test]
    fn code_character() {
        assert_eq!(2u16, SasVariableType::Character.code());
    }

    #[test]
    fn into_u16_numeric() {
        assert_eq!(
            SasVariableType::Numeric.code(),
            u16::from(SasVariableType::Numeric)
        );
    }

    #[test]
    fn into_u16_character() {
        assert_eq!(
            SasVariableType::Character.code(),
            u16::from(SasVariableType::Character)
        );
    }

    #[test]
    fn try_from_trait_numeric() {
        let value = SasVariableType::Numeric.code();
        let actual: SasVariableType = value.try_into().unwrap();
        assert_eq!(SasVariableType::Numeric, actual);
    }

    #[test]
    fn try_from_trait_character() {
        let value = SasVariableType::Character.code();
        let actual: SasVariableType = value.try_into().unwrap();
        assert_eq!(SasVariableType::Character, actual);
    }

    #[test]
    fn try_from_trait_invalid() {
        let result: Result<SasVariableType, _> = 0u16.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn numeric_less_than_character() {
        assert!(SasVariableType::Numeric < SasVariableType::Character);
    }
}
