use std::fmt::Display;
use std::str;

use crate::RustADBError;

/// Represents the ADB server version.
#[derive(Debug)]
pub struct AdbVersion {
    /// Major version number.
    pub major: u32,
    /// Minor version number.
    pub minor: u32,
    /// Revision number.
    pub revision: u32,
}

impl AdbVersion {
    /// Instantiates a new [`AdbVersion`].
    #[must_use]
    pub const fn new(minor: u32, revision: u32) -> Self {
        Self {
            major: 1,
            minor,
            revision,
        }
    }
}

impl Display for AdbVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.revision)
    }
}

impl TryFrom<Vec<u8>> for AdbVersion {
    type Error = RustADBError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(Self::new(
            u32::from_str_radix(str::from_utf8(&value[0..2])?, 16)?,
            u32::from_str_radix(str::from_utf8(&value[2..4])?, 16)?,
        ))
    }
}
