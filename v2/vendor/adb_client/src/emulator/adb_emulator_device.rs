use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::LazyLock,
};

use crate::{
    ADBTransport, Result, RustADBError, emulator::tcp_emulator_transport::TCPEmulatorTransport,
    server_device::ADBServerDevice,
};
use regex::Regex;

static EMULATOR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^emulator-(?P<port>\\d+)$").expect("wrong syntax for emulator regex")
});

/// Represents an emulator connected to the ADB server.
#[derive(Debug)]
pub struct ADBEmulatorDevice {
    /// Unique device identifier.
    pub identifier: String,
    /// Internal [`TCPEmulatorTransport`]
    transport: TCPEmulatorTransport,
}

impl ADBEmulatorDevice {
    /// Instantiates a new [`ADBEmulatorDevice`]
    pub fn new(identifier: String, ip_address: Option<Ipv4Addr>) -> Result<Self> {
        let ip_address = ip_address.map_or(Ipv4Addr::LOCALHOST, |ip_address| ip_address);

        let groups = EMULATOR_REGEX
            .captures(&identifier)
            .ok_or(RustADBError::DeviceNotFound(format!(
                "Device {identifier} is likely not an emulator"
            )))?;

        let port = groups
            .name("port")
            .ok_or(RustADBError::RegexParsingError)?
            .as_str()
            .parse::<u16>()?;

        let socket_addr = SocketAddrV4::new(ip_address, port);

        let transport = TCPEmulatorTransport::new(socket_addr);
        Ok(Self {
            identifier,
            transport,
        })
    }

    pub(crate) const fn get_transport_mut(&mut self) -> &mut TCPEmulatorTransport {
        &mut self.transport
    }

    /// Connect to underlying transport
    pub(crate) fn connect(&mut self) -> Result<&mut TCPEmulatorTransport> {
        self.transport.connect()?;

        Ok(self.get_transport_mut())
    }
}

impl TryFrom<ADBServerDevice> for ADBEmulatorDevice {
    type Error = RustADBError;

    fn try_from(value: ADBServerDevice) -> std::result::Result<Self, Self::Error> {
        value.identifier.as_ref().map_or_else(
            || {
                Err(RustADBError::DeviceNotFound(
                    "cannot connect to an emulator device without knowing its identifier"
                        .to_string(),
                ))
            },
            |device_identifier| {
                Self::new(
                    device_identifier.clone(),
                    Some(*value.transport.get_socketaddr().ip()),
                )
            },
        )
    }
}

impl Drop for ADBEmulatorDevice {
    fn drop(&mut self) {
        // Best effort here
        let _ = self.transport.disconnect();
    }
}
