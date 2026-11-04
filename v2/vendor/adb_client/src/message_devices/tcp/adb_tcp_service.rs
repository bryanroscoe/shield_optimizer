use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use crate::message_devices::adb_message_device::{ADBMessageDevice, DEFAULT_AUTH_TIMEOUT};
use crate::message_devices::adb_service::ADBService;
use crate::tcp::tcp_transport::TcpTransport;
use crate::utils::get_default_adb_key_path;
use crate::{ConnectionInfo, Result};

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
    device: ADBMessageDevice<TcpTransport>,
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

    /// Connect and open `service` with explicit protocol timeouts. The
    /// connection handshake gets a 10-second default bound.
    pub fn new_with_timeouts<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        service: &str,
        private_key_path: P,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> Result<Self> {
        Self::new_with_timeouts_and_auth_timeout(
            address,
            service,
            private_key_path,
            read_timeout,
            write_timeout,
            DEFAULT_AUTH_TIMEOUT,
        )
    }

    /// Connect and open `service`. The whole connection handshake must finish
    /// within `auth_timeout`, as for
    /// [`super::ADBTcpDevice::new_with_custom_private_key_and_auth_timeout`];
    /// `read_timeout` and `write_timeout` then bound each stream operation.
    pub fn new_with_timeouts_and_auth_timeout<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        service: &str,
        private_key_path: P,
        read_timeout: Duration,
        write_timeout: Duration,
        auth_timeout: Duration,
    ) -> Result<Self> {
        let private_key_path = private_key_path.as_ref();
        let mut device = ADBMessageDevice::new_with_auth_timeout(
            TcpTransport::new(address, private_key_path),
            private_key_path,
            auth_timeout,
        )?;
        let session = device.open_raw_service(service, read_timeout)?;
        Ok(Self {
            service: ADBService::new(session, read_timeout, write_timeout),
            device,
        })
    }

    /// What the CNXN exchange of this stream's own connection negotiated.
    #[must_use]
    pub const fn connection_info(&self) -> &ConnectionInfo {
        self.device.connection_info()
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

#[cfg(test)]
mod tests {
    use super::ADBTcpService;
    use crate::RustADBError;
    use crate::message_devices::tcp::test_support::{
        drain, fake_adbd, read_message, shared_test_key,
    };
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    #[test]
    fn a_silent_device_fails_within_the_auth_timeout() {
        let (held_tx, held_rx) = mpsc::channel();
        let (address, server) = fake_adbd(move |mut stream| {
            let _cnxn = read_message(&mut stream).unwrap();
            let since_cnxn = Instant::now();
            drain(&mut stream);
            held_tx.send(since_cnxn.elapsed()).unwrap();
        });
        let error = ADBTcpService::new_with_timeouts_and_auth_timeout(
            address,
            "shell:true",
            shared_test_key(),
            Duration::from_secs(2),
            Duration::from_secs(2),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(
            matches!(&error, RustADBError::IOError(io) if io.kind() == std::io::ErrorKind::TimedOut),
            "{error:?}"
        );
        server.join().unwrap();
        // Measured from the CNXN, after key loading; the old wait was 120 s.
        let held = held_rx.recv().unwrap();
        assert!(held < Duration::from_secs(3), "held for {held:?}");
    }
}
