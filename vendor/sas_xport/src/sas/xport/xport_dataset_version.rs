use std::fmt;

/// The version of the SAS® Transport (XPORT) file format at the dataset level.
/// This includes V9, which can only be determined when a dataset's extended label
/// header is encountered during schema reading.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum XportDatasetVersion {
    /// SAS® version 5 and above.
    #[default]
    V5,
    /// SAS® version 8 and above. This version introduced extended variable names and labels.
    V8,
    /// SAS® version 9 and above. This version introduced extended variable formats.
    V9,
}

impl fmt::Display for XportDatasetVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V5 => write!(f, "V5"),
            Self::V8 => write!(f, "V8"),
            Self::V9 => write!(f, "V9"),
        }
    }
}
