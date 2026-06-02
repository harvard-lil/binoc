use std::fmt;
use std::str::FromStr;

#[cfg(feature = "chrono")]
use chrono::{DateTime, Datelike, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Timelike};

use super::{SasError, SasMonth};

/// Represents a date/time as encoded in a SAS® XPORT document. The actual representation
/// is a string with the following format: `DDMMMYY:HH:mm:ss`, e.g., `23SEP23:12:56:03`.
/// Note that this date/time format is not Y2K compliant, meaning that the century must be
/// specified when converting to an actual date/time. Also note that this data type performs
/// no validation to ensure that components can be combined to form a valid date. In other
/// words, all components can exceed their logical bounds, such as hour 26, minute 99, or
/// day 45. Leap year rules are not enforced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SasDateTime {
    year: u8,
    month: SasMonth,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl SasDateTime {
    /// Instantiates a new builder for a `SasDateTime`.
    #[inline]
    #[must_use]
    pub fn builder() -> SasDateTimeBuilder {
        SasDateTimeBuilder {
            date_time: Self::new(),
        }
    }

    /// Instantiates a new `SasDateTime` set to January 1st at midnight, year zero.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            year: 0,
            month: SasMonth::January,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }

    /// Instantiates a new `SasDateTime` set to the current time, in the local timezone.
    #[inline]
    #[cfg(feature = "chrono")]
    #[must_use]
    pub fn now() -> Self {
        Self::from_chrono_date_time(Local::now())
    }

    /// Instantiates a new `SasDateTime` set to the given local time.
    #[cfg(feature = "chrono")]
    #[must_use]
    fn from_chrono_date_time(date_time: DateTime<Local>) -> Self {
        Self {
            #[allow(clippy::cast_sign_loss)]
            year: (date_time.year() % 100) as u8,
            month: u8::try_from(date_time.month())
                .ok()
                .and_then(SasMonth::try_from_u8)
                .expect("A chrono DateTime had an invalid month"),
            day: u8::try_from(date_time.day()).expect("A chrono DateTime had an invalid day"),
            hour: u8::try_from(date_time.hour()).expect("A chrono DateTime had an invalid hour"),
            minute: u8::try_from(date_time.minute())
                .expect("A chrono DateTime had an invalid minute"),
            second: u8::try_from(date_time.second())
                .expect("A chrono DateTime had an invalid second"),
        }
    }

    /// Attempts to convert the `SasDateTime` to a `chrono::DateTime<Local>`. Since
    /// the date/time is encoded with a 2-digit year, the base century must be specified,
    /// e.g., 1900, 2000, etc. The base century is simply added to the 2-digit `SasDateTime` year.
    /// If the `SasDateTime` does not represent a valid date/time, `LocalResult::None` is returned.
    #[cfg(feature = "chrono")]
    #[must_use]
    pub fn to_chrono_date_time(self, base_year: i32) -> LocalResult<DateTime<Local>> {
        let year = base_year + i32::from(self.year);
        let month = self.month as u32;
        let Some(date) = NaiveDate::from_ymd_opt(year, month, u32::from(self.day)) else {
            return LocalResult::None;
        };
        let Some(time) = NaiveTime::from_hms_opt(
            u32::from(self.hour),
            u32::from(self.minute),
            u32::from(self.second),
        ) else {
            return LocalResult::None;
        };

        let date_time = date.and_time(time);
        Local.from_local_datetime(&date_time)
    }

    /// Gets the year (0-99). Since `SasDateTime` is not Y2K-compliant, how the
    /// year is interpreted is up to the consumer.
    #[inline]
    #[must_use]
    pub fn year(&self) -> u8 {
        self.year
    }

    /// Gets the month.
    #[inline]
    #[must_use]
    pub fn month(&self) -> SasMonth {
        self.month
    }

    /// Gets the day of the month (1-31). Not guaranteed to be valid.
    #[inline]
    #[must_use]
    pub fn day(&self) -> u8 {
        self.day
    }

    /// Gets the hour (0-23). Not guaranteed to be valid.
    #[inline]
    #[must_use]
    pub fn hour(&self) -> u8 {
        self.hour
    }

    /// Gets the minute (0-59). Not guaranteed to be valid.
    #[inline]
    #[must_use]
    pub fn minute(&self) -> u8 {
        self.minute
    }

    /// Gets the second (0-59). Not guaranteed to be valid.
    #[inline]
    #[must_use]
    pub fn second(&self) -> u8 {
        self.second
    }
}

impl Default for SasDateTime {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// A builder for constructing `SasDateTime` instances.
#[derive(Clone, Copy, Debug, Default)]
pub struct SasDateTimeBuilder {
    date_time: SasDateTime,
}

impl SasDateTimeBuilder {
    /// Sets the year. This should be a value between 0 and 99, but this is not enforced.
    #[inline]
    #[must_use]
    pub fn year(mut self, year: u8) -> Self {
        self.date_time.year = year;
        self
    }

    /// Sets the month.
    #[inline]
    #[must_use]
    pub fn month(mut self, month: SasMonth) -> Self {
        self.date_time.month = month;
        self
    }

    /// Sets the day of the month. This should be a valid day for the month
    /// and year, but this is not enforced.
    #[inline]
    #[must_use]
    pub fn day(mut self, day: u8) -> Self {
        self.date_time.day = day;
        self
    }

    /// Sets the hour. This should be a value between 0 and 23, but this is not enforced.
    #[inline]
    #[must_use]
    pub fn hour(mut self, hour: u8) -> Self {
        self.date_time.hour = hour;
        self
    }

    /// Sets the minute. This should be a value between 0 and 59, but this is not enforced.
    #[inline]
    #[must_use]
    pub fn minute(mut self, minute: u8) -> Self {
        self.date_time.minute = minute;
        self
    }

    /// Sets the second. This should be a value between 0 and 59, but this is not enforced.
    #[inline]
    #[must_use]
    pub fn second(mut self, second: u8) -> Self {
        self.date_time.second = second;
        self
    }

    /// Creates a `SasDateTime` based on the current configuration.
    #[inline]
    #[must_use]
    pub fn build(self) -> SasDateTime {
        self.date_time
    }
}

impl From<SasDateTimeBuilder> for SasDateTime {
    #[inline]
    fn from(builder: SasDateTimeBuilder) -> Self {
        builder.build()
    }
}

#[cfg(feature = "chrono")]
impl From<DateTime<Local>> for SasDateTime {
    #[inline]
    fn from(value: DateTime<Local>) -> Self {
        Self::from_chrono_date_time(value)
    }
}

/// Formats the `SasDateTime` in the SAS XPORT format: `DDMMMYY:HH:mm:ss`.
///
/// **Warning:** This format is a legacy SAS format that is not Y2K compliant and uses
/// a 2-digit year. It is provided for compatibility with SAS XPORT files and should
/// not be used for general-purpose date/time storage or transmission. Prefer ISO 8601
/// or other standard formats for new applications.
///
/// # Example
///
/// ```
/// use sas_xport::sas::{SasDateTime, SasMonth};
///
/// let dt = SasDateTime::builder()
///     .day(23)
///     .month(SasMonth::September)
///     .year(23)
///     .hour(12)
///     .minute(56)
///     .second(3)
///     .build();
///
/// assert_eq!(dt.to_string(), "23SEP23:12:56:03");
/// ```
impl fmt::Display for SasDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}{}{}:{:02}:{:02}:{:02}",
            self.day,
            self.month.abbreviation(),
            format_args!("{:02}", self.year),
            self.hour,
            self.minute,
            self.second
        )
    }
}

/// Parses a `SasDateTime` from the SAS XPORT format: `DDMMMYY:HH:mm:ss`.
///
/// **Warning:** This format is a legacy SAS format that is not Y2K compliant and uses
/// a 2-digit year. It is provided for compatibility with SAS XPORT files and should
/// not be used for general-purpose date/time storage or transmission. Prefer ISO 8601
/// or other standard formats for new applications.
///
/// # Example
///
/// ```
/// use sas_xport::sas::{SasDateTime, SasMonth};
///
/// let dt: SasDateTime = "23SEP23:12:56:03".parse().unwrap();
///
/// assert_eq!(dt.day(), 23);
/// assert_eq!(dt.month(), SasMonth::September);
/// assert_eq!(dt.year(), 23);
/// assert_eq!(dt.hour(), 12);
/// assert_eq!(dt.minute(), 56);
/// assert_eq!(dt.second(), 3);
/// ```
impl FromStr for SasDateTime {
    type Err = SasError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 16 {
            return Err(SasError::new(
                "Invalid length: expected 16 characters (DDMMMYY:HH:mm:ss)",
            ));
        }

        let day: u8 = s[0..2]
            .parse()
            .map_err(|e| SasError::wrap("Invalid day", e))?;

        let month = SasMonth::from_abbreviation(&s[2..5])
            .ok_or_else(|| SasError::new("Invalid month abbreviation"))?;

        let year: u8 = s[5..7]
            .parse()
            .map_err(|e| SasError::wrap("Invalid year", e))?;

        if s.as_bytes()[7] != b':' {
            return Err(SasError::new("Expected ':' after year"));
        }

        let hour: u8 = s[8..10]
            .parse()
            .map_err(|e| SasError::wrap("Invalid hour", e))?;

        if s.as_bytes()[10] != b':' {
            return Err(SasError::new("Expected ':' after hour"));
        }

        let minute: u8 = s[11..13]
            .parse()
            .map_err(|e| SasError::wrap("Invalid minute", e))?;

        if s.as_bytes()[13] != b':' {
            return Err(SasError::new("Expected ':' after minute"));
        }

        let second: u8 = s[14..16]
            .parse()
            .map_err(|e| SasError::wrap("Invalid second", e))?;

        let result = SasDateTime::builder()
            .year(year)
            .month(month)
            .day(day)
            .hour(hour)
            .minute(minute)
            .second(second)
            .build();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "chrono")]
    use chrono::{Datelike, Local, LocalResult, NaiveDateTime, TimeZone, Timelike};

    use super::*;

    #[test]
    fn new_returns_expected_defaults() {
        let dt = SasDateTime::new();

        assert_eq!(0, dt.year());
        assert_eq!(SasMonth::January, dt.month());
        assert_eq!(1, dt.day());
        assert_eq!(0, dt.hour());
        assert_eq!(0, dt.minute());
        assert_eq!(0, dt.second());
    }

    #[test]
    fn default_equals_new() {
        assert_eq!(SasDateTime::new(), SasDateTime::default());
    }

    #[test]
    fn builder_sets_all_fields() {
        let dt = SasDateTime::builder()
            .year(99)
            .month(SasMonth::December)
            .day(31)
            .hour(23)
            .minute(59)
            .second(58)
            .build();

        assert_eq!(99, dt.year());
        assert_eq!(SasMonth::December, dt.month());
        assert_eq!(31, dt.day());
        assert_eq!(23, dt.hour());
        assert_eq!(59, dt.minute());
        assert_eq!(58, dt.second());
    }

    #[test]
    fn builder_into_datetime_via_from_trait() {
        let builder = SasDateTime::builder()
            .year(25)
            .month(SasMonth::March)
            .day(15);

        let dt: SasDateTime = builder.into();

        assert_eq!(25, dt.year());
        assert_eq!(SasMonth::March, dt.month());
        assert_eq!(15, dt.day());
    }

    #[test]
    fn builder_is_copy_allows_variations() {
        let base = SasDateTime::builder().year(23).month(SasMonth::January);

        let jan1 = base.day(1).build();
        let jan15 = base.day(15).build();

        assert_eq!(1, jan1.day());
        assert_eq!(15, jan15.day());
        assert_eq!(jan1.year(), jan15.year());
        assert_eq!(jan1.month(), jan15.month());
    }

    #[test]
    fn sas_datetime_is_copy() {
        let dt1 = SasDateTime::builder()
            .year(23)
            .month(SasMonth::October)
            .build();
        let dt2 = dt1; // Copy
        let _dt3 = dt1; // Still valid because Copy

        assert_eq!(dt1, dt2);
    }

    #[test]
    #[cfg(feature = "chrono")]
    fn round_trips_chrono() {
        let naive =
            NaiveDateTime::parse_from_str("2023-09-24T10:23:54", "%Y-%m-%dT%H:%M:%S").unwrap();
        let local = Local.from_local_datetime(&naive).unwrap();
        let sas = SasDateTime::from_chrono_date_time(local);
        assert_eq!(23, sas.year()); // Years are only stored as 2-digits
        assert_eq!(SasMonth::September, sas.month());

        let chrono = sas.to_chrono_date_time(2000).single().unwrap();

        assert_eq!(2023, chrono.year());
        assert_eq!(9, chrono.month());
        assert_eq!(24, chrono.day());
        assert_eq!(10, chrono.hour());
        assert_eq!(23, chrono.minute());
        assert_eq!(54, chrono.second());
    }

    #[test]
    #[cfg(feature = "chrono")]
    fn to_chrono_returns_none_for_invalid_date() {
        let dt = SasDateTime::builder()
            .year(23)
            .month(SasMonth::February)
            .day(30) // February 30th doesn't exist
            .build();

        let result = dt.to_chrono_date_time(2000);
        assert!(matches!(result, LocalResult::None));
    }

    #[test]
    #[cfg(feature = "chrono")]
    fn to_chrono_returns_none_for_invalid_time() {
        let dt = SasDateTime::builder()
            .year(23)
            .month(SasMonth::January)
            .day(15)
            .hour(25) // Invalid hour
            .build();

        let result = dt.to_chrono_date_time(2000);
        assert!(matches!(result, LocalResult::None));
    }

    #[test]
    #[cfg(feature = "chrono")]
    fn from_chrono_datetime_trait() {
        let naive =
            NaiveDateTime::parse_from_str("2024-06-15T08:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let local = Local.from_local_datetime(&naive).unwrap();

        let sas: SasDateTime = local.into();

        assert_eq!(24, sas.year());
        assert_eq!(SasMonth::June, sas.month());
        assert_eq!(15, sas.day());
        assert_eq!(8, sas.hour());
        assert_eq!(30, sas.minute());
        assert_eq!(0, sas.second());
    }

    #[test]
    fn display_formats_correctly() {
        let dt = SasDateTime::builder()
            .day(23)
            .month(SasMonth::September)
            .year(23)
            .hour(12)
            .minute(56)
            .second(3)
            .build();

        assert_eq!("23SEP23:12:56:03", dt.to_string());
    }

    #[test]
    fn display_pads_single_digits_with_zeros() {
        let dt = SasDateTime::builder()
            .day(1)
            .month(SasMonth::January)
            .year(5)
            .hour(8)
            .minute(9)
            .second(7)
            .build();

        assert_eq!("01JAN05:08:09:07", dt.to_string());
    }

    #[test]
    fn from_str_parses_valid_datetime() {
        let dt: SasDateTime = "23SEP23:12:56:03".parse().unwrap();

        assert_eq!(23, dt.day());
        assert_eq!(SasMonth::September, dt.month());
        assert_eq!(23, dt.year());
        assert_eq!(12, dt.hour());
        assert_eq!(56, dt.minute());
        assert_eq!(3, dt.second());
    }

    #[test]
    fn from_str_round_trips_with_display() {
        let original = SasDateTime::builder()
            .day(31)
            .month(SasMonth::December)
            .year(99)
            .hour(23)
            .minute(59)
            .second(58)
            .build();

        let formatted = original.to_string();
        let parsed: SasDateTime = formatted.parse().unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn from_str_rejects_wrong_length() {
        let result: Result<SasDateTime, _> = "23SEP23:12:56".parse();
        assert!(result.is_err());
        assert_eq!(
            "Invalid length: expected 16 characters (DDMMMYY:HH:mm:ss)",
            result.unwrap_err().to_string()
        );
    }

    #[test]
    fn from_str_rejects_invalid_month() {
        let result: Result<SasDateTime, _> = "23XXX23:12:56:03".parse();
        assert!(result.is_err());
        assert_eq!(
            "Invalid month abbreviation",
            result.unwrap_err().to_string()
        );
    }

    #[test]
    fn from_str_rejects_missing_colons() {
        let result: Result<SasDateTime, _> = "23SEP23-12:56:03".parse();
        assert!(result.is_err());
        assert_eq!("Expected ':' after year", result.unwrap_err().to_string());
    }

    #[test]
    fn from_str_rejects_non_numeric_day() {
        let result: Result<SasDateTime, _> = "XXSEP23:12:56:03".parse();
        assert!(result.is_err());
        assert_eq!("Invalid day", result.unwrap_err().to_string());
    }
}
