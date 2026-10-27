#![doc = include_str!("./README.md")]

mod adb_tcp_device;
mod adb_tcp_service;
mod bound_service;
mod bounded_shell;
mod pairing;
mod streamed_install;
mod tcp_transport;

pub use adb_tcp_device::ADBTcpDevice;
pub use adb_tcp_service::ADBTcpService;
pub use bounded_shell::{
    run_bounded_shell, BoundedShellControl, BoundedShellOutput, BoundedShellTermination,
    SHELL_OUTPUT_LIMIT,
};
pub use pairing::{pair, PairingError, PairingOutcome, PeerInfo};

pub use streamed_install::{
    run_streamed_install, InstallOutcome, InstallStatus, INSTALL_RESPONSE_LIMIT,
};

pub use bound_service::{open_bound_service, BoundDeviceService, BoundServiceError};
