use std::io::{ErrorKind, Read, Write};

use crate::models::ADBLocalCommand;
use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
        adb_transport_message::ADBTransportMessage, commands::utils::ShellMessageWriter,
        message_commands::MessageCommand,
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Runs 'command' in a shell on the device, and write its output and error streams into output.
    pub(crate) fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        mut stdout: Option<&mut dyn Write>,
        _stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>> {
        let mut session = self.open_session(&ADBLocalCommand::ShellCommand(
            command.as_ref().to_string(),
            Vec::new(),
        ))?;

        loop {
            let message = session.recv_and_reply_okay()?;
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
