// Derived in part from Kadb (Apache-2.0): Kadb.kt (shell selection) and
// shell/AdbShellStream.kt (shell v2 packets).

use std::{
    io::{ErrorKind, Read, Write},
    time::Duration,
};

use crate::models::{ADBLocalCommand, ShellExit};
use crate::{
    Result, RustADBError,
    message_devices::{
        adb_connection_info::MAX_PAYLOAD_V1,
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
        adb_service::{ADBService, adb_error},
        adb_transport_message::ADBTransportMessage,
        commands::utils::ShellMessageWriter,
        message_commands::MessageCommand,
    },
};

const MAX_SHELL_PACKET: usize = 64 * 1024 * 1024;
const SHELL_V2_FEATURE: &str = "shell_v2";
/// How long to wait for adbd to close a shell stream after its exit packet.
const CLOSE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const CLOSE_DRAIN_LIMIT: usize = 64 * 1024;

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

/// Fill `buf`. `Ok(false)` means the stream ended cleanly before its first
/// byte; ending part-way through is an error.
fn fill(input: &mut dyn Read, buf: &mut [u8]) -> Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match input.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => return Err(truncated_packet()),
            Ok(read) => filled += read,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(adb_error(error)),
        }
    }
    Ok(true)
}

fn truncated_packet() -> RustADBError {
    RustADBError::ADBShellV2ParseError("the shell stream ended inside a packet".to_string())
}

fn write_local(writer: &mut dyn Write, bytes: &[u8]) -> Result<()> {
    writer.write_all(bytes).map_err(RustADBError::LocalIo)
}

/// Decode shell v2 packets until the exit packet and return its status.
/// Output is passed on as raw bytes, so a UTF-8 character split across
/// packets or WRTEs reaches the writer intact. A stream that closes before
/// the exit packet is an error: the command's outcome is unknown.
fn decode_shell_v2(
    input: &mut dyn Read,
    mut stdout: Option<&mut dyn Write>,
    mut stderr: Option<&mut dyn Write>,
) -> Result<u8> {
    loop {
        let mut metadata = [0_u8; 5];
        if !fill(input, &mut metadata)? {
            return Err(RustADBError::ADBShellV2ParseError(
                "the shell stream closed without reporting an exit status".to_string(),
            ));
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
        if !payload.is_empty() && !fill(input, &mut payload)? {
            return Err(truncated_packet());
        }
        match channel {
            ShellChannel::Stdout => {
                if let Some(writer) = stdout.as_mut() {
                    write_local(*writer, &payload)?;
                }
            }
            ShellChannel::Stderr => {
                if let Some(writer) = stderr.as_mut() {
                    write_local(*writer, &payload)?;
                } else if let Some(writer) = stdout.as_mut() {
                    write_local(*writer, &payload)?;
                }
            }
            ShellChannel::ExitStatus => return Ok(payload[0]),
        }
    }
}

/// Copy a stream to its end (the device's CLSE).
fn copy_to_end<T: ADBMessageTransport>(
    stream: &mut ADBService<T>,
    mut output: Option<&mut dyn Write>,
) -> Result<()> {
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                if let Some(writer) = output.as_mut() {
                    write_local(*writer, &buffer[..read])?;
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(adb_error(error)),
        }
    }
}

fn run_shell_v2<T: ADBMessageTransport>(
    mut stream: ADBService<T>,
    stdout: Option<&mut dyn Write>,
    stderr: Option<&mut dyn Write>,
) -> Result<ShellExit> {
    let code = decode_shell_v2(&mut stream, stdout, stderr)?;
    // adbd closes the stream right after the exit packet. Waiting for that
    // briefly keeps its CLSE from trailing into the next request.
    stream.drain_until_closed(CLOSE_DRAIN_TIMEOUT, CLOSE_DRAIN_LIMIT);
    Ok(ShellExit::Code(code))
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Runs 'command' in a shell on the device, and write its output and error streams into output.
    pub(crate) fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>> {
        self.shell_command_with_timeout(command, stdout, stderr, DEFAULT_READ_TIMEOUT)
            .map(ShellExit::code)
    }

    /// Open `command` over shell protocol v2 when the device negotiated it,
    /// as AOSP's host does. `None` means use the legacy stream: the feature
    /// is absent, or the device refused the v2 service.
    fn open_shell_v2(&mut self, command: &str, timeout: Duration) -> Result<Option<ADBService<T>>> {
        if !self.connection_info().has_feature(SHELL_V2_FEATURE) {
            return Ok(None);
        }
        let service =
            ADBLocalCommand::ShellCommand(command.to_string(), vec!["v2".to_string()]).to_string();
        match self.open_stream_service(service.as_bytes(), timeout) {
            Ok(stream) => Ok(Some(stream)),
            Err(RustADBError::ADBRequestFailed(reason)) => {
                log::debug!("shell v2 was refused ({reason}); using the legacy shell");
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Run `command`, waiting at most `timeout` for each packet. Shell v2
    /// separates stderr and reports the exit status; the legacy shell, used
    /// only when v2 is unavailable, merges the streams and reports none.
    pub(crate) fn shell_command_with_timeout(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<ShellExit> {
        if let Some(stream) = self.open_shell_v2(command.as_ref(), timeout)? {
            return run_shell_v2(stream, stdout, stderr);
        }
        let service =
            ADBLocalCommand::ShellCommand(command.as_ref().to_string(), Vec::new()).to_string();
        // AOSP's host refuses this too: adbd builds without shell v2 cannot
        // take a longer service name, whatever payload size they negotiated.
        if service.len() > MAX_PAYLOAD_V1 {
            return Err(RustADBError::ADBRequestFailed(format!(
                "the command is too long for this device's legacy shell ({} bytes, limit {MAX_PAYLOAD_V1})",
                service.len()
            )));
        }
        let mut stream = self.open_stream_service(service.as_bytes(), timeout)?;
        copy_to_end(&mut stream, stdout)?;
        Ok(ShellExit::Unreported)
    }

    /// Run `command` for binary output, like `adb exec-out`. Shell v2 when
    /// negotiated (raw, stderr kept apart, exit status); otherwise `exec:`,
    /// which unlike the legacy `shell:` never runs under a pty that would
    /// rewrite line endings, but merges stderr and reports no exit status.
    pub(crate) fn exec_out_with_timeout(
        &mut self,
        command: &str,
        stdout: &mut dyn Write,
        stderr: Option<&mut dyn Write>,
        timeout: Duration,
    ) -> Result<ShellExit> {
        if let Some(stream) = self.open_shell_v2(command, timeout)? {
            return run_shell_v2(stream, Some(stdout), stderr);
        }
        let mut stream = self.open_service(&ADBLocalCommand::Exec(command.to_string()), timeout)?;
        copy_to_end(&mut stream, Some(stdout))?;
        Ok(ShellExit::Unreported)
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
                if message.header().arg1() != local_id {
                    continue;
                }
                match message.header().command() {
                    MessageCommand::Write => {
                        // Acknowledge for more data
                        let response = ADBTransportMessage::try_new(
                            MessageCommand::Okay,
                            local_id,
                            remote_id,
                            &[],
                        )?;
                        transport.write_message(response)?;
                        writer.write_all(&message.into_payload())?;
                        writer.flush()?;
                    }
                    MessageCommand::Okay => {}
                    // The session is over. CLSE is never answered.
                    MessageCommand::Clse => return Ok(()),
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
    use crate::RustADBError;
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
        assert_eq!(status, 7);
    }

    #[test]
    fn shell_v2_merges_stderr_when_no_separate_writer_is_given() {
        let mut bytes = packet(2, b"failure");
        bytes.extend(packet(3, &[1]));
        let mut stdout = Vec::new();
        decode_shell_v2(&mut Cursor::new(bytes), Some(&mut stdout), None).unwrap();
        assert_eq!(stdout, b"failure");
    }

    #[test]
    fn shell_v2_rejects_invalid_exit_packet() {
        let error = decode_shell_v2(&mut Cursor::new(packet(3, &[])), None, None).unwrap_err();
        assert!(error.to_string().contains("expected 1"));
    }

    #[test]
    fn a_v2_stream_without_an_exit_packet_is_an_error_not_success() {
        let mut stdout = Vec::new();
        let error = decode_shell_v2(
            &mut Cursor::new(packet(1, b"partial output")),
            Some(&mut stdout),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(error, RustADBError::ADBShellV2ParseError(ref m) if m.contains("without reporting an exit status")),
            "{error:?}"
        );
        assert_eq!(stdout, b"partial output");
    }

    #[test]
    fn a_v2_stream_cut_inside_a_packet_is_an_error() {
        let bytes = packet(1, b"hello");
        for cut in [3, 7] {
            let error =
                decode_shell_v2(&mut Cursor::new(bytes[..cut].to_vec()), None, None).unwrap_err();
            assert!(error.to_string().contains("inside a packet"), "{error}");
        }
    }

    #[test]
    fn a_utf8_character_split_across_packets_arrives_intact() {
        // "é" is C3 A9; each half travels in its own stdout packet.
        let mut bytes = packet(1, b"caf\xC3");
        bytes.extend(packet(1, b"\xA9!"));
        bytes.extend(packet(3, &[0]));
        let mut stdout = Vec::new();
        decode_shell_v2(&mut Cursor::new(bytes), Some(&mut stdout), None).unwrap();
        assert_eq!(String::from_utf8(stdout).unwrap(), "café!");
    }
}
