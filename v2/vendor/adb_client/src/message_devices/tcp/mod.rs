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
    BoundedShellControl, BoundedShellOutput, BoundedShellTermination, SHELL_OUTPUT_LIMIT,
    run_bounded_shell, run_identity_bound_shell,
};
pub use pairing::{PairingError, PairingOutcome, PeerInfo, pair};

pub use streamed_install::{
    INSTALL_RESPONSE_LIMIT, InstallOutcome, InstallStatus, run_streamed_install,
};

pub use bound_service::{BoundDeviceService, BoundServiceError, open_bound_service};
