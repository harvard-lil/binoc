/// Controls whether the writer reports an error when a value must be
/// truncated to fit its designated field width.
#[derive(Copy, Clone, Debug, Default, Hash, PartialEq, Eq)]
pub enum TruncationPolicy {
    /// Silently truncate the value (current default behavior).
    #[default]
    Silent,
    /// Return an error if truncation would occur.
    Report,
}
