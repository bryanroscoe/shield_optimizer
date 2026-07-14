use std::{
    io::{Cursor, Read, Seek},
    time::Duration,
};

use byteorder::ReadBytesExt;

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
        self.transport.write_message(ADBTransportMessage::try_new(
            MessageCommand::Okay,
            self.local_id,
            self.remote_id,
            &[],
        )?)?;
        Ok(message)
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

    pub(crate) fn recv_file<W: std::io::Write>(
        &mut self,
        mut output: W,
    ) -> std::result::Result<(), RustADBError> {
        let mut len: Option<u64> = None;
        loop {
            let payload = self.recv_and_reply_okay()?.into_payload();
            let mut rdr = Cursor::new(&payload);
            while rdr.position() != payload.len() as u64 {
                match len.take() {
                    Some(0) | None => {
                        rdr.seek_relative(4)?;
                        len.replace(u64::from(rdr.read_u32::<byteorder::LittleEndian>()?));
                    }
                    Some(length) => {
                        let remaining_bytes = payload.len() as u64 - rdr.position();
                        if length < remaining_bytes {
                            std::io::copy(&mut rdr.by_ref().take(length), &mut output)?;
                        } else {
                            std::io::copy(&mut rdr.take(remaining_bytes), &mut output)?;
                            len.replace(length - remaining_bytes);
                            // this payload is now exhausted
                            break;
                        }
                    }
                }
            }
            if Cursor::new(&payload[(payload.len() - 8)..(payload.len() - 4)])
                .read_u32::<byteorder::LittleEndian>()?
                == MessageSubcommand::Done as u32
            {
                break;
            }
        }
        Ok(())
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

                    // Command should end with a Write => Okay
                    let received = self.transport.read_message()?;
                    match received.header().command() {
                        MessageCommand::Write => return Ok(()),
                        c => {
                            return Err(RustADBError::ADBRequestFailed(format!(
                                "Wrong command received {c}"
                            )));
                        }
                    }
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

        let response = self.transport.read_message()?;
        // Skip first 4 bytes as this is the literal "STAT".
        // Interesting part starts right after

        AdbStatResponse::decode(&response.into_payload()[4..])
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
