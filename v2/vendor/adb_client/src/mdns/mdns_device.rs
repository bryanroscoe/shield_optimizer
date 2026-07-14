use std::{
    collections::HashSet,
    fmt::Display,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    num::NonZeroU16,
};

use mdns_sd::{ResolvedService, ScopedIp};

use crate::RustADBError;

/// Represent a device found from mdns search
#[derive(Debug)]
pub struct MDNSDevice {
    /// Full device address when resolved
    pub fullname: String,
    /// Device IP addresses
    addresses: HashSet<IpAddr>,
    /// Device port
    port: NonZeroU16,
}

impl MDNSDevice {
    /// Return all adresses linked to this device
    #[must_use]
    pub fn addresses(&self) -> HashSet<IpAddr> {
        self.addresses.clone()
    }

    /// Return the port of this device
    #[must_use]
    pub const fn port(&self) -> NonZeroU16 {
        self.port
    }

    /// Return all IPv4 addresses linked to this device
    #[must_use]
    pub fn ipv4_addresses(&self) -> HashSet<Ipv4Addr> {
        self.addresses
            .iter()
            .filter_map(|addr| match addr {
                IpAddr::V4(addr) => Some(addr),
                IpAddr::V6(_) => None,
            })
            .copied()
            .collect()
    }

    /// Return all IPv6 addresses linked to this device
    #[must_use]
    pub fn ipv6_addresses(&self) -> HashSet<Ipv6Addr> {
        self.addresses
            .iter()
            .filter_map(|addr| match addr {
                IpAddr::V4(_) => None,
                IpAddr::V6(addr) => Some(addr),
            })
            .copied()
            .collect()
    }
}

impl TryFrom<Box<ResolvedService>> for MDNSDevice {
    type Error = RustADBError;

    fn try_from(value: Box<ResolvedService>) -> Result<Self, Self::Error> {
        let fullname = value.fullname.clone();
        Ok(Self {
            fullname: value.fullname,
            port: NonZeroU16::new(value.port).ok_or(RustADBError::UnknownDeviceState(format!(
                "device {} has a non-u16 port: {}",
                fullname, value.port
            )))?,
            addresses: value.addresses.iter().map(ScopedIp::to_ip_addr).collect(),
        })
    }
}

impl Display for MDNSDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Device fullname: {}", self.fullname)?;
        writeln!(f, "Device port: {}", self.port)?;
        writeln!(f, "IPv4 Addresses: {:?}", self.ipv4_addresses())?;
        write!(f, "IPv6 Addresses: {:?}", self.ipv6_addresses())?;

        Ok(())
    }
}
