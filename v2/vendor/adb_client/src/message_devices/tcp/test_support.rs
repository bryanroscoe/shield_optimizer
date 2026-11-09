//! Loopback fakes for connection-layer tests: a scripted adbd on a real TCP
//! socket, optionally upgrading to TLS with a configurable client-cert check.

use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread::JoinHandle,
};

use rustls::{
    CertificateError, DigitallySignedStruct, DistinguishedName, ServerConfig, ServerConnection,
    SignatureScheme, StreamOwned,
    client::danger::HandshakeSignatureValid,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
};

use crate::message_devices::{
    adb_transport_message::{ADBTransportMessage, ADBTransportMessageHeader},
    message_commands::MessageCommand,
};

/// One RSA key for every test in the binary, reused across runs, because
/// generating a 2048-bit key in a debug build takes seconds.
pub(crate) fn shared_test_key() -> &'static Path {
    static KEY: OnceLock<PathBuf> = OnceLock::new();
    KEY.get_or_init(|| {
        let path = std::env::temp_dir().join("adb_client_connection_test_key.pem");
        let reusable = std::fs::read_to_string(&path).is_ok_and(|pem| {
            crate::message_devices::models::ADBRsaKey::new_from_pkcs8(&pem).is_ok()
        });
        if !reusable {
            use rsa::pkcs8::{EncodePrivateKey, LineEnding};
            let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
            let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap();
            let staging = path.with_extension(format!("{}.tmp", std::process::id()));
            std::fs::write(&staging, pem.as_bytes()).unwrap();
            std::fs::rename(&staging, &path).unwrap();
        }
        path
    })
}

pub(crate) fn read_message(stream: &mut impl Read) -> io::Result<ADBTransportMessage> {
    let mut header = [0_u8; 24];
    stream.read_exact(&mut header)?;
    let header = ADBTransportMessageHeader::try_from(header).map_err(io::Error::other)?;
    let mut payload = vec![0; header.data_length() as usize];
    stream.read_exact(&mut payload)?;
    Ok(ADBTransportMessage::from_header_and_payload(
        header, payload,
    ))
}

pub(crate) fn write_message(
    stream: &mut impl Write,
    command: MessageCommand,
    arg0: u32,
    arg1: u32,
    payload: &[u8],
) -> io::Result<()> {
    let message = ADBTransportMessage::try_new(command, arg0, arg1, payload).unwrap();
    stream.write_all(&message.header().as_bytes())?;
    stream.write_all(message.payload())?;
    stream.flush()
}

/// Wait for the client to hang up, so the fake never closes first by accident.
pub(crate) fn drain(stream: &mut impl Read) {
    let _ = io::copy(stream, &mut io::sink());
}

/// Run `script` against each of the first `count` connections to a fresh
/// loopback port, in turn, with the connection's number.
pub(crate) fn fake_adbd_connections<F>(count: usize, script: F) -> (SocketAddr, JoinHandle<()>)
where
    F: Fn(TcpStream, usize) + Send + 'static,
{
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for number in 0..count {
            let (stream, _) = listener.accept().unwrap();
            script(stream, number);
        }
    });
    (address, handle)
}

/// Run `script` against the first connection to a fresh loopback port.
pub(crate) fn fake_adbd<F>(script: F) -> (SocketAddr, JoinHandle<()>)
where
    F: FnOnce(TcpStream) + Send + 'static,
{
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        script(stream);
    });
    (address, handle)
}

/// A TLS server that requires a client certificate and either trusts it or
/// refuses it the way a TLS stack refuses an unknown issuer.
pub(crate) fn tls_server(
    stream: TcpStream,
    trust_client: bool,
    seen_client_cert: Arc<Mutex<Option<Vec<u8>>>>,
) -> StreamOwned<ServerConnection, TcpStream> {
    let trust = if trust_client {
        ClientTrust::Any
    } else {
        ClientTrust::Nothing
    };
    tls_server_with(stream, trust, Vec::new(), seen_client_cert)
}

/// Which client certificates [`tls_server_with`] accepts.
#[derive(Debug)]
pub(crate) enum ClientTrust {
    Any,
    Nothing,
    /// Only a certificate carrying one of these `SubjectPublicKeyInfo`s, as
    /// adbd trusts only its paired keys.
    PublicKeys(Vec<Vec<u8>>),
}

/// [`tls_server`] with a trust policy and the CA hints (DER `Name`s) its
/// `CertificateRequest` advertises, as adbd advertises its paired keys.
pub(crate) fn tls_server_with(
    stream: TcpStream,
    trust: ClientTrust,
    hints: Vec<Vec<u8>>,
    seen_client_cert: Arc<Mutex<Option<Vec<u8>>>>,
) -> StreamOwned<ServerConnection, TcpStream> {
    let identity = rcgen::generate_simple_self_signed(vec!["adbd.test".to_string()]).unwrap();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        identity.signing_key.serialize_der(),
    ));
    let verifier = TestClientVerifier {
        trust,
        hints: hints.into_iter().map(DistinguishedName::from).collect(),
        seen_client_cert,
    };
    let config = ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(verifier))
        .with_single_cert(vec![identity.cert.der().clone()], key)
        .unwrap();
    StreamOwned::new(ServerConnection::new(Arc::new(config)).unwrap(), stream)
}

/// Run the server half of the handshake. Errors are returned rather than
/// unwrapped, because a refused client is an expected outcome.
pub(crate) fn complete_server_handshake(
    tls: &mut StreamOwned<ServerConnection, TcpStream>,
) -> io::Result<()> {
    while tls.conn.is_handshaking() {
        tls.conn.complete_io(&mut tls.sock)?;
    }
    Ok(())
}

#[derive(Debug)]
struct TestClientVerifier {
    trust: ClientTrust,
    hints: Vec<DistinguishedName>,
    seen_client_cert: Arc<Mutex<Option<Vec<u8>>>>,
}

impl ClientCertVerifier for TestClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &self.hints
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        *self.seen_client_cert.lock().unwrap() = Some(end_entity.to_vec());
        let trusted = match &self.trust {
            ClientTrust::Any => true,
            ClientTrust::Nothing => false,
            ClientTrust::PublicKeys(keys) => keys.iter().any(|spki| {
                end_entity
                    .windows(spki.len())
                    .any(|window| window == spki.as_slice())
            }),
        };
        if trusted {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::UnknownIssuer,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

/// One logical stream as the fake device sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stream {
    /// The host's id for the stream.
    pub(crate) local: u32,
    /// The fake device's id for it.
    pub(crate) remote: u32,
}

/// The device end of a connection after CNXN, for scripting stream tests.
/// It acknowledges each host WRTE the way adbd does and records their sizes.
pub(crate) struct Peer {
    stream: TcpStream,
    max_payload: usize,
    host_bytes: std::collections::VecDeque<u8>,
    next_remote: u32,
    /// Payload size of every WRTE the host sent.
    pub(crate) host_writes: Vec<usize>,
    /// OKAYs sent for each host WRTE. Android 13 and older send one; Android
    /// 14 sends one per partial flush into the service, so often more.
    pub(crate) okays_per_write: usize,
}

impl Peer {
    /// Answer the host's CNXN with `features` and `max_payload`.
    pub(crate) fn connect(mut stream: TcpStream, features: &str, max_payload: u32) -> Self {
        let cnxn = read_message(&mut stream).unwrap();
        assert_eq!(cnxn.header().command(), MessageCommand::Cnxn);
        let banner = format!("device::ro.product.model=Fake;features={features}");
        write_message(
            &mut stream,
            MessageCommand::Cnxn,
            0x0100_0000,
            max_payload,
            banner.as_bytes(),
        )
        .unwrap();
        Self {
            stream,
            max_payload: max_payload as usize,
            host_bytes: std::collections::VecDeque::new(),
            next_remote: 100,
            host_writes: Vec::new(),
            okays_per_write: 1,
        }
    }

    pub(crate) fn next(&mut self) -> ADBTransportMessage {
        read_message(&mut self.stream).unwrap()
    }

    /// The next message, or the error that ended the connection, waiting at
    /// most `timeout`.
    pub(crate) fn try_next(
        &mut self,
        timeout: std::time::Duration,
    ) -> io::Result<ADBTransportMessage> {
        self.stream.set_read_timeout(Some(timeout))?;
        let message = read_message(&mut self.stream);
        self.stream.set_read_timeout(None)?;
        message
    }

    pub(crate) fn raw(&mut self, command: MessageCommand, arg0: u32, arg1: u32, payload: &[u8]) {
        write_message(&mut self.stream, command, arg0, arg1, payload).unwrap();
    }

    /// [`Self::raw`] for a peer that may already have been hung up on: a
    /// host that ran out its deadline closes the socket, so the write can fail.
    pub(crate) fn try_raw(
        &mut self,
        command: MessageCommand,
        arg0: u32,
        arg1: u32,
        payload: &[u8],
    ) -> io::Result<()> {
        write_message(&mut self.stream, command, arg0, arg1, payload)
    }

    /// The next OPEN, skipping CLSEs for streams the host finished. Returns
    /// the host's local id and the destination without its NUL.
    pub(crate) fn expect_open(&mut self) -> (u32, String) {
        loop {
            let message = self.next();
            match message.header().command() {
                MessageCommand::Clse => {}
                MessageCommand::Open => {
                    let destination = message.payload();
                    assert_eq!(destination.last(), Some(&0), "OPEN without its NUL");
                    let destination =
                        String::from_utf8(destination[..destination.len() - 1].to_vec()).unwrap();
                    return (message.header().arg0(), destination);
                }
                other => panic!("expected OPEN, got {other}"),
            }
        }
    }

    pub(crate) fn accept(&mut self, local: u32) -> Stream {
        let stream = Stream {
            local,
            remote: self.next_remote,
        };
        self.next_remote += 1;
        self.raw(MessageCommand::Okay, stream.remote, local, &[]);
        self.host_bytes.clear();
        stream
    }

    /// Accept the next OPEN, asserting its destination.
    pub(crate) fn accept_open(&mut self, expected: &str) -> Stream {
        let (local, destination) = self.expect_open();
        assert_eq!(destination, expected);
        self.accept(local)
    }

    /// Send `data` in WRTEs of at most `chunk` bytes, each waiting for the
    /// host's OKAY.
    pub(crate) fn send(&mut self, stream: Stream, data: &[u8], chunk: usize) {
        for part in data.chunks(chunk.max(1)) {
            self.raw(MessageCommand::Write, stream.remote, stream.local, part);
            let okay = self.next();
            assert_eq!(okay.header().command(), MessageCommand::Okay);
            assert_eq!(
                (okay.header().arg0(), okay.header().arg1()),
                (stream.local, stream.remote)
            );
        }
    }

    /// The next `len` bytes the host wrote on `stream`.
    pub(crate) fn read_host(&mut self, stream: Stream, len: usize) -> Vec<u8> {
        while self.host_bytes.len() < len {
            let message = self.next();
            assert_eq!(message.header().command(), MessageCommand::Write);
            assert_eq!(
                (message.header().arg0(), message.header().arg1()),
                (stream.local, stream.remote)
            );
            assert!(message.payload().len() <= self.max_payload);
            self.host_writes.push(message.payload().len());
            for _ in 0..self.okays_per_write {
                self.raw(MessageCommand::Okay, stream.remote, stream.local, &[]);
            }
            self.host_bytes.extend(message.payload());
        }
        self.host_bytes.drain(..len).collect()
    }

    /// A sync request header: id and argument.
    pub(crate) fn sync_header(&mut self, stream: Stream) -> ([u8; 4], u32) {
        let header = self.read_host(stream, 8);
        (
            header[..4].try_into().unwrap(),
            u32::from_le_bytes(header[4..].try_into().unwrap()),
        )
    }

    /// A sync request: id and its length-prefixed body.
    pub(crate) fn sync_request(&mut self, stream: Stream) -> ([u8; 4], Vec<u8>) {
        let (id, len) = self.sync_header(stream);
        (id, self.read_host(stream, len as usize))
    }

    /// QUIT, then close the stream as adbd does.
    pub(crate) fn expect_quit(&mut self, stream: Stream) {
        assert_eq!(self.sync_header(stream), (*b"QUIT", 0));
        self.close(stream);
    }

    pub(crate) fn close(&mut self, stream: Stream) {
        self.raw(MessageCommand::Clse, stream.remote, stream.local, &[]);
    }

    /// Drop the whole connection, as adbd does when it restarts.
    pub(crate) fn hang_up(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    /// Wait for the host to hang up.
    pub(crate) fn finish(mut self) {
        drain(&mut self.stream);
    }
}

/// A sync reply: id, little-endian argument, then `body`.
pub(crate) fn sync_reply(id: [u8; 4], arg: u32, body: &[u8]) -> Vec<u8> {
    let mut reply = id.to_vec();
    reply.extend(arg.to_le_bytes());
    reply.extend(body);
    reply
}

/// A shell protocol v2 packet.
pub(crate) fn shell_packet(channel: u8, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![channel];
    packet.extend(u32::try_from(payload.len()).unwrap().to_le_bytes());
    packet.extend(payload);
    packet
}

/// A file for install tests, removed when dropped.
pub(crate) struct TempApk(pub(crate) PathBuf);

impl TempApk {
    pub(crate) fn new(tag: &str, bytes: &[u8]) -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "adb_client_install_test_{}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.apk"));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for TempApk {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
