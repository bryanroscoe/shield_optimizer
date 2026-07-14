use std::collections::VecDeque;
use std::io::{Error, ErrorKind, Read, Write};
use std::time::Duration;

use crate::message_devices::adb_message_transport::ADBMessageTransport;
use crate::message_devices::adb_session::ADBSession;
use crate::message_devices::adb_transport_message::ADBTransportMessage;
use crate::message_devices::message_commands::MessageCommand;
use crate::{Result, RustADBError};

// ADB's oldest supported protocol payload. Modern devices advertise larger
// values, but a conservative cap keeps this generic stream interoperable
// without expanding adb_client's connection handshake state.
const MAX_MESSAGE_PAYLOAD: usize = 4 * 1024;

pub(crate) fn open_service_session<T: ADBMessageTransport>(
    transport: &mut T,
    service: &str,
    local_id: u32,
    timeout: Duration,
) -> Result<ADBSession<T>> {
    if service.is_empty() || service.as_bytes().contains(&0) {
        return Err(RustADBError::ADBRequestFailed(
            "ADB service must be non-empty and contain no NUL bytes".to_string(),
        ));
    }
    if local_id == 0 {
        return Err(RustADBError::ADBRequestFailed(
            "ADB service local id must not be zero".to_string(),
        ));
    }

    // AOSP retains the trailing NUL for compatibility with older adbd builds
    // that treat the OPEN destination as a C string.
    let mut destination = Vec::with_capacity(service.len() + 1);
    destination.extend_from_slice(service.as_bytes());
    destination.push(0);

    transport.write_message_with_timeout(
        ADBTransportMessage::try_new(MessageCommand::Open, local_id, 0, &destination)?,
        timeout,
    )?;
    let response = transport.read_message_with_timeout(timeout)?;
    if response.header().command() != MessageCommand::Okay {
        return Err(RustADBError::ADBRequestFailed(format!(
            "Open service {service:?} failed: got {} instead of OKAY",
            response.header().command()
        )));
    }
    if response.header().arg1() != local_id {
        return Err(RustADBError::ADBRequestFailed(format!(
            "Open service {service:?} used local id {} instead of {local_id}",
            response.header().arg1()
        )));
    }
    if response.header().arg0() == 0 {
        return Err(RustADBError::ADBRequestFailed(format!(
            "Open service {service:?} returned an invalid remote id"
        )));
    }

    Ok(ADBSession::new(
        transport.clone(),
        local_id,
        response.header().arg0(),
    ))
}

#[derive(Debug)]
pub(crate) struct ADBService<T: ADBMessageTransport> {
    session: ADBSession<T>,
    pending: VecDeque<u8>,
    closed: bool,
    read_timeout: Duration,
    write_timeout: Duration,
}

impl<T: ADBMessageTransport> ADBService<T> {
    pub(crate) const fn new(
        session: ADBSession<T>,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> Self {
        Self {
            session,
            pending: VecDeque::new(),
            closed: false,
            read_timeout,
            write_timeout,
        }
    }

    fn protocol_error(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::InvalidData, message.into())
    }

    fn io_error(error: RustADBError) -> Error {
        match error {
            RustADBError::IOError(error) => error,
            other => Error::other(other),
        }
    }

    fn validate_incoming(&self, message: &ADBTransportMessage) -> std::io::Result<()> {
        let header = message.header();
        // Older adbd versions sometimes use local-id 0 for a normal CLSE,
        // despite the protocol reserving it for a failed OPEN. This connection
        // carries one stream, so accepting that compatibility form is safe.
        let remote_id_matches = header.arg0() == self.session.remote_id()
            || (header.command() == MessageCommand::Clse && header.arg0() == 0);
        if !remote_id_matches || header.arg1() != self.session.local_id() {
            return Err(Self::protocol_error(format!(
                "ADB service message used stream ids {}:{}, expected {}:{}",
                header.arg0(),
                header.arg1(),
                self.session.remote_id(),
                self.session.local_id()
            )));
        }
        Ok(())
    }

    fn send(&mut self, command: MessageCommand, payload: &[u8]) -> std::io::Result<()> {
        let message = ADBTransportMessage::try_new(
            command,
            self.session.local_id(),
            self.session.remote_id(),
            payload,
        )
        .map_err(Error::other)?;
        self.session
            .get_transport_mut()
            .write_message_with_timeout(message, self.write_timeout)
            .map_err(Self::io_error)
    }

    fn receive(&mut self) -> std::io::Result<ADBTransportMessage> {
        let message = self
            .session
            .get_transport_mut()
            .read_message_with_timeout(self.read_timeout)
            .map_err(Self::io_error)?;
        self.validate_incoming(&message)?;
        Ok(message)
    }

    fn acknowledge_payload(&mut self, message: ADBTransportMessage) -> std::io::Result<()> {
        self.send(MessageCommand::Okay, &[])?;
        self.pending.extend(message.into_payload());
        Ok(())
    }

    const fn mark_peer_closed(&mut self) {
        // ADB protocol.txt explicitly says the recipient must not reply to
        // CLSE. Any pending local WRTE or CLSE is simply abandoned.
        self.closed = true;
    }

    pub(crate) fn close(&mut self) -> std::io::Result<()> {
        if self.closed {
            return Ok(());
        }
        self.send(MessageCommand::Clse, &[])?;
        self.closed = true;
        Ok(())
    }

    fn drain_pending(&mut self, buf: &mut [u8]) -> usize {
        let amount = buf.len().min(self.pending.len());
        for slot in &mut buf[..amount] {
            *slot = self
                .pending
                .pop_front()
                .expect("pending length was checked");
        }
        amount
    }
}

impl<T: ADBMessageTransport> Read for ADBService<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if !self.pending.is_empty() {
            return Ok(self.drain_pending(buf));
        }
        if self.closed {
            return Ok(0);
        }

        loop {
            let message = self.receive()?;
            match message.header().command() {
                MessageCommand::Write => {
                    self.acknowledge_payload(message)?;
                    if !self.pending.is_empty() {
                        return Ok(self.drain_pending(buf));
                    }
                }
                MessageCommand::Clse => {
                    self.mark_peer_closed();
                    return Ok(0);
                }
                command => {
                    return Err(Self::protocol_error(format!(
                        "Unexpected {command} while reading ADB service"
                    )));
                }
            }
        }
    }
}

impl<T: ADBMessageTransport> Write for ADBService<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.closed {
            return Err(Error::new(ErrorKind::BrokenPipe, "ADB service is closed"));
        }
        if buf.is_empty() {
            return Ok(0);
        }

        let amount = buf.len().min(MAX_MESSAGE_PAYLOAD);
        self.send(MessageCommand::Write, &buf[..amount])?;
        loop {
            let response = self.receive()?;
            match response.header().command() {
                MessageCommand::Okay => return Ok(amount),
                MessageCommand::Write => self.acknowledge_payload(response)?,
                MessageCommand::Clse => {
                    self.mark_peer_closed();
                    return Err(Error::new(
                        ErrorKind::BrokenPipe,
                        "ADB service closed while acknowledging a write",
                    ));
                }
                command => {
                    return Err(Self::protocol_error(format!(
                        "Unexpected {command} while writing ADB service"
                    )));
                }
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<T: ADBMessageTransport> Drop for ADBService<T> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb_transport::ADBTransport;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Default)]
    struct FakeTransport {
        state: Arc<Mutex<FakeState>>,
    }

    #[derive(Debug, Default)]
    struct FakeState {
        incoming: VecDeque<ADBTransportMessage>,
        outgoing: VecDeque<ADBTransportMessage>,
        read_timeouts: Vec<Duration>,
        write_timeouts: Vec<Duration>,
    }

    impl FakeTransport {
        fn queue(&self, command: MessageCommand, arg0: u32, arg1: u32, payload: &[u8]) {
            self.state
                .lock()
                .unwrap()
                .incoming
                .push_back(ADBTransportMessage::try_new(command, arg0, arg1, payload).unwrap());
        }

        fn take_outgoing(&self) -> ADBTransportMessage {
            self.state.lock().unwrap().outgoing.pop_front().unwrap()
        }
    }

    impl ADBTransport for FakeTransport {
        fn connect(&mut self) -> Result<()> {
            Ok(())
        }

        fn disconnect(&mut self) -> Result<()> {
            Ok(())
        }
    }

    impl ADBMessageTransport for FakeTransport {
        fn read_message_with_timeout(&mut self, timeout: Duration) -> Result<ADBTransportMessage> {
            let mut state = self.state.lock().unwrap();
            state.read_timeouts.push(timeout);
            state.incoming.pop_front().ok_or_else(|| {
                RustADBError::IOError(Error::new(ErrorKind::TimedOut, "fake read timeout"))
            })
        }

        fn write_message_with_timeout(
            &mut self,
            message: ADBTransportMessage,
            timeout: Duration,
        ) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            state.write_timeouts.push(timeout);
            state.outgoing.push_back(message);
            Ok(())
        }
    }

    fn service(transport: &FakeTransport) -> ADBService<FakeTransport> {
        ADBService::new(
            ADBSession::new(transport.clone(), 7, 9),
            Duration::from_millis(123),
            Duration::from_millis(456),
        )
    }

    fn assert_message(
        message: &ADBTransportMessage,
        command: MessageCommand,
        arg0: u32,
        arg1: u32,
        payload: &[u8],
    ) {
        assert_eq!(message.header().command(), command);
        assert_eq!(message.header().arg0(), arg0);
        assert_eq!(message.header().arg1(), arg1);
        assert_eq!(message.payload(), payload);
    }

    #[test]
    fn open_service_sends_exact_destination_and_ids() {
        let mut transport = FakeTransport::default();
        transport.queue(MessageCommand::Okay, 9, 7, &[]);
        let session = open_service_session(
            &mut transport,
            "localabstract:scrcpy_0a1b2c3d",
            7,
            Duration::from_secs(2),
        )
        .unwrap();

        assert_eq!(session.local_id(), 7);
        assert_eq!(session.remote_id(), 9);
        assert_message(
            &transport.take_outgoing(),
            MessageCommand::Open,
            7,
            0,
            b"localabstract:scrcpy_0a1b2c3d\0",
        );
    }

    #[test]
    fn read_acknowledges_once_and_buffers_partial_payload() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Write, 9, 7, b"hello");
        let mut service = service(&transport);
        let mut first = [0; 2];
        let mut second = [0; 3];

        assert_eq!(service.read(&mut first).unwrap(), 2);
        assert_eq!(&first, b"he");
        assert_eq!(service.read(&mut second).unwrap(), 3);
        assert_eq!(&second, b"llo");
        assert_message(&transport.take_outgoing(), MessageCommand::Okay, 7, 9, &[]);
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }

    #[test]
    fn write_waits_for_okay_and_uses_configured_timeout() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Okay, 9, 7, &[]);
        let mut service = service(&transport);

        assert_eq!(service.write(b"control").unwrap(), 7);
        assert_message(
            &transport.take_outgoing(),
            MessageCommand::Write,
            7,
            9,
            b"control",
        );
        let state = transport.state.lock().unwrap();
        assert_eq!(state.read_timeouts, [Duration::from_millis(123)]);
        assert_eq!(state.write_timeouts, [Duration::from_millis(456)]);
    }

    #[test]
    fn write_all_splits_at_the_conservative_adb_payload_limit() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Okay, 9, 7, &[]);
        transport.queue(MessageCommand::Okay, 9, 7, &[]);
        let mut service = service(&transport);
        let payload = vec![0x5a; MAX_MESSAGE_PAYLOAD + 3];

        service.write_all(&payload).unwrap();
        let first = transport.take_outgoing();
        assert_message(
            &first,
            MessageCommand::Write,
            7,
            9,
            &payload[..MAX_MESSAGE_PAYLOAD],
        );
        let second = transport.take_outgoing();
        assert_message(
            &second,
            MessageCommand::Write,
            7,
            9,
            &payload[MAX_MESSAGE_PAYLOAD..],
        );
    }

    #[test]
    fn incoming_payload_during_write_is_buffered_and_acknowledged() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Write, 9, 7, b"x");
        transport.queue(MessageCommand::Okay, 9, 7, &[]);
        let mut service = service(&transport);

        assert_eq!(service.write(b"y").unwrap(), 1);
        assert_message(
            &transport.take_outgoing(),
            MessageCommand::Write,
            7,
            9,
            b"y",
        );
        assert_message(&transport.take_outgoing(), MessageCommand::Okay, 7, 9, &[]);
        let mut received = [0];
        assert_eq!(service.read(&mut received).unwrap(), 1);
        assert_eq!(received, *b"x");
    }

    #[test]
    fn peer_close_is_not_acknowledged_and_becomes_eof() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Clse, 9, 7, &[]);
        let mut service = service(&transport);
        let mut byte = [0];

        assert_eq!(service.read(&mut byte).unwrap(), 0);
        assert_eq!(service.read(&mut byte).unwrap(), 0);
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }

    #[test]
    fn explicit_local_close_sends_clse_once() {
        let transport = FakeTransport::default();
        let mut service = service(&transport);

        service.close().unwrap();
        service.close().unwrap();
        assert_message(&transport.take_outgoing(), MessageCommand::Clse, 7, 9, &[]);
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }

    #[test]
    fn legacy_zero_local_id_close_is_accepted() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Clse, 0, 7, &[]);
        let mut service = service(&transport);
        let mut byte = [0];

        assert_eq!(service.read(&mut byte).unwrap(), 0);
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }

    #[test]
    fn wrong_stream_ids_are_rejected() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Write, 99, 7, b"wrong stream");
        let mut service = service(&transport);
        let mut byte = [0];

        let error = service.read(&mut byte).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("expected 9:7"));
    }

    #[test]
    fn transport_read_timeout_preserves_io_error_kind() {
        let transport = FakeTransport::default();
        let mut service = service(&transport);
        let mut byte = [0];

        let error = service.read(&mut byte).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[test]
    fn empty_or_nul_service_is_rejected_without_writing() {
        for invalid in ["", "localabstract:bad\0name"] {
            let mut transport = FakeTransport::default();
            assert!(
                open_service_session(&mut transport, invalid, 7, Duration::from_secs(1)).is_err()
            );
            assert!(transport.state.lock().unwrap().outgoing.is_empty());
        }
    }

    #[test]
    fn zero_local_id_is_rejected_without_writing() {
        let mut transport = FakeTransport::default();
        assert!(
            open_service_session(
                &mut transport,
                "localabstract:scrcpy_0a1b2c3d",
                0,
                Duration::from_secs(1),
            )
            .is_err()
        );
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }
}
