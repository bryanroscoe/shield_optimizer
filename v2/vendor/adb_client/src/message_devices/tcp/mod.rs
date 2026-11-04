#![doc = include_str!("./README.md")]

mod adb_tcp_device;
mod adb_tcp_service;
mod pairing;
mod tcp_transport;
#[cfg(test)]
pub(crate) mod test_support;
mod tls_identity;

pub use adb_tcp_device::ADBTcpDevice;
pub use adb_tcp_service::ADBTcpService;
pub use pairing::{PairingError, PairingOutcome, PeerInfo, pair};
