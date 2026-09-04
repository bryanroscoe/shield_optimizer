use std::{
    io::{ErrorKind, Read, Write},
    time::Duration,
};

use crate::models::ADBLocalCommand;
use crate::{
    Result, RustADBError,
    message_devices::adb_message_transport::DEFAULT_READ_TIMEOUT,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
        adb_service::ADBService, adb_transport_message::ADBTransportMessage,
        commands::utils::ShellMessageWriter, message_commands::MessageCommand,
    },
};

const MAX_SHELL_PACKET: usize = 64 * 1024 * 1024;

enum ShellChannel {
    Stdout,
    Stderr,
    ExitStatus,
}

impl TryFrom<u8> for ShellChannel {
    type Error = RustADBError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Stdout),
            2 => Ok(Self::Stderr),
            3 => Ok(Self::ExitStatus),
            _ => Err(RustADBError::ADBShellV2ParseError(format!(
                "invalid shell channel {value}"
            ))),
        }
    }
}

fn decode_shell_v2(
    input: &mut dyn Read,
    mut stdout: Option<&mut dyn Write>,
    mut stderr: Option<&mut dyn Write>,
) -> Result<Option<u8>> {
    let mut exit = None;
    loop {
        let mut metadata = [0_u8; 5];
        if let Err(error) = input.read_exact(&mut metadata) {
            return match error.kind() {
                ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe => Ok(exit),
                _ => Err(RustADBError::IOError(error)),
            };
        }
        let channel = ShellChannel::try_from(metadata[0])?;
        let payload_size = u32::from_le_bytes(metadata[1..5].try_into()?) as usize;
        if payload_size > MAX_SHELL_PACKET {
            return Err(RustADBError::ADBShellV2ParseError(format!(
                "shell packet is too large: {payload_size} bytes"
            )));
        }
        if matches!(channel, ShellChannel::ExitStatus) && payload_size != 1 {
            return Err(RustADBError::ADBShellV2ParseError(format!(
                "exit status packet has size {payload_size}, expected 1"
            )));
        }

        let mut payload = vec![0_u8; payload_size];
        input.read_exact(&mut payload)?;
        match channel {
            ShellChannel::Stdout => {
                if let Some(writer) = stdout.as_mut() {
                    writer.write_all(&payload)?;
                }
            }
            ShellChannel::Stderr => {
                if let Some(writer) = stderr.as_mut() {
                    writer.write_all(&payload)?;
                } else if let Some(writer) = stdout.as_mut() {
                    writer.write_all(&payload)?;
                }
            }
            ShellChannel::ExitStatus => exit = Some(payload[0]),
        }
    }
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Runs 'command' in a shell on the device, and write its output and error streams into output.
    pub(crate) fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>> {
        self.shell_command_v1_with_timeout(command, stdout, stderr, DEFAULT_READ_TIMEOUT)
    }

    fn shell_command_v1_with_timeout(
        &mut self,
        command: &dyn AsRef<str>,
        mut stdout: Option<&mut dyn Write>,
        _stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<Option<u8>> {
        let mut session = self.open_session_with_timeout(
            &ADBLocalCommand::ShellCommand(command.as_ref().to_string(), Vec::new()),
            timeout,
        )?;

        loop {
            let message = session.recv_and_reply_okay_with_timeout(timeout)?;
            if message.header().command() == MessageCommand::Clse {
                break;
            }
            // should this just write for ::Write messages?
            if let Some(ref mut stdout) = stdout {
                stdout.write_all(&message.into_payload())?;
            }
        }

        Ok(None)
    }

    pub(crate) fn shell_command_prefer_v2_with_timeout(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<Option<u8>> {
        let service =
            ADBLocalCommand::ShellCommand(command.as_ref().to_string(), vec!["v2".to_string()])
                .to_string();
        match self.open_raw_service(&service, timeout) {
            Ok(session) => {
                let mut stream = ADBService::new(session, timeout, Duration::from_secs(2));
                decode_shell_v2(&mut stream, stdout, stderr)
            }
            Err(RustADBError::ADBRequestFailed(_)) => {
                self.shell_command_v1_with_timeout(command, stdout, stderr, timeout)
            }
            Err(error) => Err(error),
        }
    }

    /// Starts an interactive shell session on the device.
    /// Input data is read from [reader] and write to [writer].
    pub(crate) fn shell(
        &mut self,
        reader: &mut dyn Read,
        writer: Box<dyn Write + Send>,
    ) -> Result<()> {
        self.bidirectional_session(&ADBLocalCommand::Shell, reader, writer)
    }

    /// Runs `command` on the device.
    /// Input data is read from [reader] and write to [writer].
    pub(crate) fn exec(
        &mut self,
        command: &str,
        reader: &mut dyn Read,
        writer: Box<dyn Write + Send>,
    ) -> Result<()> {
        self.bidirectional_session(&ADBLocalCommand::Exec(command.to_string()), reader, writer)
    }

    /// Starts an bidirectional(interactive) session. This can be a shell or an exec session.
    fn bidirectional_session(
        &mut self,
        local_command: &ADBLocalCommand,
        mut reader: &mut dyn Read,
        mut writer: Box<dyn Write + Send>,
    ) -> Result<()> {
        let session = self.open_session(local_command)?;

        let local_id = session.local_id();
        let remote_id = session.remote_id();

        let mut transport = self.get_transport_mut().clone();

        // Reading thread, reads response from adbd
        std::thread::spawn(move || -> Result<()> {
            loop {
                let message = transport.read_message()?;

                // Acknowledge for more data
                let response =
                    ADBTransportMessage::try_new(MessageCommand::Okay, local_id, remote_id, &[])?;
                transport.write_message(response)?;

                match message.header().command() {
                    MessageCommand::Write => {
                        writer.write_all(&message.into_payload())?;
                        writer.flush()?;
                    }
                    MessageCommand::Okay => {}
                    _ => return Err(RustADBError::ADBShellNotSupported),
                }
            }
        });

        let transport = self.get_transport_mut().clone();
        let mut shell_writer = ShellMessageWriter::new(transport, local_id, remote_id);

        // Read from given reader (that could be stdin e.g), and write content to device adbd
        if let Err(e) = std::io::copy(&mut reader, &mut shell_writer) {
            match e.kind() {
                ErrorKind::BrokenPipe => return Ok(()),
                _ => return Err(RustADBError::IOError(e)),
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::decode_shell_v2;
    use std::io::Cursor;

    fn packet(channel: u8, payload: &[u8]) -> Vec<u8> {
        let mut packet = vec![channel];
        packet.extend(u32::try_from(payload.len()).unwrap().to_le_bytes());
        packet.extend(payload);
        packet
    }

    #[test]
    fn shell_v2_separates_output_and_returns_exit_status() {
        let mut bytes = packet(1, b"hello\n");
        bytes.extend(packet(2, b"warning\n"));
        bytes.extend(packet(3, &[7]));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = decode_shell_v2(
            &mut Cursor::new(bytes),
            Some(&mut stdout),
            Some(&mut stderr),
        )
        .unwrap();

        assert_eq!(stdout, b"hello\n");
        assert_eq!(stderr, b"warning\n");
        assert_eq!(status, Some(7));
    }

    #[test]
    fn shell_v2_merges_stderr_when_no_separate_writer_is_given() {
        let bytes = packet(2, b"failure");
        let mut stdout = Vec::new();
        decode_shell_v2(&mut Cursor::new(bytes), Some(&mut stdout), None).unwrap();
        assert_eq!(stdout, b"failure");
    }

    #[test]
    fn shell_v2_rejects_invalid_exit_packet() {
        let error = decode_shell_v2(&mut Cursor::new(packet(3, &[])), None, None).unwrap_err();
        assert!(error.to_string().contains("expected 1"));
    }
}
