use std::io::{Error, ErrorKind, Read};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::message_devices::adb_message_device::ADBMessageDevice;
use crate::message_devices::adb_service::ADBService;
use crate::{Result, RustADBError};

use super::tcp_transport::TcpTransport;

/// Maximum retained bytes in each output stream.
pub const SHELL_OUTPUT_LIMIT: usize = 256 * 1024;
const CLOSE_STDIN_PACKET: [u8; 5] = [4, 0, 0, 0, 0];

/// Why a bounded shell command stopped collecting output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundedShellTermination {
    /// The peer reported an exit status and closed the stream.
    Completed,
    /// One output stream reached its byte ceiling.
    OutputLimit,
    /// The total operation deadline elapsed.
    Timeout,
}

/// Output retained from one bounded shell-v2 command.
#[derive(Debug)]
pub struct BoundedShellOutput {
    /// Standard output, limited independently of standard error.
    pub stdout: Vec<u8>,
    /// Standard error, limited independently of standard output.
    pub stderr: Vec<u8>,
    /// Device exit status when received before termination.
    pub exit_code: Option<u8>,
    /// Completion, output ceiling, or total deadline.
    pub termination: BoundedShellTermination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Finished,
    Cancelled,
    TimedOut,
}

#[derive(Debug)]
struct ControlState {
    state: State,
    socket: Option<TcpStream>,
    close_on_finish: bool,
}

#[derive(Debug)]
struct ControlInner {
    deadline: Instant,
    state: Mutex<ControlState>,
    changed: Condvar,
    timer: Mutex<Option<JoinHandle<()>>>,
}

/// Owns a command's deadline and shutdown handle without locking its I/O stream.
#[derive(Debug, Clone)]
pub struct BoundedShellControl(Arc<ControlInner>);

impl BoundedShellControl {
    /// Starts a total deadline that also interrupts stalled TLS/TCP I/O.
    pub fn new(duration: Duration) -> std::io::Result<Self> {
        let inner = Arc::new(ControlInner {
            deadline: Instant::now()
                .checked_add(duration)
                .ok_or_else(|| Error::other("shell deadline overflow"))?,
            state: Mutex::new(ControlState {
                state: State::Running,
                socket: None,
                close_on_finish: true,
            }),
            changed: Condvar::new(),
            timer: Mutex::new(None),
        });
        let timer_inner = inner.clone();
        let timer = std::thread::Builder::new()
            .name("adb-shell-deadline".into())
            .spawn(move || {
                let mut state = timer_inner
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                while state.state == State::Running {
                    let remaining = timer_inner
                        .deadline
                        .saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        state.state = State::TimedOut;
                        if let Some(socket) = &state.socket {
                            let _ = socket.shutdown(Shutdown::Both);
                        }
                        break;
                    }
                    state = timer_inner
                        .changed
                        .wait_timeout(state, remaining)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0;
                }
            })?;
        *inner
            .timer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(timer);
        Ok(Self(inner))
    }

    /// Interrupts the dedicated socket; worker cleanup must still be awaited.
    pub fn cancel(&self) {
        self.stop(State::Cancelled);
    }

    fn stop(&self, reason: State) {
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.state == State::Running {
            state.state = reason;
        }
        if let (true, Some(socket)) = (
            reason != State::Finished || state.close_on_finish,
            &state.socket,
        ) {
            let _ = socket.shutdown(Shutdown::Both);
        }
        state.socket = None;
        self.0.changed.notify_all();
    }

    pub(crate) fn remaining(&self) -> std::io::Result<Duration> {
        let state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let remaining = self.0.deadline.saturating_duration_since(Instant::now());
        match state.state {
            State::Running if !remaining.is_zero() => Ok(remaining),
            State::Running | State::TimedOut => Err(Error::new(
                ErrorKind::TimedOut,
                "shell total deadline reached",
            )),
            _ => Err(Error::new(ErrorKind::Interrupted, "shell operation closed")),
        }
    }

    pub(crate) fn register(&self, socket: &TcpStream) -> std::io::Result<()> {
        self.register_kind(socket, true)
    }

    pub(crate) fn register_borrowed(&self, socket: &TcpStream) -> std::io::Result<()> {
        self.register_kind(socket, false)
    }

    fn register_kind(&self, socket: &TcpStream, owned: bool) -> std::io::Result<()> {
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.state != State::Running || Instant::now() >= self.0.deadline {
            if owned {
                let _ = socket.shutdown(Shutdown::Both);
            }
            return Err(Error::new(
                ErrorKind::TimedOut,
                "operation no longer active",
            ));
        }
        state.socket = Some(socket.try_clone()?);
        state.close_on_finish = owned;
        Ok(())
    }

    fn timed_out(&self) -> bool {
        let state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.state == State::TimedOut
            || (state.state == State::Running && Instant::now() >= self.0.deadline)
    }

    fn finish(&self) {
        self.stop(State::Finished);
        if let Some(timer) = self
            .0
            .timer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = timer.join();
        }
    }
}

pub(crate) struct Finish(pub(crate) BoundedShellControl);
impl Drop for Finish {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// Opens one dedicated authenticated command connection; never creates an identity.
/// The connection is closed before returning, including partial and error results.
pub fn run_bounded_shell(
    address: SocketAddr,
    key: &Path,
    command: &str,
    control: BoundedShellControl,
) -> Result<BoundedShellOutput> {
    run_with_device(command, control, |control| {
        let remaining = control.remaining()?;
        let transport = TcpTransport::with_shell_control(address, key, control.clone());
        ADBMessageDevice::new_with_required_key(transport, key, remaining)
    })
}

fn run_with_device<F>(
    command: &str,
    control: BoundedShellControl,
    connect: F,
) -> Result<BoundedShellOutput>
where
    F: FnOnce(&BoundedShellControl) -> Result<ADBMessageDevice<TcpTransport>>,
{
    let finish = Finish(control);
    let control = &finish.0;
    let mut output = BoundedShellOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: None,
        termination: BoundedShellTermination::Completed,
    };
    let result: Result<()> = (|| {
        let mut device = connect(control)?;
        let remaining = control.remaining()?;
        let session =
            device.open_bounded_shell_service(&format!("shell,v2,raw:{command}"), remaining)?;
        let mut stream = ADBService::new(session, remaining, remaining.min(Duration::from_secs(2)));
        // Match null stdin without buffering output while waiting for its ACK.
        stream.write_final(&CLOSE_STDIN_PACKET)?;
        collect_v2(&mut stream, control, &mut output)?;
        Ok(())
    })();
    if control.timed_out() {
        output.termination = BoundedShellTermination::Timeout;
        return Ok(output);
    }
    result?;
    Ok(output)
}

fn collect_v2(
    input: &mut dyn Read,
    control: &BoundedShellControl,
    output: &mut BoundedShellOutput,
) -> Result<()> {
    collect_v2_with_limit(input, control, output, SHELL_OUTPUT_LIMIT)
}

pub(crate) fn collect_v2_with_limit(
    input: &mut dyn Read,
    control: &BoundedShellControl,
    output: &mut BoundedShellOutput,
    limit: usize,
) -> Result<()> {
    loop {
        control.remaining()?;
        let mut header = [0_u8; 5];
        let first = input.read(&mut header[..1])?;
        if first == 0 {
            return if output.exit_code.is_some() {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    "shell-v2 closed without exit status",
                )
                .into())
            };
        }
        input.read_exact(&mut header[1..])?;
        let channel = header[0];
        let mut remaining = u32::from_le_bytes(header[1..].try_into()?) as usize;
        if !matches!(channel, 1..=3) || (channel == 3 && remaining != 1) {
            return Err(RustADBError::ADBShellV2ParseError(
                "invalid bounded shell packet".into(),
            ));
        }
        if channel == 3 {
            let mut code = [0];
            input.read_exact(&mut code)?;
            output.exit_code = Some(code[0]);
            continue;
        }
        let target = if channel == 1 {
            &mut output.stdout
        } else {
            &mut output.stderr
        };
        let mut chunk = [0_u8; 8192];
        while remaining > 0 {
            control.remaining()?;
            let count = remaining.min(chunk.len()).min(limit - target.len());
            if count == 0 {
                output.termination = BoundedShellTermination::OutputLimit;
                return Ok(());
            }
            let received = input.read(&mut chunk[..count])?;
            if received == 0 {
                return Err(
                    Error::new(ErrorKind::UnexpectedEof, "shell payload ended early").into(),
                );
            }
            target.extend_from_slice(&chunk[..received]);
            remaining -= received;
            if target.len() == limit {
                output.termination = BoundedShellTermination::OutputLimit;
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_devices::adb_transport_message::{
        ADBTransportMessage, ADBTransportMessageHeader,
    };
    use crate::message_devices::message_commands::MessageCommand;
    use std::io::Write;
    use std::net::TcpListener;

    fn fixture_key() -> crate::message_devices::models::ADBRsaKey {
        let source = include_str!("../models/adb_rsa_key.rs");
        let start = source.find("-----BEGIN PRIVATE KEY-----").unwrap();
        let end = source[start..].find("-----END PRIVATE KEY-----").unwrap()
            + start
            + "-----END PRIVATE KEY-----".len();
        crate::message_devices::models::ADBRsaKey::new_from_pkcs8(&source[start..end]).unwrap()
    }

    fn run_fixture_shell(
        address: SocketAddr,
        command: &str,
        control: BoundedShellControl,
    ) -> Result<BoundedShellOutput> {
        run_with_device(command, control, |control| {
            let transport = TcpTransport::with_shell_control(
                address,
                Path::new("fixture-in-memory-never-written"),
                control.clone(),
            );
            ADBMessageDevice::new_with_fixture_key(transport, &fixture_key(), control.remaining()?)
        })
    }

    fn read_message(socket: &mut TcpStream) -> std::io::Result<ADBTransportMessage> {
        let mut header = [0; 24];
        socket.read_exact(&mut header)?;
        let header = ADBTransportMessageHeader::try_from(header).unwrap();
        assert!(header.data_length() < 1_048_576);
        let mut payload = vec![0; header.data_length() as usize];
        socket.read_exact(&mut payload)?;
        Ok(ADBTransportMessage::from_header_and_payload(
            header, payload,
        ))
    }

    fn send(
        socket: &mut TcpStream,
        command: MessageCommand,
        remote: u32,
        local: u32,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        let message = ADBTransportMessage::try_new(command, remote, local, bytes).unwrap();
        socket.write_all(&message.header().as_bytes())?;
        socket.write_all(message.payload())
    }

    fn packet(channel: u8, bytes: &[u8]) -> Vec<u8> {
        let mut packet = vec![channel];
        packet.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        packet.extend_from_slice(bytes);
        packet
    }

    fn write_packet(socket: &mut TcpStream, local: u32, bytes: &[u8]) -> std::io::Result<()> {
        send(socket, MessageCommand::Write, 7, local, bytes)?;
        let ack = read_message(socket)?;
        if ack.header().command() != MessageCommand::Okay {
            return Err(Error::new(
                ErrorKind::BrokenPipe,
                "client stopped the stream",
            ));
        }
        Ok(())
    }

    fn server<F>(run: F) -> (SocketAddr, JoinHandle<()>)
    where
        F: FnOnce(&mut TcpStream, u32) + Send + 'static,
    {
        server_with_stdin_ack(true, run)
    }

    fn server_with_stdin_ack<F>(acknowledge: bool, run: F) -> (SocketAddr, JoinHandle<()>)
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
                read_message(&mut socket).unwrap().header().command(),
                MessageCommand::Cnxn
            );
            send(
                &mut socket,
                MessageCommand::Cnxn,
                0x01000000,
                1_048_576,
                b"device::features=shell_v2\0",
            )
            .unwrap();
            let open = read_message(&mut socket).unwrap();
            assert_eq!(open.header().command(), MessageCommand::Open);
            assert!(open.payload().starts_with(b"shell,v2,raw:"));
            let local = open.header().arg0();
            send(&mut socket, MessageCommand::Okay, 7, local, &[]).unwrap();
            let close_stdin = read_message(&mut socket).unwrap();
            assert_eq!(close_stdin.header().command(), MessageCommand::Write);
            assert_eq!(
                close_stdin.payload(),
                &[4, 0, 0, 0, 0],
                "non-interactive bounded commands see stdin EOF before output"
            );
            if acknowledge {
                send(&mut socket, MessageCommand::Okay, 7, local, &[]).unwrap();
            }
            run(&mut socket, local);
        });
        (address, thread)
    }

    #[test]
    fn bounded_shell_keeps_both_streams_and_nonzero_exit() {
        let (address, server) = server_with_stdin_ack(false, |socket, local| {
            write_packet(socket, local, &packet(1, b"stdout")).unwrap();
            write_packet(socket, local, &packet(2, b"stderr")).unwrap();
            write_packet(socket, local, &packet(3, &[7])).unwrap();
            send(socket, MessageCommand::Clse, 7, local, &[]).unwrap();
        });
        let output = run_fixture_shell(
            address,
            "fixture command",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        )
        .unwrap();
        assert_eq!(output.stdout, b"stdout");
        assert_eq!(output.stderr, b"stderr");
        assert_eq!(output.exit_code, Some(7));
        assert_eq!(output.termination, BoundedShellTermination::Completed);
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_caps_either_stream_while_output_is_continuous() {
        for channel in [1, 2] {
            let (address, server) = server_with_stdin_ack(channel == 1, move |socket, local| {
                write_packet(socket, local, &packet(3 - channel, b"other stream")).unwrap();
                while write_packet(socket, local, &packet(channel, &[b'x'; 8192])).is_ok() {}
            });
            let output = run_fixture_shell(
                address,
                "fixture infinite output",
                BoundedShellControl::new(Duration::from_secs(3)).unwrap(),
            )
            .unwrap();
            assert_eq!(output.termination, BoundedShellTermination::OutputLimit);
            let (full, other) = if channel == 1 {
                (output.stdout, output.stderr)
            } else {
                (output.stderr, output.stdout)
            };
            assert_eq!(full.len(), SHELL_OUTPUT_LIMIT);
            assert_eq!(other, b"other stream");
            assert_eq!(output.exit_code, None);
            server.join().unwrap();
        }
    }

    #[test]
    fn bounded_shell_deadline_keeps_partial_payload() {
        let (address, server) = server(|socket, local| {
            let mut partial = vec![1];
            partial.extend_from_slice(&100u32.to_le_bytes());
            partial.extend_from_slice(b"part");
            write_packet(socket, local, &partial).unwrap();
            let mut byte = [0];
            assert_eq!(socket.read(&mut byte).unwrap(), 0);
        });
        let started = Instant::now();
        let output = run_fixture_shell(
            address,
            "fixture partial",
            BoundedShellControl::new(Duration::from_millis(500)).unwrap(),
        )
        .unwrap();
        assert_eq!(output.stdout, b"part");
        assert_eq!(output.termination, BoundedShellTermination::Timeout);
        assert_eq!(output.exit_code, None);
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_total_deadline_is_not_reset_by_output() {
        let (address, server) = server(|socket, local| {
            while write_packet(socket, local, &packet(1, b"x")).is_ok() {
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let started = Instant::now();
        let output = run_fixture_shell(
            address,
            "fixture trickle",
            BoundedShellControl::new(Duration::from_millis(500)).unwrap(),
        )
        .unwrap();
        assert!(!output.stdout.is_empty());
        assert_eq!(output.termination, BoundedShellTermination::Timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_rejects_close_without_exit_status() {
        let (address, server) = server(|socket, local| {
            write_packet(socket, local, &packet(1, b"incomplete")).unwrap();
            send(socket, MessageCommand::Clse, 7, local, &[]).unwrap();
        });
        assert!(run_fixture_shell(
            address,
            "fixture interrupted",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap()
        )
        .is_err());
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_cancel_interrupts_a_stalled_socket() {
        let (ready, ready_rx) = std::sync::mpsc::channel();
        let (address, server) = server(move |socket, local| {
            write_packet(socket, local, &packet(1, b"partial")).unwrap();
            ready.send(()).unwrap();
            let mut byte = [0];
            assert_eq!(socket.read(&mut byte).unwrap(), 0);
        });
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let cancel = control.clone();
        let worker =
            std::thread::spawn(move || run_fixture_shell(address, "fixture cancel", control));
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        cancel.cancel();
        assert!(worker.join().unwrap().is_err());
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_rejects_oversized_outer_frame_before_allocating_payload() {
        let (address, server) = server(|socket, local| {
            let message =
                ADBTransportMessage::try_new(MessageCommand::Write, 7, local, &[]).unwrap();
            let mut header = message.header().as_bytes();
            header[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
            socket.write_all(&header).unwrap();
        });
        let error = run_fixture_shell(
            address,
            "private command marker",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("exceeds negotiated maximum"));
        assert!(!error.contains("private command marker"));
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_deadline_interrupts_trickled_outer_header() {
        let (address, server) = server(|socket, local| {
            let message =
                ADBTransportMessage::try_new(MessageCommand::Write, 7, local, &[]).unwrap();
            for byte in message.header().as_bytes() {
                if socket.write_all(&[byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        });
        let started = Instant::now();
        let output = run_fixture_shell(
            address,
            "fixture trickled header",
            BoundedShellControl::new(Duration::from_millis(500)).unwrap(),
        )
        .unwrap();
        assert!(output.stdout.is_empty());
        assert_eq!(output.termination, BoundedShellTermination::Timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }

    #[test]
    fn bounded_shell_open_rejection_redacts_the_command() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            read_message(&mut socket).unwrap();
            send(
                &mut socket,
                MessageCommand::Cnxn,
                0x01000000,
                1_048_576,
                b"device::features=shell_v2\0",
            )
            .unwrap();
            let open = read_message(&mut socket).unwrap();
            assert!(String::from_utf8_lossy(open.payload()).contains("private command marker"));
            send(
                &mut socket,
                MessageCommand::Clse,
                0,
                open.header().arg0(),
                &[],
            )
            .unwrap();
        });
        let error = run_fixture_shell(
            address,
            "private command marker",
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("bounded shell command redacted"));
        assert!(!error.contains("private command marker"));
        server.join().unwrap();
    }
}
