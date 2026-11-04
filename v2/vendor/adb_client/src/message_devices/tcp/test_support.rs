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
    let identity = rcgen::generate_simple_self_signed(vec!["adbd.test".to_string()]).unwrap();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        identity.signing_key.serialize_der(),
    ));
    let verifier = TestClientVerifier {
        trust_client,
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
    trust_client: bool,
    seen_client_cert: Arc<Mutex<Option<Vec<u8>>>>,
}

impl ClientCertVerifier for TestClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        *self.seen_client_cert.lock().unwrap() = Some(end_entity.to_vec());
        if self.trust_client {
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
