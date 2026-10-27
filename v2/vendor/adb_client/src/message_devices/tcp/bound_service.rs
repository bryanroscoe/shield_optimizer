use super::{
    BoundedShellControl, bounded_shell::Finish, streamed_install::verify_hardware_binding,
    tcp_transport::TcpTransport,
};
use crate::{
    RustADBError,
    message_devices::{adb_message_device::ADBMessageDevice, adb_service::ADBService},
};
use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
    time::Duration,
};
use thiserror::Error;

/// Whether setup failed before the requested service OPEN or after it may have arrived.
#[derive(Debug, Error)]
pub enum BoundServiceError {
    /// No requested service OPEN was attempted.
    #[error("bound service setup failed: {0}")]
    BeforeOpen(#[source] RustADBError),
    /// The peer explicitly rejected the requested OPEN before accepting input.
    #[error("bound service OPEN was rejected: {0}")]
    Rejected(#[source] RustADBError),
    /// A requested service OPEN may have reached the peer; never replay automatically.
    #[error("bound service OPEN outcome is unknown: {0}")]
    OpenUncertain(#[source] RustADBError),
}

/// One dedicated identity-bound service with a finite lifetime and independent cancellation.
/// Dropping the stream cancels its socket before potentially blocking protocol cleanup.
pub struct BoundDeviceService {
    stream: ADBService<TcpTransport>,
    _device: ADBMessageDevice<TcpTransport>,
    finish: Finish,
}
impl std::fmt::Debug for BoundDeviceService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundDeviceService").finish_non_exhaustive()
    }
}
impl BoundDeviceService {
    /// A handle that can interrupt blocking I/O while another worker owns this stream.
    #[must_use]
    pub fn cancellation(&self) -> BoundedShellControl {
        self.finish.0.clone()
    }
}
impl Read for BoundDeviceService {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(bytes)
    }
}
impl Write for BoundDeviceService {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
impl Drop for BoundDeviceService {
    fn drop(&mut self) {
        self.finish.0.cancel();
    }
}

/// Opens only after the dedicated connection reports the selected hardware identity.
/// The identity must have been captured on the original owned control connection.
/// No key creation, subprocess fallback, retry or service replay occurs.
pub fn open_bound_service(
    address: SocketAddr,
    key: &Path,
    expected_id: &str,
    service: &str,
    control: BoundedShellControl,
) -> Result<BoundDeviceService, BoundServiceError> {
    open_with_device(expected_id, service, control, |control| {
        let transport = TcpTransport::with_shell_control(address, key, control.clone());
        ADBMessageDevice::new_with_required_key(
            transport,
            key,
            control.remaining()?.min(Duration::from_secs(30)),
        )
    })
}

fn open_with_device<F>(
    expected_id: &str,
    service: &str,
    control: BoundedShellControl,
    connect: F,
) -> Result<BoundDeviceService, BoundServiceError>
where
    F: FnOnce(&BoundedShellControl) -> crate::Result<ADBMessageDevice<TcpTransport>>,
{
    let finish = Finish(control);
    if service.is_empty() || service.len() > 4096 || service.as_bytes().contains(&0) {
        return Err(BoundServiceError::BeforeOpen(
            RustADBError::ADBRequestFailed("invalid bounded service name".into()),
        ));
    }
    let mut device = connect(&finish.0).map_err(BoundServiceError::BeforeOpen)?;
    verify_hardware_binding(&mut device, expected_id, &finish.0)
        .map_err(BoundServiceError::BeforeOpen)?;
    let remaining = finish
        .0
        .remaining()
        .map_err(RustADBError::from)
        .map_err(BoundServiceError::BeforeOpen)?;
    let session = device
        .open_bounded_shell_service(service, remaining)
        .map_err(|error| match error {
            RustADBError::ServiceRejected(_) => BoundServiceError::Rejected(error),
            _ => BoundServiceError::OpenUncertain(error),
        })?;
    finish
        .0
        .remaining()
        .map_err(RustADBError::from)
        .map_err(BoundServiceError::OpenUncertain)?;
    let stream =
        ADBService::new(session, remaining, Duration::from_secs(2)).with_pending_limit(64 * 1024);
    Ok(BoundDeviceService {
        stream,
        _device: device,
        finish,
    })
}

#[cfg(test)]
mod tests {
    use super::super::streamed_install::tests::{
        fixture_key, hello, receive, reply, send, serve_identity,
    };
    use super::*;
    use crate::message_devices::message_commands::MessageCommand;
    use std::{
        net::{TcpListener, TcpStream},
        thread::JoinHandle,
    };

    fn server<F>(id: &'static [u8], run: F) -> (SocketAddr, JoinHandle<()>)
    where
        F: FnOnce(&mut TcpStream, u32) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            hello(&mut socket);
            serve_identity(&mut socket, id, 0, b"");
            if id != b"expected-TV\n" {
                assert!(
                    receive(&mut socket).is_err(),
                    "no requested service may open on another TV"
                );
                return;
            }
            let open = receive(&mut socket).unwrap();
            assert_eq!(open.payload(), b"localabstract:fixture\0");
            let local = open.header().arg0();
            run(&mut socket, local);
        });
        (address, thread)
    }
    fn open(
        address: SocketAddr,
        control: BoundedShellControl,
    ) -> Result<BoundDeviceService, BoundServiceError> {
        open_with_device("expected-TV", "localabstract:fixture", control, |control| {
            let transport = TcpTransport::with_shell_control(
                address,
                Path::new("fixture-never-written"),
                control.clone(),
            );
            ADBMessageDevice::new_with_fixture_key(transport, &fixture_key(), control.remaining()?)
        })
    }
    #[test]
    fn bound_service_mismatch_or_unknown_identity_opens_no_requested_service() {
        for id in [&b"other-TV\n"[..], &b"unknown\n"[..]] {
            let (address, server) =
                server(id, |_, _| panic!("mismatch may not reach requested OPEN"));
            assert!(matches!(
                open(
                    address,
                    BoundedShellControl::new(Duration::from_secs(2)).unwrap()
                ),
                Err(BoundServiceError::BeforeOpen(_))
            ));
            server.join().unwrap();
        }
    }
    #[test]
    fn bound_service_explicit_open_refusal_is_distinct_from_lost_acknowledgement() {
        let (address, server) = server(b"expected-TV\n", |socket, local| {
            send(socket, MessageCommand::Clse, local, &[]).unwrap();
        });
        assert!(matches!(
            open(
                address,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap()
            ),
            Err(BoundServiceError::Rejected(_))
        ));
        server.join().unwrap();
    }

    #[test]
    fn bound_service_lost_open_ack_is_uncertain_and_invalid_names_never_connect() {
        let (address, server) = server(b"expected-TV\n", |_, _| {});
        assert!(matches!(
            open(
                address,
                BoundedShellControl::new(Duration::from_secs(2)).unwrap()
            ),
            Err(BoundServiceError::OpenUncertain(_))
        ));
        server.join().unwrap();
        assert!(matches!(
            open_with_device(
                "expected-TV",
                "bad\0name",
                BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
                |_| panic!("invalid service must not connect")
            ),
            Err(BoundServiceError::BeforeOpen(_))
        ));
    }
    #[test]
    fn bound_service_transfers_exact_bytes_and_drop_closes_only_its_connection() {
        let (address, server) = server(b"expected-TV\n", |socket, local| {
            send(socket, MessageCommand::Okay, local, &[]).unwrap();
            let message = receive(socket).unwrap();
            assert_eq!(message.payload(), b"fixture-input");
            send(socket, MessageCommand::Okay, local, &[]).unwrap();
            reply(socket, local, b"fixture-output", false);
            assert!(receive(socket).is_err());
        });
        let mut service = open(
            address,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        )
        .unwrap();
        service.write_all(b"fixture-input").unwrap();
        let mut output = [0; 14];
        service.read_exact(&mut output).unwrap();
        assert_eq!(&output, b"fixture-output");
        drop(service);
        server.join().unwrap();
    }
    #[test]
    fn bound_service_idle_read_survives_within_its_total_lifetime() {
        let (address, server) = server(b"expected-TV\n", |socket, local| {
            send(socket, MessageCommand::Okay, local, &[]).unwrap();
            std::thread::sleep(Duration::from_secs(6));
            reply(socket, local, b"alive", false);
            assert!(receive(socket).is_err());
        });
        let mut service = open(
            address,
            BoundedShellControl::new(Duration::from_secs(10)).unwrap(),
        )
        .unwrap();
        let mut bytes = [0; 5];
        service.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"alive");
        drop(service);
        server.join().unwrap();
    }

    #[test]
    fn bound_service_independent_cancel_and_total_deadline_interrupt_owned_reads() {
        for cancel in [false, true] {
            let (address, server) = server(b"expected-TV\n", |socket, local| {
                send(socket, MessageCommand::Okay, local, &[]).unwrap();
                assert!(receive(socket).is_err());
            });
            let mut service = open(
                address,
                BoundedShellControl::new(if cancel {
                    Duration::from_secs(2)
                } else {
                    Duration::from_millis(300)
                })
                .unwrap(),
            )
            .unwrap();
            let started = std::time::Instant::now();
            let handle = service.cancellation();
            let worker = std::thread::spawn(move || {
                let mut byte = [0];
                let result = service.read(&mut byte);
                drop(service);
                result
            });
            if cancel {
                std::thread::sleep(Duration::from_millis(25));
                handle.cancel();
            }
            assert!(worker.join().unwrap().is_err());
            assert!(started.elapsed() < Duration::from_secs(1));
            server.join().unwrap();
        }
    }
    #[test]
    fn bound_service_caps_output_while_a_write_waits_for_ack() {
        let (address, server) = server(b"expected-TV\n", |socket, local| {
            send(socket, MessageCommand::Okay, local, &[]).unwrap();
            assert_eq!(receive(socket).unwrap().payload(), b"input");
            for _ in 0..100 {
                if send(socket, MessageCommand::Write, local, &[42; 1024]).is_err() {
                    break;
                }
                match receive(socket) {
                    Ok(message) if message.header().command() == MessageCommand::Okay => {}
                    _ => break,
                }
            }
        });
        let mut service = open(
            address,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        )
        .unwrap();
        assert!(service.write_all(b"input").is_err());
        drop(service);
        server.join().unwrap();
    }
    #[test]
    fn bound_service_cancel_during_requested_open_is_uncertain_and_closes_setup_socket() {
        let (started, started_rx) = std::sync::mpsc::channel();
        let (address, server) = server(b"expected-TV\n", move |socket, _| {
            started.send(()).unwrap();
            assert!(receive(socket).is_err());
        });
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let signal = control.clone();
        let worker = std::thread::spawn(move || open(address, control));
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let start = std::time::Instant::now();
        signal.cancel();
        assert!(matches!(
            worker.join().unwrap(),
            Err(BoundServiceError::OpenUncertain(_))
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }
}
