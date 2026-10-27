use super::{BoundedShellControl, bounded_shell::Finish, tcp_transport::TcpTransport};
use crate::{
    Result, RustADBError,
    message_devices::{adb_message_device::ADBMessageDevice, adb_service::ADBService},
};
use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
};

/// Maximum retained package-manager response, including output received during upload.
pub const INSTALL_RESPONSE_LIMIT: usize = 64 * 1024;

/// What the package manager authoritatively reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    /// Exact success response followed by clean service closure.
    Success,
    /// Recognized rejection response followed by clean service closure.
    Rejected,
    /// An attempted operation has no authoritative final outcome; never replay automatically.
    Indeterminate,
}

/// Bounded result from one single-APK streaming installation attempt.
#[derive(Debug)]
pub struct InstallOutcome {
    /// Installation certainty, independent of transport success.
    pub status: InstallStatus,
    /// Exact response bytes retained before completion or failure.
    pub response: Vec<u8>,
    /// Payload bytes submitted successfully to the service; not proof of installation.
    pub bytes_submitted: u64,
    /// Transport or protocol context when the result is indeterminate.
    pub detail: String,
}

/// Streams one declared-size APK over a dedicated required-identity connection.
/// No subprocess, staging path, key creation or automatic retry is used.
/// The expected ID must come from the selected owned control connection.
/// Errors before the install-service OPEN return Err; later errors are indeterminate.
pub fn run_streamed_install(
    address: SocketAddr,
    key: &Path,
    input: &mut dyn Read,
    size: u64,
    replace: bool,
    expected_hardware_id: &str,
    control: BoundedShellControl,
) -> Result<InstallOutcome> {
    run_with_device(
        input,
        size,
        replace,
        expected_hardware_id,
        control,
        |control| {
            let transport = TcpTransport::with_shell_control(address, key, control.clone());
            ADBMessageDevice::new_with_required_key(
                transport,
                key,
                control.remaining()?.min(std::time::Duration::from_secs(30)),
            )
        },
    )
}

fn run_with_device<F>(
    input: &mut dyn Read,
    size: u64,
    replace: bool,
    expected_hardware_id: &str,
    control: BoundedShellControl,
    connect: F,
) -> Result<InstallOutcome>
where
    F: FnOnce(&BoundedShellControl) -> Result<ADBMessageDevice<TcpTransport>>,
{
    let finish = Finish(control);
    let control = &finish.0;
    if size == 0 {
        return Err(RustADBError::ADBRequestFailed(
            "APK must not be empty".into(),
        ));
    }
    let mut device = connect(control)?;
    let probe =
        BoundedShellControl::new(control.remaining()?.min(std::time::Duration::from_secs(5)))?;
    let actual = read_hardware_serial_on_current(&mut device, probe)?;
    if actual.trim() != expected_hardware_id || expected_hardware_id.is_empty() {
        return Err(RustADBError::ADBRequestFailed(
            "hardware identity did not match the selected TV; installation not started".into(),
        ));
    }
    let remaining = control.remaining()?;
    let service = format!(
        "exec:cmd package install{} -S {size}",
        if replace { " -r" } else { "" }
    );
    let mut outcome = InstallOutcome {
        status: InstallStatus::Indeterminate,
        response: Vec::new(),
        bytes_submitted: 0,
        detail: String::new(),
    };
    // The install-service OPEN may reach the peer even when its ACK is lost.
    let result: Result<()> = (|| {
        let session = device.open_raw_service(&service, remaining)?;
        let mut stream = ADBService::new(session, remaining, remaining)
            .with_pending_limit(INSTALL_RESPONSE_LIMIT);
        let transfer: Result<()> = (|| {
            let mut remaining_bytes = size;
            let mut early_response = false;
            let mut buffer = [0; 4096];
            while remaining_bytes != 0 {
                control.remaining()?;
                let amount = usize::try_from(remaining_bytes.min(buffer.len() as u64))?;
                input.read_exact(&mut buffer[..amount])?;
                if remaining_bytes == amount as u64 {
                    let mut extra = [0];
                    if input.read(&mut extra)? != 0 {
                        return Err(RustADBError::ADBRequestFailed(
                            "APK size changed during upload".into(),
                        ));
                    }
                    stream.write_final(&buffer[..amount])?;
                } else {
                    stream.write_all(&buffer[..amount])?;
                    early_response |= stream.has_pending();
                }
                outcome.bytes_submitted += amount as u64;
                remaining_bytes -= amount as u64;
            }
            loop {
                control.remaining()?;
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                let available = INSTALL_RESPONSE_LIMIT.saturating_sub(outcome.response.len());
                outcome
                    .response
                    .extend_from_slice(&buffer[..count.min(available)]);
                if count > available {
                    return Err(RustADBError::ADBRequestFailed(
                        "install response limit reached".into(),
                    ));
                }
            }
            let reported = classify_response(&outcome.response);
            if reported != InstallStatus::Success || !early_response {
                outcome.status = reported;
            }
            Ok(())
        })();
        if transfer.is_err() {
            outcome.response.extend(
                stream.take_pending(INSTALL_RESPONSE_LIMIT.saturating_sub(outcome.response.len())),
            );
            if stream.peer_closed()
                && classify_response(&outcome.response) == InstallStatus::Rejected
            {
                outcome.status = InstallStatus::Rejected;
            }
        }
        transfer?;
        Ok(())
    })();
    if let Err(error) = result {
        outcome.detail = error.to_string().chars().take(1024).collect();
    }
    if outcome.status == InstallStatus::Indeterminate && outcome.detail.is_empty() {
        outcome.detail = "package manager did not return an exact recognized result".into();
    }
    Ok(outcome)
}

pub(crate) fn read_hardware_serial_on_current(
    device: &mut ADBMessageDevice<TcpTransport>,
    control: BoundedShellControl,
) -> Result<String> {
    struct Restore<'a> {
        device: &'a mut ADBMessageDevice<TcpTransport>,
        previous: Option<BoundedShellControl>,
    }
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            self.device
                .get_transport_mut()
                .restore_control(self.previous.take());
        }
    }
    let finish = Finish(control);
    let previous = device
        .get_transport_mut()
        .set_current_control(finish.0.clone())?;
    let restore = Restore { device, previous };
    read_hardware_serial(restore.device, &finish.0)
}

pub(crate) fn read_hardware_serial(
    device: &mut ADBMessageDevice<TcpTransport>,
    control: &BoundedShellControl,
) -> Result<String> {
    use super::bounded_shell::{
        BoundedShellOutput, BoundedShellTermination, collect_v2_with_limit,
    };
    let remaining = control.remaining()?;
    let session =
        device.open_bounded_shell_service("shell,v2,raw:getprop ro.serialno", remaining)?;
    let mut stream = ADBService::new(session, remaining, remaining).with_pending_limit(2048);
    stream.write_final(&[4, 0, 0, 0, 0])?;
    let mut output = BoundedShellOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: None,
        termination: BoundedShellTermination::Completed,
    };
    collect_v2_with_limit(&mut stream, control, &mut output, 1024)?;
    if output.exit_code != Some(0)
        || !output.stderr.is_empty()
        || output.termination != BoundedShellTermination::Completed
    {
        return Err(RustADBError::ADBRequestFailed(
            "hardware identity query was not authoritatively successful".into(),
        ));
    }
    let property = String::from_utf8(output.stdout)?;
    if property.trim().chars().any(char::is_control) {
        return Err(RustADBError::ADBRequestFailed(
            "hardware identity property was not a single readable line".into(),
        ));
    }
    Ok(property)
}

fn classify_response(response: &[u8]) -> InstallStatus {
    if response == b"Success\n" || response == b"Success\r\n" {
        return InstallStatus::Success;
    }
    if let Ok(text) = std::str::from_utf8(response) {
        let line = text.trim_end_matches(['\r', '\n']);
        if !line.contains(['\r', '\n'])
            && ((line.starts_with("Failure [") && line.ends_with(']'))
                || line.starts_with("Error: "))
        {
            return InstallStatus::Rejected;
        }
    }
    InstallStatus::Indeterminate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_devices::{
        adb_transport_message::{ADBTransportMessage, ADBTransportMessageHeader},
        message_commands::MessageCommand,
        models::ADBRsaKey,
    };
    use std::{
        io::Cursor,
        net::{TcpListener, TcpStream},
        thread::JoinHandle,
        time::Duration,
    };

    fn fixture_key() -> ADBRsaKey {
        let source = include_str!("../models/adb_rsa_key.rs");
        let start = source.find("-----BEGIN PRIVATE KEY-----").unwrap();
        let end = source[start..].find("-----END PRIVATE KEY-----").unwrap()
            + start
            + "-----END PRIVATE KEY-----".len();
        ADBRsaKey::new_from_pkcs8(&source[start..end]).unwrap()
    }

    fn receive(socket: &mut TcpStream) -> std::io::Result<ADBTransportMessage> {
        let mut bytes = [0; 24];
        socket.read_exact(&mut bytes)?;
        let header = ADBTransportMessageHeader::try_from(bytes).unwrap();
        assert!(header.data_length() <= 1_048_576);
        let mut payload = vec![0; header.data_length() as usize];
        socket.read_exact(&mut payload)?;
        Ok(ADBTransportMessage::from_header_and_payload(
            header, payload,
        ))
    }

    fn send(
        socket: &mut TcpStream,
        command: MessageCommand,
        local: u32,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        let message = ADBTransportMessage::try_new(command, 7, local, bytes).unwrap();
        socket.write_all(&message.header().as_bytes())?;
        socket.write_all(message.payload())
    }

    fn server<F>(size: u64, replace: bool, run: F) -> (SocketAddr, JoinHandle<()>)
    where
        F: FnOnce(&mut TcpStream, u32) + Send + 'static,
    {
        server_with_open_ack(size, replace, true, run)
    }

    fn server_with_open_ack<F>(
        size: u64,
        replace: bool,
        acknowledge: bool,
        run: F,
    ) -> (SocketAddr, JoinHandle<()>)
    where
        F: FnOnce(&mut TcpStream, u32) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            assert_eq!(
                receive(&mut socket).unwrap().header().command(),
                MessageCommand::Cnxn
            );
            let hello = ADBTransportMessage::try_new(
                MessageCommand::Cnxn,
                0x0100_0000,
                1_048_576,
                b"device::\0",
            )
            .unwrap();
            socket.write_all(&hello.header().as_bytes()).unwrap();
            socket.write_all(hello.payload()).unwrap();
            serve_identity(&mut socket, b"fixture-hardware\n", 0, b"");
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.header().command(), MessageCommand::Open);
            assert_eq!(
                open.payload(),
                format!(
                    "exec:cmd package install{} -S {size}\0",
                    if replace { " -r" } else { "" }
                )
                .as_bytes()
            );
            let local = open.header().arg0();
            if acknowledge {
                send(&mut socket, MessageCommand::Okay, local, &[]).unwrap();
            }
            run(&mut socket, local);
        });
        (address, thread)
    }

    fn serve_identity(socket: &mut TcpStream, id: &[u8], code: u8, stderr: &[u8]) {
        let open = receive(socket).unwrap();
        assert_eq!(open.payload(), b"shell,v2,raw:getprop ro.serialno\0");
        let local = open.header().arg0();
        send(socket, MessageCommand::Okay, local, &[]).unwrap();
        assert_eq!(receive(socket).unwrap().payload(), &[4, 0, 0, 0, 0]);
        for (channel, payload) in [(1, id), (2, stderr), (3, &[code][..])] {
            let mut packet = vec![channel];
            packet.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
            packet.extend_from_slice(payload);
            reply(socket, local, &packet, false);
        }
        send(socket, MessageCommand::Clse, local, &[]).unwrap();
    }

    fn fixture_install(
        address: SocketAddr,
        bytes: &[u8],
        size: u64,
        replace: bool,
        control: BoundedShellControl,
    ) -> InstallOutcome {
        run_with_device(
            &mut Cursor::new(bytes),
            size,
            replace,
            "fixture-hardware",
            control,
            |control| {
                let transport = TcpTransport::with_shell_control(
                    address,
                    Path::new("fixture-never-written"),
                    control.clone(),
                );
                ADBMessageDevice::new_with_fixture_key(
                    transport,
                    &fixture_key(),
                    control.remaining()?,
                )
            },
        )
        .unwrap()
    }

    fn upload(socket: &mut TcpStream, local: u32, expected: &[u8]) {
        let mut received = Vec::new();
        while received.len() < expected.len() {
            let message = receive(socket).unwrap();
            assert_eq!(message.header().command(), MessageCommand::Write);
            received.extend_from_slice(message.payload());
            if received.len() < expected.len() {
                send(socket, MessageCommand::Okay, local, &[]).unwrap();
            }
        }
        assert_eq!(received, expected);
    }

    fn reply(socket: &mut TcpStream, local: u32, bytes: &[u8], close: bool) {
        send(socket, MessageCommand::Write, local, bytes).unwrap();
        assert_eq!(
            receive(socket).unwrap().header().command(),
            MessageCommand::Okay
        );
        if close {
            let _ = send(socket, MessageCommand::Clse, local, &[]);
        }
    }

    #[test]
    fn streamed_install_preserves_exact_bytes_size_and_replace_flags_without_final_ack() {
        for replace in [false, true] {
            let bytes: Vec<u8> = (0_u8..=250).cycle().take(8201).collect();
            let expected = bytes.clone();
            let (address, server) = server(bytes.len() as u64, replace, move |socket, local| {
                upload(socket, local, &expected);
                reply(socket, local, b"Success\n", true);
            });
            let result = fixture_install(
                address,
                &bytes,
                bytes.len() as u64,
                replace,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            );
            server.join().unwrap();
            assert_eq!(result.status, InstallStatus::Success);
            assert_eq!(result.bytes_submitted, bytes.len() as u64);
            assert_eq!(result.response, b"Success\n");
        }
    }

    #[test]
    fn streamed_install_requires_exact_authoritative_result_and_clean_close() {
        for (response, expected) in [
            (
                &b"Failure [INSTALL_FAILED_INVALID_APK]\n"[..],
                InstallStatus::Rejected,
            ),
            (&b"Error: rejected\n"[..], InstallStatus::Rejected),
            (&b"Successish\n"[..], InstallStatus::Indeterminate),
            (
                &b"Success\nFailure [bad]\n"[..],
                InstallStatus::Indeterminate,
            ),
        ] {
            let (address, server) = server(3, false, move |socket, local| {
                upload(socket, local, b"abc");
                reply(socket, local, response, true);
            });
            let result = fixture_install(
                address,
                b"abc",
                3,
                false,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            );
            server.join().unwrap();
            assert_eq!(result.status, expected);
        }
        let (address, server) = server(3, false, |socket, local| {
            upload(socket, local, b"abc");
            reply(socket, local, b"Success\n", false);
        });
        let result = fixture_install(
            address,
            b"abc",
            3,
            false,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        server.join().unwrap();
        assert_eq!(result.status, InstallStatus::Indeterminate);
        assert_eq!(result.response, b"Success\n");
    }

    #[test]
    fn streamed_install_short_or_growing_input_never_submits_a_false_final_chunk() {
        for input in [&b"ab"[..], &b"abcd"[..]] {
            let (address, server) = server(3, false, |socket, _| {
                let message = receive(socket).unwrap();
                assert_eq!(message.header().command(), MessageCommand::Clse);
            });
            let result = fixture_install(
                address,
                input,
                3,
                false,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            );
            server.join().unwrap();
            assert_eq!(result.status, InstallStatus::Indeterminate);
            assert_eq!(result.bytes_submitted, 0);
        }
    }

    #[test]
    fn streamed_install_lost_response_and_interrupted_transfer_stay_indeterminate() {
        for size in [3, 8192] {
            let (address, server) = server(size, false, |socket, _| {
                assert_eq!(
                    receive(socket).unwrap().header().command(),
                    MessageCommand::Write
                );
            });
            let result = fixture_install(
                address,
                &vec![42; usize::try_from(size).unwrap()],
                size,
                false,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            );
            server.join().unwrap();
            assert_eq!(result.status, InstallStatus::Indeterminate);
        }
    }

    #[test]
    fn streamed_install_bounds_pending_output_while_waiting_for_upload_ack() {
        let (address, server) = server(8192, false, |socket, local| {
            assert_eq!(
                receive(socket).unwrap().header().command(),
                MessageCommand::Write
            );
            for _ in 0..100 {
                if send(socket, MessageCommand::Write, local, &[b'x'; 1024]).is_err() {
                    break;
                }
                match receive(socket) {
                    Ok(ack) if ack.header().command() == MessageCommand::Okay => {}
                    _ => break,
                }
            }
        });
        let result = fixture_install(
            address,
            &[42; 8192],
            8192,
            false,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        server.join().unwrap();
        assert_eq!(result.status, InstallStatus::Indeterminate);
        assert!(result.detail.contains("output limit"), "{}", result.detail);
        assert!(result.response.len() <= INSTALL_RESPONSE_LIMIT);
    }

    #[test]
    fn streamed_install_total_deadline_and_cancel_interrupt_stalled_response() {
        for cancel in [false, true] {
            let (address, server) = server(3, false, |socket, _| {
                assert_eq!(
                    receive(socket).unwrap().header().command(),
                    MessageCommand::Write
                );
                std::thread::sleep(Duration::from_millis(300));
            });
            let control = BoundedShellControl::new(if cancel {
                Duration::from_secs(2)
            } else {
                Duration::from_millis(100)
            })
            .unwrap();
            let signal = control.clone();
            let canceller = cancel.then(|| {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(100));
                    signal.cancel();
                })
            });
            let started = std::time::Instant::now();
            let result = fixture_install(address, b"abc", 3, false, control);
            assert!(started.elapsed() < Duration::from_millis(250));
            if let Some(canceller) = canceller {
                canceller.join().unwrap();
            }
            server.join().unwrap();
            assert_eq!(result.status, InstallStatus::Indeterminate);
        }
    }
    #[test]
    fn streamed_install_retains_early_rejection_but_never_premature_success() {
        for (response, close, status) in [
            (
                &b"Failure [INSTALL_FAILED_TEST_ONLY]\n"[..],
                true,
                InstallStatus::Rejected,
            ),
            (
                &b"Failure [INSTALL_FAILED_TEST_ONLY]\n"[..],
                false,
                InstallStatus::Indeterminate,
            ),
            (&b"Success\n"[..], true, InstallStatus::Indeterminate),
        ] {
            let (address, server) = server(8192, false, move |socket, local| {
                assert_eq!(
                    receive(socket).unwrap().header().command(),
                    MessageCommand::Write
                );
                reply(socket, local, response, close);
            });
            let result = fixture_install(
                address,
                &[42; 8192],
                8192,
                false,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            );
            server.join().unwrap();
            assert_eq!(result.status, status);
            assert_eq!(result.response, response);
            assert_eq!(result.bytes_submitted, 0);
        }
    }

    #[test]
    fn streamed_install_bounds_post_upload_output() {
        let (address, server) = server(3, false, |socket, local| {
            upload(socket, local, b"abc");
            for _ in 0..100 {
                if send(socket, MessageCommand::Write, local, &[b'x'; 4096]).is_err() {
                    break;
                }
                match receive(socket) {
                    Ok(ack) if ack.header().command() == MessageCommand::Okay => {}
                    _ => break,
                }
            }
        });
        let result = fixture_install(
            address,
            b"abc",
            3,
            false,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        server.join().unwrap();
        assert_eq!(result.status, InstallStatus::Indeterminate);
        assert_eq!(result.response.len(), INSTALL_RESPONSE_LIMIT);
    }
    #[test]
    fn streamed_install_lost_open_ack_is_unknown_and_missing_key_is_preopen_error() {
        let (address, server) = server_with_open_ack(3, false, false, |_, _| {});
        let result = fixture_install(
            address,
            b"abc",
            3,
            false,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        server.join().unwrap();
        assert_eq!(result.status, InstallStatus::Indeterminate);
        assert_eq!(result.bytes_submitted, 0);
        let result = run_streamed_install(
            "127.0.0.1:0".parse().unwrap(),
            Path::new("missing-install-fixture-key-never-created"),
            &mut Cursor::new(b"abc"),
            3,
            false,
            "fixture-hardware",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        assert!(result.is_err());
    }
    #[test]
    fn streamed_install_cancel_before_open_is_an_error_without_install_attempt() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            assert_eq!(
                receive(&mut socket).unwrap().header().command(),
                MessageCommand::Cnxn
            );
            let hello = ADBTransportMessage::try_new(
                MessageCommand::Cnxn,
                0x0100_0000,
                1_048_576,
                b"device::\0",
            )
            .unwrap();
            socket.write_all(&hello.header().as_bytes()).unwrap();
            socket.write_all(hello.payload()).unwrap();
            assert!(
                receive(&mut socket).is_err(),
                "no OPEN may follow pre-install cancellation"
            );
        });
        let result = run_with_device(
            &mut Cursor::new(b"abc"),
            3,
            false,
            "fixture-hardware",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            |control| {
                let transport = TcpTransport::with_shell_control(
                    address,
                    Path::new("fixture-never-written"),
                    control.clone(),
                );
                let device = ADBMessageDevice::new_with_fixture_key(
                    transport,
                    &fixture_key(),
                    control.remaining()?,
                )?;
                control.cancel();
                Ok(device)
            },
        );
        server.join().unwrap();
        assert!(result.is_err());
    }
    #[test]
    fn streamed_install_empty_input_is_rejected_before_connection() {
        let result = run_with_device(
            &mut Cursor::new([]),
            0,
            false,
            "fixture-hardware",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            |_| panic!("empty input must not connect"),
        );
        assert!(result.is_err());
    }
    #[test]
    fn streamed_install_early_success_stays_unknown_even_if_upload_later_finishes() {
        let (address, server) = server(8192, false, |socket, local| {
            assert_eq!(
                receive(socket).unwrap().header().command(),
                MessageCommand::Write
            );
            reply(socket, local, b"Success\n", false);
            send(socket, MessageCommand::Okay, local, &[]).unwrap();
            assert_eq!(
                receive(socket).unwrap().header().command(),
                MessageCommand::Write
            );
            send(socket, MessageCommand::Clse, local, &[]).unwrap();
        });
        let result = fixture_install(
            address,
            &[42; 8192],
            8192,
            false,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        server.join().unwrap();
        assert_eq!(result.bytes_submitted, 8192);
        assert_eq!(result.response, b"Success\n");
        assert_eq!(result.status, InstallStatus::Indeterminate);
    }
    fn hello(socket: &mut TcpStream) {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(
            receive(socket).unwrap().header().command(),
            MessageCommand::Cnxn
        );
        let response = ADBTransportMessage::try_new(
            MessageCommand::Cnxn,
            0x0100_0000,
            1_048_576,
            b"device::\0",
        )
        .unwrap();
        socket.write_all(&response.header().as_bytes()).unwrap();
        socket.write_all(response.payload()).unwrap();
    }

    #[test]
    fn streamed_install_mismatched_unknown_or_failed_identity_never_opens_installer() {
        for (id, code, stderr) in [
            (&b"different-TV\n"[..], 0, &b""[..]),
            (b"unknown\n", 0, b""),
            (b"first\nsecond\n", 0, b""),
            (b"\xff", 0, b""),
            (b"", 0, b""),
            (b"fixture-hardware\n", 1, b""),
            (b"fixture-hardware\n", 0, b"getprop failed"),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                hello(&mut socket);
                serve_identity(&mut socket, id, code, stderr);
                assert!(
                    receive(&mut socket).is_err(),
                    "identity failure must send no install OPEN or bytes"
                );
            });
            let result = run_with_device(
                &mut Cursor::new(b"abc"),
                3,
                false,
                "fixture-hardware",
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
                |control| {
                    let transport = TcpTransport::with_shell_control(
                        address,
                        Path::new("fixture-never-written"),
                        control.clone(),
                    );
                    ADBMessageDevice::new_with_fixture_key(
                        transport,
                        &fixture_key(),
                        control.remaining()?,
                    )
                },
            );
            server.join().unwrap();
            assert!(result.is_err());
        }
    }

    #[test]
    fn borrowed_identity_probe_preserves_original_socket_and_canceled_registration_does_not_close_it()
     {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
            serve_identity(&mut socket, b"original-TV\n", 0, b"");
            serve_identity(&mut socket, b"original-TV\n", 0, b"");
            assert!(receive(&mut socket).is_err());
        });
        let inner = ADBMessageDevice::new_with_fixture_key(
            TcpTransport::new(address, Path::new("fixture-never-written")),
            &fixture_key(),
            Duration::from_secs(2),
        )
        .unwrap();
        let mut device = super::super::ADBTcpDevice::from_fixture_device(inner);
        assert_eq!(
            device
                .hardware_serial_bounded(BoundedShellControl::new(Duration::from_secs(2)).unwrap())
                .unwrap(),
            "original-TV\n"
        );
        let canceled = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        canceled.cancel();
        assert!(device.hardware_serial_bounded(canceled).is_err());
        assert_eq!(
            device
                .hardware_serial_bounded(BoundedShellControl::new(Duration::from_secs(2)).unwrap())
                .unwrap(),
            "original-TV\n"
        );
        drop(device);
        server.join().unwrap();
    }

    #[test]
    fn borrowed_identity_probe_deadline_and_cancellation_interrupt_active_socket() {
        for cancel in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                hello(&mut socket);
                let open = receive(&mut socket).unwrap();
                assert_eq!(open.payload(), b"shell,v2,raw:getprop ro.serialno\0");
                let local = open.header().arg0();
                send(&mut socket, MessageCommand::Okay, local, &[]).unwrap();
                assert_eq!(receive(&mut socket).unwrap().payload(), &[4, 0, 0, 0, 0]);
                assert!(receive(&mut socket).is_err());
            });
            let inner = ADBMessageDevice::new_with_fixture_key(
                TcpTransport::new(address, Path::new("fixture-never-written")),
                &fixture_key(),
                Duration::from_secs(2),
            )
            .unwrap();
            let mut device = super::super::ADBTcpDevice::from_fixture_device(inner);
            let control = BoundedShellControl::new(if cancel {
                Duration::from_secs(2)
            } else {
                Duration::from_millis(100)
            })
            .unwrap();
            let signal = control.clone();
            let canceller = cancel.then(|| {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(100));
                    signal.cancel();
                })
            });
            let started = std::time::Instant::now();
            assert!(device.hardware_serial_bounded(control).is_err());
            assert!(started.elapsed() < Duration::from_millis(400));
            if let Some(canceller) = canceller {
                canceller.join().unwrap();
            }
            drop(device);
            server.join().unwrap();
        }
    }
    #[test]
    fn original_owned_identity_is_not_replaced_by_a_new_tv_at_the_same_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut original, _) = listener.accept().unwrap();
            hello(&mut original);
            serve_identity(&mut original, b"original-TV\n", 0, b"");
            let (mut replacement, _) = listener.accept().unwrap();
            hello(&mut replacement);
            serve_identity(&mut replacement, b"replacement-TV\n", 0, b"");
            assert!(
                receive(&mut replacement).is_err(),
                "replacement must receive no install OPEN or bytes"
            );
            serve_identity(&mut original, b"original-TV\n", 0, b"");
        });
        let inner = ADBMessageDevice::new_with_fixture_key(
            TcpTransport::new(address, Path::new("fixture-never-written")),
            &fixture_key(),
            Duration::from_secs(2),
        )
        .unwrap();
        let mut original = super::super::ADBTcpDevice::from_fixture_device(inner);
        let expected = original
            .hardware_serial_bounded(BoundedShellControl::new(Duration::from_secs(2)).unwrap())
            .unwrap();
        let result = run_with_device(
            &mut Cursor::new(b"abc"),
            3,
            false,
            expected.trim(),
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            |control| {
                let transport = TcpTransport::with_shell_control(
                    address,
                    Path::new("fixture-never-written"),
                    control.clone(),
                );
                ADBMessageDevice::new_with_fixture_key(
                    transport,
                    &fixture_key(),
                    control.remaining()?,
                )
            },
        );
        assert!(result.is_err());
        assert_eq!(
            original
                .hardware_serial_bounded(BoundedShellControl::new(Duration::from_secs(2)).unwrap())
                .unwrap(),
            expected
        );
        drop(original);
        server.join().unwrap();
    }

    #[test]
    fn missing_identity_service_never_opens_installer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.payload(), b"shell,v2,raw:getprop ro.serialno\0");
            send(&mut socket, MessageCommand::Clse, open.header().arg0(), &[]).unwrap();
            assert!(receive(&mut socket).is_err());
        });
        let result = run_with_device(
            &mut Cursor::new(b"abc"),
            3,
            false,
            "original-TV",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            |control| {
                let transport = TcpTransport::with_shell_control(
                    address,
                    Path::new("fixture-never-written"),
                    control.clone(),
                );
                ADBMessageDevice::new_with_fixture_key(
                    transport,
                    &fixture_key(),
                    control.remaining()?,
                )
            },
        );
        server.join().unwrap();
        assert!(result.is_err());
    }
    #[test]
    fn oversized_identity_output_stops_before_install_open() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.payload(), b"shell,v2,raw:getprop ro.serialno\0");
            let local = open.header().arg0();
            send(&mut socket, MessageCommand::Okay, local, &[]).unwrap();
            assert_eq!(receive(&mut socket).unwrap().payload(), &[4, 0, 0, 0, 0]);
            let mut packet = vec![1];
            packet.extend_from_slice(&1024_u32.to_le_bytes());
            packet.extend_from_slice(&[b'x'; 1024]);
            reply(&mut socket, local, &packet, false);
            if let Ok(message) = receive(&mut socket) {
                assert_eq!(message.header().command(), MessageCommand::Clse);
            }
        });
        let result = run_with_device(
            &mut Cursor::new(b"abc"),
            3,
            false,
            "original-TV",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            |control| {
                let transport = TcpTransport::with_shell_control(
                    address,
                    Path::new("fixture-never-written"),
                    control.clone(),
                );
                ADBMessageDevice::new_with_fixture_key(
                    transport,
                    &fixture_key(),
                    control.remaining()?,
                )
            },
        );
        server.join().unwrap();
        assert!(result.is_err());
    }
}
