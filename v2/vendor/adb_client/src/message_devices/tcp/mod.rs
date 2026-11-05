#![doc = include_str!("./README.md")]

mod adb_tcp_device;
mod adb_tcp_service;
mod bound_service;
mod bounded_shell;
mod pairing;
#[cfg(test)]
mod stream_tests;
mod streamed_install;
mod tcp_transport;
#[cfg(test)]
pub(crate) mod test_support;
mod tls_identity;

pub use adb_tcp_device::ADBTcpDevice;
pub use adb_tcp_service::ADBTcpService;
pub use bound_service::{BoundDeviceService, BoundServiceError, open_bound_service};
pub use bounded_shell::{
    BoundedShellControl, BoundedShellOutput, BoundedShellTermination, SHELL_OUTPUT_LIMIT,
    run_bounded_shell, run_identity_bound_shell,
};
pub use pairing::{PairingError, PairingOutcome, PeerInfo, pair};
pub use streamed_install::{
    INSTALL_RESPONSE_LIMIT, InstallOutcome, InstallStatus, run_streamed_install,
};

pub(crate) use tcp_transport::is_untrusted_key_alert;
