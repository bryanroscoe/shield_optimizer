use std::{
    io::{Cursor, Read},
    time::Duration,
};

use crate::{
    AdbStatResponse, BinaryDecodable, Result, RustADBError,
    message_devices::{
        adb_message_transport::ADBMessageTransport,
        adb_transport_message::ADBTransportMessage,
        message_commands::{MessageCommand, MessageSubcommand},
        utils::BinaryEncodable,
    },
};

const BUFFER_SIZE: usize = 65535;

/// Represent a session between an `ADBDevice` and remote `adbd`.
#[derive(Debug)]
pub struct ADBSession<T: ADBMessageTransport> {
    transport: T,
    local_id: u32,
    remote_id: u32,
}

impl<T: ADBMessageTransport> ADBSession<T> {
    pub const fn new(transport: T, local_id: u32, remote_id: u32) -> Self {
        Self {
            transport,
            local_id,
            remote_id,
        }
    }

    pub const fn get_transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub const fn local_id(&self) -> u32 {
        self.local_id
    }

    pub const fn remote_id(&self) -> u32 {
        self.remote_id
    }

    /// Receive a message and acknowledge it by replying with an `OKAY` command
    pub(crate) fn recv_and_reply_okay(&mut self) -> Result<ADBTransportMessage> {
        let message = self.transport.read_message()?;
        self.reply_okay()?;
        Ok(message)
    }

    pub(crate) fn recv_and_reply_okay_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<ADBTransportMessage> {
        let message = self.transport.read_message_with_timeout(timeout)?;
        self.reply_okay()?;
        Ok(message)
    }

    fn reply_okay(&mut self) -> Result<()> {
        self.transport.write_message(ADBTransportMessage::try_new(
            MessageCommand::Okay,
            self.local_id,
            self.remote_id,
            &[],
        )?)?;
        Ok(())
    }

    /// Expect a message with an `OKAY` command after sending a message.
    pub(crate) fn send_and_expect_okay(
        &mut self,
        message: ADBTransportMessage,
    ) -> Result<ADBTransportMessage> {
        self.transport.write_message(message)?;

        self.transport.read_message().and_then(|message| {
            message.assert_command(MessageCommand::Okay)?;
            Ok(message)
        })
    }

    fn read_sync_exact(
        &mut self,
        pending: &mut Cursor<Vec<u8>>,
        mut bytes: &mut [u8],
    ) -> Result<()> {
        while !bytes.is_empty() {
            let count = pending.read(bytes)?;
            if count > 0 {
                bytes = &mut bytes[count..];
                continue;
            }
            let message = self.transport.read_message()?;
            message.assert_command(MessageCommand::Write)?;
            if message.header().arg0() != self.remote_id || message.header().arg1() != self.local_id
            {
                return Err(RustADBError::ADBRequestFailed(
                    "Sync response belongs to another stream".into(),
                ));
            }
            self.reply_okay()?;
            *pending = Cursor::new(message.into_payload());
        }
        Ok(())
    }

    pub(crate) fn recv_file<W: std::io::Write>(&mut self, mut output: W) -> Result<()> {
        // Sync records are a byte stream, independent of ADB WRTE boundaries.
        // FAIL is a remote error body, never file content or a successful EOF.
        let mut pending = Cursor::new(Vec::new());
        loop {
            let mut header = [0u8; 8];
            self.read_sync_exact(&mut pending, &mut header)?;
            let length =
                u32::from_le_bytes(header[4..].try_into().expect("fixed sync header")) as usize;
            match &header[..4] {
                b"DONE" => {
                    if pending.position() != pending.get_ref().len() as u64 {
                        return Err(RustADBError::UnknownResponseType(
                            "Unexpected sync bytes after DONE".into(),
                        ));
                    }
                    return Ok(());
                }
                b"FAIL" => return Err(self.read_sync_failure(&mut pending, length)?),
                b"DATA" => {
                    if length > 64 * 1024 {
                        return Err(RustADBError::UnknownResponseType(
                            "Sync record exceeds the 64 KiB limit".into(),
                        ));
                    }
                    let mut body = vec![0u8; length];
                    self.read_sync_exact(&mut pending, &mut body)?;
                    output.write_all(&body)?;
                }
                _ => {
                    return Err(RustADBError::UnknownResponseType(
                        "Unexpected sync record while downloading".into(),
                    ));
                }
            }
        }
    }

    fn read_sync_failure(
        &mut self,
        pending: &mut Cursor<Vec<u8>>,
        length: usize,
    ) -> Result<RustADBError> {
        if length > 64 * 1024 {
            return Err(RustADBError::UnknownResponseType(
                "Sync failure exceeds the 64 KiB limit".into(),
            ));
        }
        let mut body = vec![0u8; length];
        self.read_sync_exact(pending, &mut body)?;
        Ok(RustADBError::ADBRequestFailed(format!(
            "Sync transfer rejected: {}",
            String::from_utf8_lossy(&body[..body.len().min(4096)])
        )))
    }

    fn recv_sync_status(&mut self) -> Result<()> {
        let mut pending = Cursor::new(Vec::new());
        let mut header = [0u8; 8];
        self.read_sync_exact(&mut pending, &mut header)?;
        match &header[..4] {
            b"OKAY" if pending.position() == pending.get_ref().len() as u64 => Ok(()),
            b"FAIL" => {
                let length =
                    u32::from_le_bytes(header[4..].try_into().expect("fixed sync header")) as usize;
                Err(self.read_sync_failure(&mut pending, length)?)
            }
            _ => Err(RustADBError::UnknownResponseType(
                "Missing or malformed sync upload acknowledgement".into(),
            )),
        }
    }

    pub(crate) fn push_file<R: std::io::Read>(&mut self, mut reader: R) -> Result<()> {
        let mut buffer = vec![0; BUFFER_SIZE].into_boxed_slice();
        let amount_read = reader.read(&mut buffer)?;
        let subcommand_data = MessageSubcommand::Data.with_arg(u32::try_from(amount_read)?);

        let mut serialized_message = subcommand_data.encode();
        serialized_message.append(&mut buffer[..amount_read].to_vec());

        let message = ADBTransportMessage::try_new(
            MessageCommand::Write,
            self.local_id(),
            self.remote_id(),
            &serialized_message,
        )?;

        self.send_and_expect_okay(message)?;

        loop {
            let mut buffer = vec![0; BUFFER_SIZE].into_boxed_slice();

            match reader.read(&mut buffer) {
                Ok(0) => {
                    // Currently file mtime is not forwarded
                    let subcommand_data = MessageSubcommand::Done.with_arg(0);

                    let message = ADBTransportMessage::try_new(
                        MessageCommand::Write,
                        self.local_id(),
                        self.remote_id(),
                        &subcommand_data.encode(),
                    )?;

                    self.send_and_expect_okay(message)?;

                    return self.recv_sync_status();
                }
                Ok(size) => {
                    let subcommand_data = MessageSubcommand::Data.with_arg(u32::try_from(size)?);

                    let mut serialized_message = subcommand_data.encode();
                    serialized_message.append(&mut buffer[..size].to_vec());

                    let message = ADBTransportMessage::try_new(
                        MessageCommand::Write,
                        self.local_id(),
                        self.remote_id(),
                        &serialized_message,
                    )?;

                    self.send_and_expect_okay(message)?;
                }
                Err(e) => {
                    return Err(RustADBError::IOError(e));
                }
            }
        }
    }

    pub(crate) fn stat_with_explicit_ids(&mut self, remote_path: &str) -> Result<AdbStatResponse> {
        let stat_buffer = MessageSubcommand::Stat.with_arg(u32::try_from(remote_path.len())?);
        let message = ADBTransportMessage::try_new(
            MessageCommand::Write,
            self.local_id(),
            self.remote_id(),
            &stat_buffer.encode(),
        )?;
        self.send_and_expect_okay(message)?;
        self.send_and_expect_okay(ADBTransportMessage::try_new(
            MessageCommand::Write,
            self.local_id(),
            self.remote_id(),
            remote_path.as_bytes(),
        )?)?;

        let mut pending = Cursor::new(Vec::new());
        let mut header = [0u8; 8];
        self.read_sync_exact(&mut pending, &mut header)?;
        if &header[..4] == b"FAIL" {
            let length =
                u32::from_le_bytes(header[4..].try_into().expect("fixed sync header")) as usize;
            return Err(self.read_sync_failure(&mut pending, length)?);
        }
        if &header[..4] != b"STAT" {
            return Err(RustADBError::UnknownResponseType(
                "Expected sync STAT response".into(),
            ));
        }
        let mut fields = [0u8; 12];
        fields[..4].copy_from_slice(&header[4..]);
        self.read_sync_exact(&mut pending, &mut fields[4..])?;
        if pending.position() != pending.get_ref().len() as u64 {
            return Err(RustADBError::UnknownResponseType(
                "Unexpected sync bytes after STAT".into(),
            ));
        }
        AdbStatResponse::decode(&fields)
    }
}

impl<T: ADBMessageTransport> Drop for ADBSession<T> {
    fn drop(&mut self) {
        // some devices will repeat the trailing CLSE command to ensure
        // the client has acknowledged it. Read them quickly if present.
        while let Ok(_discard_close_message) = self
            .transport
            .read_message_with_timeout(Duration::from_millis(20))
        {}
    }
}

#[cfg(test)]
mod recv_tests {
    use super::*;
    use crate::adb_transport::ADBTransport;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    #[derive(Clone)]
    struct Transport(Arc<Mutex<VecDeque<ADBTransportMessage>>>);
    impl ADBTransport for Transport {
        fn connect(&mut self) -> Result<()> {
            Ok(())
        }
        fn disconnect(&mut self) -> Result<()> {
            Ok(())
        }
    }
    impl ADBMessageTransport for Transport {
        fn read_message_with_timeout(&mut self, _: Duration) -> Result<ADBTransportMessage> {
            self.0.lock().unwrap().pop_front().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "fixture stream ended")
                    .into()
            })
        }
        fn write_message_with_timeout(
            &mut self,
            message: ADBTransportMessage,
            _: Duration,
        ) -> Result<()> {
            assert_eq!(message.header().command(), MessageCommand::Okay);
            assert_eq!((message.header().arg0(), message.header().arg1()), (7, 9));
            Ok(())
        }
    }
    fn session(bytes: &[u8], width: usize) -> ADBSession<Transport> {
        let messages = bytes
            .chunks(width)
            .map(|part| ADBTransportMessage::try_new(MessageCommand::Write, 9, 7, part).unwrap())
            .collect();
        ADBSession::new(Transport(Arc::new(Mutex::new(messages))), 7, 9)
    }
    fn record(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_vec();
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn downloads_fragmented_headers_bodies_and_done_without_inspecting_file_bytes() {
        let first = b"file contents ending with DONE\0\0\0\0";
        let second = b"more contents";
        let bytes = [
            record(b"DATA", first),
            record(b"DATA", second),
            record(b"DONE", b""),
        ]
        .concat();
        for width in 1..=bytes.len() {
            let mut output = Vec::new();
            session(&bytes, width).recv_file(&mut output).unwrap();
            assert_eq!(
                output,
                [first.as_slice(), second.as_slice()].concat(),
                "WRTE width {width}"
            );
        }
        for width in 1..=8 {
            let mut output = Vec::new();
            session(&record(b"DONE", b""), width)
                .recv_file(&mut output)
                .unwrap();
            assert!(output.is_empty());
        }
    }

    #[test]
    fn rejected_download_never_writes_failure_text_as_file_contents() {
        let bytes = record(b"FAIL", b"open failed: Permission denied");
        for width in 1..=bytes.len() {
            let mut output = Vec::new();
            let error = session(&bytes, width).recv_file(&mut output).unwrap_err();
            assert!(error.to_string().contains("Permission denied"));
            assert!(output.is_empty());
        }
    }

    #[test]
    fn uploads_require_explicit_sync_okay_and_preserve_failure_reason() {
        let okay = record(b"OKAY", b"");
        let failure = record(b"FAIL", b"Permission denied");
        for width in 1..=okay.len() {
            session(&okay, width).recv_sync_status().unwrap();
        }
        for width in 1..=failure.len() {
            assert!(
                session(&failure, width)
                    .recv_sync_status()
                    .unwrap_err()
                    .to_string()
                    .contains("Permission denied")
            );
        }
        for bytes in [
            record(b"DONE", b""),
            b"OKA".to_vec(),
            [okay, vec![1]].concat(),
        ] {
            assert!(session(&bytes, bytes.len()).recv_sync_status().is_err());
        }
    }

    #[test]
    fn incomplete_unknown_oversized_and_wrong_stream_replies_fail() {
        for bytes in [
            b"DAT".to_vec(),
            record(b"DATA", b"abc"),
            b"DATA\x04\0\0\0abc".to_vec(),
            record(b"WHAT", b""),
            b"DATA\x01\0\x01\0".to_vec(),
            [record(b"DONE", b""), vec![1]].concat(),
        ] {
            assert!(
                session(&bytes, bytes.len())
                    .recv_file(&mut Vec::new())
                    .is_err()
            );
        }
        for (command, remote, local) in [
            (MessageCommand::Clse, 9, 7),
            (MessageCommand::Write, 10, 7),
            (MessageCommand::Write, 9, 8),
        ] {
            let message =
                ADBTransportMessage::try_new(command, remote, local, &record(b"DONE", b""))
                    .unwrap();
            let transport = Transport(Arc::new(Mutex::new(VecDeque::from([message]))));
            assert!(
                ADBSession::new(transport, 7, 9)
                    .recv_file(&mut Vec::new())
                    .is_err()
            );
        }
    }
}
