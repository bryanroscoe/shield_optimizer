use std::io::Write;
use std::path::Path;
use std::time::Duration;
use std::{io::Read, net::SocketAddr};

use crate::message_devices::adb_message_device::ADBMessageDevice;
use crate::models::RemountInfo;
use crate::tcp::tcp_transport::TcpTransport;
use crate::utils::get_default_adb_key_path;
use crate::{
    ADBDeviceExt, ADBListItemType, ConnectionInfo, Result, ShellExit,
    message_devices::adb_message_transport::DEFAULT_READ_TIMEOUT,
};

/// Represent a device reached and available over TCP.
#[derive(Debug)]
pub struct ADBTcpDevice {
    inner: ADBMessageDevice<TcpTransport>,
}

impl ADBTcpDevice {
    /// Instantiate a new [`ADBTcpDevice`]
    pub fn new<A: Into<SocketAddr>>(address: A) -> Result<Self> {
        Self::new_with_custom_private_key(address, get_default_adb_key_path()?)
    }

    /// Instantiate a new [`ADBTcpDevice`] using a custom private key path
    pub fn new_with_custom_private_key<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        private_key_path: P,
    ) -> Result<Self> {
        Ok(Self {
            inner: ADBMessageDevice::new(
                TcpTransport::new(address, &private_key_path),
                private_key_path,
            )?,
        })
    }

    /// Instantiate an [`ADBTcpDevice`] using a custom private key path. The
    /// whole connection handshake (TLS upgrade, RSA authorization and the
    /// user's approval on the device) must finish within `auth_timeout`.
    pub fn new_with_custom_private_key_and_auth_timeout<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Ok(Self {
            inner: ADBMessageDevice::new_with_auth_timeout(
                TcpTransport::new(address, &private_key_path),
                private_key_path,
                auth_timeout,
            )?,
        })
    }

    /// What the CNXN exchange negotiated: protocol version, maximum payload,
    /// and the features both this client and the device support.
    #[must_use]
    pub const fn connection_info(&self) -> &ConnectionInfo {
        self.inner.connection_info()
    }

    /// Whether both this client and the device support `feature`, for example
    /// `sendrecv_v2` or `abb_exec`.
    #[must_use]
    pub fn has_feature(&self, feature: &str) -> bool {
        self.inner.connection_info().has_feature(feature)
    }

    /// Largest payload one ADB message may carry on this connection.
    #[must_use]
    pub const fn max_payload(&self) -> usize {
        self.inner.connection_info().max_payload()
    }

    /// Run a shell command with a finite timeout for each transport response.
    /// This is useful for liveness probes, where an unbounded read would keep
    /// the owning connection mutex locked forever after a network loss.
    ///
    /// Shell protocol v2 is used whenever the device negotiated `shell_v2`
    /// and returns [`ShellExit::Code`]. Only a device without it (or one
    /// that refuses the v2 service) gets the legacy shell, which merges
    /// stderr into stdout and returns [`ShellExit::Unreported`]. A v2 stream
    /// that closes before its exit status is an error, never a success.
    pub fn shell_command_with_timeout(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<ShellExit> {
        self.inner
            .shell_command_with_timeout(command, stdout, stderr, timeout)
    }

    /// Run a command whose output is binary, like `adb exec-out`: shell v2
    /// when negotiated, otherwise `exec:`, which (unlike the legacy shell)
    /// never passes output through a pty. Without v2, stderr is merged into
    /// `stdout` and there is no exit status.
    pub fn exec_out_with_timeout(
        &mut self,
        command: &str,
        stdout: &mut dyn Write,
        stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<ShellExit> {
        self.inner
            .exec_out_with_timeout(command, stdout, stderr, timeout)
    }

    /// Pull `source` into `output` unless the device reports it larger than
    /// `max_bytes`, which is [`crate::RustADBError::FileTooLarge`] with
    /// nothing written. One sync session does the size check and the
    /// transfer. `output` must still bound a file that grows meanwhile.
    pub fn pull_with_limit(
        &mut self,
        source: &str,
        output: &mut dyn Write,
        max_bytes: u64,
    ) -> Result<()> {
        self.inner.pull_at_most(source, output, Some(max_bytes))
    }

    /// Install one APK, or the base and splits of one app together, choosing
    /// the transport as AOSP's `adb install` does: streamed through
    /// `abb_exec` or `exec:cmd` when negotiated, otherwise pushed to
    /// `/data/local/tmp` (and removed again) for `pm`. `options` are passed
    /// to the package manager, e.g. `["-r"]`. Returns its `Success` reply.
    /// Anything else is [`crate::RustADBError::InstallFailed`] with the
    /// package manager's text, and an unfinished session is abandoned.
    /// `timeout` bounds each wait for the device, including verification
    /// after the last byte.
    pub fn install_apks<P: AsRef<Path>>(
        &mut self,
        apks: &[P],
        options: &[&str],
        timeout: Duration,
    ) -> Result<String> {
        let paths: Vec<&Path> = apks.iter().map(AsRef::as_ref).collect();
        self.inner.install_packages(&paths, options, timeout)
    }
}

impl ADBDeviceExt for ADBTcpDevice {
    #[inline]
    fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>> {
        self.shell_command_with_timeout(command, stdout, stderr, DEFAULT_READ_TIMEOUT)
            .map(ShellExit::code)
    }

    #[inline]
    fn shell(&mut self, reader: &mut dyn Read, writer: Box<dyn Write + Send>) -> Result<()> {
        self.inner.shell(reader, writer)
    }

    #[inline]
    fn stat(&mut self, remote_path: &dyn AsRef<str>) -> Result<crate::AdbStatResponse> {
        self.inner.stat(remote_path)
    }

    #[inline]
    fn pull(&mut self, source: &dyn AsRef<str>, output: &mut dyn Write) -> Result<()> {
        self.inner.pull(source, output)
    }

    #[inline]
    fn push(&mut self, stream: &mut dyn Read, path: &dyn AsRef<str>) -> Result<()> {
        self.inner.push(stream, path)
    }

    #[inline]
    fn reboot(&mut self, reboot_type: crate::RebootType) -> Result<()> {
        self.inner.reboot(reboot_type)
    }

    #[inline]
    fn remount(&mut self) -> Result<Vec<RemountInfo>> {
        self.inner.remount()
    }

    #[inline]
    fn root(&mut self) -> Result<()> {
        self.inner.root()
    }

    #[inline]
    fn install(&mut self, apk_path: &dyn AsRef<Path>, user: Option<&str>) -> Result<()> {
        self.inner.install(apk_path, user)
    }

    #[inline]
    fn uninstall(&mut self, package: &dyn AsRef<str>, user: Option<&str>) -> Result<()> {
        self.inner.uninstall(package, user)
    }

    #[inline]
    fn enable_verity(&mut self) -> Result<()> {
        self.inner.enable_verity()
    }

    #[inline]
    fn disable_verity(&mut self) -> Result<()> {
        self.inner.disable_verity()
    }

    #[inline]
    #[cfg(feature = "framebuffer")]
    fn framebuffer_inner(&mut self) -> Result<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
        self.inner.framebuffer_inner()
    }

    #[inline]
    fn list(&mut self, path: &dyn AsRef<str>) -> Result<Vec<ADBListItemType>> {
        self.inner.list(path)
    }

    #[inline]
    fn exec(
        &mut self,
        command: &str,
        reader: &mut dyn Read,
        writer: Box<dyn Write + Send>,
    ) -> Result<()> {
        self.inner.exec(command, reader, writer)
    }
}

#[cfg(test)]
mod tests {
    use super::ADBTcpDevice;
    use crate::message_devices::{
        adb_connection_info::{A_VERSION_MIN, A_VERSION_SKIP_CHECKSUM, MAX_PAYLOAD, host_banner},
        adb_transport_message::{AUTH_RSAPUBLICKEY, AUTH_SIGNATURE, AUTH_TOKEN},
        message_commands::MessageCommand,
        tcp::test_support::{
            complete_server_handshake, drain, fake_adbd, read_message, shared_test_key, tls_server,
            write_message,
        },
    };
    use crate::{ADBDeviceExt, Result, RustADBError};
    use std::{
        io::ErrorKind,
        net::{SocketAddr, TcpStream},
        sync::{Arc, Mutex, mpsc},
        time::{Duration, Instant},
    };

    const DEVICE_BANNER: &[u8] = b"device::ro.product.name=sabrina;ro.product.model=Chromecast;\
features=shell_v2,cmd,stat_v2,ls_v2,fixed_push_mkdir,apex,abb,abb_exec,sendrecv_v2,delayed_ack";
    const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
    const TOKEN: [u8; 20] = [0x5a; 20];

    fn connect_with(address: SocketAddr, auth_timeout: Duration) -> Result<ADBTcpDevice> {
        ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(
            address,
            shared_test_key(),
            auth_timeout,
        )
    }

    fn connect(address: SocketAddr) -> Result<ADBTcpDevice> {
        connect_with(address, AUTH_TIMEOUT)
    }

    fn expect_host_cnxn(stream: &mut TcpStream) {
        let cnxn = read_message(stream).unwrap();
        assert_eq!(cnxn.header().command(), MessageCommand::Cnxn);
        assert_eq!(cnxn.header().arg0(), A_VERSION_MIN);
        assert_eq!(cnxn.header().arg1(), MAX_PAYLOAD);
        assert_eq!(cnxn.payload(), &host_banner());
    }

    fn send_device_cnxn(stream: &mut impl std::io::Write, max_payload: u32) {
        write_message(
            stream,
            MessageCommand::Cnxn,
            A_VERSION_SKIP_CHECKSUM,
            max_payload,
            DEVICE_BANNER,
        )
        .unwrap();
    }

    fn assert_timed_out(result: Result<ADBTcpDevice>) {
        match result {
            Err(RustADBError::IOError(error)) => assert_eq!(error.kind(), ErrorKind::TimedOut),
            Err(other) => panic!("expected a timeout, got {other:?}"),
            Ok(_) => panic!("expected a timeout, got a connection"),
        }
    }

    /// `held` is measured by the fake from a point where the client's
    /// handshake deadline is already running, so slow debug-build key loading
    /// beforehand does not count. Before this bound the wait was 120 s.
    fn assert_gave_up_within(held: Duration, auth_timeout: Duration) {
        assert!(
            held < auth_timeout + Duration::from_secs(3),
            "held for {held:?}"
        );
    }

    #[test]
    fn plain_cnxn_negotiates_features_and_payload() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            send_device_cnxn(&mut stream, 256 * 1024);
            drain(&mut stream);
        });
        let device = connect(address).unwrap();
        let info = device.connection_info();
        assert_eq!(info.protocol_version(), A_VERSION_MIN);
        assert_eq!(device.max_payload(), 256 * 1024);
        assert!(device.has_feature("abb_exec"));
        assert!(device.has_feature("sendrecv_v2"));
        assert!(!device.has_feature("delayed_ack"));
        assert!(!device.has_feature("abb"));
        assert!(info.device_features().contains("fixed_push_mkdir"));
        assert_eq!(info.property("ro.product.model"), Some("Chromecast"));
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn negotiated_payload_limit_bounds_incoming_messages() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            send_device_cnxn(&mut stream, 4096);
            let open = read_message(&mut stream).unwrap();
            assert_eq!(open.header().command(), MessageCommand::Open);
            let local_id = open.header().arg0();
            let _ = write_message(&mut stream, MessageCommand::Write, 1, local_id, &[0; 4097]);
            drain(&mut stream);
        });
        let mut device = connect(address).unwrap();
        let error = device
            .shell_command_with_timeout(&"true", None, None, Duration::from_secs(2))
            .unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn auth_signs_the_token_then_offers_the_public_key() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).unwrap();
            let signature = read_message(&mut stream).unwrap();
            assert_eq!(signature.header().command(), MessageCommand::Auth);
            assert_eq!(signature.header().arg0(), AUTH_SIGNATURE);
            assert_eq!(signature.payload().len(), 256);

            write_message(&mut stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).unwrap();
            let public_key = read_message(&mut stream).unwrap();
            assert_eq!(public_key.header().arg0(), AUTH_RSAPUBLICKEY);
            assert!(public_key.payload().starts_with(b"QAAAA"));
            assert_eq!(public_key.payload().last(), Some(&0));

            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let device = connect(address).unwrap();
        assert!(device.has_feature("shell_v2"));
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn auth_that_is_not_a_token_is_rejected() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Auth, AUTH_SIGNATURE, 0, &TOKEN).unwrap();
            drain(&mut stream);
        });
        let error = connect(address).unwrap_err();
        assert!(
            matches!(error, RustADBError::ADBRequestFailed(_)),
            "{error:?}"
        );
        server.join().unwrap();
    }

    #[test]
    fn endless_auth_tokens_are_bounded() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            loop {
                if write_message(&mut stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).is_err()
                    || read_message(&mut stream).is_err()
                {
                    break;
                }
            }
        });
        let error = connect(address).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
        server.join().unwrap();
    }

    #[test]
    fn a_stream_packet_during_the_handshake_fails_it() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Okay, 1, 2, &[]).unwrap();
            drain(&mut stream);
        });
        let error = connect(address).unwrap_err();
        assert!(
            matches!(error, RustADBError::WrongResponseReceived(..)),
            "{error:?}"
        );
        server.join().unwrap();
    }

    #[test]
    fn a_cnxn_with_zero_max_payload_is_rejected() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            send_device_cnxn(&mut stream, 0);
            drain(&mut stream);
        });
        let error = connect(address).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
        server.join().unwrap();
    }

    #[test]
    fn a_silent_device_times_out_within_the_auth_timeout() {
        let (held_tx, held_rx) = mpsc::channel();
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            let since_cnxn = Instant::now();
            drain(&mut stream);
            held_tx.send(since_cnxn.elapsed()).unwrap();
        });
        let auth_timeout = Duration::from_millis(300);
        assert_timed_out(connect_with(address, auth_timeout));
        server.join().unwrap();
        assert_gave_up_within(held_rx.recv().unwrap(), auth_timeout);
    }

    #[test]
    fn a_trickling_device_cannot_stretch_the_handshake_deadline() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            // Each reply arrives well inside a per-read timeout, but together
            // they outlast the handshake deadline, so this must not connect.
            for _ in 0..3 {
                std::thread::sleep(Duration::from_millis(200));
                if write_message(&mut stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).is_err()
                    || read_message(&mut stream).is_err()
                {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
            let _ = write_message(
                &mut stream,
                MessageCommand::Cnxn,
                A_VERSION_MIN,
                MAX_PAYLOAD,
                DEVICE_BANNER,
            );
            drain(&mut stream);
        });
        assert_timed_out(connect_with(address, Duration::from_millis(500)));
        server.join().unwrap();
    }

    type ServerTls = rustls::StreamOwned<rustls::ServerConnection, TcpStream>;
    type SeenCertificate = Arc<Mutex<Option<Vec<u8>>>>;

    fn stls_then<F>(
        trust_client: bool,
        after_handshake: F,
    ) -> (SocketAddr, std::thread::JoinHandle<()>, SeenCertificate)
    where
        F: FnOnce(ServerTls, std::io::Result<()>) + Send + 'static,
    {
        let seen = Arc::new(Mutex::new(None));
        let seen_by_server = Arc::clone(&seen);
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Stls, 0x0100_0000, 0, &[]).unwrap();
            let stls = read_message(&mut stream).unwrap();
            assert_eq!(stls.header().command(), MessageCommand::Stls);
            assert_eq!(stls.header().arg0(), 0x0100_0000);
            let mut tls = tls_server(stream, trust_client, seen_by_server);
            let handshake = complete_server_handshake(&mut tls);
            after_handshake(tls, handshake);
        });
        (address, server, seen)
    }

    #[test]
    fn tls_upgrade_presents_our_key_and_negotiates_over_tls() {
        let (address, server, seen) = stls_then(true, |mut tls, handshake| {
            handshake.unwrap();
            send_device_cnxn(&mut tls, MAX_PAYLOAD);
            drain(&mut tls);
        });
        let device = connect(address).unwrap();
        assert!(device.has_feature("sendrecv_v2"));
        drop(device);
        server.join().unwrap();

        let pem = std::fs::read_to_string(shared_test_key()).unwrap();
        let key =
            rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256).unwrap();
        let spki = rcgen::PublicKeyData::subject_public_key_info(&key);
        let presented = seen.lock().unwrap().clone().expect("client certificate");
        assert!(presented.windows(spki.len()).any(|window| window == spki));
    }

    #[test]
    fn a_refused_certificate_is_reported_as_not_paired() {
        let (address, server, _seen) = stls_then(false, |mut tls, handshake| {
            assert!(handshake.is_err());
            drain(&mut tls.sock);
        });
        let error = connect(address).unwrap_err();
        assert!(
            matches!(error, RustADBError::TlsAuthRejected(_)),
            "{error:?}"
        );
        server.join().unwrap();
    }

    #[test]
    fn a_hang_up_after_our_certificate_is_a_closed_connection_not_a_pairing_verdict() {
        let (address, server, _seen) = stls_then(true, |tls, handshake| {
            handshake.unwrap();
            drop(tls);
        });
        let error = connect(address).unwrap_err();
        assert!(
            matches!(error, RustADBError::TlsConnectionClosed(_)),
            "{error:?}"
        );
        server.join().unwrap();
    }

    #[test]
    fn a_silent_tls_peer_times_out_within_the_auth_timeout() {
        let (held_tx, held_rx) = mpsc::channel();
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Stls, 0x0100_0000, 0, &[]).unwrap();
            let _stls = read_message(&mut stream).unwrap();
            let since_stls = Instant::now();
            // Swallow the ClientHello and never answer it.
            drain(&mut stream);
            held_tx.send(since_stls.elapsed()).unwrap();
        });
        let auth_timeout = Duration::from_millis(400);
        assert_timed_out(connect_with(address, auth_timeout));
        server.join().unwrap();
        assert_gave_up_within(held_rx.recv().unwrap(), auth_timeout);
    }

    #[test]
    #[ignore = "requires ADB_CLIENT_TEST_ADDR and ADB_CLIENT_TEST_KEY"]
    fn shell_v2_live_separates_stderr_and_exit_status() {
        let address = std::env::var("ADB_CLIENT_TEST_ADDR")
            .expect("ADB_CLIENT_TEST_ADDR")
            .parse::<std::net::SocketAddr>()
            .expect("socket address");
        let key = std::env::var("ADB_CLIENT_TEST_KEY").expect("ADB_CLIENT_TEST_KEY");
        let mut device = ADBTcpDevice::new_with_custom_private_key(address, key).expect("connect");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = device
            .shell_command(
                &"sh -c 'printf out; printf err >&2; exit 7'",
                Some(&mut stdout),
                Some(&mut stderr),
            )
            .expect("shell command");

        assert_eq!(stdout, b"out");
        assert_eq!(stderr, b"err");
        assert_eq!(status, Some(7));
    }
}
