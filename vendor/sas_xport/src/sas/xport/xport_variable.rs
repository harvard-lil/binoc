use crate::sas::{SasJustification, SasVariableType};

fn truncate(value: &str, max_length: usize) -> &str {
    if value.len() <= max_length {
        return value;
    }
    // Truncate at a char boundary to avoid splitting a multibyte character.
    let mut end = max_length;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Represents a variable in a dataset schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XportVariable {
    value_type: SasVariableType,
    hash: u16,
    value_length: u16,
    number: u16,
    short_name: String,
    short_label: String,
    short_format: String,
    format_length: u16,
    format_precision: u16,
    justification: SasJustification,
    short_input_format: String,
    input_format_length: u16,
    input_format_precision: u16,
    position: u32,
    /// The actual byte offset of this variable within a record, computed
    /// as the cumulative sum of preceding variable lengths. Unlike
    /// `position`, this is always derived from the schema and never
    /// overridden by the user.
    record_offset: u32,
    medium_name: String,
    long_name: String,
    long_label: String,
    long_format: String,
    long_input_format: String,
}

impl XportVariable {
    /// Gets the maximum length, in bytes, of a variable name in V5 formats.
    pub const MAX_SHORT_NAME_LENGTH_IN_BYTES: u8 = 8;
    /// Gets the maximum length, in bytes, of a variable name in V8 formats.
    pub const MAX_MEDIUM_NAME_LENGTH_IN_BYTES: u8 = 32;
    /// Gets the maximum length, in bytes, of a variable label in V8 formats.
    pub const MAX_SHORT_LABEL_LENGTH_IN_BYTES: u8 = 40;
    /// Gets the maximum length, in bytes, of a variable format in V5 formats.
    pub const MAX_SHORT_FORMAT_LENGTH_IN_BYTES: u8 = 8;
    /// Gets the maximum length, in bytes, of a variable format in V5 formats.
    pub const MAX_SHORT_INPUT_FORMAT_LENGTH_IN_BYTES: u8 = 8;
    /// Gets the default length, in bytes, of numeric values.
    pub const DEFAULT_NUMERIC_LENGTH: u8 = 8;
    /// Gets the maximum length, in bytes, of a V5 textual value.
    pub const MAX_V5_CHARACTER_LENGTH_IN_BYTES: u8 = 200;
    /// Gets the maximum length, in bytes, of a V8/V9 textual value.
    pub const MAX_V8_CHARACTER_LENGTH_IN_BYTES: u16 = 32_767;

    /// Creates a builder for configuring a variable.
    #[inline]
    #[must_use]
    pub fn builder() -> XportVariableBuilder {
        XportVariableBuilder::new()
    }

    /// Gets the type of value stored in the variable.
    #[inline]
    #[must_use]
    pub const fn value_type(&self) -> SasVariableType {
        self.value_type
    }

    /// Gets a hash of the variable name. This is usually left as zero.
    #[inline]
    #[must_use]
    pub const fn hash(&self) -> u16 {
        self.hash
    }

    /// Gets the length, in bytes, of the variable.
    #[inline]
    #[must_use]
    pub const fn value_length(&self) -> u16 {
        self.value_length
    }

    /// Gets the ordinal position of the variable, 1-based.
    #[inline]
    #[must_use]
    pub const fn number(&self) -> u16 {
        self.number
    }

    /// Gets the name of the variable, truncated to 8 bytes.
    #[inline]
    #[must_use]
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// Gets the label of the variable, truncated to 40 bytes.
    #[inline]
    #[must_use]
    pub fn short_label(&self) -> &str {
        &self.short_label
    }

    /// Gets the format of the variable, truncated to 8 bytes. The format
    /// should not include a leading '.'.
    #[inline]
    #[must_use]
    pub fn short_format(&self) -> &str {
        &self.short_format
    }

    /// Gets the length associated with the format, in digits. If a length is not required,
    /// provide 0.
    #[inline]
    #[must_use]
    pub const fn format_length(&self) -> u16 {
        self.format_length
    }

    /// Gets the precision of the format, in digits. If a precision is not required, provide 0.
    #[inline]
    #[must_use]
    pub const fn format_precision(&self) -> u16 {
        self.format_precision
    }

    /// Gets whether the value is justified to the left or right. This controls
    /// where padding appears.
    #[inline]
    #[must_use]
    pub const fn justification(&self) -> SasJustification {
        self.justification
    }

    /// Gets the input format.
    #[inline]
    #[must_use]
    pub fn short_input_format(&self) -> &str {
        &self.short_input_format
    }

    /// Gets the input format length, in digits. Use zero if no length is required.
    #[inline]
    #[must_use]
    pub const fn input_format_length(&self) -> u16 {
        self.input_format_length
    }

    /// Gets the input format precision, in digits. Use zero if no precision is required.
    #[inline]
    #[must_use]
    pub const fn input_format_precision(&self) -> u16 {
        self.input_format_precision
    }

    /// Gets the offset in bytes of the variable from the beginning of each record.
    /// This should be equal to the previous variable's position, plus its length, in bytes.
    #[inline]
    #[must_use]
    pub const fn position(&self) -> u32 {
        self.position
    }

    /// Gets the actual byte offset of this variable within a record.
    #[inline]
    #[must_use]
    pub(crate) const fn record_offset(&self) -> u32 {
        self.record_offset
    }

    /// Gets the name, truncated to 32-bytes.
    #[inline]
    #[must_use]
    pub fn medium_name(&self) -> &str {
        &self.medium_name
    }

    /// Gets the full name of the variable.
    #[inline]
    #[must_use]
    pub fn long_name(&self) -> &str {
        &self.long_name
    }

    /// Gets the full label of the variable.
    #[inline]
    #[must_use]
    pub fn long_label(&self) -> &str {
        &self.long_label
    }

    /// Gets the full format of the variable.
    #[inline]
    #[must_use]
    pub fn long_format(&self) -> &str {
        &self.long_format
    }

    /// Gets the full input format of the variable.
    #[inline]
    #[must_use]
    pub fn long_input_format(&self) -> &str {
        &self.long_input_format
    }

    /// Gets the full name of the variable by looking for the first non-empty value
    /// among the long name, medium name, and finally the short name.
    #[inline]
    #[must_use]
    pub fn full_name(&self) -> &str {
        if !self.long_name.is_empty() {
            &self.long_name
        } else if !self.medium_name.is_empty() {
            &self.medium_name
        } else {
            &self.short_name
        }
    }

    /// Gets the full label of the variable by looking for the first non-empty value
    /// between the long label and the short label.
    #[inline]
    #[must_use]
    pub fn full_label(&self) -> &str {
        if self.long_label.is_empty() {
            &self.short_label
        } else {
            &self.long_label
        }
    }

    /// Gets the full format of the variable by looking for the first non-empty value
    /// between the long format and the short format.
    #[inline]
    #[must_use]
    pub fn full_format(&self) -> &str {
        if self.long_format.is_empty() {
            &self.short_format
        } else {
            &self.long_format
        }
    }

    /// Gets the full input format of the variable by looking for the first non-empty value
    /// between the long input format and the short input format.
    #[inline]
    #[must_use]
    pub fn full_input_format(&self) -> &str {
        if self.long_input_format.is_empty() {
            &self.short_input_format
        } else {
            &self.long_input_format
        }
    }
}

/// Allows building a `XportVariable`.
#[derive(Clone, Debug)]
pub struct XportVariableBuilder {
    value_type: SasVariableType,
    hash: u16,
    pub(crate) value_length: u16,
    pub(crate) number: Option<u16>,
    short_name: String,
    short_label: String,
    short_format: String,
    format_length: u16,
    format_precision: u16,
    justification: SasJustification,
    short_input_format: String,
    input_format_length: u16,
    input_format_precision: u16,
    pub(crate) position: Option<u32>,
    pub(crate) record_offset: u32,
    medium_name: String,
    long_name: String,
    long_label: String,
    long_format: String,
    long_input_format: String,
}

impl Default for XportVariableBuilder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl XportVariableBuilder {
    fn new() -> Self {
        XportVariableBuilder {
            value_type: SasVariableType::Character,
            hash: 0u16,
            value_length: 0u16,
            number: None,
            short_name: String::new(),
            short_label: String::new(),
            short_format: String::new(),
            format_length: 0u16,
            format_precision: 0u16,
            justification: SasJustification::Left,
            short_input_format: String::new(),
            input_format_length: 0u16,
            input_format_precision: 0u16,
            position: None,
            record_offset: 0,
            medium_name: String::new(),
            long_name: String::new(),
            long_label: String::new(),
            long_format: String::new(),
            long_input_format: String::new(),
        }
    }

    /// Sets the type of value stored in the variable.
    #[inline]
    pub fn value_type(&mut self, value_type: SasVariableType) -> &mut Self {
        self.value_type = value_type;
        self
    }

    /// Sets the hash of the variable name. This should be left 0 unless there is a
    /// compelling reason to do otherwise.
    #[inline]
    pub fn hash(&mut self, hash: u16) -> &mut Self {
        self.hash = hash;
        self
    }

    /// Sets the length, in bytes, of the variable.
    #[inline]
    pub fn value_length(&mut self, value_length: u16) -> &mut Self {
        self.value_length = value_length;
        self
    }

    /// Sets the ordinal position of the variable, 1-based. If not set,
    /// the number will be auto-computed when building an `XportSchema`.
    #[inline]
    pub fn number(&mut self, number: u16) -> &mut Self {
        self.number = Some(number);
        self
    }

    /// Clears the ordinal position so it will be auto-computed when building
    /// an `XportSchema`.
    #[inline]
    pub fn clear_number(&mut self) -> &mut Self {
        self.number = None;
        self
    }

    /// Sets the name of the variable, which will be truncated to 8 bytes.
    #[inline]
    pub fn short_name(&mut self, short_name: impl Into<String>) -> &mut Self {
        self.short_name = short_name.into();
        self
    }

    /// Sets the label of the variable, which will be truncated to 40 bytes.
    #[inline]
    pub fn short_label(&mut self, short_label: impl Into<String>) -> &mut Self {
        self.short_label = short_label.into();
        self
    }

    /// Sets the format of the variable, which will be truncated to 8 bytes. The format
    /// should not include a leading '.'.
    #[inline]
    pub fn short_format(&mut self, short_format: impl Into<String>) -> &mut Self {
        self.short_format = short_format.into();
        self
    }

    /// Sets the length associated with the format, in digits. If a length is not required,
    /// provide 0 (the default).
    #[inline]
    pub fn format_length(&mut self, format_length: u16) -> &mut Self {
        self.format_length = format_length;
        self
    }

    /// Sets the precision of the format, in digits. If the precision is not required, provide 0
    /// (the default).
    #[inline]
    pub fn format_precision(&mut self, format_precision: u16) -> &mut Self {
        self.format_precision = format_precision;
        self
    }

    /// Sets whether the value is justified to the left or right. This controls where
    /// padding appears.
    #[inline]
    pub fn justification(&mut self, justification: SasJustification) -> &mut Self {
        self.justification = justification;
        self
    }

    /// Sets the input format.
    #[inline]
    pub fn short_input_format(&mut self, short_input_format: impl Into<String>) -> &mut Self {
        self.short_input_format = short_input_format.into();
        self
    }

    /// Sets the input format length, in digits. Use zero if no length is required (the default).
    #[inline]
    pub fn input_format_length(&mut self, input_format_length: u16) -> &mut Self {
        self.input_format_length = input_format_length;
        self
    }

    /// Sets the input format precision, in digits. Use zero if no precision is required
    /// (the default).
    #[inline]
    pub fn input_format_precision(&mut self, input_format_precision: u16) -> &mut Self {
        self.input_format_precision = input_format_precision;
        self
    }

    /// Sets the offset, in bytes, of the variable from the beginning of each record.
    /// If not set, the position will be auto-computed when building an `XportSchema`.
    #[inline]
    pub fn position(&mut self, position: u32) -> &mut Self {
        self.position = Some(position);
        self
    }

    /// Clears the byte offset so it will be auto-computed when building
    /// an `XportSchema`.
    #[inline]
    pub fn clear_position(&mut self) -> &mut Self {
        self.position = None;
        self
    }

    /// Sets the name, which will be truncated to 32 bytes.
    #[inline]
    pub fn medium_name(&mut self, medium_name: impl Into<String>) -> &mut Self {
        self.medium_name = medium_name.into();
        self
    }

    /// Sets the full name of the variable.
    #[inline]
    pub fn long_name(&mut self, long_name: impl Into<String>) -> &mut Self {
        self.long_name = long_name.into();
        self
    }

    /// Sets the full label of the variable.
    #[inline]
    pub fn long_label(&mut self, long_label: impl Into<String>) -> &mut Self {
        self.long_label = long_label.into();
        self
    }

    /// Sets the full format of the variable.
    #[inline]
    pub fn long_format(&mut self, long_format: impl Into<String>) -> &mut Self {
        self.long_format = long_format.into();
        self
    }

    /// Sets the full input format of the variable.
    #[inline]
    pub fn long_input_format(&mut self, long_input_format: impl Into<String>) -> &mut Self {
        self.long_input_format = long_input_format.into();
        self
    }

    /// Sets the short, medium, and long name from a single value. The long name is
    /// stored as-is, while the medium and short names are truncated to 32 and 8 bytes
    /// respectively.
    #[inline]
    pub fn full_name(&mut self, name: impl Into<String>) -> &mut Self {
        let name = name.into();
        let short_name = truncate(
            &name,
            XportVariable::MAX_SHORT_NAME_LENGTH_IN_BYTES as usize,
        );
        self.short_name = short_name.into();
        let medium_name = truncate(
            &name,
            XportVariable::MAX_MEDIUM_NAME_LENGTH_IN_BYTES as usize,
        );
        self.medium_name = medium_name.into();
        self.long_name = name;
        self
    }

    /// Builds a variable from the current configuration.
    #[inline]
    #[must_use]
    pub fn build(&self) -> XportVariable {
        self.clone().build_into()
    }

    /// Builds a variable from the current configuration, consuming the builder.
    #[inline]
    #[must_use]
    pub fn build_into(self) -> XportVariable {
        XportVariable {
            value_type: self.value_type,
            hash: self.hash,
            value_length: self.value_length,
            number: self.number.unwrap_or(0),
            short_name: self.short_name,
            short_label: self.short_label,
            short_format: self.short_format,
            format_length: self.format_length,
            format_precision: self.format_precision,
            justification: self.justification,
            short_input_format: self.short_input_format,
            input_format_length: self.input_format_length,
            input_format_precision: self.input_format_precision,
            position: self.position.unwrap_or(0),
            record_offset: self.record_offset,
            medium_name: self.medium_name,
            long_name: self.long_name,
            long_label: self.long_label,
            long_format: self.long_format,
            long_input_format: self.long_input_format,
        }
    }
}

impl From<XportVariableBuilder> for XportVariable {
    fn from(builder: XportVariableBuilder) -> Self {
        builder.build_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getters_work_char_build() {
        let variable = XportVariable::builder()
            .value_type(SasVariableType::Character)
            .hash(0)
            .value_length(8)
            .number(1)
            .short_name("STUDYID")
            .short_label("Study Identifier")
            .short_format("")
            .format_length(0)
            .format_precision(0)
            .justification(SasJustification::Left)
            .short_input_format("")
            .input_format_length(0)
            .input_format_precision(0)
            .position(0)
            .medium_name("STUDYID")
            .long_name("STUDYID")
            .long_label("Study Identifier")
            .long_format("")
            .long_input_format("")
            .build();
        assert_eq!(SasVariableType::Character, variable.value_type());
        assert_eq!(0, variable.hash());
        assert_eq!(8, variable.value_length());
        assert_eq!(1, variable.number());
        assert_eq!("STUDYID", variable.short_name());
        assert_eq!("Study Identifier", variable.short_label());
        assert_eq!("", variable.short_format());
        assert_eq!(0, variable.format_length());
        assert_eq!(0, variable.format_precision());
        assert_eq!(SasJustification::Left, variable.justification());
        assert_eq!("", variable.short_input_format());
        assert_eq!(0, variable.input_format_length());
        assert_eq!(0, variable.input_format_precision());
        assert_eq!(0, variable.position());
        assert_eq!("STUDYID", variable.medium_name());
        assert_eq!("STUDYID", variable.long_name());
        assert_eq!("Study Identifier", variable.long_label());
        assert_eq!("", variable.long_format());
        assert_eq!("", variable.long_input_format());
    }

    #[test]
    fn full_name_prefers_long_name() {
        let variable = XportVariable::builder()
            .short_name("SHORT")
            .medium_name("MEDIUM_NAME")
            .long_name("long_name_value")
            .build();
        assert_eq!("long_name_value", variable.full_name());
    }

    #[test]
    fn full_name_falls_back_to_medium_name() {
        let variable = XportVariable::builder()
            .short_name("SHORT")
            .medium_name("MEDIUM_NAME")
            .build();
        assert_eq!("MEDIUM_NAME", variable.full_name());
    }

    #[test]
    fn full_name_falls_back_to_short_name() {
        let variable = XportVariable::builder().short_name("SHORT").build();
        assert_eq!("SHORT", variable.full_name());
    }

    #[test]
    fn full_label_prefers_long_label() {
        let variable = XportVariable::builder()
            .short_label("Short Label")
            .long_label("Long Label Value")
            .build();
        assert_eq!("Long Label Value", variable.full_label());
    }

    #[test]
    fn full_label_falls_back_to_short_label() {
        let variable = XportVariable::builder().short_label("Short Label").build();
        assert_eq!("Short Label", variable.full_label());
    }

    #[test]
    fn full_format_prefers_long_format() {
        let variable = XportVariable::builder()
            .short_format("DOLLAR")
            .long_format("DOLLAR32.2")
            .build();
        assert_eq!("DOLLAR32.2", variable.full_format());
    }

    #[test]
    fn full_format_falls_back_to_short_format() {
        let variable = XportVariable::builder().short_format("DOLLAR").build();
        assert_eq!("DOLLAR", variable.full_format());
    }

    #[test]
    fn full_input_format_prefers_long_input_format() {
        let variable = XportVariable::builder()
            .short_input_format("BEST")
            .long_input_format("BEST32.")
            .build();
        assert_eq!("BEST32.", variable.full_input_format());
    }

    #[test]
    fn full_input_format_falls_back_to_short_input_format() {
        let variable = XportVariable::builder().short_input_format("BEST").build();
        assert_eq!("BEST", variable.full_input_format());
    }

    #[test]
    fn set_full_name_short_name_fits_all_tiers() {
        let variable = XportVariable::builder().full_name("STUDYID").build();
        assert_eq!("STUDYID", variable.short_name());
        assert_eq!("STUDYID", variable.medium_name());
        assert_eq!("STUDYID", variable.long_name());
        assert_eq!("STUDYID", variable.full_name());
    }

    #[test]
    fn set_full_name_truncates_short_and_medium() {
        // 40 chars — exceeds both short (8) and medium (32) limits
        let name = "a_variable_name_that_is_forty_characters";
        assert_eq!(40, name.len());
        let variable = XportVariable::builder().full_name(name).build();
        assert_eq!("a_variab", variable.short_name());
        assert_eq!("a_variable_name_that_is_forty_ch", variable.medium_name());
        assert_eq!(name, variable.long_name());
        assert_eq!(name, variable.full_name());
    }

    #[test]
    fn set_full_name_truncates_short_only() {
        // 20 chars — exceeds short (8) but fits medium (32)
        let name = "a_medium_length_name";
        assert_eq!(20, name.len());
        let variable = XportVariable::builder().full_name(name).build();
        assert_eq!("a_medium", variable.short_name());
        assert_eq!(name, variable.medium_name());
        assert_eq!(name, variable.long_name());
    }

    #[test]
    fn set_full_name_truncates_at_char_boundary() {
        // 'é' is 2 bytes — placing it at byte 8 means truncation can't cut at 8
        let name = "1234567é9";
        assert_eq!(10, name.len());
        let variable = XportVariable::builder().full_name(name).build();
        assert_eq!("1234567", variable.short_name());
        assert_eq!(7, variable.short_name().len());
    }

    #[test]
    fn getters_work_number_with_format() {
        let variable = XportVariable::builder()
            .value_type(SasVariableType::Numeric)
            .hash(0)
            .value_length(8)
            .number(1)
            .short_name("LBSTRESN")
            .short_label("Standard Result")
            .short_format("DOLLAR")
            .format_length(10)
            .format_precision(2)
            .justification(SasJustification::Left)
            .short_input_format("")
            .input_format_length(0)
            .input_format_precision(0)
            .position(0)
            .medium_name("LBSTRESN")
            .long_name("LBSTRESN")
            .long_label("Standard Result")
            .long_format("DOLLAR")
            .long_input_format("")
            .build();
        assert_eq!(SasVariableType::Numeric, variable.value_type());
        assert_eq!(0, variable.hash());
        assert_eq!(8, variable.value_length());
        assert_eq!(1, variable.number());
        assert_eq!("LBSTRESN", variable.short_name());
        assert_eq!("Standard Result", variable.short_label());
        assert_eq!("DOLLAR", variable.short_format());
        assert_eq!(10, variable.format_length());
        assert_eq!(2, variable.format_precision());
        assert_eq!(SasJustification::Left, variable.justification());
        assert_eq!("", variable.short_input_format());
        assert_eq!(0, variable.input_format_length());
        assert_eq!(0, variable.input_format_precision());
        assert_eq!(0, variable.position());
        assert_eq!("LBSTRESN", variable.medium_name());
        assert_eq!("LBSTRESN", variable.long_name());
        assert_eq!("Standard Result", variable.long_label());
        assert_eq!("DOLLAR", variable.long_format());
        assert_eq!("", variable.long_input_format());
    }

    #[test]
    fn truncate_returns_full_string_when_within_limit() {
        assert_eq!("hello", truncate("hello", 5));
        assert_eq!("hello", truncate("hello", 10));
    }

    #[test]
    fn truncate_returns_full_string_when_empty() {
        assert_eq!("", truncate("", 8));
    }

    #[test]
    fn truncate_cuts_at_limit() {
        assert_eq!("hel", truncate("hello", 3));
    }

    #[test]
    fn truncate_respects_char_boundary_two_byte() {
        // 'é' is 2 bytes (0xC3 0xA9)
        assert_eq!("ab", truncate("abé", 3));
        assert_eq!("abé", truncate("abé", 4));
    }

    #[test]
    fn truncate_respects_char_boundary_three_byte() {
        // '€' is 3 bytes (0xE2 0x82 0xAC)
        assert_eq!("a", truncate("a€b", 2));
        assert_eq!("a", truncate("a€b", 3));
        assert_eq!("a€", truncate("a€b", 4));
    }

    #[test]
    fn truncate_respects_char_boundary_four_byte() {
        // '𝄞' (musical symbol) is 4 bytes
        assert_eq!("a", truncate("a𝄞b", 2));
        assert_eq!("a", truncate("a𝄞b", 4));
        assert_eq!("a𝄞", truncate("a𝄞b", 5));
    }

    #[test]
    fn truncate_at_zero_returns_empty() {
        assert_eq!("", truncate("hello", 0));
    }
}
