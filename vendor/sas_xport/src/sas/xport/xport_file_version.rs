use super::xport_dataset_version::XportDatasetVersion;
use std::fmt;

/// The version of the SAS® Transport (XPORT) file format, as determined by the
/// library header. Only V5 and V8 can be distinguished at the file level.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum XportFileVersion {
    /// SAS® version 5 and above.
    #[default]
    V5,
    /// SAS® version 8 and above. This version introduced extended variable names and labels.
    V8,
}

impl fmt::Display for XportFileVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V5 => write!(f, "V5"),
            Self::V8 => write!(f, "V8"),
        }
    }
}

impl From<XportFileVersion> for XportDatasetVersion {
    fn from(version: XportFileVersion) -> Self {
        match version {
            XportFileVersion::V5 => Self::V5,
            XportFileVersion::V8 => Self::V8,
        }
    }
}
