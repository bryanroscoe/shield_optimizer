// Derived in part from Kadb (Apache-2.0): stream/AdbStream.kt.

use std::collections::VecDeque;
use std::io::{Error, ErrorKind, Read, Write};
use std::time::Duration;

use crate::message_devices::adb_message_transport::ADBMessageTransport;
use crate::message_devices::adb_session::ADBSession;
use crate::message_devices::adb_transport_message::ADBTransportMessage;
use crate::message_devices::message_commands::MessageCommand;
use crate::{Result, RustADBError};

// One physical connection carries many sequential streams. A late CLSE or
// WRTE from a stream that already finished (or timed out) must not be
// mistaken for the current stream's traffic, so readers skip a bounded
// number of foreign messages instead of failing the connection.
const MAX_FOREIGN_MESSAGES: usize = 64;

/// Socket write bound for bulk streams (sync, installs). A full TCP send
/// buffer on slow Wi-Fi can legitimately stall one write for seconds.
pub(crate) const STREAM_WRITE_TIMEOUT: Duration = Duration::from_secs(30);

fn is_for_stream(message: &ADBTransportMessage, local_id: u32) -> bool {
    message.header().arg1() == local_id
}

/// Convert an error that crossed `std::io` back into the crate error it
/// started as, so a transport `ProtocolError` stays one.
pub(crate) fn adb_error(error: Error) -> RustADBError {
    if error
        .get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<RustADBError>)
    {
        match error.downcast::<RustADBError>() {
            Ok(inner) => inner,
            Err(error) => RustADBError::IOError(error),
        }
    } else {
        RustADBError::IOError(error)
    }
}

/// Answer a packet addressed to a stream we no longer have, as AOSP's host
/// does: an OKAY for an OPEN we gave up on gets a CLSE so the device closes
/// that service. A WRTE or CLSE gets nothing; acknowledging the WRTE would
/// only invite more data nobody reads, and nobody answers a CLSE.
fn answer_foreign<T: ADBMessageTransport>(
    transport: &mut T,
    message: &ADBTransportMessage,
    timeout: Duration,
) -> Result<()> {
    let header = message.header();
    log::debug!(
        "skipping {} for stream {}:{}",
        header.command(),
        header.arg0(),
        header.arg1()
    );
    if header.command() == MessageCommand::Okay && header.arg0() != 0 {
        transport.write_message_with_timeout(
            ADBTransportMessage::try_new(MessageCommand::Clse, 0, header.arg0(), &[])?,
            timeout,
        )?;
    }
    Ok(())
}

fn too_many_foreign(context: &str) -> RustADBError {
    RustADBError::ProtocolError(format!(
        "{context}: more than {MAX_FOREIGN_MESSAGES} messages for other streams"
    ))
}

/// The service name without its arguments (`shell,v2,raw`, `exec`, `sync`),
/// so errors never carry a whole command line.
fn service_kind(destination: &[u8]) -> String {
    let end = destination
        .iter()
        .position(|&byte| byte == b':')
        .unwrap_or(destination.len());
    String::from_utf8_lossy(&destination[..end]).into_owned()
}

/// Open the device service `destination` as stream `local_id`. Packets for
/// other streams that arrive first are skipped (bounded). A CLSE in reply is
/// the device refusing the service and leaves the connection healthy.
pub(crate) fn open_stream<T: ADBMessageTransport>(
    transport: &mut T,
    destination: &[u8],
    local_id: u32,
    timeout: Duration,
) -> Result<ADBSession<T>> {
    if destination.is_empty() {
        return Err(RustADBError::ADBRequestFailed(
            "ADB service must be non-empty".to_string(),
        ));
    }
    if local_id == 0 {
        return Err(RustADBError::ADBRequestFailed(
            "ADB service local id must not be zero".to_string(),
        ));
    }

    // AOSP retains the trailing NUL for compatibility with older adbd builds
    // that treat the OPEN destination as a C string.
    let mut payload = Vec::with_capacity(destination.len() + 1);
    payload.extend_from_slice(destination);
    payload.push(0);

    transport.write_message_with_timeout(
        ADBTransportMessage::try_new(MessageCommand::Open, local_id, 0, &payload)?,
        timeout,
    )?;
    let mut skipped = 0usize;
    let response = loop {
        let candidate = transport.read_message_with_timeout(timeout)?;
        if is_for_stream(&candidate, local_id) {
            break candidate;
        }
        skipped += 1;
        if skipped > MAX_FOREIGN_MESSAGES {
            return Err(too_many_foreign(&format!(
                "opening {:?}",
                service_kind(destination)
            )));
        }
        answer_foreign(transport, &candidate, timeout)?;
    };
    match response.header().command() {
        MessageCommand::Okay if response.header().arg0() != 0 => Ok(ADBSession::new(
            transport.clone(),
            local_id,
            response.header().arg0(),
        )),
        MessageCommand::Okay => Err(RustADBError::ProtocolError(format!(
            "OKAY for {:?} carried no remote id",
            service_kind(destination)
        ))),
        MessageCommand::Clse => Err(RustADBError::ADBRequestFailed(format!(
            "the device refused to open {:?}",
            service_kind(destination)
        ))),
        other => Err(RustADBError::ProtocolError(format!(
            "{other} in reply to OPEN {:?}",
            service_kind(destination)
        ))),
    }
}

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
    open_stream(transport, service.as_bytes(), local_id, timeout)
}

/// One logical stream as a byte stream, with AOSP's flow control: each WRTE
/// we send waits for its OKAY, and each WRTE we receive is acknowledged once.
/// Every packet's stream ids are checked.
#[derive(Debug)]
pub(crate) struct ADBService<T: ADBMessageTransport> {
    session: ADBSession<T>,
    pending: VecDeque<u8>,
    /// No more traffic is allowed: we sent CLSE, or the device did.
    closed: bool,
    /// The device sent CLSE. Data it sent before that stays readable.
    peer_closed: bool,
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
            peer_closed: false,
            read_timeout,
            write_timeout,
        }
    }

    fn protocol_error(message: impl Into<String>) -> Error {
        Error::new(
            ErrorKind::InvalidData,
            RustADBError::ProtocolError(message.into()),
        )
    }

    fn io_error(error: RustADBError) -> Error {
        match error {
            RustADBError::IOError(error) => error,
            other => Error::other(other),
        }
    }

    /// Received bytes are waiting to be read.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// A write to this stream failed because the device closed it, so what
    /// the device sent before closing is the answer. Any other failure
    /// (a timeout, lost framing, the local source) is the result itself.
    pub(crate) const fn closed_by_peer_during_write(&self, error: &RustADBError) -> bool {
        self.peer_closed && !matches!(error, RustADBError::LocalIo(_))
    }

    /// Convert a failed `read_exact` on this stream. Running into the
    /// device's CLSE ends this one request early and leaves the connection
    /// usable; a transport failure keeps its own (fatal) type.
    pub(crate) fn read_error(&self, error: Error, what: &str) -> RustADBError {
        if error.kind() == ErrorKind::UnexpectedEof && self.peer_closed {
            RustADBError::ADBRequestFailed(format!("the device closed {what} before it finished"))
        } else {
            adb_error(error)
        }
    }

    /// Largest WRTE this connection accepts.
    pub(crate) fn max_write(&self) -> usize {
        self.session.transport().max_payload().max(1)
    }

    fn validate_incoming(&self, message: &ADBTransportMessage) -> std::io::Result<()> {
        let header = message.header();
        // Older adbd versions sometimes use local-id 0 for a normal CLSE,
        // despite the protocol reserving it for a failed OPEN. The local id
        // already matched, and ids are never reused on a connection, so
        // accepting that compatibility form is safe.
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
        let mut skipped = 0usize;
        loop {
            let timeout = self.read_timeout;
            let message = self
                .session
                .get_transport_mut()
                .read_message_with_timeout(timeout)
                .map_err(Self::io_error)?;
            if is_for_stream(&message, self.session.local_id()) {
                self.validate_incoming(&message)?;
                return Ok(message);
            }
            skipped += 1;
            if skipped > MAX_FOREIGN_MESSAGES {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    too_many_foreign("reading an ADB stream"),
                ));
            }
            let write_timeout = self.write_timeout;
            answer_foreign(self.session.get_transport_mut(), &message, write_timeout)
                .map_err(Self::io_error)?;
        }
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
        self.peer_closed = true;
    }

    pub(crate) fn close(&mut self) -> std::io::Result<()> {
        if self.closed {
            return Ok(());
        }
        // Even if the CLSE cannot be sent, the stream is finished locally.
        self.closed = true;
        self.send(MessageCommand::Clse, &[])
    }

    /// Read and discard until the device closes the stream, waiting at most
    /// `timeout` for each packet and reading at most `limit` bytes. Used once
    /// a stream's answer is complete, so the device's CLSE is consumed here
    /// rather than trailing into the next request. Failures are only logged:
    /// the answer is already in hand, and a broken connection fails the
    /// next request instead.
    pub(crate) fn drain_until_closed(&mut self, timeout: Duration, limit: usize) {
        self.read_timeout = self.read_timeout.min(timeout);
        let mut discard = [0_u8; 512];
        let mut drained = 0;
        while drained <= limit {
            match self.read(&mut discard) {
                Ok(0) => return,
                Ok(read) => drained += read,
                Err(error) => {
                    log::debug!("ADB stream did not close after its answer: {error}");
                    return;
                }
            }
        }
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

        let amount = buf.len().min(self.max_write());
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
        max_payload: Option<usize>,
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

        fn with_max_payload(max_payload: usize) -> Self {
            let transport = Self::default();
            transport.state.lock().unwrap().max_payload = Some(max_payload);
            transport
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
        fn max_payload(&self) -> usize {
            self.state
                .lock()
                .unwrap()
                .max_payload
                .unwrap_or(1024 * 1024)
        }

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
    fn write_all_splits_at_the_negotiated_payload_limit() {
        for limit in [4096, 10] {
            let transport = FakeTransport::with_max_payload(limit);
            transport.queue(MessageCommand::Okay, 9, 7, &[]);
            transport.queue(MessageCommand::Okay, 9, 7, &[]);
            let mut service = service(&transport);
            let payload = vec![0x5a; limit + 3];

            service.write_all(&payload).unwrap();
            let first = transport.take_outgoing();
            assert_message(&first, MessageCommand::Write, 7, 9, &payload[..limit]);
            let second = transport.take_outgoing();
            assert_message(&second, MessageCommand::Write, 7, 9, &payload[limit..]);
        }
    }

    #[test]
    fn a_large_negotiated_limit_sends_one_write() {
        let transport = FakeTransport::with_max_payload(256 * 1024);
        transport.queue(MessageCommand::Okay, 9, 7, &[]);
        let mut service = service(&transport);
        let payload = vec![1; 200 * 1024];
        service.write_all(&payload).unwrap();
        assert_eq!(transport.take_outgoing().payload().len(), payload.len());
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
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
    fn late_messages_from_a_finished_stream_are_skipped() {
        let transport = FakeTransport::default();
        // A CLSE and a WRTE left over from the previous stream (local id 7)
        // arrive before the current stream's data.
        transport.queue(MessageCommand::Clse, 99, 7, &[]);
        transport.queue(MessageCommand::Write, 99, 7, b"stale");
        transport.queue(MessageCommand::Write, 2, 1, b"fresh");
        let mut service = ADBService::new(
            ADBSession::new(transport.clone(), 1, 2),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let mut buf = [0u8; 16];
        let n = service.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"fresh");
        // Only the fresh payload is acknowledged.
        let okay = transport.take_outgoing();
        assert_eq!(okay.header().command(), MessageCommand::Okay);
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }

    #[test]
    fn open_service_skips_messages_for_other_streams() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Clse, 99, 7, &[]);
        transport.queue(MessageCommand::Okay, 5, 42, &[]);
        let mut t = transport.clone();
        let session =
            open_service_session(&mut t, "shell:echo", 42, Duration::from_secs(1)).unwrap();
        assert_eq!(session.local_id(), 42);
        assert_eq!(session.remote_id(), 5);
    }

    #[test]
    fn an_orphan_okay_is_answered_with_clse_and_a_stray_write_is_not_acknowledged() {
        let transport = FakeTransport::default();
        // An OKAY for an OPEN we abandoned, then a WRTE from an old stream.
        transport.queue(MessageCommand::Okay, 55, 3, &[]);
        transport.queue(MessageCommand::Write, 66, 4, b"old");
        transport.queue(MessageCommand::Write, 9, 7, b"new");
        let mut service = service(&transport);
        let mut buf = [0u8; 8];
        let n = service.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"new");
        assert_message(&transport.take_outgoing(), MessageCommand::Clse, 0, 55, &[]);
        assert_message(&transport.take_outgoing(), MessageCommand::Okay, 7, 9, &[]);
        assert!(transport.state.lock().unwrap().outgoing.is_empty());
    }

    #[test]
    fn endless_foreign_traffic_is_a_protocol_error() {
        let transport = FakeTransport::default();
        for _ in 0..=MAX_FOREIGN_MESSAGES {
            transport.queue(MessageCommand::Clse, 1, 2, &[]);
        }
        let mut service = service(&transport);
        let error = service.read(&mut [0u8; 1]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(matches!(adb_error(error), RustADBError::ProtocolError(_)));
    }

    #[test]
    fn a_refused_open_is_a_request_failure_not_a_broken_connection() {
        let mut transport = FakeTransport::default();
        transport.queue(MessageCommand::Clse, 0, 7, &[]);
        let error = open_service_session(
            &mut transport,
            "shell,v2,raw:true",
            7,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(
            matches!(error, RustADBError::ADBRequestFailed(_)),
            "{error:?}"
        );

        let mut transport = FakeTransport::default();
        transport.queue(MessageCommand::Write, 9, 7, b"early");
        let error = open_service_session(&mut transport, "shell:true", 7, Duration::from_secs(1))
            .unwrap_err();
        assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    }

    #[test]
    fn data_sent_before_a_close_during_write_stays_readable() {
        let transport = FakeTransport::default();
        transport.queue(MessageCommand::Write, 9, 7, b"FAIL");
        transport.queue(MessageCommand::Clse, 9, 7, &[]);
        let mut service = service(&transport);
        let error = service.write(b"data").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
        assert!(service.peer_closed);
        let mut rest = Vec::new();
        service.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"FAIL");
    }

    #[test]
    fn transport_errors_keep_their_type_across_io() {
        let wrapped = ADBService::<FakeTransport>::io_error(RustADBError::ProtocolError(
            "bad magic".to_string(),
        ));
        assert!(matches!(adb_error(wrapped), RustADBError::ProtocolError(_)));
        let plain = Error::new(ErrorKind::TimedOut, "slow");
        assert!(
            matches!(adb_error(plain), RustADBError::IOError(io) if io.kind() == ErrorKind::TimedOut)
        );
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
