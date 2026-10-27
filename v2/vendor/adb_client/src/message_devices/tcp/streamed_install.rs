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
/// Pre-OPEN errors return Err; any error after attempting OPEN is indeterminate.
pub fn run_streamed_install(
    address: SocketAddr,
    key: &Path,
    input: &mut dyn Read,
    size: u64,
    replace: bool,
    control: BoundedShellControl,
) -> Result<InstallOutcome> {
    run_with_device(input, size, replace, control, |control| {
        let transport = TcpTransport::with_shell_control(address, key, control.clone());
        ADBMessageDevice::new_with_required_key(transport, key, control.remaining()?)
    })
}

fn run_with_device<F>(
    input: &mut dyn Read,
    size: u64,
    replace: bool,
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
    // OPEN may have reached the peer even when its acknowledgement is lost.
    let result: Result<()> = (|| {
        let remaining = control.remaining()?;
        let session = device.open_raw_service(&service, remaining)?;
        let mut stream = ADBService::new(session, remaining, remaining)
            .with_pending_limit(INSTALL_RESPONSE_LIMIT);
        let transfer: Result<()> = (|| {
            let mut remaining_bytes = size;
            let mut buffer = [0; 4096];
            while remaining_bytes != 0 {
                control.remaining()?;
                let amount = remaining_bytes.min(buffer.len() as u64) as usize;
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
            outcome.status = classify_response(&outcome.response);
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

    fn fixture_install(
        address: SocketAddr,
        bytes: &[u8],
        size: u64,
        replace: bool,
        control: BoundedShellControl,
    ) -> InstallOutcome {
        run_with_device(&mut Cursor::new(bytes), size, replace, control, |control| {
            let transport = TcpTransport::with_shell_control(
                address,
                Path::new("fixture-never-written"),
                control.clone(),
            );
            ADBMessageDevice::new_with_fixture_key(transport, &fixture_key(), control.remaining()?)
        })
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
            let bytes: Vec<u8> = (0..8201).map(|n| (n % 251) as u8).collect();
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
                &vec![42; size as usize],
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
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        );
        assert!(result.is_err());
    }
}
