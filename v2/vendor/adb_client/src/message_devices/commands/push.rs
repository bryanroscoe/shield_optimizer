use std::io::Read;

use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::ADBMessageTransport,
        adb_transport_message::ADBTransportMessage,
        message_commands::{MessageCommand, MessageSubcommand},
        utils::BinaryEncodable,
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn push<R: Read, A: AsRef<str>>(&mut self, stream: R, path: A) -> Result<()> {
        let mut session = self.open_synchronization_session()?;

        let path_header = format!("{},0777", path.as_ref());

        let send_buffer = MessageSubcommand::Send.with_arg(u32::try_from(path_header.len())?);
        let mut send_buffer = send_buffer.encode();
        send_buffer.append(&mut path_header.as_bytes().to_vec());

        session.send_and_expect_okay(ADBTransportMessage::try_new(
            MessageCommand::Write,
            session.local_id(),
            session.remote_id(),
            &send_buffer,
        )?)?;

        session.push_file(stream)?;
        self.end_transaction(&mut session)?;

        Ok(())
    }
}
