#![doc = include_str!("./README.md")]

mod adb_tcp_device;
mod adb_tcp_service;
mod bound_service;
mod bounded_shell;
mod pairing;
mod preferred_keys;
#[cfg(test)]
mod stream_tests;
mod streamed_install;
mod tcp_transport;
#[cfg(test)]
pub(crate) mod test_support;
mod tls_identity;

pub use crate::message_devices::adb_message_device::AcceptedKey;
pub use adb_tcp_device::ADBTcpDevice;
pub use adb_tcp_service::ADBTcpService;
pub use bound_service::{
    BoundDeviceService, BoundServiceError, open_bound_service, open_bound_service_with_keys,
};
pub use bounded_shell::{
    BoundedShellControl, BoundedShellOutput, BoundedShellTermination, SHELL_OUTPUT_LIMIT,
    run_bounded_shell, run_identity_bound_shell, run_identity_bound_shell_with_keys,
};
pub use pairing::{PairingError, PairingOutcome, PeerInfo, pair, pair_with_keys};
pub use preferred_keys::PreferredKeys;
pub use streamed_install::{
    INSTALL_RESPONSE_LIMIT, InstallOutcome, InstallStatus, run_streamed_install,
    run_streamed_install_with_keys,
};

pub(crate) use tcp_transport::is_untrusted_key_alert;

/// A dedicated connection under `control` that needs the key file to exist
/// and offers `preferred` before it. The handshake gets at most `budget`.
/// With [`PreferredKeys::signing_only`] it never offers a public key.
pub(crate) fn connect_required(
    address: std::net::SocketAddr,
    key: &std::path::Path,
    preferred: &PreferredKeys,
    control: &BoundedShellControl,
    budget: std::time::Duration,
) -> crate::Result<
    crate::message_devices::adb_message_device::ADBMessageDevice<tcp_transport::TcpTransport>,
> {
    let transport = tcp_transport::TcpTransport::with_shell_control(address, key, control.clone())
        .with_preferred_keys(preferred);
    crate::message_devices::adb_message_device::ADBMessageDevice::new_with_required_keys(
        transport, key, preferred, budget,
    )
}
