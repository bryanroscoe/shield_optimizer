use std::{fmt::Display, str::FromStr};

use crate::RustADBError;

#[derive(Clone, Debug, Default)]
/// List of available transports to wait for.
pub enum WaitForDeviceTransport {
    /// USB transport
    Usb,
    /// Local transport
    Local,
    /// Any transport (default value)
    #[default]
    Any,
}

impl Display for WaitForDeviceTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usb => write!(f, "usb"),
            Self::Local => write!(f, "local"),
            Self::Any => write!(f, "any"),
        }
    }
}

impl FromStr for WaitForDeviceTransport {
    type Err = RustADBError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "usb" => Ok(Self::Usb),
            "local" => Ok(Self::Local),
            "any" => Ok(Self::Any),
            t => Err(RustADBError::UnknownTransport(t.to_string())),
        }
    }
}

#[derive(Debug)]
/// List of available states to wait for.
pub enum WaitForDeviceState {
    /// Device in "device" state
    Device,
    /// Device in "recovery" state
    Recovery,
    /// Device in "sideload" state
    Sideload,
    /// Device in "bootloader" state
    Bootloader,
}

impl Display for WaitForDeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Device => write!(f, "device"),
            Self::Recovery => write!(f, "recovery"),
            Self::Sideload => write!(f, "sideload"),
            Self::Bootloader => write!(f, "bootloader"),
        }
    }
}
