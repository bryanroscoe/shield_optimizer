use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use crate::Result;
use crate::message_devices::adb_message_device::ADBMessageDevice;
use crate::message_devices::adb_service::ADBService;
use crate::tcp::tcp_transport::TcpTransport;
use crate::utils::get_default_adb_key_path;

/// A persistent raw byte stream to one service exposed by a device's `adbd`.
///
/// Unlike [`super::ADBTcpDevice`], this type owns a dedicated authenticated TCP
/// connection. That is required because ADB logical streams share one wire and
/// must have a dispatcher before they can be read concurrently.
#[derive(Debug)]
pub struct ADBTcpService {
    service: ADBService<TcpTransport>,
    // Keep the parent device alive until after the logical service closes. Its
    // Drop implementation owns the physical connection shutdown.
    _device: ADBMessageDevice<TcpTransport>,
}

impl ADBTcpService {
    /// Connect to `address` and open an arbitrary ADB device service.
    pub fn new<A: Into<SocketAddr>>(address: A, service: &str) -> Result<Self> {
        Self::new_with_custom_private_key(address, service, get_default_adb_key_path()?)
    }

    /// Connect using a custom ADB private key and open `service`.
    pub fn new_with_custom_private_key<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        service: &str,
        private_key_path: P,
    ) -> Result<Self> {
        Self::new_with_timeouts(
            address,
            service,
            private_key_path,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
    }

    /// Connect and open `service` with explicit protocol timeouts.
    pub fn new_with_timeouts<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        service: &str,
        private_key_path: P,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> Result<Self> {
        let private_key_path = private_key_path.as_ref();
        let mut device = ADBMessageDevice::new(
            TcpTransport::new(address, private_key_path),
            private_key_path,
        )?;
        let session = device.open_raw_service(service, read_timeout)?;
        Ok(Self {
            service: ADBService::new(session, read_timeout, write_timeout),
            _device: device,
        })
    }

    /// Send `CLSE` for the logical stream. Calling this more than once is safe.
    pub fn close(&mut self) -> std::io::Result<()> {
        self.service.close()
    }
}

impl Read for ADBTcpService {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.service.read(buf)
    }
}

impl Write for ADBTcpService {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.service.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.service.flush()
    }
}
