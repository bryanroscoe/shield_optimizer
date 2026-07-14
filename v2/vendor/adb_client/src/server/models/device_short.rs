use regex::bytes::Regex;
use std::{fmt::Display, str::FromStr, sync::LazyLock};

use crate::{RustADBError, server::DeviceState};

static DEVICES_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^(\\S+)\t(\\w+)\n?$").expect("Cannot build devices regex"));

/// Represents a device connected to the ADB server.
#[derive(Debug, Clone)]
pub struct DeviceShort {
    /// Unique device identifier.
    pub identifier: String,
    /// Connection state of the device.
    pub state: DeviceState,
}

impl Display for DeviceShort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}", self.identifier, self.state)
    }
}

impl TryFrom<Vec<u8>> for DeviceShort {
    type Error = RustADBError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        // Optional final '\n' is used to match TrackDevices inputs
        let groups = DEVICES_REGEX
            .captures(&value)
            .ok_or(RustADBError::RegexParsingError)?;
        Ok(Self {
            identifier: String::from_utf8(
                groups
                    .get(1)
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?,
            state: DeviceState::from_str(&String::from_utf8(
                groups
                    .get(2)
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?)?,
        })
    }
}
