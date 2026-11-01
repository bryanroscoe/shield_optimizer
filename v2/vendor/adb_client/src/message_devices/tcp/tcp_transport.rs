use super::bounded_shell::BoundedShellControl;
use rcgen::{CertificateParams, KeyPair, PKCS_RSA_SHA256};
use rustls::{
    ClientConfig, ClientConnection, SignatureScheme, StreamOwned,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, pem::PemObject},
};

use crate::{
    Result, RustADBError,
    adb_transport::ADBTransport,
    message_devices::{
        adb_message_transport::ADBMessageTransport,
        adb_transport_message::{ADBTransportMessage, ADBTransportMessageHeader},
        message_commands::MessageCommand,
    },
};
use std::{
    fs::read_to_string,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

// Keep cancellation and I/O on the same socket handle, including after TLS upgrade.
#[derive(Clone, Debug)]
struct SharedSocket(Arc<TcpStream>);

impl std::ops::Deref for SharedSocket {
    type Target = TcpStream;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Read for SharedSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl Write for SharedSocket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        (&*self.0).flush()
    }
}

#[derive(Debug)]
enum CurrentConnection {
    Tcp(SharedSocket),
    Tls(Box<StreamOwned<ClientConnection, SharedSocket>>),
}

impl CurrentConnection {
    fn read_exact_controlled(
        &mut self,
        mut buf: &mut [u8],
        timeout: Duration,
        control: Option<&BoundedShellControl>,
    ) -> Result<()> {
        let Some(control) = control else {
            self.set_read_timeout(timeout)?;
            self.read_exact(buf)?;
            return Ok(());
        };
        let started = std::time::Instant::now();
        while !buf.is_empty() {
            let remaining = control.remaining()?.min(timeout.saturating_sub(started.elapsed()));
            if remaining.is_zero() {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "ADB read timed out").into());
            }
            // shutdown does not reliably wake an already-blocked Winsock recv.
            self.set_read_timeout(remaining.min(Duration::from_millis(50)))?;
            let result = self.read(buf);
            control.remaining()?;
            match result {
                Ok(0) => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "ADB stream ended mid-frame").into()),
                Ok(count) => buf = &mut buf[count..],
                Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {},
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn set_read_timeout(&self, read_timeout: Duration) -> Result<()> {
        match self {
            Self::Tcp(tcp_stream) => Ok(tcp_stream.set_read_timeout(Some(read_timeout))?),
            Self::Tls(stream_owned) => {
                Ok(stream_owned.sock.set_read_timeout(Some(read_timeout))?)
            }
        }
    }

    fn set_write_timeout(&self, write_timeout: Duration) -> Result<()> {
        match self {
            Self::Tcp(tcp_stream) => Ok(tcp_stream.set_write_timeout(Some(write_timeout))?),
            Self::Tls(stream_owned) => {
                Ok(stream_owned.sock.set_write_timeout(Some(write_timeout))?)
            }
        }
    }
}

impl Read for CurrentConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.read(buf),
            Self::Tls(tls_conn) => tls_conn.read(buf),
        }
    }
}

impl Write for CurrentConnection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.write(buf),
            Self::Tls(tls_conn) => tls_conn.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.flush(),
            Self::Tls(tls_conn) => tls_conn.flush(),
        }
    }
}

/// Transport running on USB
#[derive(Clone, Debug)]
pub struct TcpTransport {
    address: SocketAddr,
    current_connection: Option<Arc<Mutex<CurrentConnection>>>,
    private_key_path: PathBuf,
    shell_control: Option<BoundedShellControl>,
}

fn certificate_from_pk(key_pair: &KeyPair) -> Result<Vec<CertificateDer<'static>>> {
    let certificate_params = CertificateParams::default();
    let certificate = certificate_params.self_signed(key_pair)?;
    Ok(vec![certificate.der().to_owned()])
}

impl TcpTransport {
    /// Instantiate a new [`TcpTransport`] using a given private key
    pub fn new<A: Into<SocketAddr>, P: AsRef<Path>>(address: A, private_key_path: P) -> Self {
        Self {
            address: address.into(),
            current_connection: None,
            private_key_path: private_key_path.as_ref().to_path_buf(),
            shell_control: None,
        }
    }

    pub(crate) fn with_shell_control(
        address: SocketAddr,
        key: &Path,
        control: BoundedShellControl,
    ) -> Self {
        let mut transport = Self::new(address, key);
        transport.shell_control = Some(control);
        transport
    }

    pub(crate) fn set_current_control(
        &mut self,
        control: BoundedShellControl,
    ) -> Result<Option<BoundedShellControl>> {
        let connection = self.get_current_connection()?;
        let guard = connection.lock()?;
        match &*guard {
            CurrentConnection::Tcp(socket) => control.register_borrowed(&socket.0)?,
            CurrentConnection::Tls(tls) => control.register_borrowed(&tls.sock.0)?,
        }
        Ok(self.shell_control.replace(control))
    }

    pub(crate) fn restore_control(&mut self, previous: Option<BoundedShellControl>) {
        self.shell_control = previous;
    }

    fn get_current_connection(&self) -> Result<Arc<Mutex<CurrentConnection>>> {
        self.current_connection
            .as_ref()
            .ok_or(RustADBError::IOError(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "not connected",
            )))
            .cloned()
    }
}

impl ADBTransport for TcpTransport {
    fn connect(&mut self) -> Result<()> {
        // A powered-off TV blackholes SYNs; without this the OS SYN timeout
        // (over a minute) is the user's wait.
        let timeout = self
            .shell_control
            .as_ref()
            .map(BoundedShellControl::remaining)
            .transpose()?
            .unwrap_or(Duration::from_secs(5))
            .min(Duration::from_secs(5));
        let stream = Arc::new(TcpStream::connect_timeout(&self.address, timeout)?);
        // ADB exchanges small header/payload frames; do not stall them behind delayed ACKs.
        stream.set_nodelay(true)?;
        if let Some(control) = &self.shell_control {
            control.register(&stream)?;
        }
        self.current_connection = Some(Arc::new(Mutex::new(CurrentConnection::Tcp(SharedSocket(
            stream,
        )))));
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        log::debug!("disconnecting...");
        if let Some(current_connection) = &self.current_connection {
            let mut lock = current_connection.lock()?;
            match &mut *lock {
                CurrentConnection::Tcp(tcp_stream) => {
                    let _ = tcp_stream.shutdown(Shutdown::Both);
                }
                CurrentConnection::Tls(tls_conn) => {
                    tls_conn.conn.send_close_notify();
                    let _ = tls_conn.sock.shutdown(Shutdown::Both);
                }
            }
        }

        Ok(())
    }
}

impl ADBMessageTransport for TcpTransport {
    fn read_message_with_timeout(
        &mut self,
        read_timeout: std::time::Duration,
    ) -> Result<ADBTransportMessage> {
        let raw_connection_lock = self.get_current_connection()?;
        let mut raw_connection = raw_connection_lock.lock()?;

        let read_timeout = if let Some(control) = &self.shell_control {
            control.remaining()?.min(read_timeout)
        } else {
            read_timeout
        };
        let read_started = std::time::Instant::now();
        let mut data = [0; 24];
        raw_connection.read_exact_controlled(&mut data, read_timeout, self.shell_control.as_ref())?;

        let header = ADBTransportMessageHeader::try_from(data)?;

        if header.data_length() > 1_048_576 {
            return Err(RustADBError::ADBRequestFailed(
                "ADB payload exceeds negotiated maximum".into(),
            ));
        }
        if header.data_length() != 0 {
            let mut msg_data = vec![0_u8; header.data_length() as usize];
            let payload_timeout = if self.shell_control.is_some() {
                read_timeout.saturating_sub(read_started.elapsed())
            } else {
                read_timeout
            };
            raw_connection.read_exact_controlled(&mut msg_data, payload_timeout, self.shell_control.as_ref())?;
            // raw_connection is not used anymore, let's drop it
            drop(raw_connection);

            let message = ADBTransportMessage::from_header_and_payload(header, msg_data);

            // Check message integrity
            if !message.check_message_integrity() {
                return Err(RustADBError::InvalidIntegrity(
                    ADBTransportMessageHeader::compute_crc32(message.payload()),
                    message.header().data_crc32(),
                ));
            }

            return Ok(message);
        }

        Ok(ADBTransportMessage::from_header_and_payload(header, vec![]))
    }

    fn write_message_with_timeout(
        &mut self,
        message: ADBTransportMessage,
        write_timeout: Duration,
    ) -> Result<()> {
        let message_bytes = message.header().as_bytes();
        let raw_connection_lock = self.get_current_connection()?;
        let mut raw_connection = raw_connection_lock.lock()?;
        let write_timeout = if let Some(control) = &self.shell_control {
            control.remaining()?.min(write_timeout)
        } else {
            write_timeout
        };
        raw_connection.set_write_timeout(write_timeout)?;

        let mut total_written = 0;
        loop {
            total_written += raw_connection.write(&message_bytes[total_written..])?;
            if total_written == message_bytes.len() {
                raw_connection.flush()?;
                break;
            }
        }

        let payload = message.into_payload();
        if !payload.is_empty() {
            let mut total_written = 0;
            loop {
                total_written += raw_connection.write(&payload[total_written..])?;
                if total_written == payload.len() {
                    raw_connection.flush()?;
                    drop(raw_connection);
                    break;
                }
            }
        }

        Ok(())
    }

    fn upgrade_connection(&mut self) -> Result<()> {
        let Some(current_connection) = self.current_connection.clone() else {
            return Err(RustADBError::UpgradeError(
                "cannot upgrade a non-existing connection...".into(),
            ));
        };

        {
            let mut current_conn_locked = current_connection.lock()?;
            match &*current_conn_locked {
                CurrentConnection::Tcp(tcp_stream) => {
                    // TODO: Check if we cannot be more precise
                    let pk_content = read_to_string(&self.private_key_path)?;

                    let key_pair =
                        KeyPair::from_pkcs8_pem_and_sign_algo(&pk_content, &PKCS_RSA_SHA256)?;

                    let certificate = certificate_from_pk(&key_pair)?;
                    let private_key = PrivatePkcs8KeyDer::from_pem_file(&self.private_key_path)?;

                    let client_config = ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification {}))
                        .with_client_auth_cert(certificate, private_key.into())?;

                    let rc_config = Arc::new(client_config);
                    let server_name = self.address.ip().into();
                    let conn = ClientConnection::new(rc_config, server_name)?;
                    let owned = tcp_stream.clone();
                    let client = StreamOwned::new(conn, owned);

                    // Update current connection state to now use TLS protocol
                    *current_conn_locked = CurrentConnection::Tls(Box::new(client));
                    drop(current_conn_locked);
                }
                CurrentConnection::Tls(_) => {
                    return Err(RustADBError::UpgradeError(
                        "cannot upgrade a TLS connection...".into(),
                    ));
                }
            }
        }

        let message = self.read_message()?;
        match message.header().command() {
            MessageCommand::Cnxn => {
                let device_infos = String::from_utf8(message.into_payload())?;
                log::debug!("received device info: {device_infos}");
                Ok(())
            }
            c => Err(RustADBError::ADBRequestFailed(format!(
                "Wrong command received {c}"
            ))),
        }
    }
}

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_read_cancels_even_without_a_socket_shutdown_handle() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"part").unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let mut connection = CurrentConnection::Tcp(SharedSocket(Arc::new(TcpStream::connect(address).unwrap())));
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let signal = control.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            signal.cancel();
        });
        let started = std::time::Instant::now();
        let result = connection.read_exact_controlled(&mut [0; 24], Duration::from_secs(2), Some(&control));
        let elapsed = started.elapsed();
        canceller.join().unwrap();
        server.join().unwrap();
        assert!(matches!(result, Err(RustADBError::IOError(error)) if error.kind() == std::io::ErrorKind::ConnectionAborted));
        assert!(elapsed < Duration::from_millis(250), "{elapsed:?}");
    }

    #[test]
    fn controlled_read_retains_partial_bytes_across_poll_timeouts() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"ab").unwrap();
            std::thread::sleep(Duration::from_millis(120));
            stream.write_all(b"cd").unwrap();
        });
        let mut connection = CurrentConnection::Tcp(SharedSocket(Arc::new(TcpStream::connect(address).unwrap())));
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let mut bytes = [0; 4];
        connection.read_exact_controlled(&mut bytes, Duration::from_secs(1), Some(&control)).unwrap();
        control.cancel();
        server.join().unwrap();
        assert_eq!(&bytes, b"abcd");
    }

    #[test]
    fn controlled_transport_retains_fragmented_header_and_payload() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).unwrap();
            let message = ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, b"hello").unwrap();
            let header = message.header().as_bytes();
            stream.write_all(&header[..7]).unwrap();
            std::thread::sleep(Duration::from_millis(80));
            stream.write_all(&header[7..]).unwrap();
            stream.write_all(b"he").unwrap();
            std::thread::sleep(Duration::from_millis(80));
            stream.write_all(b"llo").unwrap();
        });
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let mut transport = TcpTransport::with_shell_control(address, Path::new("unused"), control.clone());
        transport.connect().unwrap();
        let message = transport.read_message_with_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(message.payload(), b"hello");
        control.cancel();
        server.join().unwrap();
    }

    #[test]
    fn controlled_read_honors_shorter_caller_budget() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let mut connection = CurrentConnection::Tcp(SharedSocket(Arc::new(TcpStream::connect(address).unwrap())));
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let started = std::time::Instant::now();
        let result = connection.read_exact_controlled(&mut [0; 1], Duration::from_millis(20), Some(&control));
        let elapsed = started.elapsed();
        control.cancel();
        server.join().unwrap();
        assert!(matches!(result, Err(RustADBError::IOError(error)) if error.kind() == std::io::ErrorKind::TimedOut));
        assert!(elapsed < Duration::from_millis(150), "{elapsed:?}");
    }

    fn assert_unexpected_eof(result: Result<ADBTransportMessage>) {
        match result {
            Err(RustADBError::IOError(error)) => {
                assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
            }
            other => panic!("expected UnexpectedEof, got {other:?}"),
        }
    }

    #[test]
    fn physical_eof_while_reading_header_returns_error() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
        });
        let mut transport = TcpTransport::new(address, PathBuf::new());
        transport.connect().unwrap();

        assert_unexpected_eof(transport.read_message_with_timeout(Duration::from_secs(1)));
        server.join().unwrap();
    }

    #[test]
    fn physical_eof_while_reading_payload_returns_error() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let message =
                ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, b"hello").unwrap();
            stream.write_all(&message.header().as_bytes()).unwrap();
            stream.write_all(b"he").unwrap();
        });
        let mut transport = TcpTransport::new(address, PathBuf::new());
        transport.connect().unwrap();

        assert_unexpected_eof(transport.read_message_with_timeout(Duration::from_secs(1)));
        server.join().unwrap();
    }
}
