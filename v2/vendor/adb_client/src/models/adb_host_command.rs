use std::{fmt::Display, net::SocketAddrV4};

use crate::server::{WaitForDeviceState, WaitForDeviceTransport};

/// ADB commands that relates to the host and are handled by the ADB server.
pub enum ADBHostCommand {
    Version,
    Kill,
    Devices,
    DevicesLong,
    TrackDevices,
    HostFeatures,
    Connect(SocketAddrV4),
    Disconnect(SocketAddrV4),
    Pair(SocketAddrV4, String),
    TransportAny,
    TransportSerial(String),
    TransportId(u32),
    MDNSCheck,
    MDNSServices,
    ServerStatus,
    ReconnectOffline,
    WaitForDevice(WaitForDeviceState, WaitForDeviceTransport),
}

impl Display for ADBHostCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version => write!(f, "host:version"),
            Self::Kill => write!(f, "host:kill"),
            Self::Devices => write!(f, "host:devices"),
            Self::DevicesLong => write!(f, "host:devices-l"),
            Self::TrackDevices => write!(f, "host:track-devices"),
            Self::TransportAny => write!(f, "host:transport-any"),
            Self::TransportSerial(serial) => write!(f, "host:transport:{serial}"),
            Self::TransportId(id) => write!(f, "host:transport-id:{id}"),
            Self::Connect(addr) => write!(f, "host:connect:{addr}"),
            Self::Disconnect(addr) => write!(f, "host:disconnect:{addr}"),
            Self::Pair(addr, code) => {
                write!(f, "host:pair:{code}:{addr}")
            }
            Self::MDNSCheck => write!(f, "host:mdns:check"),
            Self::MDNSServices => write!(f, "host:mdns:services"),
            Self::ServerStatus => write!(f, "host:server-status"),
            Self::ReconnectOffline => write!(f, "host:reconnect-offline"),
            Self::WaitForDevice(wait_for_device_state, wait_for_device_transport) => {
                write!(
                    f,
                    "host:wait-for-{wait_for_device_transport}-{wait_for_device_state}"
                )
            }
            Self::HostFeatures => write!(f, "host:features"),
        }
    }
}
