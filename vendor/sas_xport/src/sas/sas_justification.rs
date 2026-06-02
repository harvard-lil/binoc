use super::SasError;

/// Represents the justification of a variable when padding is required.
#[repr(u16)]
#[derive(Copy, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SasJustification {
    /// The variable is left-aligned, with padding on the right (the default).
    #[default]
    Left = 0,
    /// The variable is right-aligned, with padding on the left.
    Right = 1,
}

impl SasJustification {
    /// Returns the variable justification encoded as a 16-bit unsigned integer.
    #[inline]
    #[must_use]
    pub fn code(&self) -> u16 {
        *self as u16
    }

    /// Attempts to parse a justification from a numeric value, or `None` if the value
    /// is not a valid representation of a justification.
    #[inline]
    #[must_use]
    pub const fn try_from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            _ => None,
        }
    }
}

impl From<SasJustification> for u16 {
    fn from(value: SasJustification) -> Self {
        value as u16
    }
}

impl TryFrom<u16> for SasJustification {
    type Error = SasError;

    #[inline]
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_from_u16(value).ok_or_else(|| SasError::new("Invalid justification value"))
    }
}

#[cfg(test)]
mod tests {
    use super::SasJustification;

    #[test]
    fn default_is_left() {
        assert_eq!(SasJustification::Left, SasJustification::default());
    }

    #[test]
    fn try_from_u16_left() {
        let actual = SasJustification::try_from_u16(0u16).unwrap();
        let expected = SasJustification::Left;
        assert_eq!(expected, actual);
        assert_eq!(0u16, u16::from(actual));
    }

    #[test]
    fn try_from_u16_right() {
        let actual = SasJustification::try_from_u16(1u16).unwrap();
        let expected = SasJustification::Right;
        assert_eq!(expected, actual);
        assert_eq!(1u16, u16::from(actual));
    }

    #[test]
    fn try_from_u16_invalid() {
        let actual = SasJustification::try_from_u16(2u16);
        assert!(actual.is_none());
    }

    #[test]
    fn try_from_trait_left() {
        let actual: SasJustification = 0u16.try_into().unwrap();
        assert_eq!(SasJustification::Left, actual);
    }

    #[test]
    fn try_from_trait_right() {
        let actual: SasJustification = 1u16.try_into().unwrap();
        assert_eq!(SasJustification::Right, actual);
    }

    #[test]
    fn try_from_trait_invalid() {
        let result: Result<SasJustification, _> = 2u16.try_into();
        assert!(result.is_err());
    }
}
