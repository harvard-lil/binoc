use std::collections::HashMap;

use super::{XportDatasetVersion, XportVariable, XportVariableBuilder};
use crate::sas::xport::xport_error::XportErrorKind;
use crate::sas::xport::{Result, XportError};
use crate::sas::{SasDateTime, SasVariableType};

/// Represents the schema of a SAS® transport dataset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XportSchema {
    xport_dataset_version: XportDatasetVersion,
    variable_descriptor_length: u16,
    format: String,
    dataset_name: String,
    sas_data: String,
    version: String,
    operating_system: String,
    created: SasDateTime,
    modified: SasDateTime,
    dataset_label: String,
    dataset_type: String,
    variables: Vec<XportVariable>,
    variable_indexes: HashMap<String, usize>,
    record_count: Option<u64>,
}

impl XportSchema {
    /// The default variable descriptor length. It is 140 except on the VAX/VMS operating system,
    /// where it should be 136 instead.
    pub const DEFAULT_VARIABLE_DESCRIPTOR_LENGTH: u16 = 140; // Max of 4 decimal digits
    /// The default format of "SAS".
    pub const DEFAULT_FORMAT: &'static str = "SAS";
    /// The default sas data value of "SASDATA".
    pub const DEFAULT_SAS_DATA: &'static str = "SASDATA";

    /// Creates a builder for constructing an `XportSchemaBuilder`.
    #[inline]
    #[must_use]
    pub fn builder() -> XportSchemaBuilder {
        XportSchemaBuilder::new(SasDateTime::new())
    }

    /// Gets the transport format version for this dataset.
    #[inline]
    #[must_use]
    pub fn xport_dataset_version(&self) -> XportDatasetVersion {
        self.xport_dataset_version
    }

    /// Gets the variable descriptor length, in bytes. This should be at most
    /// a 4-digit number.
    #[inline]
    #[must_use]
    pub fn variable_descriptor_length(&self) -> u16 {
        self.variable_descriptor_length
    }

    /// Gets the format, which is usually "SAS".
    #[inline]
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Gets the name of the dataset, usually truncated to 8 bytes.
    #[inline]
    #[must_use]
    pub fn dataset_name(&self) -> &str {
        &self.dataset_name
    }

    /// Gets the SAS data value, which is usually "SASDATA".
    #[inline]
    #[must_use]
    pub fn sas_data(&self) -> &str {
        &self.sas_data
    }

    /// Gets the version string of the SAS® environment where the dataset was created.
    #[inline]
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Gets the operating system of the SAS® environment where the dataset was created.
    #[inline]
    #[must_use]
    pub fn operating_system(&self) -> &str {
        &self.operating_system
    }

    /// Gets the creation date of the dataset.
    #[inline]
    #[must_use]
    pub fn created(&self) -> SasDateTime {
        self.created
    }

    /// Gets the last modified date of the dataset.
    #[inline]
    #[must_use]
    pub fn modified(&self) -> SasDateTime {
        self.modified
    }

    /// Gets the label of the dataset, truncated to 40 bytes.
    #[inline]
    #[must_use]
    pub fn dataset_label(&self) -> &str {
        &self.dataset_label
    }

    /// Gets the type of the dataset. Often left blank.
    #[inline]
    #[must_use]
    pub fn dataset_type(&self) -> &str {
        &self.dataset_type
    }

    /// Gets the variables in the dataset.
    #[inline]
    #[must_use]
    pub fn variables(&self) -> &[XportVariable] {
        &self.variables
    }

    /// Gets the variable at the given index, or `None` if the index is invalid.
    #[inline]
    #[must_use]
    pub fn variable_at(&self, index: usize) -> Option<&XportVariable> {
        self.variables.get(index)
    }

    /// Gets the variable with the given name. The name is the full name of the variable,
    /// not one of its truncated values.
    #[inline]
    #[must_use]
    pub fn variable_named(&self, name: &str) -> Option<&XportVariable> {
        self.variable_indexes
            .get(name)
            .and_then(|i| self.variables.get(*i))
    }

    /// Gets the index of the variable with the given name, or `None` if no such variable exists.
    #[inline]
    #[must_use]
    pub fn variable_ordinal(&self, name: &str) -> Option<usize> {
        self.variable_indexes.get(name).copied()
    }

    /// Gets the number of records in the dataset, or `None` if this information
    /// is not available.
    #[inline]
    #[must_use]
    pub fn record_count(&self) -> Option<u64> {
        self.record_count
    }

    #[must_use]
    pub(crate) fn compute_record_length(&self) -> usize {
        self.variables
            .iter()
            .map(|v| v.value_length() as usize)
            .sum()
    }
}

/// Allows configuring and creating an `XportSchema`.
#[derive(Debug, Clone)]
pub struct XportSchemaBuilder {
    xport_dataset_version: XportDatasetVersion,
    variable_descriptor_length: u16,
    format: String,
    dataset_name: String,
    sas_data: String,
    version: String,
    operating_system: String,
    created: SasDateTime,
    modified: SasDateTime,
    dataset_label: String,
    dataset_type: String,
    variables: Vec<XportVariableBuilder>,
    record_count: Option<u64>,
}

impl Default for XportSchemaBuilder {
    #[inline]
    fn default() -> Self {
        Self::new(SasDateTime::new())
    }
}

impl XportSchemaBuilder {
    #[must_use]
    fn new(timestamp: SasDateTime) -> Self {
        // In the default schema we leave the variable map empty. We track new variables in a Vec
        // within the builder and then construct a separate variable map when building.
        Self {
            xport_dataset_version: XportDatasetVersion::V5,
            variable_descriptor_length: XportSchema::DEFAULT_VARIABLE_DESCRIPTOR_LENGTH,
            format: String::from(XportSchema::DEFAULT_FORMAT),
            dataset_name: String::new(),
            sas_data: String::from(XportSchema::DEFAULT_SAS_DATA),
            version: String::new(),
            operating_system: String::new(),
            created: timestamp,
            modified: timestamp,
            dataset_label: String::new(),
            dataset_type: String::new(),
            variables: Vec::new(),
            record_count: None,
        }
    }

    /// Sets the transport format version for this dataset.
    #[inline]
    pub fn xport_dataset_version(
        &mut self,
        xport_dataset_version: XportDatasetVersion,
    ) -> &mut Self {
        self.xport_dataset_version = xport_dataset_version;
        self
    }

    /// Sets the variable descriptor length, in bytes. This should be at most a 4-digit number.
    #[inline]
    pub fn variable_descriptor_length(&mut self, variable_descriptor_length: u16) -> &mut Self {
        self.variable_descriptor_length = variable_descriptor_length;
        self
    }

    /// Sets the format, which is usually "SAS".
    #[inline]
    pub fn format(&mut self, format: impl Into<String>) -> &mut Self {
        self.format = format.into();
        self
    }

    /// Sets the name of the dataset, which will be truncated to 8 bytes, at most.
    #[inline]
    pub fn dataset_name(&mut self, dataset_name: impl Into<String>) -> &mut Self {
        self.dataset_name = dataset_name.into();
        self
    }

    /// Sets the SAS data value, which is usually "SASDATA".
    #[inline]
    pub fn sas_data(&mut self, sas_data: impl Into<String>) -> &mut Self {
        self.sas_data = sas_data.into();
        self
    }

    /// Sets the version string of the SAS® environment where the dataset was created.
    #[inline]
    pub fn version(&mut self, version: impl Into<String>) -> &mut Self {
        self.version = version.into();
        self
    }

    /// Sets the operating system of the SAS® environment where the dataset was created.
    #[inline]
    pub fn operating_system(&mut self, operating_system: impl Into<String>) -> &mut Self {
        self.operating_system = operating_system.into();
        self
    }

    /// Sets the creation date of the dataset.
    #[inline]
    pub fn created(&mut self, created: SasDateTime) -> &mut Self {
        self.created = created;
        self
    }

    /// Sets the last modified date of the dataset.
    #[inline]
    pub fn modified(&mut self, modified: SasDateTime) -> &mut Self {
        self.modified = modified;
        self
    }

    /// Sets the label of the dataset, which will be truncated to 40 bytes, at most.
    #[inline]
    pub fn dataset_label(&mut self, dataset_label: impl Into<String>) -> &mut Self {
        self.dataset_label = dataset_label.into();
        self
    }

    /// Sets the type of the dataset. Often left blank.
    #[inline]
    pub fn dataset_type(&mut self, dataset_type: impl Into<String>) -> &mut Self {
        self.dataset_type = dataset_type.into();
        self
    }

    /// Adds the variable builder to the schema. The variable's `number` and
    /// `position` will be auto-computed during building if not explicitly set.
    #[inline]
    pub fn add_variable(&mut self, variable: XportVariableBuilder) -> &mut Self {
        self.variables.push(variable);
        self
    }

    /// Adds the variable builders to the schema.
    #[inline]
    pub fn add_variables(
        &mut self,
        iterator: impl IntoIterator<Item = XportVariableBuilder>,
    ) -> &mut Self {
        self.variables.extend(iterator);
        self
    }

    /// Sets the number of records in the dataset. Use `None` to indicate the record count
    /// is unknown.
    #[inline]
    pub fn record_count(&mut self, record_count: Option<u64>) -> &mut Self {
        self.record_count = record_count;
        self
    }

    /// Attempts to build an `XportSchema` from the current configuration.
    ///
    /// # Errors
    /// An error will occur if:
    /// * Multiple variables are given the same name
    pub fn try_build(&self) -> Result<XportSchema> {
        self.clone().try_build_into()
    }

    /// Attempts to build an `XportSchema` from the current configuration, consuming
    /// the current builder.
    ///
    /// # Errors
    /// An error is returned if:
    /// * Multiple variables are given the same name
    /// * A numeric variable has a `value_length` greater than 8
    /// * A character variable exceeds the version-specific length limit
    ///   (200 bytes for V5, 32,767 bytes for V8/V9)
    pub fn try_build_into(self) -> Result<XportSchema> {
        let mut variables = Vec::with_capacity(self.variables.len());
        let mut variable_indexes = HashMap::with_capacity(self.variables.len());
        let mut position_accumulator: u32 = 0;
        for (index, mut builder) in self.variables.into_iter().enumerate() {
            if builder.number.is_none() {
                let number = u16::try_from(index + 1).map_err(|e| {
                    XportError::of_kind(
                        XportErrorKind::Overflow,
                        "Variable count exceeds u16 range",
                    )
                    .with_source(e)
                })?;
                builder.number(number);
            }
            let position = builder.position.unwrap_or(position_accumulator);
            builder.position(position);
            builder.record_offset = position_accumulator;
            position_accumulator = position_accumulator
                .checked_add(u32::from(builder.value_length))
                .ok_or_else(|| {
                    XportError::of_kind(
                        XportErrorKind::Overflow,
                        "Variable position exceeds u32 range",
                    )
                })?;
            let variable = builder.build_into();
            Self::check_value_length(self.xport_dataset_version, &variable)?;
            let name = variable.full_name();
            if variable_indexes.insert(name.to_string(), index).is_some() {
                return Err(XportError::of_kind(
                    XportErrorKind::Validation,
                    "Encountered a duplicate variable name.",
                ));
            }
            variables.push(variable);
        }
        Ok(XportSchema {
            xport_dataset_version: self.xport_dataset_version,
            variable_descriptor_length: self.variable_descriptor_length,
            format: self.format,
            dataset_name: self.dataset_name,
            sas_data: self.sas_data,
            version: self.version,
            operating_system: self.operating_system,
            created: self.created,
            modified: self.modified,
            dataset_label: self.dataset_label,
            dataset_type: self.dataset_type,
            variables,
            variable_indexes,
            record_count: self.record_count,
        })
    }

    fn check_value_length(
        xport_dataset_version: XportDatasetVersion,
        variable: &XportVariable,
    ) -> Result<()> {
        match variable.value_type() {
            SasVariableType::Numeric => {
                let max = u16::from(XportVariable::DEFAULT_NUMERIC_LENGTH);
                if variable.value_length() > max {
                    return Err(XportError::of_kind(
                        XportErrorKind::Validation,
                        format!(
                            "Numeric variable '{}' has value_length {} which exceeds the maximum of {}",
                            variable.full_name(),
                            variable.value_length(),
                            max,
                        ),
                    ));
                }
            }
            SasVariableType::Character => {
                let max_character_length = match xport_dataset_version {
                    XportDatasetVersion::V5 => {
                        u16::from(XportVariable::MAX_V5_CHARACTER_LENGTH_IN_BYTES)
                    }
                    XportDatasetVersion::V8 | XportDatasetVersion::V9 => {
                        XportVariable::MAX_V8_CHARACTER_LENGTH_IN_BYTES
                    }
                };
                if variable.value_length() > max_character_length {
                    return Err(XportError::of_kind(
                        XportErrorKind::Validation,
                        format!(
                            "Character variable '{}' has value_length {} which exceeds the {:?} maximum of {}",
                            variable.full_name(),
                            variable.value_length(),
                            xport_dataset_version,
                            max_character_length,
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl TryFrom<XportSchemaBuilder> for XportSchema {
    type Error = XportError;

    #[inline]
    fn try_from(builder: XportSchemaBuilder) -> Result<Self> {
        builder.try_build_into()
    }
}

#[cfg(test)]
#[cfg(feature = "chrono")]
mod chrono_tests {
    use chrono::Days;

    use crate::sas::{sas_date_time::SasDateTime, xport::xport_variable::XportVariable};

    use super::*;

    #[test]
    fn getters_work_builder_with_chained_setters_clones() {
        let created = SasDateTime::now();
        let modified: SasDateTime = created
            .to_chrono_date_time(2000)
            .unwrap()
            .checked_add_days(Days::new(1))
            .unwrap()
            .into();
        let variable = XportVariable::builder().short_name("STUDYID").clone();
        let schema = XportSchema::builder()
            .xport_dataset_version(XportDatasetVersion::V9)
            .format(XportSchema::DEFAULT_FORMAT)
            .dataset_name("AE")
            .sas_data(XportSchema::DEFAULT_SAS_DATA)
            .version("1.0")
            .operating_system("WinXP")
            .created(created)
            .modified(modified)
            .dataset_label("Adverse Events")
            .dataset_type("Data")
            .record_count(Some(100))
            .add_variable(variable)
            .try_build()
            .unwrap();

        assert_eq!(
            XportSchema::DEFAULT_VARIABLE_DESCRIPTOR_LENGTH,
            schema.variable_descriptor_length()
        );
        assert_eq!(XportDatasetVersion::V9, schema.xport_dataset_version());
        assert_eq!(XportSchema::DEFAULT_FORMAT, schema.format());
        assert_eq!("AE", schema.dataset_name());
        assert_eq!(XportSchema::DEFAULT_SAS_DATA, schema.sas_data());
        assert_eq!("1.0", schema.version());
        assert_eq!("WinXP", schema.operating_system());
        assert_eq!(created, schema.created());
        assert_eq!(modified, schema.modified());
        assert_eq!("Adverse Events", schema.dataset_label());
        assert_eq!("Data", schema.dataset_type());
        assert_eq!(Some(100), schema.record_count());

        assert!(schema.variable_at(0).is_some());
        assert!(schema.variable_named("STUDYID").is_some());
        assert_eq!(1, schema.variables().len());
    }
}

#[cfg(test)]
mod tests {
    use crate::sas::xport::xport_variable::{XportVariable, XportVariableBuilder};

    use super::*;

    fn build_variable(name: &str, value_length: u16) -> XportVariableBuilder {
        XportVariable::builder()
            .short_name(name)
            .value_length(value_length)
            .clone()
    }

    #[test]
    fn variable_ordinal_returns_index() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(build_variable("B", 8))
            .try_build()
            .unwrap();
        assert_eq!(Some(0), schema.variable_ordinal("A"));
        assert_eq!(Some(1), schema.variable_ordinal("B"));
    }

    #[test]
    fn variable_ordinal_returns_none_for_missing() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .try_build()
            .unwrap();
        assert_eq!(None, schema.variable_ordinal("MISSING"));
    }

    #[test]
    fn variable_at_returns_none_for_out_of_bounds() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .try_build()
            .unwrap();
        assert!(schema.variable_at(0).is_some());
        assert!(schema.variable_at(1).is_none());
    }

    #[test]
    fn variable_named_returns_none_for_missing() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .try_build()
            .unwrap();
        assert!(schema.variable_named("MISSING").is_none());
    }

    #[test]
    fn try_build_fails_on_duplicate_variable_names() {
        let result = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(build_variable("A", 16))
            .try_build();
        assert!(result.is_err());
    }

    #[test]
    fn compute_record_length_sums_value_lengths() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(build_variable("B", 20))
            .add_variable(build_variable("C", 4))
            .try_build()
            .unwrap();
        assert_eq!(32, schema.compute_record_length());
    }

    #[test]
    fn compute_record_length_is_zero_with_no_variables() {
        let schema = XportSchema::builder().try_build().unwrap();
        assert_eq!(0, schema.compute_record_length());
    }

    #[test]
    fn add_variables_adds_multiple() {
        let vars: Vec<XportVariableBuilder> = vec![
            build_variable("A", 8),
            build_variable("B", 8),
            build_variable("C", 8),
        ];
        let schema = XportSchema::builder()
            .add_variables(vars)
            .try_build()
            .unwrap();
        assert_eq!(3, schema.variables().len());
        assert_eq!(Some(0), schema.variable_ordinal("A"));
        assert_eq!(Some(2), schema.variable_ordinal("C"));
    }

    #[test]
    fn defaults_without_chrono() {
        let schema = XportSchema::builder().try_build().unwrap();
        assert_eq!(
            XportSchema::DEFAULT_VARIABLE_DESCRIPTOR_LENGTH,
            schema.variable_descriptor_length()
        );
        assert_eq!(XportSchema::DEFAULT_FORMAT, schema.format());
        assert_eq!(XportSchema::DEFAULT_SAS_DATA, schema.sas_data());
        assert_eq!("", schema.dataset_name());
        assert_eq!("", schema.dataset_label());
        assert_eq!("", schema.dataset_type());
        assert_eq!("", schema.version());
        assert_eq!("", schema.operating_system());
        assert_eq!(None, schema.record_count());
        assert_eq!(0, schema.variables().len());
    }

    #[test]
    fn auto_computes_number_from_insertion_order() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(build_variable("B", 8))
            .add_variable(build_variable("C", 8))
            .try_build()
            .unwrap();
        assert_eq!(1, schema.variables()[0].number());
        assert_eq!(2, schema.variables()[1].number());
        assert_eq!(3, schema.variables()[2].number());
    }

    #[test]
    fn preserves_explicit_number() {
        let mut var_b = build_variable("B", 8);
        var_b.number(99);
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(var_b)
            .add_variable(build_variable("C", 8))
            .try_build()
            .unwrap();
        assert_eq!(1, schema.variables()[0].number());
        assert_eq!(99, schema.variables()[1].number());
        assert_eq!(3, schema.variables()[2].number());
    }

    #[test]
    fn auto_computes_position_from_cumulative_lengths() {
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(build_variable("B", 20))
            .add_variable(build_variable("C", 4))
            .try_build()
            .unwrap();
        assert_eq!(0, schema.variables()[0].position());
        assert_eq!(8, schema.variables()[1].position());
        assert_eq!(28, schema.variables()[2].position());
    }

    #[test]
    fn preserves_explicit_position_without_shifting_accumulator() {
        let mut var_b = build_variable("B", 20);
        var_b.position(100);
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 8))
            .add_variable(var_b)
            .add_variable(build_variable("C", 4))
            .try_build()
            .unwrap();
        assert_eq!(0, schema.variables()[0].position());
        assert_eq!(100, schema.variables()[1].position());
        // Accumulator advances by value_length (8 + 20 = 28), not by explicit position
        assert_eq!(28, schema.variables()[2].position());
    }

    #[test]
    fn cleared_number_gets_auto_computed() {
        let mut var = build_variable("A", 8);
        var.number(50);
        var.clear_number();
        let schema = XportSchema::builder()
            .add_variable(var)
            .try_build()
            .unwrap();
        assert_eq!(1, schema.variables()[0].number());
    }

    #[test]
    fn cleared_position_gets_auto_computed() {
        let mut var = build_variable("B", 8);
        var.position(100);
        var.clear_position();
        let schema = XportSchema::builder()
            .add_variable(build_variable("A", 4))
            .add_variable(var)
            .try_build()
            .unwrap();
        assert_eq!(4, schema.variables()[1].position());
    }

    fn build_numeric_variable(name: &str, value_length: u16) -> XportVariableBuilder {
        let mut builder = XportVariable::builder();
        builder
            .short_name(name)
            .value_type(SasVariableType::Numeric)
            .value_length(value_length);
        builder
    }

    fn build_character_variable(name: &str, value_length: u16) -> XportVariableBuilder {
        let mut builder = XportVariable::builder();
        builder
            .short_name(name)
            .value_type(SasVariableType::Character)
            .value_length(value_length);
        builder
    }

    #[test]
    fn numeric_value_length_below_max_succeeds() {
        for length in 3..u16::from(XportVariable::DEFAULT_NUMERIC_LENGTH) {
            let result = XportSchema::builder()
                .add_variable(build_numeric_variable("A", length))
                .try_build();
            assert!(result.is_ok(), "value_length {length} should be valid");
        }
    }

    #[test]
    fn numeric_value_length_at_max_succeeds() {
        let result = XportSchema::builder()
            .add_variable(build_numeric_variable(
                "A",
                u16::from(XportVariable::DEFAULT_NUMERIC_LENGTH),
            ))
            .try_build();
        assert!(result.is_ok());
    }

    #[test]
    fn numeric_value_length_exceeding_max_fails() {
        let result = XportSchema::builder()
            .add_variable(build_numeric_variable(
                "A",
                u16::from(XportVariable::DEFAULT_NUMERIC_LENGTH) + 1,
            ))
            .try_build();
        assert!(result.is_err());
    }

    #[test]
    fn v5_character_at_max_length_succeeds() {
        let result = XportSchema::builder()
            .add_variable(build_character_variable(
                "A",
                u16::from(XportVariable::MAX_V5_CHARACTER_LENGTH_IN_BYTES),
            ))
            .try_build();
        assert!(result.is_ok());
    }

    #[test]
    fn v5_character_exceeding_max_length_fails() {
        let result = XportSchema::builder()
            .add_variable(build_character_variable(
                "A",
                u16::from(XportVariable::MAX_V5_CHARACTER_LENGTH_IN_BYTES) + 1,
            ))
            .try_build();
        assert!(result.is_err());
    }

    #[test]
    fn v8_character_exceeding_v5_max_succeeds() {
        let result = XportSchema::builder()
            .xport_dataset_version(XportDatasetVersion::V8)
            .add_variable(build_character_variable(
                "A",
                u16::from(XportVariable::MAX_V5_CHARACTER_LENGTH_IN_BYTES) + 1,
            ))
            .try_build();
        assert!(result.is_ok());
    }

    #[test]
    fn v8_character_at_max_length_succeeds() {
        let result = XportSchema::builder()
            .xport_dataset_version(XportDatasetVersion::V8)
            .add_variable(build_character_variable(
                "A",
                XportVariable::MAX_V8_CHARACTER_LENGTH_IN_BYTES,
            ))
            .try_build();
        assert!(result.is_ok());
    }

    #[test]
    fn v8_character_exceeding_max_length_fails() {
        let result = XportSchema::builder()
            .xport_dataset_version(XportDatasetVersion::V8)
            .add_variable(build_character_variable(
                "A",
                XportVariable::MAX_V8_CHARACTER_LENGTH_IN_BYTES + 1,
            ))
            .try_build();
        assert!(result.is_err());
    }
}
