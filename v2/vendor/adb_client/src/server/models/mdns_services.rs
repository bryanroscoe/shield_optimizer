use regex::bytes::Regex;
use std::net::SocketAddrV4;
use std::sync::LazyLock;
use std::{fmt::Display, str::FromStr};

use crate::RustADBError;

static MDNS_SERVICES_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^(\\S+)\t(\\S+)\t([\\d\\.]+:\\d+)\n?$").expect("Cannot build mdns services regex")
});

/// Represents MDNS Services
#[derive(Debug, Clone)]
pub struct MDNSServices {
    /// Service name
    pub service_name: String,
    /// Reg type
    pub reg_type: String,
    /// IP addr with port
    pub socket_v4: SocketAddrV4,
}

impl Display for MDNSServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\t{}\t{}",
            self.service_name, self.reg_type, self.socket_v4
        )
    }
}

impl TryFrom<&[u8]> for MDNSServices {
    type Error = RustADBError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let groups = MDNS_SERVICES_REGEX
            .captures(value)
            .ok_or(RustADBError::RegexParsingError)?;
        Ok(Self {
            service_name: String::from_utf8(
                groups
                    .get(1)
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?,
            reg_type: String::from_utf8(
                groups
                    .get(2)
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?,
            socket_v4: SocketAddrV4::from_str(&String::from_utf8(
                groups
                    .get(3)
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?)?,
        })
    }
}
