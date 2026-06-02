pub(crate) const LIBRARY_HEADER_V5: &[u8] =
    b"HEADER RECORD*******LIBRARY HEADER RECORD!!!!!!!000000000000000000000000000000  ";
pub(crate) const LIBRARY_HEADER_V8: &[u8] =
    b"HEADER RECORD*******LIBV8   HEADER RECORD!!!!!!!000000000000000000000000000000  ";
pub(crate) const MEMBER_HEADER_PREFIX_V5: &[u8] =
    b"HEADER RECORD*******MEMBER  HEADER RECORD!!!!!!!00000000000000000160000000";
pub(crate) const MEMBER_HEADER_PREFIX_V8: &[u8] =
    b"HEADER RECORD*******MEMBV8  HEADER RECORD!!!!!!!00000000000000000160000000";
pub(crate) const DESCRIPTOR_HEADER_V5: &[u8] =
    b"HEADER RECORD*******DSCRPTR HEADER RECORD!!!!!!!000000000000000000000000000000  ";
pub(crate) const DESCRIPTOR_HEADER_V8: &[u8] =
    b"HEADER RECORD*******DSCPTV8 HEADER RECORD!!!!!!!000000000000000000000000000000  ";
pub(crate) const NAMESTR_HEADER_PREFIX_V5: &[u8] =
    b"HEADER RECORD*******NAMESTR HEADER RECORD!!!!!!!000000";
pub(crate) const NAMESTR_HEADER_PREFIX_V8: &[u8] =
    b"HEADER RECORD*******NAMSTV8 HEADER RECORD!!!!!!!000000";
pub(crate) const NAMESTR_HEADER_SUFFIX: &[u8] = b"00000000000000000000  ";
pub(crate) const OBSERVATION_HEADER_PREFIX_V5: &[u8] =
    b"HEADER RECORD*******OBS     HEADER RECORD!!!!!!!";
pub(crate) const OBSERVATION_HEADER_SUFFIX_V5: &[u8] = b"000000000000000000000000000000  ";
pub(crate) const OBSERVATION_HEADER_PREFIX_V8: &[u8] =
    b"HEADER RECORD*******OBSV8   HEADER RECORD!!!!!!!";
pub(crate) const OBSERVATION_HEADER_SUFFIX_V8: &[u8] = b"                 ";
pub(crate) const LABEL_HEADER_V8_PREFIX: &[u8] =
    b"HEADER RECORD*******LABELV8 HEADER RECORD!!!!!!!";
pub(crate) const LABEL_HEADER_V9_PREFIX: &[u8] =
    b"HEADER RECORD*******LABELV9 HEADER RECORD!!!!!!!";
pub(crate) const HEADER_LENGTH: usize = 80;

pub(crate) fn variable_error(message: &str, variable_index: u16) -> String {
    format!("Failed to read the variable at index {variable_index}. {message}")
}
