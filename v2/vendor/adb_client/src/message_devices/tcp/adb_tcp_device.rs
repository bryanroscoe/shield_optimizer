use std::io::Write;
use std::path::Path;
use std::time::Duration;
use std::{io::Read, net::SocketAddr};

use crate::message_devices::adb_message_device::{ADBMessageDevice, AcceptedKey};
use crate::models::RemountInfo;
use crate::tcp::PreferredKeys;
use crate::tcp::tcp_transport::TcpTransport;
use crate::utils::get_default_adb_key_path;
use crate::{
    ADBDeviceExt, ADBListItemType, ConnectionInfo, Result, RustADBError, ShellExit,
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

    /// Connect using an existing identity, failing with `IOError(NotFound)`
    /// before connecting rather than generating an ephemeral key when the
    /// identity has disappeared. `auth_timeout` bounds the whole handshake.
    pub fn new_with_required_private_key_and_auth_timeout<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Ok(Self {
            inner: ADBMessageDevice::new_with_required_key(
                TcpTransport::new(address, &private_key_path),
                private_key_path,
                auth_timeout,
            )?,
        })
    }

    /// Connect, calling `public_key_sent` once our RSA public key was written
    /// in answer to an AUTH token, when the device may be showing its
    /// "Allow debugging?" prompt. It does not run for an already trusted key,
    /// over TLS, or when that write fails.
    pub fn new_with_key_and_auth_observer<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        private_key_path: P,
        auth_timeout: Duration,
        require_existing_key: bool,
        public_key_sent: &dyn Fn(),
    ) -> Result<Self> {
        Ok(Self {
            inner: ADBMessageDevice::new_with_auth_observer(
                TcpTransport::new(address, &private_key_path),
                private_key_path,
                &PreferredKeys::none(),
                auth_timeout,
                require_existing_key,
                public_key_sent,
            )?,
        })
    }

    /// Authenticate under `control`: its deadline caps the one handshake
    /// deadline (TCP connect, TLS, AUTH rounds and the user's approval), and
    /// cancelling it shuts down only this socket. `public_key_sent` reports
    /// an AUTH public-key round as for [`Self::new_with_key_and_auth_observer`].
    /// The control keeps applying to the socket until [`Self::finish_connection`]
    /// hands it to its owner; a failed attempt closes its socket.
    pub fn new_with_control_and_auth_observer(
        address: SocketAddr,
        key: &Path,
        require_existing_key: bool,
        control: super::BoundedShellControl,
        public_key_sent: &dyn Fn(),
    ) -> Result<Self> {
        Self::new_with_keys_control_and_auth_observer(
            address,
            key,
            &PreferredKeys::none(),
            require_existing_key,
            control,
            public_key_sent,
        )
    }

    /// [`Self::new_with_control_and_auth_observer`], offering `preferred`
    /// before the key file as [`PreferredKeys`] describes: AUTH signs with
    /// each key in turn and then offers the first one's public key, and TLS
    /// presents the key the device's hints name, retrying a refused
    /// certificate once with each other key. With no preferred keys it is
    /// exactly that constructor.
    pub fn new_with_keys_control_and_auth_observer(
        address: SocketAddr,
        key: &Path,
        preferred: &PreferredKeys,
        require_existing_key: bool,
        control: super::BoundedShellControl,
        public_key_sent: &dyn Fn(),
    ) -> Result<Self> {
        let transport = TcpTransport::with_shell_control(address, key, control.clone())
            .with_preferred_keys(preferred);
        let result = control
            .remaining()
            .map_err(RustADBError::from)
            .and_then(|budget| {
                ADBMessageDevice::new_with_auth_observer(
                    transport,
                    key,
                    preferred,
                    budget,
                    require_existing_key,
                    public_key_sent,
                )
            });
        if result.is_err() {
            let _finish = super::bounded_shell::Finish(control);
        }
        result.map(|inner| Self { inner })
    }

    /// Detach the handshake deadline while transferring the live socket to
    /// its owner. Fails, and the connection must be dropped, once the
    /// control was cancelled or expired.
    pub fn finish_connection(&mut self) -> Result<()> {
        self.inner.get_transport_mut().finish_connect_control()
    }

    /// Connect with an existing key and one total deadline across the
    /// handshake (at most 30 seconds of it) and every later service.
    pub fn new_with_required_key_and_control(
        address: SocketAddr,
        key: &Path,
        control: &super::BoundedShellControl,
    ) -> Result<Self> {
        Self::new_with_required_keys_and_control(address, key, &PreferredKeys::none(), control)
    }

    /// [`Self::new_with_required_key_and_control`], offering `preferred`
    /// before the key file as [`PreferredKeys`] describes.
    pub fn new_with_required_keys_and_control(
        address: SocketAddr,
        key: &Path,
        preferred: &PreferredKeys,
        control: &super::BoundedShellControl,
    ) -> Result<Self> {
        Ok(Self {
            inner: super::connect_required(
                address,
                key,
                preferred,
                control,
                control.remaining()?.min(Duration::from_secs(30)),
            )?,
        })
    }

    /// Which of this connection's keys the device accepted: preferred keys
    /// count first, the key file last. `None` when it asked for no key.
    #[must_use]
    pub const fn accepted_key(&self) -> Option<AcceptedKey> {
        self.inner.accepted_key()
    }

    /// Run `command` once on this connection under a total deadline, with
    /// the shell chosen as for [`Self::shell_command_with_timeout`]. `None` is
    /// the legacy shell's missing exit status. Normal completion leaves the
    /// connection open; the deadline or a cancel can close it.
    pub fn shell_command_with_control(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
        control: super::BoundedShellControl,
    ) -> Result<Option<u8>> {
        self.with_control(control, |device, timeout| {
            device
                .shell_command_with_timeout(command, stdout, stderr, timeout)
                .map(ShellExit::code)
        })
    }

    /// Run `command` for binary output once on this connection under a
    /// total deadline, as [`Self::exec_out_with_timeout`] does: shell v2
    /// when negotiated, otherwise `exec:` (stderr merged, no exit status).
    /// Normal completion leaves the connection open; the deadline or a
    /// cancel can close it.
    pub fn exec_out_with_control(
        &mut self,
        command: &str,
        stdout: &mut dyn Write,
        stderr: Option<&mut dyn Write>,
        control: super::BoundedShellControl,
    ) -> Result<ShellExit> {
        self.with_control(control, |device, timeout| {
            device.exec_out_with_timeout(command, stdout, stderr, timeout)
        })
    }

    /// Uploads once under a total deadline, without retrying uncertain transfer results.
    pub fn push_with_control(
        &mut self,
        input: &mut dyn Read,
        path: &dyn AsRef<str>,
        control: super::BoundedShellControl,
    ) -> Result<()> {
        self.with_control(control, |device, _| device.push(input, path))
    }

    /// Downloads once under a total deadline while retaining the caller's output policy.
    pub fn pull_with_control(
        &mut self,
        source: &dyn AsRef<str>,
        output: &mut dyn Write,
        control: super::BoundedShellControl,
    ) -> Result<()> {
        self.with_control(control, |device, _| {
            device.pull(source, &mut *output)?;
            output.flush().map_err(RustADBError::LocalIo)?;
            Ok(())
        })
    }

    /// Sends the reboot request once; acceptance still requires the service OPEN acknowledgement.
    pub fn reboot_with_control(
        &mut self,
        reboot_type: crate::RebootType,
        control: super::BoundedShellControl,
    ) -> Result<()> {
        self.with_control(control, |device, _| device.reboot(reboot_type))
    }

    fn with_control<T, F>(&mut self, control: super::BoundedShellControl, run: F) -> Result<T>
    where
        F: FnOnce(&mut ADBMessageDevice<TcpTransport>, Duration) -> Result<T>,
    {
        struct Restore<'a> {
            device: &'a mut ADBMessageDevice<TcpTransport>,
            previous: Option<super::BoundedShellControl>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.device
                    .get_transport_mut()
                    .restore_control(self.previous.take());
            }
        }
        let finish = super::bounded_shell::Finish(control);
        let previous = self
            .inner
            .get_transport_mut()
            .set_current_control(finish.0.clone())?;
        let restore = Restore {
            device: &mut self.inner,
            previous,
        };
        let result = run(&mut *restore.device, finish.0.remaining()?);
        if result.is_ok() {
            finish.0.remaining()?;
        }
        result
    }

    /// Reads a bounded, successful shell-v2 hardware identity on this owned connection.
    /// A successful query preserves the connection; deadline/cancel can close it.
    pub fn hardware_serial_bounded(
        &mut self,
        control: super::BoundedShellControl,
    ) -> Result<String> {
        super::streamed_install::read_hardware_serial_on_current(&mut self.inner, control)
    }

    /// Verifies this dedicated connection before any caller-selected operation.
    pub fn verify_hardware_binding_bounded(
        &mut self,
        expected: &str,
        control: super::BoundedShellControl,
    ) -> Result<()> {
        let finish = super::bounded_shell::Finish(control);
        super::streamed_install::verify_hardware_binding(&mut self.inner, expected, &finish.0)
    }

    #[cfg(test)]
    pub(crate) const fn from_fixture_device(inner: ADBMessageDevice<TcpTransport>) -> Self {
        Self { inner }
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
    use crate::{ADBDeviceExt, Result, RustADBError, ShellExit};
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

    // ---- controlled connections (Shield Optimizer) ----

    use super::super::BoundedShellControl;
    use super::super::streamed_install::tests::{
        fixture_key, hello_with_features, receive, reply, send,
    };

    fn controlled_fixture(address: SocketAddr) -> ADBTcpDevice {
        let transport = super::TcpTransport::new(
            address,
            std::path::Path::new("fixture-in-memory-never-written"),
        );
        ADBTcpDevice::from_fixture_device(
            super::ADBMessageDevice::new_with_fixture_key(
                transport,
                &fixture_key(),
                Duration::from_secs(2),
            )
            .unwrap(),
        )
    }

    fn v2_hello(socket: &mut TcpStream) {
        hello_with_features(socket, "shell_v2");
    }

    fn shell_packet(channel: u8, bytes: &[u8]) -> Vec<u8> {
        let mut packet = vec![channel];
        packet.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
        packet.extend_from_slice(bytes);
        packet
    }

    fn fixture_listener() -> (std::net::TcpListener, SocketAddr) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        (listener, address)
    }

    #[test]
    fn controlled_owned_shell_normal_completion_preserves_socket_and_queued_cancel_cannot_close_it()
    {
        let (listener, address) = fixture_listener();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            v2_hello(&mut socket);
            for command in [b"shell,v2,raw:one\0".as_slice(), b"shell,v2,raw:two\0"] {
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), command);
                let local = open.header().arg0();
                send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
                reply(&mut socket, local, &shell_packet(1, b"ok"), false);
                reply(&mut socket, local, &shell_packet(3, &[0]), true);
            }
        });
        let mut device = controlled_fixture(address);
        let first = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let mut out = Vec::new();
        assert_eq!(
            device
                .shell_command_with_control(&"one", Some(&mut out), None, first.clone())
                .unwrap(),
            Some(0)
        );
        assert!(first.was_registered());
        assert_eq!(out, b"ok");
        let canceled = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        canceled.cancel();
        assert!(
            device
                .shell_command_with_control(&"never", None, None, canceled.clone())
                .is_err()
        );
        assert!(!canceled.was_registered());
        assert_eq!(
            device
                .shell_command_with_control(
                    &"two",
                    None,
                    None,
                    BoundedShellControl::new(Duration::from_secs(2)).unwrap()
                )
                .unwrap(),
            Some(0)
        );
        peer.join().unwrap();
    }

    #[test]
    fn controlled_owned_shell_total_deadline_survives_continuous_output_without_replay() {
        let (listener, address) = fixture_listener();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            v2_hello(&mut socket);
            let open = receive(&mut socket).unwrap();
            let local = open.header().arg0();
            send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            for _ in 0..100 {
                if send(
                    &mut socket,
                    MessageCommand::Write,
                    local,
                    &shell_packet(1, b"x"),
                )
                .is_err()
                {
                    return;
                }
                match receive(&mut socket) {
                    Ok(message) if message.header().command() == MessageCommand::Okay => {}
                    _ => return,
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("total deadline failed to interrupt continuous output");
        });
        let mut device = controlled_fixture(address);
        let control = BoundedShellControl::new(Duration::from_millis(70)).unwrap();
        let started = Instant::now();
        let mut out = Vec::new();
        assert!(
            device
                .shell_command_with_control(&"once", Some(&mut out), None, control)
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!out.is_empty());
        peer.join().unwrap();
    }

    #[test]
    fn controlled_owned_shell_cancellation_interrupts_only_admitted_socket() {
        let (listener, address) = fixture_listener();
        let (started, ready) = mpsc::channel();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            v2_hello(&mut socket);
            let open = receive(&mut socket).unwrap();
            send(&mut socket, MessageCommand::Okay, open.header().arg0(), b"").unwrap();
            started.send(()).unwrap();
            assert!(receive(&mut socket).is_err());
        });
        let mut device = controlled_fixture(address);
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let cancel = control.clone();
        let canceler = std::thread::spawn(move || {
            ready.recv().unwrap();
            cancel.cancel();
        });
        assert!(
            device
                .shell_command_with_control(&"once", None, None, control.clone())
                .is_err()
        );
        assert!(control.was_registered());
        canceler.join().unwrap();
        peer.join().unwrap();
    }

    #[test]
    fn controlled_owned_shell_keeps_explicit_v1_rejection_fallback() {
        let (listener, address) = fixture_listener();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            v2_hello(&mut socket);
            let rejected = receive(&mut socket).unwrap();
            assert_eq!(rejected.payload(), b"shell,v2,raw:legacy\0");
            send(
                &mut socket,
                MessageCommand::Clse,
                rejected.header().arg0(),
                b"",
            )
            .unwrap();
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.payload(), b"shell:legacy\0");
            let local = open.header().arg0();
            send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            reply(&mut socket, local, b"legacy", true);
        });
        let mut device = controlled_fixture(address);
        let mut out = Vec::new();
        assert_eq!(
            device
                .shell_command_with_control(
                    &"legacy",
                    Some(&mut out),
                    None,
                    BoundedShellControl::new(Duration::from_secs(2)).unwrap()
                )
                .unwrap(),
            None
        );
        assert_eq!(out, b"legacy");
        peer.join().unwrap();
    }

    #[test]
    fn controlled_owned_shell_ambiguous_open_never_replays_as_v1() {
        for malformed in [false, true] {
            let (listener, address) = fixture_listener();
            let peer = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                v2_hello(&mut socket);
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), b"shell,v2,raw:once\0");
                if malformed {
                    send(
                        &mut socket,
                        MessageCommand::Write,
                        open.header().arg0(),
                        b"not-an-open-ack",
                    )
                    .unwrap();
                }
                assert!(
                    receive(&mut socket).is_err(),
                    "ambiguous OPEN must not replay as v1"
                );
            });
            let mut device = controlled_fixture(address);
            assert!(
                device
                    .shell_command_with_control(
                        &"once",
                        None,
                        None,
                        BoundedShellControl::new(Duration::from_millis(70)).unwrap()
                    )
                    .is_err()
            );
            drop(device);
            peer.join().unwrap();
        }
    }

    #[test]
    fn controlled_owned_transfer_pre_cancel_preserves_socket_for_following_command() {
        let (listener, address) = fixture_listener();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            v2_hello(&mut socket);
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.payload(), b"shell,v2,raw:after\0");
            let local = open.header().arg0();
            send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            reply(&mut socket, local, &shell_packet(3, &[0]), true);
        });
        let mut device = controlled_fixture(address);
        let canceled = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        canceled.cancel();
        assert!(
            device
                .push_with_control(
                    &mut std::io::Cursor::new(b"fixture"),
                    &"/fixture",
                    canceled.clone()
                )
                .is_err()
        );
        assert!(
            device
                .pull_with_control(&"/fixture", &mut Vec::new(), canceled.clone())
                .is_err()
        );
        assert!(
            device
                .reboot_with_control(crate::RebootType::System, canceled.clone())
                .is_err()
        );
        assert!(!canceled.was_registered());
        assert_eq!(
            device
                .shell_command_with_control(
                    &"after",
                    None,
                    None,
                    BoundedShellControl::new(Duration::from_secs(2)).unwrap()
                )
                .unwrap(),
            Some(0)
        );
        peer.join().unwrap();
    }

    #[test]
    fn controlled_owned_reboot_requires_ack_and_does_not_replay_lost_ack() {
        for acknowledge in [false, true] {
            let (listener, address) = fixture_listener();
            let peer = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                v2_hello(&mut socket);
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), b"reboot:\0");
                if acknowledge {
                    send(&mut socket, MessageCommand::Okay, open.header().arg0(), b"").unwrap();
                }
                assert!(
                    receive(&mut socket).is_err(),
                    "reboot must send one OPEN only"
                );
            });
            let mut device = controlled_fixture(address);
            let result = device.reboot_with_control(
                crate::RebootType::System,
                BoundedShellControl::new(Duration::from_millis(100)).unwrap(),
            );
            assert_eq!(result.is_ok(), acknowledge);
            drop(device);
            peer.join().unwrap();
        }
    }

    /// The fake's side of a key the device does not know yet: a token, our
    /// signature, a second token, our public key.
    fn expect_public_key_round(stream: &mut TcpStream) {
        write_message(stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).unwrap();
        assert_eq!(
            read_message(stream).unwrap().header().arg0(),
            AUTH_SIGNATURE
        );
        write_message(stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).unwrap();
        assert_eq!(
            read_message(stream).unwrap().header().arg0(),
            AUTH_RSAPUBLICKEY
        );
    }

    #[test]
    fn a_controlled_handshake_reports_the_approval_prompt_and_outlives_its_deadline_once_handed_over()
     {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            expect_public_key_round(&mut stream);
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            // Past the handshake control's deadline the socket still works.
            let open = read_message(&mut stream).unwrap();
            assert_eq!(open.payload(), b"shell,v2,raw:echo ok\0");
            let local = open.header().arg0();
            write_message(&mut stream, MessageCommand::Okay, 1, local, &[]).unwrap();
            for packet in [shell_packet(1, b"ok\n"), shell_packet(3, &[0])] {
                write_message(&mut stream, MessageCommand::Write, 1, local, &packet).unwrap();
                assert_eq!(
                    read_message(&mut stream).unwrap().header().command(),
                    MessageCommand::Okay
                );
            }
            write_message(&mut stream, MessageCommand::Clse, 1, local, &[]).unwrap();
            drain(&mut stream);
        });
        let key = shared_test_key();
        let control = BoundedShellControl::new(Duration::from_millis(1500)).unwrap();
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let mut device = ADBTcpDevice::new_with_control_and_auth_observer(
            address,
            key,
            true,
            control.clone(),
            &|| {
                prompts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .unwrap();
        assert_eq!(prompts.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(control.was_registered());
        device.finish_connection().unwrap();
        // A second hand-over has no control left to release.
        assert!(device.finish_connection().is_err());
        std::thread::sleep(Duration::from_millis(1600));
        let mut out = Vec::new();
        assert_eq!(
            device
                .shell_command_with_timeout(&"echo ok", Some(&mut out), None, AUTH_TIMEOUT)
                .unwrap(),
            ShellExit::Code(0)
        );
        assert_eq!(out, b"ok\n");
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn a_trusted_key_never_reports_an_approval_prompt() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).unwrap();
            assert_eq!(
                read_message(&mut stream).unwrap().header().arg0(),
                AUTH_SIGNATURE
            );
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let device = ADBTcpDevice::new_with_key_and_auth_observer(
            address,
            shared_test_key(),
            AUTH_TIMEOUT,
            true,
            &|| panic!("a trusted key showed no prompt"),
        )
        .unwrap();
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn cancelling_a_controlled_handshake_during_approval_closes_its_socket() {
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            expect_public_key_round(&mut stream);
            // The user never answers; only the cancel may end the wait.
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            assert!(
                read_message(&mut stream).is_err(),
                "the canceled socket stayed open"
            );
        });
        let key = shared_test_key();
        let control = BoundedShellControl::new(Duration::from_secs(10)).unwrap();
        let signal = control.clone();
        let started = Instant::now();
        let error =
            ADBTcpDevice::new_with_control_and_auth_observer(address, key, true, control, &|| {
                signal.cancel();
            })
            .unwrap_err();
        assert!(
            matches!(&error, RustADBError::IOError(io) if io.kind() == ErrorKind::ConnectionAborted),
            "{error:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }

    #[test]
    fn a_controlled_handshake_ends_at_the_control_deadline_and_cannot_be_handed_over_after_it() {
        let (held_tx, held_rx) = mpsc::channel();
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            let since_cnxn = Instant::now();
            drain(&mut stream);
            held_tx.send(since_cnxn.elapsed()).unwrap();
        });
        let key = shared_test_key();
        let control = BoundedShellControl::new(Duration::from_millis(300)).unwrap();
        let result =
            ADBTcpDevice::new_with_control_and_auth_observer(address, key, true, control, &|| {
                panic!("no AUTH challenge was sent")
            });
        assert_timed_out(result);
        server.join().unwrap();
        assert_gave_up_within(held_rx.recv().unwrap(), Duration::from_millis(300));

        // Approved, but the control expired before publication.
        let (address, server) = fake_adbd(|mut stream| {
            expect_host_cnxn(&mut stream);
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let control = BoundedShellControl::new(Duration::from_millis(400)).unwrap();
        let mut device =
            ADBTcpDevice::new_with_control_and_auth_observer(address, key, true, control, &|| {})
                .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(device.finish_connection().is_err());
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn required_key_constructors_refuse_a_missing_key_before_connecting() {
        let (listener, address) = fixture_listener();
        listener.set_nonblocking(true).unwrap();
        let missing = std::env::temp_dir().join("adb_client_device_missing_key.pem");
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        for result in [
            ADBTcpDevice::new_with_required_private_key_and_auth_timeout(
                address,
                &missing,
                AUTH_TIMEOUT,
            ),
            ADBTcpDevice::new_with_required_key_and_control(address, &missing, &control),
            ADBTcpDevice::new_with_control_and_auth_observer(
                address,
                &missing,
                true,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
                &|| {},
            ),
        ] {
            assert!(
                matches!(&result, Err(RustADBError::IOError(io)) if io.kind() == ErrorKind::NotFound),
                "{result:?}"
            );
        }
        assert!(listener.accept().is_err(), "a connection was made");
        assert!(!missing.exists());
    }

    // ---- several keys: preferred keys before the key file ----

    use super::super::{
        AcceptedKey, PreferredKeys,
        preferred_keys::tests::second_test_pem,
        test_support::{ClientTrust, fake_adbd_connections, tls_server_with},
        tls_identity::adb_ca_hint,
    };
    use rsa::{
        Pkcs1v15Sign, RsaPublicKey,
        pkcs8::{DecodePrivateKey, EncodePublicKey},
    };

    /// The preferred key (index 0) and the key file (index 1).
    fn two_keys() -> (PreferredKeys, RsaPublicKey, RsaPublicKey) {
        let preferred = PreferredKeys::from_pems([second_test_pem()]).unwrap();
        let first = rsa::RsaPrivateKey::from_pkcs8_pem(second_test_pem())
            .unwrap()
            .to_public_key();
        let file_pem = std::fs::read_to_string(shared_test_key()).unwrap();
        let second = rsa::RsaPrivateKey::from_pkcs8_pem(&file_pem)
            .unwrap()
            .to_public_key();
        (preferred, first, second)
    }

    fn spki(key: &RsaPublicKey) -> Vec<u8> {
        key.to_public_key_der().unwrap().as_bytes().to_vec()
    }

    fn fingerprint(key: &RsaPublicKey) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(spki(key)).into()
    }

    fn signed_by(key: &RsaPublicKey, signature: &[u8]) -> bool {
        key.verify(Pkcs1v15Sign::new::<sha1::Sha1>(), &TOKEN, signature)
            .is_ok()
    }

    fn connect_with_keys(
        address: SocketAddr,
        preferred: &PreferredKeys,
        prompts: &std::sync::atomic::AtomicUsize,
    ) -> Result<ADBTcpDevice> {
        let control = BoundedShellControl::new(AUTH_TIMEOUT).unwrap();
        let mut device = ADBTcpDevice::new_with_keys_control_and_auth_observer(
            address,
            shared_test_key(),
            preferred,
            true,
            control,
            &|| {
                prompts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )?;
        device.finish_connection()?;
        Ok(device)
    }

    /// Send an AUTH token and return the host's answer.
    fn auth_round(
        stream: &mut TcpStream,
    ) -> crate::message_devices::adb_transport_message::ADBTransportMessage {
        write_message(stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).unwrap();
        let answer = read_message(stream).unwrap();
        assert_eq!(answer.header().command(), MessageCommand::Auth);
        answer
    }

    #[test]
    fn each_auth_token_is_signed_with_the_next_key_in_order() {
        let (preferred, first, second) = two_keys();
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            let answer = auth_round(&mut stream);
            assert_eq!(answer.header().arg0(), AUTH_SIGNATURE);
            assert!(
                signed_by(&first, answer.payload()),
                "token 1 not signed by the first key"
            );
            let answer = auth_round(&mut stream);
            assert_eq!(answer.header().arg0(), AUTH_SIGNATURE);
            assert!(
                signed_by(&second, answer.payload()),
                "token 2 not signed by the key file"
            );
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(device.accepted_key(), Some(AcceptedKey::Signature(1)));
        assert_eq!(prompts.load(std::sync::atomic::Ordering::Relaxed), 0);
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn a_first_signature_accepted_needs_no_second() {
        let (preferred, first, _) = two_keys();
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            let answer = auth_round(&mut stream);
            assert!(signed_by(&first, answer.payload()));
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(device.accepted_key(), Some(AcceptedKey::Signature(0)));
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn after_every_signature_is_refused_the_first_key_is_offered_once() {
        let (preferred, _, _) = two_keys();
        let mut expected = preferred.keys()[0].public_key.clone().into_bytes();
        expected.push(0);
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            for _ in 0..2 {
                assert_eq!(auth_round(&mut stream).header().arg0(), AUTH_SIGNATURE);
            }
            for _ in 0..2 {
                let answer = auth_round(&mut stream);
                assert_eq!(answer.header().arg0(), AUTH_RSAPUBLICKEY);
                assert_eq!(answer.payload(), expected.as_slice());
            }
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(device.accepted_key(), Some(AcceptedKey::PublicKey(0)));
        assert_eq!(prompts.load(std::sync::atomic::Ordering::Relaxed), 1);
        drop(device);
        server.join().unwrap();
    }

    /// Answers AUTH tokens until the host stops, returning how many it answered.
    fn count_answered_tokens(mut stream: TcpStream) -> usize {
        expect_host_cnxn(&mut stream);
        let mut answered = 0;
        while write_message(&mut stream, MessageCommand::Auth, AUTH_TOKEN, 0, &TOKEN).is_ok()
            && read_message(&mut stream).is_ok()
        {
            answered += 1;
        }
        answered
    }

    #[test]
    fn endless_tokens_get_one_signature_per_key_then_three_public_keys() {
        let (answered_tx, answered_rx) = mpsc::channel();
        let single = answered_tx.clone();
        let (address, server) = fake_adbd(move |stream| {
            single.send(count_answered_tokens(stream)).unwrap();
        });
        let error = connect(address).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
        server.join().unwrap();
        assert_eq!(answered_rx.recv().unwrap(), 4, "one key: unchanged");

        let (preferred, _, _) = two_keys();
        let (address, server) = fake_adbd(move |stream| {
            answered_tx.send(count_answered_tokens(stream)).unwrap();
        });
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let error = connect_with_keys(address, &preferred, &prompts).unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
        server.join().unwrap();
        assert_eq!(answered_rx.recv().unwrap(), 5);
        assert_eq!(prompts.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn no_preferred_keys_answer_auth_exactly_as_the_key_file_alone() {
        let (_, _, file_key) = two_keys();
        let (address, server) = fake_adbd(move |mut stream| {
            expect_host_cnxn(&mut stream);
            let answer = auth_round(&mut stream);
            assert!(signed_by(&file_key, answer.payload()));
            let answer = auth_round(&mut stream);
            assert_eq!(answer.header().arg0(), AUTH_RSAPUBLICKEY);
            send_device_cnxn(&mut stream, MAX_PAYLOAD);
            drain(&mut stream);
        });
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &PreferredKeys::none(), &prompts).unwrap();
        assert_eq!(device.accepted_key(), Some(AcceptedKey::PublicKey(0)));
        assert_eq!(prompts.load(std::sync::atomic::Ordering::Relaxed), 1);
        drop(device);
        server.join().unwrap();
    }

    type Presented = Arc<Mutex<Vec<Vec<u8>>>>;

    /// A TLS adbd for `connections` connections that trusts only `trusted`
    /// and hints `hinted`. Records each presented certificate in order.
    fn tls_adbd(
        connections: usize,
        trusted: Vec<Vec<u8>>,
        hinted: Vec<[u8; 32]>,
    ) -> (SocketAddr, std::thread::JoinHandle<()>, Presented) {
        let presented: Presented = Arc::default();
        let record = Arc::clone(&presented);
        let (address, server) = fake_adbd_connections(connections, move |mut stream, _| {
            expect_host_cnxn(&mut stream);
            write_message(&mut stream, MessageCommand::Stls, 0x0100_0000, 0, &[]).unwrap();
            assert_eq!(
                read_message(&mut stream).unwrap().header().command(),
                MessageCommand::Stls
            );
            let seen = Arc::new(Mutex::new(None));
            let hints = hinted.iter().map(adb_ca_hint).collect();
            let mut tls = tls_server_with(
                stream,
                ClientTrust::PublicKeys(trusted.clone()),
                hints,
                Arc::clone(&seen),
            );
            let handshake = complete_server_handshake(&mut tls);
            record
                .lock()
                .unwrap()
                .push(seen.lock().unwrap().clone().expect("client certificate"));
            if handshake.is_ok() {
                send_device_cnxn(&mut tls, MAX_PAYLOAD);
                drain(&mut tls);
            } else {
                drain(&mut tls.sock);
            }
        });
        (address, server, presented)
    }

    fn carries(certificate: &[u8], key: &RsaPublicKey) -> bool {
        let spki = spki(key);
        certificate
            .windows(spki.len())
            .any(|window| window == spki.as_slice())
    }

    #[test]
    fn tls_presents_the_key_the_device_hints_at_first_try() {
        let (preferred, first, second) = two_keys();
        let (address, server, presented) = tls_adbd(
            1,
            vec![spki(&second)],
            vec![[0x11; 32], fingerprint(&second)],
        );
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(
            device.accepted_key(),
            Some(AcceptedKey::Tls {
                index: 1,
                hinted: true,
                refused: 0
            })
        );
        drop(device);
        server.join().unwrap();
        let presented = presented.lock().unwrap();
        assert_eq!(presented.len(), 1);
        assert!(carries(&presented[0], &second) && !carries(&presented[0], &first));
    }

    #[test]
    fn without_a_matching_hint_tls_presents_the_first_key() {
        let (preferred, first, _) = two_keys();
        let (address, server, presented) = tls_adbd(1, vec![spki(&first)], vec![[0x22; 32]]);
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(
            device.accepted_key(),
            Some(AcceptedKey::Tls {
                index: 0,
                hinted: false,
                refused: 0
            })
        );
        drop(device);
        server.join().unwrap();
        assert!(carries(&presented.lock().unwrap()[0], &first));
    }

    #[test]
    fn a_refused_certificate_is_retried_once_with_the_next_key() {
        let (preferred, first, second) = two_keys();
        // No hints, so the first key goes first and is refused.
        let (address, server, presented) = tls_adbd(2, vec![spki(&second)], Vec::new());
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(
            device.accepted_key(),
            Some(AcceptedKey::Tls {
                index: 1,
                hinted: false,
                refused: 1
            })
        );
        assert!(device.has_feature("shell_v2"));
        drop(device);
        server.join().unwrap();
        let presented = presented.lock().unwrap();
        assert_eq!(presented.len(), 2);
        assert!(carries(&presented[0], &first));
        assert!(carries(&presented[1], &second));
    }

    #[test]
    fn a_wrong_hint_falls_back_to_the_other_key_once() {
        let (preferred, first, second) = two_keys();
        // The hint names the first key but the device trusts only the second.
        let (address, server, presented) =
            tls_adbd(2, vec![spki(&second)], vec![fingerprint(&first)]);
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let device = connect_with_keys(address, &preferred, &prompts).unwrap();
        assert_eq!(
            device.accepted_key(),
            Some(AcceptedKey::Tls {
                index: 1,
                hinted: false,
                refused: 1
            })
        );
        drop(device);
        server.join().unwrap();
        assert_eq!(presented.lock().unwrap().len(), 2);
    }

    #[test]
    fn when_every_certificate_is_refused_each_is_tried_once_and_the_refusal_returned() {
        let (preferred, first, second) = two_keys();
        let (address, server, presented) = tls_adbd(2, Vec::new(), Vec::new());
        let prompts = std::sync::atomic::AtomicUsize::new(0);
        let error = connect_with_keys(address, &preferred, &prompts).unwrap_err();
        assert!(
            matches!(error, RustADBError::TlsAuthRejected(_)),
            "{error:?}"
        );
        server.join().unwrap();
        let presented = presented.lock().unwrap();
        assert_eq!(presented.len(), 2);
        assert!(carries(&presented[0], &first));
        assert!(carries(&presented[1], &second));
    }

    #[test]
    fn one_key_presents_it_despite_hints_and_never_retries() {
        let (_, _, second) = two_keys();
        let (address, server, presented) = tls_adbd(1, Vec::new(), vec![[0x33; 32]]);
        let error = connect(address).unwrap_err();
        assert!(
            matches!(error, RustADBError::TlsAuthRejected(_)),
            "{error:?}"
        );
        server.join().unwrap();
        let presented = presented.lock().unwrap();
        assert_eq!(presented.len(), 1);
        assert!(carries(&presented[0], &second));
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
