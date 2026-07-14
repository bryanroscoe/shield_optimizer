use std::{fmt::Display, str::FromStr};

use crate::RustADBError;

/// Represents the connection state of the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    /// The device is not connected to adb or is not responding.
    Offline,
    /// The device is now connected to the adb server. Note that this state does not imply that the Android system is fully booted and operational because the device connects to adb while the system is still booting. However, after boot-up, this is the normal operational state of an device.
    Device,
    /// There is no device connected.
    NoDevice,
    /// Device is being authorized
    Authorizing,
    /// The device is unauthorized.
    Unauthorized,
    /// Haven't received a response from the device yet.
    Connecting,
    /// Insufficient permissions to communicate with the device.
    NoPerm,
    /// USB device detached from the adb server (known but not opened/claimed).
    Detached,
    /// Device running fastboot OS (fastboot) or userspace fastboot (fastbootd).
    Bootloader,
    /// What a device sees from its end of a Transport (adb host).
    Host,
    /// Device with bootloader loaded but no ROM OS loaded (adbd).
    Recovery,
    /// Device running Android OS Sideload mode (minadbd sideload mode).
    Sideload,
    /// Device running Android OS Rescue mode (minadbd rescue mode).
    Rescue,
}

impl Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline => write!(f, "offline"),
            Self::Device => write!(f, "device"),
            Self::NoDevice => write!(f, "no device"),
            Self::Authorizing => write!(f, "authorizing"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::Connecting => write!(f, "connecting"),
            Self::NoPerm => write!(f, "noperm"),
            Self::Detached => write!(f, "detached"),
            Self::Bootloader => write!(f, "bootloader"),
            Self::Host => write!(f, "host"),
            Self::Recovery => write!(f, "recovery"),
            Self::Sideload => write!(f, "sideload"),
            Self::Rescue => write!(f, "rescue"),
        }
    }
}

impl FromStr for DeviceState {
    type Err = RustADBError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lowercased = s.to_ascii_lowercase();
        match lowercased.as_str() {
            "offline" => Ok(Self::Offline),
            "device" => Ok(Self::Device),
            "no device" => Ok(Self::NoDevice),
            "authorizing" => Ok(Self::Authorizing),
            "unauthorized" => Ok(Self::Unauthorized),
            "connecting" => Ok(Self::Connecting),
            "noperm" => Ok(Self::NoPerm),
            "detached" => Ok(Self::Detached),
            "bootloader" => Ok(Self::Bootloader),
            "host" => Ok(Self::Host),
            "recovery" => Ok(Self::Recovery),
            "sideload" => Ok(Self::Sideload),
            "rescue" => Ok(Self::Rescue),
            _ => Err(RustADBError::UnknownDeviceState(lowercased)),
        }
    }
}
