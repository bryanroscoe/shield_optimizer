// Derived in part from Kadb (Apache-2.0): core/AdbReader.kt, core/AdbWriter.kt,
// tls/TlsErrorMapper.kt, transport/PlainBlockingChannel.kt and
// transport/TcpKeepAlive.android.kt.

use rustls::{
    AlertDescription, ClientConfig, ClientConnection, SignatureScheme, StreamOwned,
    client::{
        ResolvesClientCert,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    pki_types::PrivateKeyDer,
    sign::CertifiedKey,
};
use socket2::{SockRef, TcpKeepalive};

use crate::{
    Result, RustADBError,
    adb_transport::ADBTransport,
    message_devices::{
        adb_connection_info::{ConnectionInfo, MAX_PAYLOAD},
        adb_message_transport::ADBMessageTransport,
        adb_transport_message::{ADBTransportMessage, ADBTransportMessageHeader},
        tcp::tls_identity::client_identity,
    },
};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

// A powered-off TV blackholes SYNs; without this the OS SYN timeout (over a
// minute) is the user's wait.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

// Kadb's optional keepalive policy is 1 s idle, 1 s interval, 10 probes. A
// phone does not need probes every second on an idle socket; this still
// notices a TV that vanished (powered off, roamed off the network) about 30 s
// into a silent read, long before the 120 s read bound would.
const KEEPALIVE_IDLE: Duration = Duration::from_secs(15);
#[cfg(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    target_os = "ios",
    target_os = "windows"
))]
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    target_os = "ios"
))]
const KEEPALIVE_PROBES: u32 = 3;

#[derive(Debug)]
enum CurrentConnection {
    Tcp(TcpStream),
    Tls(Box<TlsConnection>),
}

#[derive(Debug)]
struct TlsConnection {
    stream: StreamOwned<ClientConnection, TcpStream>,
    /// The device asked for our client certificate and we sent it.
    presented_certificate: bool,
    /// Nothing has arrived over TLS yet. A TLS 1.3 client finishes its side of
    /// the handshake before the server has checked the client certificate, so
    /// adbd's refusal of an unpaired key arrives on this first read.
    awaiting_first_message: bool,
}

impl CurrentConnection {
    fn socket(&self) -> &TcpStream {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream,
            Self::Tls(tls) => &tls.stream.sock,
        }
    }

    fn set_read_timeout(&self, read_timeout: Duration) -> Result<()> {
        Ok(self.socket().set_read_timeout(Some(read_timeout))?)
    }

    fn set_write_timeout(&self, write_timeout: Duration) -> Result<()> {
        Ok(self.socket().set_write_timeout(Some(write_timeout))?)
    }
}

impl Read for CurrentConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.read(buf),
            Self::Tls(tls) => tls.stream.read(buf),
        }
    }
}

impl Write for CurrentConnection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.write(buf),
            Self::Tls(tls) => tls.stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.flush(),
            Self::Tls(tls) => tls.stream.flush(),
        }
    }
}

/// Per-connection limits from CNXN, shared by every clone of the transport
/// (sessions hold clones) without taking the connection lock.
#[derive(Debug)]
struct WireLimits {
    max_payload: AtomicU32,
    verify_checksums: AtomicBool,
}

impl WireLimits {
    const fn unnegotiated() -> Self {
        Self {
            max_payload: AtomicU32::new(MAX_PAYLOAD),
            verify_checksums: AtomicBool::new(true),
        }
    }

    fn reset(&self) {
        self.max_payload.store(MAX_PAYLOAD, Ordering::Relaxed);
        self.verify_checksums.store(true, Ordering::Relaxed);
    }

    fn apply(&self, info: &ConnectionInfo) {
        let max_payload = u32::try_from(info.max_payload()).unwrap_or(MAX_PAYLOAD);
        self.max_payload
            .store(max_payload.min(MAX_PAYLOAD), Ordering::Relaxed);
        self.verify_checksums
            .store(info.verifies_checksums(), Ordering::Relaxed);
    }

    fn max_payload(&self) -> u32 {
        self.max_payload.load(Ordering::Relaxed)
    }

    fn verify_checksums(&self) -> bool {
        self.verify_checksums.load(Ordering::Relaxed)
    }
}

/// Transport running on TCP
#[derive(Clone, Debug)]
pub struct TcpTransport {
    address: SocketAddr,
    current_connection: Option<Arc<Mutex<CurrentConnection>>>,
    private_key_path: PathBuf,
    limits: Arc<WireLimits>,
}

impl TcpTransport {
    /// Instantiate a new [`TcpTransport`] using a given private key
    pub fn new<A: Into<SocketAddr>, P: AsRef<Path>>(address: A, private_key_path: P) -> Self {
        Self {
            address: address.into(),
            current_connection: None,
            private_key_path: private_key_path.as_ref().to_path_buf(),
            limits: Arc::new(WireLimits::unnegotiated()),
        }
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

    fn tls_config(&self, presented: Arc<AtomicBool>) -> Result<ClientConfig> {
        let (chain, key) = client_identity(&self.private_key_path)?;
        let signing_key =
            rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(key))?;
        let resolver = PresentedCertificate {
            key: Arc::new(CertifiedKey::new(chain, signing_key)),
            presented,
        };
        Ok(ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification {}))
            .with_client_cert_resolver(Arc::new(resolver)))
    }
}

/// Kadb sets `TCP_NODELAY` and `SO_KEEPALIVE` on every ADB socket. Both are
/// best effort: a platform that refuses one keeps a working connection.
fn configure_socket(stream: &TcpStream) {
    // ADB traffic is small request/response frames; Nagle only adds latency.
    if let Err(error) = stream.set_nodelay(true) {
        log::debug!("could not set TCP_NODELAY: {error}");
    }
    let keepalive = TcpKeepalive::new().with_time(KEEPALIVE_IDLE);
    #[cfg(any(
        target_os = "android",
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "windows"
    ))]
    let keepalive = keepalive.with_interval(KEEPALIVE_INTERVAL);
    #[cfg(any(
        target_os = "android",
        target_os = "linux",
        target_os = "macos",
        target_os = "ios"
    ))]
    let keepalive = keepalive.with_retries(KEEPALIVE_PROBES);
    if let Err(error) = SockRef::from(stream).set_tcp_keepalive(&keepalive) {
        log::debug!("could not configure TCP keepalive: {error}");
    }
}

/// Read one message. The header is validated before its payload is read, so
/// a corrupt length cannot drive the allocation.
fn read_frame<R: Read>(
    reader: &mut R,
    max_payload: u32,
    verify_checksums: bool,
) -> Result<ADBTransportMessage> {
    let mut header_bytes = [0_u8; 24];
    reader.read_exact(&mut header_bytes)?;
    let header = ADBTransportMessageHeader::try_from(header_bytes)?;
    header.check_payload_length(max_payload)?;
    if header.data_length() == 0 {
        return Ok(ADBTransportMessage::from_header_and_payload(
            header,
            Vec::new(),
        ));
    }

    let mut payload = vec![0_u8; header.data_length() as usize];
    reader.read_exact(&mut payload)?;
    let message = ADBTransportMessage::from_header_and_payload(header, payload);
    if verify_checksums && !message.check_message_integrity() {
        return Err(RustADBError::InvalidIntegrity(
            ADBTransportMessageHeader::compute_crc32(message.payload()),
            message.header().data_crc32(),
        ));
    }
    Ok(message)
}

/// Header and payload leave in one `write_all`, which also turns a socket that
/// stops accepting bytes into `WriteZero` instead of a busy loop.
fn write_frame<W: Write>(writer: &mut W, frame: &[u8]) -> io::Result<()> {
    writer.write_all(frame)?;
    writer.flush()
}

fn handshake_timeout() -> RustADBError {
    RustADBError::IOError(io::Error::new(
        io::ErrorKind::TimedOut,
        "TLS handshake timed out",
    ))
}

/// Drive the TLS handshake to completion, and flush our final flight, before
/// `deadline`. Each socket operation gets only the time that remains.
fn complete_handshake(
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
    deadline: Instant,
    presented: &AtomicBool,
) -> Result<()> {
    while stream.conn.is_handshaking() || stream.conn.wants_write() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(handshake_timeout());
        }
        stream.sock.set_read_timeout(Some(remaining))?;
        stream.sock.set_write_timeout(Some(remaining))?;
        match stream.conn.complete_io(&mut stream.sock) {
            Ok((0, 0)) => {
                return Err(tls_rejection(
                    RustADBError::IOError(io::ErrorKind::UnexpectedEof.into()),
                    presented.load(Ordering::SeqCst),
                ));
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(handshake_timeout());
            }
            Err(error) => {
                return Err(tls_rejection(
                    RustADBError::IOError(error),
                    presented.load(Ordering::SeqCst),
                ));
            }
        }
    }
    Ok(())
}

/// The alerts a TLS server sends when it does not trust the client
/// certificate. adbd's custom verifier fails an unknown key, which `BoringSSL`
/// reports as `certificate_unknown`; the others cover other TLS stacks and a
/// client that offered no certificate.
const fn is_untrusted_key_alert(alert: AlertDescription) -> bool {
    matches!(
        alert,
        AlertDescription::CertificateRequired
            | AlertDescription::UnknownCA
            | AlertDescription::AccessDenied
            | AlertDescription::CertificateUnknown
            | AlertDescription::BadCertificate
    )
}

/// Map a failure during the TLS handshake, or on the first read after it, the
/// way Kadb's `TlsErrorMapper` does: a refusal of our certificate becomes
/// [`RustADBError::TlsAuthRejected`]. So does the device hanging up once it
/// has our certificate, which is how some adbd builds refuse an unpaired key.
fn tls_rejection(error: RustADBError, presented_certificate: bool) -> RustADBError {
    let RustADBError::IOError(io_error) = error else {
        return error;
    };
    let kind = io_error.kind();
    match io_error.downcast::<rustls::Error>() {
        Ok(rustls::Error::AlertReceived(alert)) if is_untrusted_key_alert(alert) => {
            RustADBError::TlsAuthRejected(format!("TLS alert {alert:?}"))
        }
        Ok(tls_error) => RustADBError::TLSError(tls_error),
        Err(_)
            if presented_certificate
                && matches!(
                    kind,
                    io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::BrokenPipe
                ) =>
        {
            RustADBError::TlsAuthRejected(
                "the device closed the connection after receiving our certificate".to_string(),
            )
        }
        Err(io_error) => RustADBError::IOError(io_error),
    }
}

impl ADBTransport for TcpTransport {
    fn connect(&mut self) -> Result<()> {
        let stream = TcpStream::connect_timeout(&self.address, CONNECT_TIMEOUT)?;
        configure_socket(&stream);
        self.limits.reset();
        self.current_connection = Some(Arc::new(Mutex::new(CurrentConnection::Tcp(stream))));
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
                CurrentConnection::Tls(tls) => {
                    tls.stream.conn.send_close_notify();
                    let _ = tls.stream.sock.shutdown(Shutdown::Both);
                }
            }
        }

        Ok(())
    }
}

impl ADBMessageTransport for TcpTransport {
    fn apply_connection_info(&mut self, info: &ConnectionInfo) {
        self.limits.apply(info);
    }

    fn max_payload(&self) -> usize {
        self.limits.max_payload() as usize
    }

    fn read_message_with_timeout(
        &mut self,
        read_timeout: std::time::Duration,
    ) -> Result<ADBTransportMessage> {
        let raw_connection_lock = self.get_current_connection()?;
        let mut raw_connection = raw_connection_lock.lock()?;

        raw_connection.set_read_timeout(read_timeout)?;
        let result = read_frame(
            &mut *raw_connection,
            self.limits.max_payload(),
            self.limits.verify_checksums(),
        );

        if let CurrentConnection::Tls(tls) = &mut *raw_connection
            && tls.awaiting_first_message
        {
            return match result {
                Ok(message) => {
                    tls.awaiting_first_message = false;
                    Ok(message)
                }
                Err(error) => Err(tls_rejection(error, tls.presented_certificate)),
            };
        }
        result
    }

    fn write_message_with_timeout(
        &mut self,
        message: ADBTransportMessage,
        write_timeout: Duration,
    ) -> Result<()> {
        // adbd drops the whole connection on a packet over its limit, so an
        // oversized payload is refused here while the connection is intact.
        let max_payload = self.max_payload();
        if message.payload().len() > max_payload {
            return Err(RustADBError::PayloadTooLarge(
                message.payload().len(),
                max_payload,
            ));
        }
        let mut frame = message.header().as_bytes();
        frame.extend_from_slice(message.payload());

        let raw_connection_lock = self.get_current_connection()?;
        let mut raw_connection = raw_connection_lock.lock()?;
        raw_connection.set_write_timeout(write_timeout)?;
        write_frame(&mut *raw_connection, &frame)?;
        Ok(())
    }

    fn upgrade_connection(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let Some(current_connection) = self.current_connection.clone() else {
            return Err(RustADBError::UpgradeError(
                "cannot upgrade a non-existing connection...".into(),
            ));
        };

        let mut current_conn_locked = current_connection.lock()?;
        let tcp_stream = match &*current_conn_locked {
            CurrentConnection::Tcp(tcp_stream) => tcp_stream.try_clone()?,
            CurrentConnection::Tls(_) => {
                return Err(RustADBError::UpgradeError(
                    "cannot upgrade a TLS connection...".into(),
                ));
            }
        };

        let presented = Arc::new(AtomicBool::new(false));
        let config = self.tls_config(Arc::clone(&presented))?;
        let conn = ClientConnection::new(Arc::new(config), self.address.ip().into())?;
        let mut stream = StreamOwned::new(conn, tcp_stream);
        // Driving the handshake here keeps it inside the caller's handshake
        // deadline; left to the first read it would run under that read's
        // much longer timeout.
        complete_handshake(&mut stream, deadline, &presented)?;
        log::debug!("TLS handshake complete");

        *current_conn_locked = CurrentConnection::Tls(Box::new(TlsConnection {
            stream,
            presented_certificate: presented.load(Ordering::SeqCst),
            awaiting_first_message: true,
        }));
        Ok(())
    }
}

/// Presents the one ADB identity whenever the server asks, and records that
/// it asked, so a hang-up afterwards can be read as a refusal of the key.
#[derive(Debug)]
struct PresentedCertificate {
    key: Arc<CertifiedKey>,
    presented: Arc<AtomicBool>,
}

impl ResolvesClientCert for PresentedCertificate {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        self.presented.store(true, Ordering::SeqCst);
        Some(Arc::clone(&self.key))
    }

    fn has_certs(&self) -> bool {
        true
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
    use crate::message_devices::message_commands::MessageCommand;
    use std::io::Cursor;

    fn assert_unexpected_eof(result: Result<ADBTransportMessage>) {
        match result {
            Err(RustADBError::IOError(error)) => {
                assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
            }
            other => panic!("expected UnexpectedEof, got {other:?}"),
        }
    }

    fn frame(message: &ADBTransportMessage) -> Vec<u8> {
        let mut bytes = message.header().as_bytes();
        bytes.extend_from_slice(message.payload());
        bytes
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

    #[test]
    fn oversized_length_is_rejected_before_reading_the_payload() {
        let mut header = ADBTransportMessage::try_new(MessageCommand::Write, 1, 2, &[])
            .unwrap()
            .header()
            .as_bytes();
        header[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        // Only the header is available: a reader that trusted the length would
        // try to allocate 4 GiB and then hit EOF.
        let error = read_frame(&mut Cursor::new(header), MAX_PAYLOAD, true).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn payload_at_the_limit_is_accepted_and_one_byte_over_is_not() {
        let message = ADBTransportMessage::try_new(MessageCommand::Write, 1, 2, &[7; 64]).unwrap();
        let bytes = frame(&message);
        let read = read_frame(&mut Cursor::new(bytes.clone()), 64, true).unwrap();
        assert_eq!(read.payload(), &vec![7; 64]);
        let error = read_frame(&mut Cursor::new(bytes), 63, true).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn zero_length_message_with_bad_magic_is_rejected() {
        let mut bytes = ADBTransportMessage::try_new(MessageCommand::Okay, 1, 2, &[])
            .unwrap()
            .header()
            .as_bytes();
        bytes[20] ^= 0x01;
        let error = read_frame(&mut Cursor::new(bytes), MAX_PAYLOAD, true).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn checksum_is_verified_only_when_the_protocol_version_requires_it() {
        let message = ADBTransportMessage::try_new(MessageCommand::Write, 1, 2, b"abc").unwrap();
        let mut bytes = frame(&message);
        bytes[16..20].copy_from_slice(&0_u32.to_le_bytes());
        let error = read_frame(&mut Cursor::new(bytes.clone()), MAX_PAYLOAD, true).unwrap_err();
        assert!(
            matches!(error, RustADBError::InvalidIntegrity(..)),
            "{error:?}"
        );
        let read = read_frame(&mut Cursor::new(bytes), MAX_PAYLOAD, false).unwrap();
        assert_eq!(read.payload(), b"abc");
    }

    struct ZeroWriter {
        calls: usize,
    }

    impl Write for ZeroWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            Ok(0)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_socket_that_accepts_nothing_is_write_zero_not_a_loop() {
        let mut writer = ZeroWriter { calls: 0 };
        let error = write_frame(&mut writer, b"header and payload").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(writer.calls, 1);
    }

    #[test]
    fn oversized_outgoing_payload_is_refused_before_writing() {
        let mut transport = TcpTransport::new(
            SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 9)),
            PathBuf::new(),
        );
        let info = ConnectionInfo::negotiate(0x0100_0000, 4096, b"device::").unwrap();
        transport.apply_connection_info(&info);
        assert_eq!(transport.max_payload(), 4096);
        let message =
            ADBTransportMessage::try_new(MessageCommand::Write, 1, 2, &[0; 4097]).unwrap();
        // The transport was never connected, so reaching the socket would be
        // NotConnected; the size check comes first.
        let error = transport
            .write_message_with_timeout(message, Duration::from_secs(1))
            .unwrap_err();
        assert!(
            matches!(error, RustADBError::PayloadTooLarge(4097, 4096)),
            "{error:?}"
        );
    }

    #[test]
    fn every_socket_gets_nodelay_and_keepalive() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let mut transport = TcpTransport::new(address, PathBuf::new());
        transport.connect().unwrap();
        let (_peer, _) = listener.accept().unwrap();

        let connection = transport.get_current_connection().unwrap();
        let connection = connection.lock().unwrap();
        let socket = connection.socket();
        assert!(socket.nodelay().unwrap());
        let socket = SockRef::from(socket);
        assert!(socket.keepalive().unwrap());
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
        {
            assert_eq!(socket.tcp_keepalive_time().unwrap(), KEEPALIVE_IDLE);
            assert_eq!(socket.tcp_keepalive_interval().unwrap(), KEEPALIVE_INTERVAL);
            assert_eq!(socket.tcp_keepalive_retries().unwrap(), KEEPALIVE_PROBES);
        }
    }

    fn io_error_with(error: rustls::Error) -> RustADBError {
        RustADBError::IOError(io::Error::new(io::ErrorKind::InvalidData, error))
    }

    #[test]
    fn untrusted_key_alerts_map_to_tls_auth_rejected() {
        for alert in [
            AlertDescription::CertificateRequired,
            AlertDescription::UnknownCA,
            AlertDescription::AccessDenied,
            AlertDescription::CertificateUnknown,
            AlertDescription::BadCertificate,
        ] {
            let mapped = tls_rejection(io_error_with(rustls::Error::AlertReceived(alert)), false);
            assert!(
                matches!(mapped, RustADBError::TlsAuthRejected(_)),
                "{alert:?}: {mapped:?}"
            );
        }
    }

    #[test]
    fn other_tls_failures_keep_their_own_error() {
        let mapped = tls_rejection(
            io_error_with(rustls::Error::AlertReceived(
                AlertDescription::HandshakeFailure,
            )),
            true,
        );
        assert!(matches!(mapped, RustADBError::TLSError(_)), "{mapped:?}");

        let mapped = tls_rejection(RustADBError::IOError(io::ErrorKind::TimedOut.into()), true);
        assert!(matches!(mapped, RustADBError::IOError(_)), "{mapped:?}");
    }

    #[test]
    fn a_hang_up_counts_as_rejection_only_after_our_certificate_went_out() {
        let eof = || RustADBError::IOError(io::ErrorKind::UnexpectedEof.into());
        assert!(matches!(
            tls_rejection(eof(), true),
            RustADBError::TlsAuthRejected(_)
        ));
        assert!(matches!(
            tls_rejection(eof(), false),
            RustADBError::IOError(_)
        ));
        let reset = RustADBError::IOError(io::ErrorKind::ConnectionReset.into());
        assert!(matches!(
            tls_rejection(reset, true),
            RustADBError::TlsAuthRejected(_)
        ));
    }
}
