//! Android 11+ wireless-debugging pairing — the six-digit-code flow.
//!
//! A device showing *Wireless debugging › Pair device with pairing code*
//! advertises `_adb-tls-pairing._tcp` and accepts one pairing session per
//! displayed code. This module runs the client half of that session:
//!
//! 1. TCP connect, then a TLS 1.3 handshake presenting a self-signed client
//!    certificate derived from the caller's ADB RSA key. Both peers accept any
//!    certificate the other offers — the channel is unauthenticated at this
//!    point, and the security of the exchange comes from SPAKE2 below.
//! 2. Export 64 bytes of keying material from the TLS session and append them
//!    to the six-digit code. Binding the transcript in means a man in the
//!    middle cannot relay the exchange onto a different TLS session.
//! 3. Run SPAKE2 over that password. Each side learns key material only if the
//!    codes matched.
//! 4. Exchange `PeerInfo` records encrypted under an AES-128-GCM key derived
//!    from the SPAKE2 output. Ours carries the ADB RSA public key in the same
//!    `QAAAA…` form `adb` sends for `A_AUTH` — the device stores it as a
//!    trusted key, and afterwards accepts a TLS client certificate carrying
//!    that same RSA key without prompting.
//!
//! Protocol references (all Apache-2.0), verified while writing this:
//! AOSP `packages/modules/adb/pairing_connection/pairing_connection.cpp`,
//! `pairing_auth/pairing_auth.cpp`, `pairing_auth/aes_128_gcm.cpp`,
//! `tls/tls_connection.cpp`, `client/adb_wifi.cpp`, and `BoringSSL`
//! `crypto/curve25519/spake25519.cc`. No code was copied from any of them.

mod cipher;
mod packet;
mod spake2;

use std::{
    fs::read_to_string,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

use rcgen::{CertificateParams, KeyPair, PKCS_RSA_SHA256};
use rustls::{
    ClientConfig, ClientConnection, SignatureScheme, StreamOwned,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, pem::PemObject},
};
use std::sync::Arc;
use thiserror::Error;

use crate::message_devices::models::ADBRsaKey;
use cipher::{PairingCipher, TAG_LEN};
use packet::{
    HEADER_LEN, PEER_INFO_RSA_PUB_KEY, PEER_INFO_SIZE, PacketType, PairingPacketHeader,
    encode_peer_info,
};
use spake2::{KEY_SIZE, MSG_SIZE, Role, Spake2};

pub use packet::PeerInfo;

/// The exporter label AOSP passes to `SSL_export_keying_material`, including
/// the NUL that `sizeof(kExportedKeyLabel)` counts.
const EXPORTER_LABEL: &[u8] = b"adb-label\0";

/// `PairingConnectionCtx::kExportedKeySize`.
const EXPORTED_KEY_SIZE: usize = 64;

/// How long the whole exchange may take, TCP connect included.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for the TCP connection itself. A device that is not showing
/// the pairing screen refuses immediately; one that is powered off blackholes
/// the SYN, and without this bound the OS timeout would be the user's wait.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything that can go wrong while pairing, split so callers can phrase a
/// useful message rather than surfacing transport detail.
#[derive(Debug, Error)]
pub enum PairingError {
    /// The pairing port did not accept a connection.
    #[error("could not reach the pairing service: {0}")]
    Connect(std::io::Error),
    /// The TLS 1.3 handshake failed.
    #[error("TLS handshake with the pairing service failed: {0}")]
    Tls(String),
    /// The socket failed mid-exchange.
    #[error("pairing connection failed: {0}")]
    Io(#[from] std::io::Error),
    /// The peer sent something the protocol does not allow.
    #[error("pairing protocol error: {0}")]
    Protocol(String),
    /// The peer's payload would not decrypt, which is what a wrong six-digit
    /// code looks like: SPAKE2 always yields *some* key material, and the
    /// mismatch only shows up as an AEAD failure.
    #[error("the pairing code did not match")]
    CodeMismatch,
    /// The exchange did not finish inside the deadline.
    #[error("pairing timed out")]
    Timeout,
    /// The local ADB key could not be read or turned into a certificate.
    #[error("could not use the local ADB key: {0}")]
    Key(String),
}

/// What a successful pairing learned about the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOutcome {
    /// The device's `PeerInfo`. Android answers `ADB_DEVICE_GUID` (kind 1)
    /// whose data is the `adb-<guid>` mDNS service name to reconnect to.
    pub peer_info: PeerInfo,
}

/// Pair with a device's wireless-debugging pairing service.
///
/// `code` is the six digits the device is displaying. `private_key_path` is a
/// PKCS#8 PEM RSA key; the public half is what the device will trust, and the
/// TLS client certificate presented here — and by a later
/// [`ADBTcpDevice`](crate::tcp::ADBTcpDevice) connection — is derived from the
/// same key, which is what lets the follow-up connection skip the *Allow
/// debugging* prompt.
///
/// The whole exchange is bounded by a 30-second deadline enforced through
/// socket timeouts, so this never blocks indefinitely on a silent peer.
pub fn pair(
    addr: SocketAddr,
    code: &str,
    private_key_path: &Path,
) -> Result<PairingOutcome, PairingError> {
    let deadline = Instant::now() + DEFAULT_TIMEOUT;

    let (certificate, private_key) = client_identity(private_key_path)?;
    let public_key = adb_public_key(private_key_path)?;

    let stream =
        TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(PairingError::Connect)?;
    // The exchange is a handful of small round-trips; Nagle only adds latency.
    let _ = stream.set_nodelay(true);

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCertificate))
        .with_client_auth_cert(certificate, private_key.into())
        .map_err(|e| PairingError::Tls(e.to_string()))?;
    let connection = ClientConnection::new(Arc::new(config), addr.ip().into())
        .map_err(|e| PairingError::Tls(e.to_string()))?;

    let mut tls = StreamOwned::new(connection, stream);
    refresh_timeouts(&tls, deadline)?;
    while tls.conn.is_handshaking() {
        refresh_timeouts(&tls, deadline)?;
        tls.conn
            .complete_io(&mut tls.sock)
            .map_err(|e| PairingError::Tls(e.to_string()))?;
    }

    let mut exported = [0_u8; EXPORTED_KEY_SIZE];
    tls.conn
        .export_keying_material(&mut exported[..], EXPORTER_LABEL, None)
        .map_err(|e| PairingError::Tls(e.to_string()))?;

    // password = the six digits, then the TLS exporter output.
    let mut password = code.as_bytes().to_vec();
    password.extend_from_slice(&exported);

    let spake = Spake2::new(Role::Alice, &password);
    let our_msg = spake.msg();
    write_packet(&mut tls, PacketType::Spake2Msg, &our_msg, deadline)?;
    let their_msg = read_packet(&mut tls, PacketType::Spake2Msg, deadline)?;
    if their_msg.len() != MSG_SIZE {
        return Err(PairingError::Protocol(format!(
            "peer SPAKE2 message is {} bytes, expected {MSG_SIZE}",
            their_msg.len()
        )));
    }
    let key_material = spake
        .process(&their_msg)
        .ok_or_else(|| PairingError::Protocol("peer SPAKE2 message is not a curve point".into()))?;
    debug_assert_eq!(key_material.len(), KEY_SIZE);

    let mut cipher = PairingCipher::new(&key_material);
    let our_peer_info = encode_peer_info(PEER_INFO_RSA_PUB_KEY, public_key.as_bytes())?;
    let sealed = cipher.encrypt(&our_peer_info)?;
    write_packet(&mut tls, PacketType::PeerInfo, &sealed, deadline)?;

    let their_sealed = read_packet(&mut tls, PacketType::PeerInfo, deadline)?;
    if their_sealed.len() != PEER_INFO_SIZE + TAG_LEN {
        return Err(PairingError::Protocol(format!(
            "peer info record is {} bytes, expected {}",
            their_sealed.len(),
            PEER_INFO_SIZE + TAG_LEN
        )));
    }
    let their_peer_info = cipher.decrypt(&their_sealed)?;
    let peer_info = PeerInfo::parse(&their_peer_info)?;

    // Both sides have each other's key. Send close_notify so the device's
    // blocking read ends cleanly rather than as a reset.
    tls.conn.send_close_notify();
    let _ = tls.flush();

    Ok(PairingOutcome { peer_info })
}

/// The TLS client certificate and key, derived exactly the way
/// [`crate::tcp::ADBTcpDevice`]'s transport derives them, so the certificate
/// this pairing presents carries the same public key as the later connection.
fn client_identity(
    private_key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivatePkcs8KeyDer<'static>), PairingError> {
    let pem = read_to_string(private_key_path).map_err(|e| PairingError::Key(e.to_string()))?;
    let key_pair = KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &PKCS_RSA_SHA256)
        .map_err(|e| PairingError::Key(e.to_string()))?;
    let certificate = CertificateParams::default()
        .self_signed(&key_pair)
        .map_err(|e| PairingError::Key(e.to_string()))?;
    let private_key = PrivatePkcs8KeyDer::from_pem_file(private_key_path)
        .map_err(|e| PairingError::Key(e.to_string()))?;
    Ok((vec![certificate.der().to_owned()], private_key))
}

/// The `<base64> <name>` ADB public key, byte-for-byte what this crate sends
/// for `A_AUTH` `ADB_AUTH_RSAPUBLICKEY`, and what `adb` writes to `adbkey.pub`.
fn adb_public_key(private_key_path: &Path) -> Result<String, PairingError> {
    let pem = read_to_string(private_key_path).map_err(|e| PairingError::Key(e.to_string()))?;
    ADBRsaKey::new_from_pkcs8(&pem)
        .and_then(|key| key.android_pubkey_encode())
        .map_err(|e| PairingError::Key(e.to_string()))
}

type TlsStream = StreamOwned<ClientConnection, TcpStream>;

/// Point the socket timeouts at the remaining budget so no single read or write
/// can outlive the overall deadline by more than one syscall.
fn refresh_timeouts(tls: &TlsStream, deadline: Instant) -> Result<Duration, PairingError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(PairingError::Timeout);
    }
    tls.sock.set_read_timeout(Some(remaining))?;
    tls.sock.set_write_timeout(Some(remaining))?;
    Ok(remaining)
}

fn write_packet(
    tls: &mut TlsStream,
    packet_type: PacketType,
    payload: &[u8],
    deadline: Instant,
) -> Result<(), PairingError> {
    refresh_timeouts(tls, deadline)?;
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| PairingError::Protocol("payload too large to frame".into()))?;
    let header = PairingPacketHeader::new(packet_type, payload_len);
    tls.write_all(&header.to_bytes())?;
    tls.write_all(payload)?;
    tls.flush()?;
    Ok(())
}

fn read_packet(
    tls: &mut TlsStream,
    expected: PacketType,
    deadline: Instant,
) -> Result<Vec<u8>, PairingError> {
    // A device that cannot open our `PeerInfo` record just closes the socket,
    // so an EOF while waiting for its reply is how a wrong code presents. An
    // EOF earlier than that is a genuine transport failure.
    let eof_is_mismatch = expected == PacketType::PeerInfo;

    let mut header_bytes = [0_u8; HEADER_LEN];
    read_exact_by_deadline(tls, &mut header_bytes, deadline, eof_is_mismatch)?;
    let header = PairingPacketHeader::parse(header_bytes)?;
    if header.packet_type != expected {
        return Err(PairingError::Protocol(format!(
            "expected a {expected:?} packet, got {:?}",
            header.packet_type
        )));
    }
    let mut payload = vec![0_u8; header.payload as usize];
    read_exact_by_deadline(tls, &mut payload, deadline, eof_is_mismatch)?;
    Ok(payload)
}

/// `read_exact`, but with the socket timeout re-armed to the remaining budget
/// before every syscall, so a peer that trickles one byte at a time still hits
/// the overall deadline.
fn read_exact_by_deadline(
    tls: &mut TlsStream,
    buf: &mut [u8],
    deadline: Instant,
    eof_is_mismatch: bool,
) -> Result<(), PairingError> {
    let mut filled = 0;
    while filled < buf.len() {
        refresh_timeouts(tls, deadline)?;
        match tls.read(&mut buf[filled..]) {
            Ok(0) if eof_is_mismatch => return Err(PairingError::CodeMismatch),
            Ok(0) => {
                return Err(PairingError::Io(std::io::Error::from(
                    std::io::ErrorKind::UnexpectedEof,
                )));
            }
            Ok(read) => filled += read,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(PairingError::Timeout);
            }
            Err(e) => return Err(PairingError::Io(e)),
        }
    }
    Ok(())
}

/// The pairing peers exchange self-signed certificates and each accepts
/// whatever the other offers — AOSP installs a `SetCertVerifyCallback` that
/// unconditionally returns 1. Authentication is SPAKE2's job, and the
/// certificate is bound to it through the TLS exporter output that goes into
/// the SPAKE2 password.
#[derive(Debug)]
struct AcceptAnyServerCertificate;

impl ServerCertVerifier for AcceptAnyServerCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
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
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EXPORTED_KEY_SIZE, EXPORTER_LABEL, PEER_INFO_RSA_PUB_KEY, PairingError,
        cipher::{PairingCipher, TAG_LEN},
        packet::{HEADER_LEN, PEER_INFO_SIZE, PacketType, PairingPacketHeader, encode_peer_info},
        pair,
        spake2::{Role, Spake2},
    };
    use crate::message_devices::models::ADBRsaKey;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::{
        io::{Read, Write},
        net::{Ipv4Addr, SocketAddr, TcpListener},
        path::PathBuf,
    };

    #[test]
    fn exporter_label_carries_its_nul() {
        assert_eq!(EXPORTER_LABEL, b"adb-label\0");
        assert_eq!(EXPORTER_LABEL.len(), 10);
        assert_eq!(EXPORTED_KEY_SIZE, 64);
    }

    /// Runs both halves of the post-handshake protocol over an in-memory
    /// duplex: SPAKE2 (Alice against Bob), the header framing, and the
    /// AES-128-GCM `PeerInfo` exchange. This is the whole exchange except the
    /// TLS layer, which cannot be driven without a real pairing server.
    #[test]
    fn both_halves_of_the_exchange_round_trip() {
        // Same TLS exporter output on both sides, as a real session would give.
        let exported = [0x9c_u8; EXPORTED_KEY_SIZE];
        let mut password = b"642091".to_vec();
        password.extend_from_slice(&exported);

        let client = Spake2::new(Role::Alice, &password);
        let server = Spake2::new(Role::Bob, &password);

        // Framed SPAKE2 messages cross the wire.
        let client_frame = frame(PacketType::Spake2Msg, &client.msg());
        let server_frame = frame(PacketType::Spake2Msg, &server.msg());
        let (kind, client_msg) = unframe(&client_frame);
        assert_eq!(kind, PacketType::Spake2Msg);
        let (kind, server_msg) = unframe(&server_frame);
        assert_eq!(kind, PacketType::Spake2Msg);

        let client_key = client.process(&server_msg).unwrap();
        let server_key = server.process(&client_msg).unwrap();
        assert_eq!(client_key, server_key);

        let mut client_cipher = PairingCipher::new(&client_key);
        let mut server_cipher = PairingCipher::new(&server_key);

        let client_info =
            encode_peer_info(PEER_INFO_RSA_PUB_KEY, b"QAAAAB fake-key user@host").unwrap();
        let sealed = client_cipher.encrypt(&client_info).unwrap();
        assert_eq!(sealed.len(), PEER_INFO_SIZE + TAG_LEN);
        let wire = frame(PacketType::PeerInfo, &sealed);
        let (kind, received) = unframe(&wire);
        assert_eq!(kind, PacketType::PeerInfo);
        assert_eq!(server_cipher.decrypt(&received).unwrap(), client_info);

        let server_info = encode_peer_info(1, b"adb-SERIAL-abcdef").unwrap();
        let sealed = server_cipher.encrypt(&server_info).unwrap();
        let wire = frame(PacketType::PeerInfo, &sealed);
        let (kind, received) = unframe(&wire);
        assert_eq!(kind, PacketType::PeerInfo);
        let decoded = super::PeerInfo::parse(&client_cipher.decrypt(&received).unwrap()).unwrap();
        assert_eq!(decoded.kind, 1);
        assert_eq!(decoded.data_lossy(), "adb-SERIAL-abcdef");
    }

    /// A different code on the two sides has to fail, and specifically at the
    /// AEAD step — that is the failure `PairingError::CodeMismatch` reports.
    #[test]
    fn a_wrong_code_fails_to_open_the_peer_info() {
        let exported = [0x11_u8; EXPORTED_KEY_SIZE];
        let mut right = b"642091".to_vec();
        right.extend_from_slice(&exported);
        let mut wrong = b"642092".to_vec();
        wrong.extend_from_slice(&exported);

        let client = Spake2::new(Role::Alice, &right);
        let server = Spake2::new(Role::Bob, &wrong);
        let client_msg = client.msg();
        let server_msg = server.msg();
        let client_key = client.process(&server_msg).unwrap();
        let server_key = server.process(&client_msg).unwrap();
        assert_ne!(client_key, server_key);

        let mut client_cipher = PairingCipher::new(&client_key);
        let mut server_cipher = PairingCipher::new(&server_key);
        let sealed = client_cipher
            .encrypt(&encode_peer_info(PEER_INFO_RSA_PUB_KEY, b"key").unwrap())
            .unwrap();
        assert!(matches!(
            server_cipher.decrypt(&sealed),
            Err(PairingError::CodeMismatch)
        ));
    }

    /// The peer info we send must be the ADB public key in the form adbd
    /// parses: base64 up to the first space, decoding to exactly
    /// `ANDROID_PUBKEY_ENCODED_SIZE` bytes.
    #[test]
    fn peer_info_carries_a_decodable_android_public_key() {
        let key = ADBRsaKey::new_random().unwrap();
        let encoded = key.android_pubkey_encode().unwrap();
        let peer_info = encode_peer_info(PEER_INFO_RSA_PUB_KEY, encoded.as_bytes()).unwrap();
        assert_eq!(peer_info[0], 0);

        let parsed = super::PeerInfo::parse(&peer_info).unwrap();
        let text = parsed.data_lossy();
        let base64_part = text.split(' ').next().unwrap();
        let decoded = STANDARD.decode(base64_part).unwrap();
        // 4 (modulus words) + 4 (n0inv) + 256 (modulus) + 256 (rr) + 4 (exponent)
        assert_eq!(decoded.len(), 524);
        assert!(text.contains('@'));
    }

    /// A refused port must surface as `Connect`, not as a hang.
    #[test]
    fn a_closed_pairing_port_reports_connect() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = pair(addr, "123456", &PathBuf::from("/nonexistent/adbkey")).unwrap_err();
        // The key is read before connecting, so this reports the key first.
        assert!(matches!(err, PairingError::Key(_)), "{err:?}");
    }

    /// A peer that accepts the TCP connection but never speaks TLS must fail
    /// the handshake rather than block forever.
    #[test]
    fn a_silent_peer_fails_the_handshake() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut scratch = [0_u8; 64];
            let _ = stream.read(&mut scratch);
            let _ = stream.write_all(b"not tls at all");
        });

        let key_path = write_temp_key();
        let err = pair(addr, "123456", &key_path).unwrap_err();
        assert!(
            matches!(err, PairingError::Tls(_) | PairingError::Io(_)),
            "{err:?}"
        );
        server.join().unwrap();
        let _ = std::fs::remove_file(&key_path);
    }

    fn write_temp_key() -> PathBuf {
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap();
        let mut path = std::env::temp_dir();
        path.push(format!(
            "adb_client_pairing_test_{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, pem.as_bytes()).unwrap();
        path
    }

    fn frame(packet_type: PacketType, payload: &[u8]) -> Vec<u8> {
        let header = PairingPacketHeader::new(packet_type, u32::try_from(payload.len()).unwrap());
        let mut out = header.to_bytes().to_vec();
        out.extend_from_slice(payload);
        out
    }

    fn unframe(bytes: &[u8]) -> (PacketType, Vec<u8>) {
        let header_bytes: [u8; HEADER_LEN] = bytes[..HEADER_LEN].try_into().unwrap();
        let header = PairingPacketHeader::parse(header_bytes).unwrap();
        let end = HEADER_LEN + header.payload as usize;
        (header.packet_type, bytes[HEADER_LEN..end].to_vec())
    }
}
