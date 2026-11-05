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
        adb_connection_info::{ConnectionInfo, MAX_PAYLOAD, MAX_PAYLOAD_V1},
        adb_message_transport::{ADBMessageTransport, is_hang_up, is_timeout},
        adb_transport_message::{ADBTransportMessage, ADBTransportMessageHeader},
        tcp::{
            bounded_shell::{BoundedShellControl, Finish},
            tls_identity::client_identity,
        },
    },
};
use std::{
    io::{self, IoSlice, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
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

// Once the first byte of a message has arrived the rest is already in flight,
// so it gets at least this long even when the caller's wait for a message was
// much shorter. Giving up part-way would leave the stream out of step.
const FRAME_COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

/// Under a [`BoundedShellControl`], no single socket wait is longer than
/// this. Shutting a socket down does not reliably wake a `recv` already
/// blocked in Winsock, so a cancel or the total deadline is also noticed by
/// polling.
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The connection's one socket, shared by the plain and TLS streams and by
/// any [`BoundedShellControl`] that may shut it down, so cancellation always
/// reaches the handle the I/O runs on, including after the TLS upgrade.
#[derive(Clone, Debug)]
struct SharedSocket(Arc<TcpStream>);

impl std::ops::Deref for SharedSocket {
    type Target = TcpStream;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Read for SharedSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl Write for SharedSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        (&*self.0).write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

#[derive(Debug)]
enum CurrentConnection {
    Tcp(SharedSocket),
    Tls(Box<TlsConnection>),
}

#[derive(Debug)]
struct TlsConnection {
    stream: StreamOwned<ClientConnection, SharedSocket>,
    /// Nothing has arrived over TLS yet. A TLS 1.3 client finishes its side of
    /// the handshake before the server has checked the client certificate, so
    /// adbd's refusal of an unpaired key arrives on this first read.
    awaiting_first_message: bool,
}

impl CurrentConnection {
    fn shared_socket(&self) -> &Arc<TcpStream> {
        match self {
            Self::Tcp(socket) => &socket.0,
            Self::Tls(tls) => &tls.stream.sock.0,
        }
    }

    fn socket(&self) -> &TcpStream {
        self.shared_socket()
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

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.write_vectored(bufs),
            Self::Tls(tls) => tls.stream.write_vectored(bufs),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(tcp_stream) => tcp_stream.flush(),
            Self::Tls(tls) => tls.stream.flush(),
        }
    }
}

/// Per-connection state shared by every clone of the transport (sessions
/// hold clones) without taking the connection lock: the limits from CNXN, and
/// whether the byte stream has fallen out of step with ADB framing.
///
/// Until CNXN settles the limit, reads accept up to [`MAX_PAYLOAD`] (as
/// AOSP's host does) and writes stay within [`MAX_PAYLOAD_V1`], which every
/// adbd accepts. Afterwards both use the negotiated value.
#[derive(Debug)]
struct WireLimits {
    read_max: AtomicU32,
    write_max: AtomicUsize,
    verify_checksums: AtomicBool,
    /// A read or write stopped part-way through a message, or the peer sent
    /// a malformed one. Nothing after that point can be parsed reliably.
    poisoned: AtomicBool,
}

impl WireLimits {
    const fn unnegotiated() -> Self {
        Self {
            read_max: AtomicU32::new(MAX_PAYLOAD),
            write_max: AtomicUsize::new(MAX_PAYLOAD_V1),
            verify_checksums: AtomicBool::new(true),
            poisoned: AtomicBool::new(false),
        }
    }

    fn reset(&self) {
        self.read_max.store(MAX_PAYLOAD, Ordering::Relaxed);
        self.write_max.store(MAX_PAYLOAD_V1, Ordering::Relaxed);
        self.verify_checksums.store(true, Ordering::Relaxed);
        self.poisoned.store(false, Ordering::Relaxed);
    }

    fn apply(&self, info: &ConnectionInfo) {
        let max_payload = u32::try_from(info.max_payload())
            .unwrap_or(MAX_PAYLOAD)
            .min(MAX_PAYLOAD);
        self.read_max.store(max_payload, Ordering::Relaxed);
        self.write_max
            .store(max_payload as usize, Ordering::Relaxed);
        self.verify_checksums
            .store(info.verifies_checksums(), Ordering::Relaxed);
    }

    fn read_limit(&self) -> u32 {
        self.read_max.load(Ordering::Relaxed)
    }

    fn write_limit(&self) -> usize {
        self.write_max.load(Ordering::Relaxed)
    }

    fn verify_checksums(&self) -> bool {
        self.verify_checksums.load(Ordering::Relaxed)
    }

    fn poison(&self) {
        self.poisoned.store(true, Ordering::Relaxed);
    }

    fn check_in_step(&self) -> Result<()> {
        if self.poisoned.load(Ordering::Relaxed) {
            return Err(RustADBError::ProtocolError(
                "the connection lost ADB message framing after an interrupted read or write; \
                 reconnect"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

/// Transport running on TCP
#[derive(Clone, Debug)]
pub struct TcpTransport {
    address: SocketAddr,
    current_connection: Option<Arc<Mutex<CurrentConnection>>>,
    private_key_path: PathBuf,
    limits: Arc<WireLimits>,
    /// A total deadline and cancel handle over every socket operation while
    /// set. Sessions opened meanwhile hold clones of the transport, so they
    /// share it.
    control: Option<BoundedShellControl>,
}

impl TcpTransport {
    /// Instantiate a new [`TcpTransport`] using a given private key
    pub fn new<A: Into<SocketAddr>, P: AsRef<Path>>(address: A, private_key_path: P) -> Self {
        Self {
            address: address.into(),
            current_connection: None,
            private_key_path: private_key_path.as_ref().to_path_buf(),
            limits: Arc::new(WireLimits::unnegotiated()),
            control: None,
        }
    }

    /// A transport whose connect, handshake and every later read and write
    /// are bounded by `control`, which owns the socket: a cancel or the
    /// deadline shuts it down.
    pub(crate) fn with_shell_control(
        address: SocketAddr,
        private_key_path: &Path,
        control: BoundedShellControl,
    ) -> Self {
        let mut transport = Self::new(address, private_key_path);
        transport.control = Some(control);
        transport
    }

    /// Put the current connection under `control` for one operation. The
    /// socket is only borrowed: finishing normally leaves it open, while a
    /// cancel or the deadline shuts it down. Returns the control it replaces,
    /// for [`Self::restore_control`]. A control that is already cancelled or
    /// expired is refused without touching the socket.
    pub(crate) fn set_current_control(
        &mut self,
        control: BoundedShellControl,
    ) -> Result<Option<BoundedShellControl>> {
        let connection = self.get_current_connection()?;
        let guard = connection.lock()?;
        control.register_borrowed(guard.shared_socket())?;
        drop(guard);
        Ok(self.control.replace(control))
    }

    pub(crate) fn restore_control(&mut self, previous: Option<BoundedShellControl>) {
        self.control = previous;
    }

    /// Hand the connection made under the handshake control to its owner:
    /// the deadline stops applying and the socket stays open. Fails, and the
    /// caller must drop the connection, if that control was already
    /// cancelled or expired.
    pub(crate) fn finish_connect_control(&mut self) -> Result<()> {
        let control = self
            .control
            .as_ref()
            .ok_or_else(|| io::Error::other("connection has no active handshake control"))?;
        let finish = Finish(control.clone());
        finish.0.release_connection()?;
        self.control = None;
        Ok(())
    }

    /// The control's own error once it was cancelled or expired, which says
    /// more than the I/O failure the shutdown caused.
    fn control_failure(&self) -> Option<RustADBError> {
        self.control
            .as_ref()
            .and_then(|control| control.remaining().err())
            .map(RustADBError::IOError)
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

    fn tls_config(&self) -> Result<ClientConfig> {
        let (chain, key) = client_identity(&self.private_key_path)?;
        let signing_key =
            rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(key))?;
        let resolver = ClientCertificate {
            key: Arc::new(CertifiedKey::new(chain, signing_key)),
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

/// Why [`read_frame`] failed, and whether any of the frame had been consumed
/// by then. A failure with nothing consumed (a read timeout between messages)
/// leaves the stream in step; anything else does not.
#[derive(Debug)]
struct FrameError {
    error: RustADBError,
    consumed: bool,
}

impl FrameError {
    const fn mid_frame(error: RustADBError) -> Self {
        Self {
            error,
            consumed: true,
        }
    }
}

/// `read_exact` that reports how many bytes it consumed before failing.
fn read_tracked<R: Read>(reader: &mut R, buf: &mut [u8]) -> (usize, io::Result<()>) {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => return (filled, Err(io::ErrorKind::UnexpectedEof.into())),
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return (filled, Err(error)),
        }
    }
    (filled, Ok(()))
}

/// Read one message. The header is validated before its payload is read, so
/// a corrupt length cannot drive the allocation. `frame_started` runs once,
/// after the first byte arrives, so the caller can give the rest of the frame
/// a longer deadline than the wait for it to begin.
fn read_frame<R: Read>(
    reader: &mut R,
    max_payload: u32,
    verify_checksums: bool,
    frame_started: impl FnOnce(&mut R) -> io::Result<()>,
) -> std::result::Result<ADBTransportMessage, FrameError> {
    let mut header_bytes = [0_u8; 24];
    let (first, result) = read_tracked(reader, &mut header_bytes[..1]);
    if let Err(error) = result {
        return Err(FrameError {
            error: error.into(),
            consumed: first > 0,
        });
    }
    frame_started(reader).map_err(|error| FrameError::mid_frame(error.into()))?;
    let (_, result) = read_tracked(reader, &mut header_bytes[1..]);
    result.map_err(|error| FrameError::mid_frame(error.into()))?;

    let header =
        ADBTransportMessageHeader::try_from(header_bytes).map_err(FrameError::mid_frame)?;
    header
        .check_payload_length(max_payload)
        .map_err(FrameError::mid_frame)?;
    if header.data_length() == 0 {
        return Ok(ADBTransportMessage::from_header_and_payload(
            header,
            Vec::new(),
        ));
    }

    let mut payload = vec![0_u8; header.data_length() as usize];
    let (_, result) = read_tracked(reader, &mut payload);
    result.map_err(|error| FrameError::mid_frame(error.into()))?;
    let message = ADBTransportMessage::from_header_and_payload(header, payload);
    if verify_checksums && !message.check_message_integrity() {
        return Err(FrameError::mid_frame(RustADBError::InvalidIntegrity(
            ADBTransportMessageHeader::compute_crc32(message.payload()),
            message.header().data_crc32(),
        )));
    }
    Ok(message)
}

/// Header and payload leave together, gathered from their own buffers so a
/// large payload is never copied. Returns how many bytes the writer accepted
/// before an error, so the caller knows whether a partial frame reached the
/// stream. A writer that accepts nothing is `WriteZero` rather than a busy
/// loop.
fn write_frame<W: Write>(writer: &mut W, header: &[u8], payload: &[u8]) -> (usize, io::Result<()>) {
    let mut slices = [IoSlice::new(header), IoSlice::new(payload)];
    let mut remaining: &mut [IoSlice<'_>] = &mut slices;
    let mut written = 0;
    while !remaining.is_empty() {
        match writer.write_vectored(remaining) {
            Ok(0) => return (written, Err(io::ErrorKind::WriteZero.into())),
            Ok(accepted) => {
                written += accepted;
                IoSlice::advance_slices(&mut remaining, accepted);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return (written, Err(error)),
        }
    }
    (written, writer.flush())
}

/// Reads that notice a [`BoundedShellControl`]'s cancel or deadline within
/// [`CONTROL_POLL_INTERVAL`], and end at `until` like a socket timeout.
struct ControlledReader<'a> {
    connection: &'a mut CurrentConnection,
    control: &'a BoundedShellControl,
    until: Instant,
}

impl Read for ControlledReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let remaining = self
                .control
                .remaining()?
                .min(self.until.saturating_duration_since(Instant::now()));
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "ADB read timed out",
                ));
            }
            self.connection
                .socket()
                .set_read_timeout(Some(remaining.min(CONTROL_POLL_INTERVAL)))?;
            let result = self.connection.read(buf);
            self.control.remaining()?;
            match result {
                Err(error) if is_timeout(error.kind()) => {}
                other => return other,
            }
        }
    }
}

fn handshake_timeout() -> RustADBError {
    RustADBError::IOError(io::Error::new(
        io::ErrorKind::TimedOut,
        "TLS handshake timed out",
    ))
}

/// Drive the TLS handshake to completion, and flush our final flight, before
/// `deadline`. Each socket operation gets only the time that remains. Under a
/// `control`, waits are also sliced so its cancel or deadline ends the
/// handshake with the control's own error.
fn complete_handshake(
    stream: &mut StreamOwned<ClientConnection, SharedSocket>,
    deadline: Instant,
    control: Option<&BoundedShellControl>,
) -> Result<()> {
    while stream.conn.is_handshaking() || stream.conn.wants_write() {
        let mut remaining = deadline.saturating_duration_since(Instant::now());
        if let Some(control) = control {
            remaining = remaining.min(control.remaining()?);
        }
        if remaining.is_zero() {
            return Err(handshake_timeout());
        }
        let wait = if control.is_some() {
            remaining.min(CONTROL_POLL_INTERVAL)
        } else {
            remaining
        };
        stream.sock.set_read_timeout(Some(wait))?;
        stream.sock.set_write_timeout(Some(wait))?;
        let result = stream.conn.complete_io(&mut stream.sock);
        if let Some(control) = control {
            control.remaining()?;
        }
        match result {
            Ok((0, 0)) => {
                return Err(tls_failure(RustADBError::IOError(
                    io::ErrorKind::UnexpectedEof.into(),
                )));
            }
            Ok(_) => {}
            // Under a control this is one slice; the loop rechecks the time.
            Err(error) if is_timeout(error.kind()) && control.is_some() => {}
            Err(error) if is_timeout(error.kind()) => return Err(handshake_timeout()),
            Err(error) => return Err(tls_failure(RustADBError::IOError(error))),
        }
    }
    Ok(())
}

/// The alerts a TLS server sends when it does not trust the client
/// certificate. adbd's custom verifier fails an unknown key, which `BoringSSL`
/// reports as `certificate_unknown`; the others cover other TLS stacks and a
/// client that offered no certificate.
pub(crate) const fn is_untrusted_key_alert(alert: AlertDescription) -> bool {
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
/// way Kadb's `TlsErrorMapper` does: only an explicit TLS alert refusing our
/// certificate becomes [`RustADBError::TlsAuthRejected`]. adbd always asks
/// for a certificate, so a hang-up after it went out says nothing about
/// pairing: a Wi-Fi drop looks the same. That is
/// [`RustADBError::TlsConnectionClosed`].
fn tls_failure(error: RustADBError) -> RustADBError {
    let RustADBError::IOError(io_error) = error else {
        return error;
    };
    match io_error.downcast::<rustls::Error>() {
        Ok(rustls::Error::AlertReceived(alert)) if is_untrusted_key_alert(alert) => {
            RustADBError::TlsAuthRejected(format!("TLS alert {alert:?}"))
        }
        Ok(tls_error) => RustADBError::TLSError(tls_error),
        Err(io_error) if is_hang_up(io_error.kind()) => RustADBError::TlsConnectionClosed(io_error),
        Err(io_error) => RustADBError::IOError(io_error),
    }
}

impl ADBTransport for TcpTransport {
    fn connect(&mut self) -> Result<()> {
        let timeout = match &self.control {
            Some(control) => control.remaining()?.min(CONNECT_TIMEOUT),
            None => CONNECT_TIMEOUT,
        };
        let stream = Arc::new(TcpStream::connect_timeout(&self.address, timeout)?);
        configure_socket(&stream);
        if let Some(control) = &self.control {
            // From here a cancel or the deadline shuts this socket down.
            control.register(&stream)?;
        }
        self.limits.reset();
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
        self.limits.write_limit()
    }

    fn read_message_with_timeout(
        &mut self,
        read_timeout: std::time::Duration,
    ) -> Result<ADBTransportMessage> {
        self.limits.check_in_step()?;
        let raw_connection_lock = self.get_current_connection()?;
        let mut raw_connection = raw_connection_lock.lock()?;

        let result = if let Some(control) = &self.control {
            let read_timeout = control.remaining()?.min(read_timeout);
            let completion_timeout = read_timeout.max(FRAME_COMPLETION_TIMEOUT);
            let mut reader = ControlledReader {
                connection: &mut raw_connection,
                control,
                until: Instant::now() + read_timeout,
            };
            read_frame(
                &mut reader,
                self.limits.read_limit(),
                self.limits.verify_checksums(),
                |reader| {
                    reader.until = Instant::now() + completion_timeout;
                    Ok(())
                },
            )
        } else {
            raw_connection.set_read_timeout(read_timeout)?;
            let completion_timeout = read_timeout.max(FRAME_COMPLETION_TIMEOUT);
            read_frame(
                &mut *raw_connection,
                self.limits.read_limit(),
                self.limits.verify_checksums(),
                |connection| {
                    connection
                        .socket()
                        .set_read_timeout(Some(completion_timeout))
                },
            )
        }
        .map_err(|failure| {
            if failure.consumed {
                self.limits.poison();
            }
            failure.error
        });

        if let CurrentConnection::Tls(tls) = &mut *raw_connection
            && tls.awaiting_first_message
        {
            return match result {
                Ok(message) => {
                    tls.awaiting_first_message = false;
                    Ok(message)
                }
                Err(error) => Err(self.control_failure().unwrap_or_else(|| tls_failure(error))),
            };
        }
        result
    }

    fn write_message_with_timeout(
        &mut self,
        message: ADBTransportMessage,
        write_timeout: Duration,
    ) -> Result<()> {
        self.limits.check_in_step()?;
        // adbd drops the whole connection on a packet over its limit, so an
        // oversized payload is refused here while the connection is intact.
        let max_payload = self.max_payload();
        if message.payload().len() > max_payload {
            return Err(RustADBError::PayloadTooLarge(
                message.payload().len(),
                max_payload,
            ));
        }
        let header = message.header().as_bytes();
        let write_timeout = match &self.control {
            Some(control) => control.remaining()?.min(write_timeout),
            None => write_timeout,
        };

        let raw_connection_lock = self.get_current_connection()?;
        let mut raw_connection = raw_connection_lock.lock()?;
        raw_connection.set_write_timeout(write_timeout)?;
        let (accepted, result) = write_frame(&mut *raw_connection, &header, message.payload());
        if let Err(error) = result {
            // Only a timeout before any byte was taken leaves the outgoing
            // stream on a message boundary.
            if accepted > 0 || !is_timeout(error.kind()) {
                self.limits.poison();
            }
            return Err(error.into());
        }
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
            CurrentConnection::Tcp(tcp_stream) => tcp_stream.clone(),
            CurrentConnection::Tls(_) => {
                return Err(RustADBError::UpgradeError(
                    "cannot upgrade a TLS connection...".into(),
                ));
            }
        };

        let config = self.tls_config()?;
        let conn = ClientConnection::new(Arc::new(config), self.address.ip().into())?;
        let mut stream = StreamOwned::new(conn, tcp_stream);
        // Driving the handshake here keeps it inside the caller's handshake
        // deadline; left to the first read it would run under that read's
        // much longer timeout.
        complete_handshake(&mut stream, deadline, self.control.as_ref())?;
        log::debug!("TLS handshake complete");

        *current_conn_locked = CurrentConnection::Tls(Box::new(TlsConnection {
            stream,
            awaiting_first_message: true,
        }));
        Ok(())
    }
}

/// Presents the one ADB identity whenever the server asks.
#[derive(Debug)]
struct ClientCertificate {
    key: Arc<CertifiedKey>,
}

impl ResolvesClientCert for ClientCertificate {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
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
        let error = read_frame(&mut Cursor::new(header), MAX_PAYLOAD, true, |_| Ok(()))
            .unwrap_err()
            .error;
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn payload_at_the_limit_is_accepted_and_one_byte_over_is_not() {
        let message = ADBTransportMessage::try_new(MessageCommand::Write, 1, 2, &[7; 64]).unwrap();
        let bytes = frame(&message);
        let read = read_frame(&mut Cursor::new(bytes.clone()), 64, true, |_| Ok(())).unwrap();
        assert_eq!(read.payload(), &vec![7; 64]);
        let error = read_frame(&mut Cursor::new(bytes), 63, true, |_| Ok(()))
            .unwrap_err()
            .error;
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn zero_length_message_with_bad_magic_is_rejected() {
        let mut bytes = ADBTransportMessage::try_new(MessageCommand::Okay, 1, 2, &[])
            .unwrap()
            .header()
            .as_bytes();
        bytes[20] ^= 0x01;
        let error = read_frame(&mut Cursor::new(bytes), MAX_PAYLOAD, true, |_| Ok(()))
            .unwrap_err()
            .error;
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn checksum_is_verified_only_when_the_protocol_version_requires_it() {
        let message = ADBTransportMessage::try_new(MessageCommand::Write, 1, 2, b"abc").unwrap();
        let mut bytes = frame(&message);
        bytes[16..20].copy_from_slice(&0_u32.to_le_bytes());
        let error = read_frame(&mut Cursor::new(bytes.clone()), MAX_PAYLOAD, true, |_| {
            Ok(())
        })
        .unwrap_err()
        .error;
        assert!(
            matches!(error, RustADBError::InvalidIntegrity(..)),
            "{error:?}"
        );
        let read = read_frame(&mut Cursor::new(bytes), MAX_PAYLOAD, false, |_| Ok(())).unwrap();
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
        let (accepted, result) = write_frame(&mut writer, b"header", b"payload");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WriteZero);
        assert_eq!(accepted, 0);
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
            let mapped = tls_failure(io_error_with(rustls::Error::AlertReceived(alert)));
            assert!(
                matches!(mapped, RustADBError::TlsAuthRejected(_)),
                "{alert:?}: {mapped:?}"
            );
        }
    }

    #[test]
    fn other_tls_failures_keep_their_own_error() {
        let mapped = tls_failure(io_error_with(rustls::Error::AlertReceived(
            AlertDescription::HandshakeFailure,
        )));
        assert!(matches!(mapped, RustADBError::TLSError(_)), "{mapped:?}");

        let mapped = tls_failure(RustADBError::IOError(io::ErrorKind::TimedOut.into()));
        assert!(matches!(mapped, RustADBError::IOError(_)), "{mapped:?}");
    }

    #[test]
    fn a_hang_up_without_an_alert_never_claims_the_key_is_unpaired() {
        // adbd always asks for our certificate, so a drop after it went out
        // is as likely to be the network as a refusal.
        for kind in [
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::BrokenPipe,
        ] {
            let mapped = tls_failure(RustADBError::IOError(kind.into()));
            assert!(
                matches!(&mapped, RustADBError::TlsConnectionClosed(io) if io.kind() == kind),
                "{kind:?}: {mapped:?}"
            );
        }
    }

    fn assert_out_of_step(result: Result<ADBTransportMessage>) {
        match result {
            Err(RustADBError::ProtocolError(message)) => {
                assert!(message.contains("lost ADB message framing"), "{message}");
            }
            other => panic!("expected the poisoned-connection error, got {other:?}"),
        }
    }

    fn connected_to(script: impl FnOnce(std::net::TcpStream) + Send + 'static) -> TcpTransport {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            script(stream);
        });
        let mut transport = TcpTransport::new(address, PathBuf::new());
        transport.connect().unwrap();
        transport
    }

    #[test]
    fn a_timeout_between_messages_leaves_the_connection_usable() {
        let mut transport = connected_to(|mut stream| {
            std::thread::sleep(Duration::from_millis(300));
            let message = ADBTransportMessage::try_new(MessageCommand::Okay, 9, 7, &[]).unwrap();
            stream.write_all(&frame(&message)).unwrap();
            std::thread::sleep(Duration::from_secs(1));
        });

        let error = transport
            .read_message_with_timeout(Duration::from_millis(50))
            .unwrap_err();
        assert!(
            matches!(&error, RustADBError::IOError(io) if is_timeout(io.kind())),
            "{error:?}"
        );
        let message = transport
            .read_message_with_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(message.header().command(), MessageCommand::Okay);
    }

    #[test]
    fn a_frame_that_has_started_gets_time_to_finish() {
        let mut transport = connected_to(|mut stream| {
            let message =
                ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, b"hello").unwrap();
            let bytes = frame(&message);
            stream.write_all(&bytes[..10]).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            stream.write_all(&bytes[10..]).unwrap();
            std::thread::sleep(Duration::from_secs(1));
        });

        // The wait for the message to begin is short, but once ten header
        // bytes are in, giving up would strand the other fourteen.
        let message = transport
            .read_message_with_timeout(Duration::from_millis(100))
            .unwrap();
        assert_eq!(message.payload(), b"hello");
        let next = transport.read_message_with_timeout(Duration::from_millis(50));
        assert!(matches!(next, Err(RustADBError::IOError(_))), "{next:?}");
    }

    #[test]
    fn a_read_that_stops_mid_frame_poisons_the_connection() {
        let mut transport = connected_to(|mut stream| {
            let message =
                ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, b"hello").unwrap();
            stream.write_all(&frame(&message)[..26]).unwrap();
        });

        assert_unexpected_eof(transport.read_message_with_timeout(Duration::from_secs(1)));
        assert_out_of_step(transport.read_message_with_timeout(Duration::from_secs(1)));
        let reply = ADBTransportMessage::try_new(MessageCommand::Okay, 7, 9, &[]).unwrap();
        let error = transport
            .write_message_with_timeout(reply, Duration::from_secs(1))
            .unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn a_malformed_header_poisons_the_connection() {
        let mut transport = connected_to(|mut stream| {
            let mut bad = ADBTransportMessage::try_new(MessageCommand::Okay, 9, 7, &[])
                .unwrap()
                .header()
                .as_bytes();
            bad[20] ^= 0x01;
            stream.write_all(&bad).unwrap();
            let good = ADBTransportMessage::try_new(MessageCommand::Okay, 9, 7, &[]).unwrap();
            stream.write_all(&frame(&good)).unwrap();
            std::thread::sleep(Duration::from_secs(1));
        });

        let error = transport
            .read_message_with_timeout(Duration::from_secs(1))
            .unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
        // The well-formed message behind it is not trusted either.
        assert_out_of_step(transport.read_message_with_timeout(Duration::from_secs(1)));
    }

    #[test]
    fn reconnecting_clears_the_poison() {
        let limits = WireLimits::unnegotiated();
        limits.poison();
        assert!(limits.check_in_step().is_err());
        limits.reset();
        assert!(limits.check_in_step().is_ok());
    }

    /// Returns `prefix`, then fails the way a socket read timeout does.
    struct StallsAfter {
        prefix: Vec<u8>,
    }

    impl Read for StallsAfter {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.prefix.is_empty() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let amount = buf.len().min(self.prefix.len());
            buf[..amount].copy_from_slice(&self.prefix[..amount]);
            self.prefix.drain(..amount);
            Ok(amount)
        }
    }

    #[test]
    fn only_a_timeout_before_the_first_byte_is_clean() {
        let message = ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, b"hello").unwrap();
        let bytes = frame(&message);
        for (cut, consumed) in [(0, false), (1, true), (24, true), (26, true)] {
            let mut reader = StallsAfter {
                prefix: bytes[..cut].to_vec(),
            };
            let failure = read_frame(&mut reader, MAX_PAYLOAD, true, |_| Ok(())).unwrap_err();
            assert_eq!(failure.consumed, consumed, "cut at {cut}");
        }
    }

    /// Accepts `budget` bytes, then times out.
    struct AcceptsThenStalls {
        budget: usize,
    }

    impl Write for AcceptsThenStalls {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.budget == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let amount = buf.len().min(self.budget);
            self.budget -= amount;
            Ok(amount)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_write_reports_how_much_of_the_frame_went_out() {
        let (accepted, result) =
            write_frame(&mut AcceptsThenStalls { budget: 0 }, &[0; 24], &[0; 6]);
        assert_eq!(accepted, 0);
        assert!(is_timeout(result.unwrap_err().kind()));
        let (accepted, result) =
            write_frame(&mut AcceptsThenStalls { budget: 27 }, &[0; 24], &[0; 6]);
        assert_eq!(accepted, 27);
        assert!(result.is_err());
    }

    /// Takes at most `step` bytes per call, gathering across buffers.
    struct Trickle {
        step: usize,
        received: Vec<u8>,
    }

    impl Write for Trickle {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.write_vectored(&[IoSlice::new(buf)])
        }

        fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
            let mut taken = 0;
            for buf in bufs {
                let amount = buf.len().min(self.step - taken);
                self.received.extend_from_slice(&buf[..amount]);
                taken += amount;
                if taken == self.step {
                    break;
                }
            }
            Ok(taken)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_frame_written_in_pieces_arrives_whole_and_in_order() {
        let header: Vec<u8> = (0..24).collect();
        let payload: Vec<u8> = (100..150).collect();
        for step in [1, 5, 24, 25, 1000] {
            let mut writer = Trickle {
                step,
                received: Vec::new(),
            };
            let (accepted, result) = write_frame(&mut writer, &header, &payload);
            result.unwrap();
            assert_eq!(accepted, 74, "step {step}");
            assert_eq!(writer.received[..24], header[..], "step {step}");
            assert_eq!(writer.received[24..], payload[..], "step {step}");
        }
    }

    #[test]
    fn writes_before_cnxn_stay_within_the_oldest_limit_but_reads_do_not() {
        let limits = WireLimits::unnegotiated();
        assert_eq!(limits.write_limit(), MAX_PAYLOAD_V1);
        assert_eq!(limits.read_limit(), MAX_PAYLOAD);
        let info = ConnectionInfo::negotiate(
            crate::message_devices::adb_connection_info::A_VERSION_MIN,
            256 * 1024,
            b"device::features=shell_v2",
        )
        .unwrap();
        limits.apply(&info);
        assert_eq!(limits.write_limit(), 256 * 1024);
        assert_eq!(limits.read_limit(), 256 * 1024);
        limits.reset();
        assert_eq!(limits.write_limit(), MAX_PAYLOAD_V1);
        assert_eq!(limits.read_limit(), MAX_PAYLOAD);
    }

    fn plain_connection(address: SocketAddr) -> CurrentConnection {
        CurrentConnection::Tcp(SharedSocket(Arc::new(TcpStream::connect(address).unwrap())))
    }

    fn controlled_read(
        connection: &mut CurrentConnection,
        buf: &mut [u8],
        timeout: Duration,
        control: &BoundedShellControl,
    ) -> io::Result<()> {
        ControlledReader {
            connection,
            control,
            until: Instant::now() + timeout,
        }
        .read_exact(buf)
    }

    #[test]
    fn controlled_read_cancels_even_without_a_socket_shutdown_handle() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (finished, hold_open) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"part").unwrap();
            // A peer close must not be what wakes the reader and makes this test pass.
            hold_open.recv_timeout(Duration::from_secs(3)).is_ok()
        });
        let mut connection = plain_connection(address);
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let signal = control.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let cancel_started = Instant::now();
            signal.cancel();
            cancel_started
        });
        let result = controlled_read(
            &mut connection,
            &mut [0; 24],
            Duration::from_secs(2),
            &control,
        );
        let completed = Instant::now();
        let _ = finished.send(());
        let cancel_started = canceller.join().unwrap();
        assert!(
            server.join().unwrap(),
            "peer closed before the read completed"
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::ConnectionAborted);
        let latency = completed.duration_since(cancel_started);
        assert!(
            latency < Duration::from_millis(250),
            "cancel latency {latency:?}"
        );
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
        let mut connection = plain_connection(address);
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let mut bytes = [0; 4];
        controlled_read(
            &mut connection,
            &mut bytes,
            Duration::from_secs(1),
            &control,
        )
        .unwrap();
        control.cancel();
        server.join().unwrap();
        assert_eq!(&bytes, b"abcd");
    }

    #[test]
    fn controlled_read_honors_a_shorter_caller_budget() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(3));
        });
        let mut connection = plain_connection(address);
        let control = BoundedShellControl::new(Duration::from_secs(5)).unwrap();
        let started = Instant::now();
        let result = controlled_read(
            &mut connection,
            &mut [0; 1],
            Duration::from_millis(20),
            &control,
        );
        let elapsed = started.elapsed();
        control.cancel();
        server.join().unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        // The peer stays silent for 3 s, so anything near the 20 ms read
        // bound shows the read timed out on its own.
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    }

    #[test]
    fn a_controlled_transport_reassembles_fragmented_frames() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).unwrap();
            let message =
                ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, b"hello").unwrap();
            let header = message.header().as_bytes();
            stream.write_all(&header[..7]).unwrap();
            std::thread::sleep(Duration::from_millis(80));
            stream.write_all(&header[7..]).unwrap();
            stream.write_all(b"he").unwrap();
            std::thread::sleep(Duration::from_millis(80));
            stream.write_all(b"llo").unwrap();
        });
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let mut transport =
            TcpTransport::with_shell_control(address, Path::new("unused"), control.clone());
        transport.connect().unwrap();
        assert!(control.was_registered());
        let message = transport
            .read_message_with_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(message.payload(), b"hello");
        control.cancel();
        server.join().unwrap();
    }

    #[test]
    fn the_control_deadline_caps_a_frame_that_has_started() {
        // A started frame normally gets FRAME_COMPLETION_TIMEOUT; the total
        // deadline still ends it, and the half-read frame poisons the link.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let message = ADBTransportMessage::try_new(MessageCommand::Okay, 1, 2, &[]).unwrap();
            stream.write_all(&message.header().as_bytes()[..3]).unwrap();
            let _ = stream.read(&mut [0]);
        });
        let control = BoundedShellControl::new(Duration::from_millis(200)).unwrap();
        let mut transport = TcpTransport::with_shell_control(address, Path::new("unused"), control);
        transport.connect().unwrap();
        let started = Instant::now();
        let error = transport
            .read_message_with_timeout(Duration::from_secs(10))
            .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        assert!(
            matches!(&error, RustADBError::IOError(io) if io.kind() == io::ErrorKind::TimedOut),
            "{error:?}"
        );
        server.join().unwrap();
        assert!(transport.limits.check_in_step().is_err());
    }

    #[test]
    fn cancelling_a_stalled_tls_handshake_reports_the_cancel_not_a_hang_up() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Swallow the ClientHello and never answer it.
            let _ = io::copy(&mut stream, &mut io::sink());
        });
        let control = BoundedShellControl::new(Duration::from_secs(5)).unwrap();
        let mut transport = TcpTransport::with_shell_control(
            address,
            crate::message_devices::tcp::test_support::shared_test_key(),
            control.clone(),
        );
        transport.connect().unwrap();
        let signal = control.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            signal.cancel();
            Instant::now()
        });
        let error = transport
            .upgrade_connection(Duration::from_secs(5))
            .unwrap_err();
        let finished = Instant::now();
        let cancelled_at = canceller.join().unwrap();
        assert!(
            matches!(&error, RustADBError::IOError(io) if io.kind() == io::ErrorKind::ConnectionAborted),
            "{error:?}"
        );
        assert!(finished.saturating_duration_since(cancelled_at) < Duration::from_millis(250));
        server.join().unwrap();
    }

    #[test]
    fn a_borrowed_control_leaves_the_socket_open_and_a_spent_one_is_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let message = ADBTransportMessage::try_new(MessageCommand::Okay, 1, 2, &[]).unwrap();
            let _ = stream.read(&mut [0]);
            stream.write_all(&frame(&message)).unwrap();
            let _ = io::copy(&mut stream, &mut io::sink());
        });
        let mut transport = TcpTransport::new(address, PathBuf::new());
        transport.connect().unwrap();
        let spent = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        spent.cancel();
        assert!(transport.set_current_control(spent.clone()).is_err());
        assert!(!spent.was_registered());

        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let previous = transport.set_current_control(control.clone()).unwrap();
        assert!(previous.is_none());
        {
            let _finish = Finish(control);
        }
        transport.restore_control(previous);
        transport
            .write_message(ADBTransportMessage::try_new(MessageCommand::Okay, 2, 1, &[]).unwrap())
            .unwrap();
        let message = transport
            .read_message_with_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(message.header().command(), MessageCommand::Okay);
        drop(transport);
        server.join().unwrap();
    }
}
