#![doc = include_str!("./README.md")]

mod adb_tcp_device;
mod adb_tcp_service;
mod bounded_shell;
mod pairing;
mod tcp_transport;

pub use adb_tcp_device::ADBTcpDevice;
pub use adb_tcp_service::ADBTcpService;
pub use bounded_shell::{
    run_bounded_shell, BoundedShellControl, BoundedShellOutput, BoundedShellTermination,
    SHELL_OUTPUT_LIMIT,
};
pub use pairing::{pair, PairingError, PairingOutcome, PeerInfo};
