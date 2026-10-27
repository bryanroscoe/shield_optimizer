use std::io::Write;
use std::path::Path;
use std::time::Duration;
use std::{io::Read, net::SocketAddr};

use crate::message_devices::adb_message_device::ADBMessageDevice;
use crate::models::RemountInfo;
use crate::tcp::tcp_transport::TcpTransport;
use crate::utils::get_default_adb_key_path;
use crate::{
    ADBDeviceExt, ADBListItemType, Result,
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

    /// Instantiate an [`ADBTcpDevice`] using a custom private key path and a
    /// finite timeout for each connection and RSA-authorization response.
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

    /// Connect using an existing identity, failing rather than generating an
    /// ephemeral key when the identity disappears before the handshake.
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

    /// Connect with an existing key and one total deadline across handshake and later services.
    pub fn new_with_required_key_and_control(
        address: SocketAddr,
        key: &Path,
        control: super::BoundedShellControl,
    ) -> Result<Self> {
        let transport = TcpTransport::with_shell_control(address, key, control.clone());
        Ok(Self {
            inner: ADBMessageDevice::new_with_required_key(
                transport,
                key,
                control.remaining()?.min(Duration::from_secs(30)),
            )?,
        })
    }

    /// Runs once on this owned connection under a total deadline; normal completion preserves it.
    pub fn shell_command_with_control(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
        control: super::BoundedShellControl,
    ) -> Result<Option<u8>> {
        self.with_control(control, |device, timeout| {
            device.shell_command_prefer_v2_with_timeout(command, stdout, stderr, timeout)
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
            output.flush()?;
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
    pub(crate) fn from_fixture_device(inner: ADBMessageDevice<TcpTransport>) -> Self {
        Self { inner }
    }

    /// Run a shell command with a finite timeout for each transport response.
    /// This is useful for liveness probes, where an unbounded read would keep
    /// the owning connection mutex locked forever after a network loss.
    pub fn shell_command_with_timeout(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<Option<u8>> {
        self.inner
            .shell_command_prefer_v2_with_timeout(command, stdout, stderr, timeout)
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
    use crate::ADBDeviceExt;

    fn controlled_fixture(address: std::net::SocketAddr) -> ADBTcpDevice {
        let transport = super::TcpTransport::new(
            address,
            std::path::Path::new("fixture-in-memory-never-written"),
        );
        ADBTcpDevice::from_fixture_device(
            super::ADBMessageDevice::new_with_fixture_key(
                transport,
                &super::super::streamed_install::tests::fixture_key(),
                std::time::Duration::from_secs(2),
            )
            .unwrap(),
        )
    }
    fn shell_packet(channel: u8, bytes: &[u8]) -> Vec<u8> {
        let mut packet = vec![channel];
        packet.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
        packet.extend_from_slice(bytes);
        packet
    }
    #[test]
    fn controlled_owned_shell_normal_completion_preserves_socket_and_queued_cancel_cannot_close_it()
    {
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, reply, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
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
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::{Duration, Instant};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
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
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (started, ready) = std::sync::mpsc::channel();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
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
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, reply, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
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
            assert_eq!(open.payload(), b"shell:legacy");
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
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        for malformed in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                hello(&mut socket);
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
    fn controlled_dedicated_sync_deadline_interrupts_stat_and_continuous_download() {
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{fixture_key, hello, receive, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::{Duration, Instant};
        for (download, borrowed) in [(false, false), (true, false), (true, true)] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                hello(&mut socket);
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), b"sync:");
                let local = open.header().arg0();
                send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
                for _ in 0..2 {
                    assert_eq!(
                        receive(&mut socket).unwrap().header().command(),
                        MessageCommand::Write
                    );
                    send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
                }
                if !download {
                    assert!(receive(&mut socket).is_err());
                    return;
                }
                let mut stat = b"STAT".to_vec();
                for value in [0o100644u32, 1000, 0] {
                    stat.extend_from_slice(&value.to_le_bytes());
                }
                send(&mut socket, MessageCommand::Write, local, &stat).unwrap();
                assert_eq!(
                    receive(&mut socket).unwrap().header().command(),
                    MessageCommand::Okay
                );
                for _ in 0..2 {
                    assert_eq!(
                        receive(&mut socket).unwrap().header().command(),
                        MessageCommand::Write
                    );
                    send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
                }
                for _ in 0..100 {
                    let mut data = b"DATA".to_vec();
                    data.extend_from_slice(&1u32.to_le_bytes());
                    data.push(b'x');
                    if send(&mut socket, MessageCommand::Write, local, &data).is_err() {
                        return;
                    }
                    match receive(&mut socket) {
                        Ok(message) if message.header().command() == MessageCommand::Okay => {}
                        _ => return,
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                panic!("continuous download outlived total deadline");
            });
            let control = BoundedShellControl::new(Duration::from_millis(100)).unwrap();
            let _finish = super::super::bounded_shell::Finish(control.clone());
            let transport = super::TcpTransport::with_shell_control(
                address,
                std::path::Path::new("fixture-in-memory-never-written"),
                control.clone(),
            );
            let mut device = if borrowed {
                controlled_fixture(address)
            } else {
                ADBTcpDevice::from_fixture_device(
                    super::ADBMessageDevice::new_with_fixture_key(
                        transport,
                        &fixture_key(),
                        Duration::from_secs(2),
                    )
                    .unwrap(),
                )
            };
            let started = Instant::now();
            let mut out = Vec::new();
            let result = if download && borrowed {
                device.pull_with_control(&"/fixture", &mut out, control.clone())
            } else if download {
                device.pull(&"/fixture", &mut out)
            } else {
                device.stat(&"/fixture").map(|_| ())
            };
            assert!(result.is_err());
            assert!(started.elapsed() < Duration::from_secs(2));
            if download {
                assert!(!out.is_empty());
            }
            peer.join().unwrap();
        }
    }

    #[test]
    fn controlled_owned_pull_preserves_bytes_and_flushes_before_success() {
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, reply, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        struct Output {
            bytes: Vec<u8>,
            flushed: bool,
        }
        impl std::io::Write for Output {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.flushed = true;
                Ok(())
            }
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.payload(), b"sync:");
            let local = open.header().arg0();
            send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            for _ in 0..2 {
                assert_eq!(
                    receive(&mut socket).unwrap().header().command(),
                    MessageCommand::Write
                );
                send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            }
            let mut stat = b"STAT".to_vec();
            for value in [0o100644u32, 3, 0] {
                stat.extend_from_slice(&value.to_le_bytes());
            }
            send(&mut socket, MessageCommand::Write, local, &stat).unwrap();
            assert_eq!(
                receive(&mut socket).unwrap().header().command(),
                MessageCommand::Okay
            );
            for _ in 0..2 {
                assert_eq!(
                    receive(&mut socket).unwrap().header().command(),
                    MessageCommand::Write
                );
                send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            }
            let mut data = b"DATA".to_vec();
            data.extend_from_slice(&3u32.to_le_bytes());
            data.extend_from_slice(b"abcDONE");
            data.extend_from_slice(&0u32.to_le_bytes());
            reply(&mut socket, local, &data, false);
            let quit = receive(&mut socket).unwrap();
            assert!(quit.payload().starts_with(b"QUIT"));
            send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
            send(&mut socket, MessageCommand::Clse, local, b"").unwrap();
        });
        let mut device = controlled_fixture(address);
        let mut output = Output {
            bytes: Vec::new(),
            flushed: false,
        };
        device
            .pull_with_control(
                &"/fixture",
                &mut output,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            )
            .unwrap();
        assert_eq!(output.bytes, b"abc");
        assert!(output.flushed);
        peer.join().unwrap();
    }

    #[test]
    fn controlled_owned_push_interrupts_stalled_open_or_data_ack_without_replay() {
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        for accept_open in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                hello(&mut socket);
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), b"sync:");
                let local = open.header().arg0();
                if accept_open {
                    send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
                    let send_request = receive(&mut socket).unwrap();
                    assert!(send_request.payload().starts_with(b"SEND"));
                    send(&mut socket, MessageCommand::Okay, local, b"").unwrap();
                    let data = receive(&mut socket).unwrap();
                    assert!(data.payload().starts_with(b"DATA"));
                }
                assert!(
                    receive(&mut socket).is_err(),
                    "stalled upload must not replay sync OPEN or DATA"
                );
            });
            let mut device = controlled_fixture(address);
            let mut input = std::io::Cursor::new(vec![42u8; 65536]);
            assert!(
                device
                    .push_with_control(
                        &mut input,
                        &"/fixture",
                        BoundedShellControl::new(Duration::from_millis(100)).unwrap()
                    )
                    .is_err()
            );
            peer.join().unwrap();
        }
    }
    #[test]
    fn controlled_owned_transfer_pre_cancel_preserves_socket_for_following_command() {
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, reply, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
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
        use super::super::BoundedShellControl;
        use super::super::streamed_install::tests::{hello, receive, send};
        use crate::message_devices::message_commands::MessageCommand;
        use std::time::Duration;
        for acknowledge in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                hello(&mut socket);
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), b"reboot:");
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
