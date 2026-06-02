use super::xport_file_version::XportFileVersion;
use crate::sas::SasDateTime;

/// Describes the SAS® transport file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XportMetadata {
    file_version: XportFileVersion,
    symbol1: String,
    symbol2: String,
    library: String,
    sas_version: String,
    operating_system: String,
    created: SasDateTime,
    modified: SasDateTime,
}

impl XportMetadata {
    /// The default first symbol value.
    pub const DEFAULT_SYMBOL1: &'static str = "SAS";
    /// The default second symbol value.
    pub const DEFAULT_SYMBOL2: &'static str = "SAS";
    /// The default library value.
    pub const DEFAULT_LIBRARY: &'static str = "SASLIB";
    /// A SAS® V5 file version.
    pub const DEFAULT_SAS_VERSION_V5: &'static str = "5.2";
    /// A SAS® V8+ file version.
    pub const DEFAULT_SAS_VERSION_V8: &'static str = "9.1";

    /// Creates a builder for constructing an `XportMetadata`.
    #[inline]
    #[must_use]
    pub fn builder() -> XportMetadataBuilder {
        XportMetadataBuilder::new()
    }

    /// Gets the SAS® transport file version. This is the version determined by the
    /// library header and can only distinguish between V5 and V8.
    #[inline]
    #[must_use]
    pub const fn file_version(&self) -> XportFileVersion {
        self.file_version
    }

    /// Gets symbol 1. This is usually "SAS".
    #[inline]
    #[must_use]
    pub fn symbol1(&self) -> &str {
        &self.symbol1
    }

    /// Gets symbol 2. This is usually "SAS".
    #[inline]
    #[must_use]
    pub fn symbol2(&self) -> &str {
        &self.symbol2
    }

    /// Gets the library value. This is usually "SASLIB".
    #[inline]
    #[must_use]
    pub fn library(&self) -> &str {
        &self.library
    }

    /// Gets the SAS® version. This is usually the specific version of the SAS®
    /// environment that generated the file.
    #[inline]
    #[must_use]
    pub fn sas_version(&self) -> &str {
        &self.sas_version
    }

    /// Gets the operating system the SAS® environment ran on.
    #[inline]
    #[must_use]
    pub fn operating_system(&self) -> &str {
        &self.operating_system
    }

    /// Gets the creation date of the file.
    #[inline]
    #[must_use]
    pub fn created(&self) -> SasDateTime {
        self.created
    }

    /// Gets the last modified date of the file.
    #[inline]
    #[must_use]
    pub fn modified(&self) -> SasDateTime {
        self.modified
    }
}

/// Allows constructing a `XportMetadata`.
#[derive(Clone, Debug)]
pub struct XportMetadataBuilder {
    file_version: XportFileVersion,
    symbol1: String,
    symbol2: String,
    library: String,
    sas_version: Option<String>,
    operating_system: String,
    created: SasDateTime,
    modified: SasDateTime,
}

impl Default for XportMetadataBuilder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl XportMetadataBuilder {
    fn new() -> Self {
        let timestamp = SasDateTime::new();
        Self {
            file_version: XportFileVersion::V5,
            symbol1: String::from(XportMetadata::DEFAULT_SYMBOL1),
            symbol2: String::from(XportMetadata::DEFAULT_SYMBOL2),
            library: String::from(XportMetadata::DEFAULT_LIBRARY),
            sas_version: None,
            operating_system: String::new(),
            created: timestamp,
            modified: timestamp,
        }
    }

    /// Sets the SAS® transport file version.
    #[inline]
    pub fn xport_file_version(&mut self, file_version: XportFileVersion) -> &mut Self {
        self.file_version = file_version;
        self
    }

    /// Sets symbol 1. This is usually the value "SAS".
    #[inline]
    pub fn symbol1(&mut self, symbol1: impl Into<String>) -> &mut Self {
        self.symbol1 = symbol1.into();
        self
    }

    /// Sets symbol 2. This is usually the value "SAS".
    #[inline]
    pub fn symbol2(&mut self, symbol2: impl Into<String>) -> &mut Self {
        self.symbol2 = symbol2.into();
        self
    }

    /// Sets the library value. This is usually "SASLIB".
    #[inline]
    pub fn library(&mut self, library: impl Into<String>) -> &mut Self {
        self.library = library.into();
        self
    }

    /// Sets the SAS® version. This is usually the specific version of the SAS®
    /// environment generating the file.
    #[inline]
    pub fn sas_version(&mut self, sas_version: impl Into<String>) -> &mut Self {
        self.sas_version = Some(sas_version.into());
        self
    }

    /// Clears the SAS® version. When cleared, this will be set automatically
    /// depending on the file version.
    #[inline]
    pub fn clear_sas_version(&mut self) -> &mut Self {
        self.sas_version = None;
        self
    }

    /// Sets the operating system the SAS® environment ran on.
    #[inline]
    pub fn operating_system(&mut self, operating_system: impl Into<String>) -> &mut Self {
        self.operating_system = operating_system.into();
        self
    }

    /// Sets the creation date of the file.
    #[inline]
    pub fn created(&mut self, created: SasDateTime) -> &mut Self {
        self.created = created;
        self
    }

    /// Sets the last modified date of the file.
    #[inline]
    pub fn modified(&mut self, modified: SasDateTime) -> &mut Self {
        self.modified = modified;
        self
    }

    /// Builds an `XportMetadata` based on the current configuration.
    #[inline]
    #[must_use]
    pub fn build(&self) -> XportMetadata {
        self.clone().build_into()
    }

    /// Builds an `XportMetadata` based on the current configuration, consuming
    /// the builder.
    #[inline]
    #[must_use]
    pub fn build_into(self) -> XportMetadata {
        let sas_version = self.sas_version.unwrap_or_else(|| match self.file_version {
            XportFileVersion::V5 => XportMetadata::DEFAULT_SAS_VERSION_V5.to_string(),
            XportFileVersion::V8 => XportMetadata::DEFAULT_SAS_VERSION_V8.to_string(),
        });
        XportMetadata {
            file_version: self.file_version,
            symbol1: self.symbol1,
            symbol2: self.symbol2,
            library: self.library,
            sas_version,
            operating_system: self.operating_system,
            created: self.created,
            modified: self.modified,
        }
    }
}

impl From<XportMetadataBuilder> for XportMetadata {
    #[inline]
    fn from(builder: XportMetadataBuilder) -> Self {
        builder.build_into()
    }
}

#[cfg(test)]
#[cfg(feature = "chrono")]
mod chrono_tests {
    use chrono::Days;

    use crate::sas::sas_date_time::SasDateTime;

    use super::{XportFileVersion, XportMetadata};

    #[test]
    fn getters_work_builder_uses_defaults() {
        let os = "WinXP_PRO";
        let created = SasDateTime::now();
        let modified: SasDateTime = created
            .to_chrono_date_time(2000)
            .unwrap()
            .checked_add_days(Days::new(1))
            .unwrap()
            .into();
        let metadata = XportMetadata::builder()
            .operating_system(os)
            .created(created)
            .modified(modified)
            .build();
        assert_eq!(XportFileVersion::V5, metadata.file_version());
        assert_eq!(XportMetadata::DEFAULT_SYMBOL1, metadata.symbol1());
        assert_eq!(XportMetadata::DEFAULT_SYMBOL2, metadata.symbol2());
        assert_eq!(XportMetadata::DEFAULT_LIBRARY, metadata.library());
        assert_eq!(
            XportMetadata::DEFAULT_SAS_VERSION_V5,
            metadata.sas_version()
        );
        assert_eq!(os, metadata.operating_system());
        assert_eq!(created, metadata.created());
        assert_eq!(modified, metadata.modified());
    }
}

#[cfg(test)]
mod tests {
    use super::{XportFileVersion, XportMetadata};

    #[test]
    fn v5_builder_defaults() {
        let metadata = XportMetadata::builder().build();
        assert_eq!(XportFileVersion::V5, metadata.file_version());
        assert_eq!(
            XportMetadata::DEFAULT_SAS_VERSION_V5,
            metadata.sas_version()
        );
        assert_eq!(XportMetadata::DEFAULT_SYMBOL1, metadata.symbol1());
        assert_eq!(XportMetadata::DEFAULT_SYMBOL2, metadata.symbol2());
        assert_eq!(XportMetadata::DEFAULT_LIBRARY, metadata.library());
        assert_eq!("", metadata.operating_system());
        assert_eq!(
            XportMetadata::DEFAULT_SAS_VERSION_V5,
            metadata.sas_version()
        );
    }

    #[test]
    fn v8_builder_defaults_sas_version() {
        let metadata = XportMetadata::builder()
            .xport_file_version(XportFileVersion::V8)
            .build();
        assert_eq!(XportFileVersion::V8, metadata.file_version());
        assert_eq!(
            XportMetadata::DEFAULT_SAS_VERSION_V8,
            metadata.sas_version()
        );
    }
}
