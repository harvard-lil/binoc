use super::SasError;

/// Represents a month that appears in a SAS® string-formatted date/time.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SasMonth {
    /// The month of January
    January = 1,
    /// The month of February
    February = 2,
    /// The month of March
    March = 3,
    /// The month of April
    April = 4,
    /// The month of May
    May = 5,
    /// The month of June
    June = 6,
    /// The month of July
    July = 7,
    /// The month of August
    August = 8,
    /// The month of September
    September = 9,
    /// The month of October
    October = 10,
    /// The month of November
    November = 11,
    /// The month of December
    December = 12,
}

impl SasMonth {
    /// Returns the 3-character uppercase abbreviation for this month (e.g., "JAN", "FEB").
    #[must_use]
    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::January => "JAN",
            Self::February => "FEB",
            Self::March => "MAR",
            Self::April => "APR",
            Self::May => "MAY",
            Self::June => "JUN",
            Self::July => "JUL",
            Self::August => "AUG",
            Self::September => "SEP",
            Self::October => "OCT",
            Self::November => "NOV",
            Self::December => "DEC",
        }
    }

    /// Attempts to parse a 3-character month abbreviation from a SAS® string-formatted date/time.
    #[must_use]
    pub fn from_abbreviation(value: &str) -> Option<Self> {
        match value {
            "JAN" => Some(Self::January),
            "FEB" => Some(Self::February),
            "MAR" => Some(Self::March),
            "APR" => Some(Self::April),
            "MAY" => Some(Self::May),
            "JUN" => Some(Self::June),
            "JUL" => Some(Self::July),
            "AUG" => Some(Self::August),
            "SEP" => Some(Self::September),
            "OCT" => Some(Self::October),
            "NOV" => Some(Self::November),
            "DEC" => Some(Self::December),
            _ => None,
        }
    }

    /// Attempts to convert from a numeric representation to a month. The only valid
    /// values are between 1 and 12, inclusive, representing January (1) through December (12).
    #[must_use]
    pub fn try_from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::January),
            2 => Some(Self::February),
            3 => Some(Self::March),
            4 => Some(Self::April),
            5 => Some(Self::May),
            6 => Some(Self::June),
            7 => Some(Self::July),
            8 => Some(Self::August),
            9 => Some(Self::September),
            10 => Some(Self::October),
            11 => Some(Self::November),
            12 => Some(Self::December),
            _ => None,
        }
    }
}

impl From<SasMonth> for u8 {
    /// Converts the month to its numeric representation, 1 through 12.
    fn from(value: SasMonth) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for SasMonth {
    type Error = SasError;

    #[inline]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from_u8(value).ok_or_else(|| SasError::new("Invalid month value"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_month_abbreviation(month: SasMonth, expected: &str) {
        assert_eq!(expected, month.abbreviation());
    }

    fn assert_month_from_abbreviation(abbr: &str, expected: SasMonth) {
        assert_eq!(Some(expected), SasMonth::from_abbreviation(abbr));
    }

    fn assert_month_try_from_u8(num: u8, expected: SasMonth) {
        assert_eq!(Some(expected), SasMonth::try_from_u8(num));
    }

    fn assert_month_into_u8(month: SasMonth, expected: u8) {
        assert_eq!(expected, u8::from(month));
    }

    // abbreviation tests
    #[test]
    fn abbreviation_january() {
        assert_month_abbreviation(SasMonth::January, "JAN");
    }

    #[test]
    fn abbreviation_february() {
        assert_month_abbreviation(SasMonth::February, "FEB");
    }

    #[test]
    fn abbreviation_march() {
        assert_month_abbreviation(SasMonth::March, "MAR");
    }

    #[test]
    fn abbreviation_april() {
        assert_month_abbreviation(SasMonth::April, "APR");
    }

    #[test]
    fn abbreviation_may() {
        assert_month_abbreviation(SasMonth::May, "MAY");
    }

    #[test]
    fn abbreviation_june() {
        assert_month_abbreviation(SasMonth::June, "JUN");
    }

    #[test]
    fn abbreviation_july() {
        assert_month_abbreviation(SasMonth::July, "JUL");
    }

    #[test]
    fn abbreviation_august() {
        assert_month_abbreviation(SasMonth::August, "AUG");
    }

    #[test]
    fn abbreviation_september() {
        assert_month_abbreviation(SasMonth::September, "SEP");
    }

    #[test]
    fn abbreviation_october() {
        assert_month_abbreviation(SasMonth::October, "OCT");
    }

    #[test]
    fn abbreviation_november() {
        assert_month_abbreviation(SasMonth::November, "NOV");
    }

    #[test]
    fn abbreviation_december() {
        assert_month_abbreviation(SasMonth::December, "DEC");
    }

    // from_abbreviation tests
    #[test]
    fn from_abbreviation_january() {
        assert_month_from_abbreviation("JAN", SasMonth::January);
    }

    #[test]
    fn from_abbreviation_february() {
        assert_month_from_abbreviation("FEB", SasMonth::February);
    }

    #[test]
    fn from_abbreviation_march() {
        assert_month_from_abbreviation("MAR", SasMonth::March);
    }

    #[test]
    fn from_abbreviation_april() {
        assert_month_from_abbreviation("APR", SasMonth::April);
    }

    #[test]
    fn from_abbreviation_may() {
        assert_month_from_abbreviation("MAY", SasMonth::May);
    }

    #[test]
    fn from_abbreviation_june() {
        assert_month_from_abbreviation("JUN", SasMonth::June);
    }

    #[test]
    fn from_abbreviation_july() {
        assert_month_from_abbreviation("JUL", SasMonth::July);
    }

    #[test]
    fn from_abbreviation_august() {
        assert_month_from_abbreviation("AUG", SasMonth::August);
    }

    #[test]
    fn from_abbreviation_september() {
        assert_month_from_abbreviation("SEP", SasMonth::September);
    }

    #[test]
    fn from_abbreviation_october() {
        assert_month_from_abbreviation("OCT", SasMonth::October);
    }

    #[test]
    fn from_abbreviation_november() {
        assert_month_from_abbreviation("NOV", SasMonth::November);
    }

    #[test]
    fn from_abbreviation_december() {
        assert_month_from_abbreviation("DEC", SasMonth::December);
    }

    #[test]
    fn from_abbreviation_returns_none_for_unknown() {
        assert_eq!(None, SasMonth::from_abbreviation("XXX"));
    }

    #[test]
    fn from_abbreviation_returns_none_for_lowercase() {
        assert_eq!(None, SasMonth::from_abbreviation("jan"));
    }

    #[test]
    fn from_abbreviation_returns_none_for_empty() {
        assert_eq!(None, SasMonth::from_abbreviation(""));
    }

    // from_u8 tests
    #[test]
    fn try_from_u8_january() {
        assert_month_try_from_u8(1, SasMonth::January);
    }

    #[test]
    fn try_from_u8_february() {
        assert_month_try_from_u8(2, SasMonth::February);
    }

    #[test]
    fn try_from_u8_march() {
        assert_month_try_from_u8(3, SasMonth::March);
    }

    #[test]
    fn try_from_u8_april() {
        assert_month_try_from_u8(4, SasMonth::April);
    }

    #[test]
    fn try_from_u8_may() {
        assert_month_try_from_u8(5, SasMonth::May);
    }

    #[test]
    fn try_from_u8_june() {
        assert_month_try_from_u8(6, SasMonth::June);
    }

    #[test]
    fn try_from_u8_july() {
        assert_month_try_from_u8(7, SasMonth::July);
    }

    #[test]
    fn try_from_u8_august() {
        assert_month_try_from_u8(8, SasMonth::August);
    }

    #[test]
    fn try_from_u8_september() {
        assert_month_try_from_u8(9, SasMonth::September);
    }

    #[test]
    fn try_from_u8_october() {
        assert_month_try_from_u8(10, SasMonth::October);
    }

    #[test]
    fn try_from_u8_november() {
        assert_month_try_from_u8(11, SasMonth::November);
    }

    #[test]
    fn try_from_u8_december() {
        assert_month_try_from_u8(12, SasMonth::December);
    }

    #[test]
    fn try_from_u8_returns_none_for_zero() {
        assert_eq!(None, SasMonth::try_from_u8(0));
    }

    #[test]
    fn try_from_u8_returns_none_for_thirteen() {
        assert_eq!(None, SasMonth::try_from_u8(13));
    }

    // into_u8 tests
    #[test]
    fn into_u8_january() {
        assert_month_into_u8(SasMonth::January, 1);
    }

    #[test]
    fn into_u8_february() {
        assert_month_into_u8(SasMonth::February, 2);
    }

    #[test]
    fn into_u8_march() {
        assert_month_into_u8(SasMonth::March, 3);
    }

    #[test]
    fn into_u8_april() {
        assert_month_into_u8(SasMonth::April, 4);
    }

    #[test]
    fn into_u8_may() {
        assert_month_into_u8(SasMonth::May, 5);
    }

    #[test]
    fn into_u8_june() {
        assert_month_into_u8(SasMonth::June, 6);
    }

    #[test]
    fn into_u8_july() {
        assert_month_into_u8(SasMonth::July, 7);
    }

    #[test]
    fn into_u8_august() {
        assert_month_into_u8(SasMonth::August, 8);
    }

    #[test]
    fn into_u8_september() {
        assert_month_into_u8(SasMonth::September, 9);
    }

    #[test]
    fn into_u8_october() {
        assert_month_into_u8(SasMonth::October, 10);
    }

    #[test]
    fn into_u8_november() {
        assert_month_into_u8(SasMonth::November, 11);
    }

    #[test]
    fn into_u8_december() {
        assert_month_into_u8(SasMonth::December, 12);
    }

    // ordering tests
    #[test]
    fn months_are_ordered_chronologically() {
        assert!(SasMonth::January < SasMonth::February);
        assert!(SasMonth::February < SasMonth::March);
        assert!(SasMonth::March < SasMonth::April);
        assert!(SasMonth::April < SasMonth::May);
        assert!(SasMonth::May < SasMonth::June);
        assert!(SasMonth::June < SasMonth::July);
        assert!(SasMonth::July < SasMonth::August);
        assert!(SasMonth::August < SasMonth::September);
        assert!(SasMonth::September < SasMonth::October);
        assert!(SasMonth::October < SasMonth::November);
        assert!(SasMonth::November < SasMonth::December);
    }

    // round-trip tests
    #[test]
    fn abbreviation_round_trips_january() {
        assert_eq!(
            Some(SasMonth::January),
            SasMonth::from_abbreviation(SasMonth::January.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_february() {
        assert_eq!(
            Some(SasMonth::February),
            SasMonth::from_abbreviation(SasMonth::February.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_march() {
        assert_eq!(
            Some(SasMonth::March),
            SasMonth::from_abbreviation(SasMonth::March.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_april() {
        assert_eq!(
            Some(SasMonth::April),
            SasMonth::from_abbreviation(SasMonth::April.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_may() {
        assert_eq!(
            Some(SasMonth::May),
            SasMonth::from_abbreviation(SasMonth::May.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_june() {
        assert_eq!(
            Some(SasMonth::June),
            SasMonth::from_abbreviation(SasMonth::June.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_july() {
        assert_eq!(
            Some(SasMonth::July),
            SasMonth::from_abbreviation(SasMonth::July.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_august() {
        assert_eq!(
            Some(SasMonth::August),
            SasMonth::from_abbreviation(SasMonth::August.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_september() {
        assert_eq!(
            Some(SasMonth::September),
            SasMonth::from_abbreviation(SasMonth::September.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_october() {
        assert_eq!(
            Some(SasMonth::October),
            SasMonth::from_abbreviation(SasMonth::October.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_november() {
        assert_eq!(
            Some(SasMonth::November),
            SasMonth::from_abbreviation(SasMonth::November.abbreviation())
        );
    }

    #[test]
    fn abbreviation_round_trips_december() {
        assert_eq!(
            Some(SasMonth::December),
            SasMonth::from_abbreviation(SasMonth::December.abbreviation())
        );
    }

    // TryFrom trait tests
    #[test]
    fn try_from_trait_january() {
        let actual: SasMonth = 1u8.try_into().unwrap();
        assert_eq!(SasMonth::January, actual);
    }

    #[test]
    fn try_from_trait_december() {
        let actual: SasMonth = 12u8.try_into().unwrap();
        assert_eq!(SasMonth::December, actual);
    }

    #[test]
    fn try_from_trait_invalid_zero() {
        let result: Result<SasMonth, _> = 0u8.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn try_from_trait_invalid_thirteen() {
        let result: Result<SasMonth, _> = 13u8.try_into();
        assert!(result.is_err());
    }
}
