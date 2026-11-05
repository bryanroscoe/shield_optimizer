use byteorder::ByteOrder;
use chrono::{DateTime, Utc};
use std::{
    fmt::Display,
    time::{Duration, UNIX_EPOCH},
};

use crate::{BinaryDecodable, RustADBError};
use byteorder::LittleEndian;

/// Represents a `stat` response
#[derive(Debug)]
pub struct AdbStatResponse {
    /// File mode: type and permission bits
    pub file_perm: u32,
    /// File size, in bytes. The legacy `STAT` request carries only 32 bits;
    /// devices with `stat_v2` report the full size.
    pub file_size: u64,
    /// File modification time, in seconds since the Unix epoch
    pub mod_time: u64,
}

/// Size of the legacy `STAT` reply after its id: mode, size and mtime.
const STAT_V1_LEN: usize = 12;

impl BinaryDecodable for AdbStatResponse {
    fn decode(bytes: &[u8]) -> crate::Result<Self> {
        let bytes: [u8; STAT_V1_LEN] = bytes
            .try_into()
            .map_err(|_| RustADBError::ConversionError)?;
        Ok(bytes.into())
    }
}

impl From<[u8; 12]> for AdbStatResponse {
    fn from(value: [u8; 12]) -> Self {
        Self {
            file_perm: LittleEndian::read_u32(&value[0..4]),
            file_size: u64::from(LittleEndian::read_u32(&value[4..8])),
            mod_time: u64::from(LittleEndian::read_u32(&value[8..])),
        }
    }
}

impl Display for AdbStatResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = UNIX_EPOCH + Duration::from_secs(self.mod_time);
        // Create DateTime from SystemTime
        let datetime = DateTime::<Utc>::from(d);

        writeln!(f, "File permissions: {}", self.file_perm)?;
        writeln!(f, "File size: {} bytes", self.file_size)?;
        write!(
            f,
            "Modification time: {}",
            datetime.format("%Y-%m-%d %H:%M:%S.%f %Z")
        )?;
        Ok(())
    }
}
