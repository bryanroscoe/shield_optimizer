use std::io::Write;

use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::ADBMessageTransport,
        adb_transport_message::ADBTransportMessage,
        message_commands::{MessageCommand, MessageSubcommand},
        utils::BinaryEncodable,
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn pull<A: AsRef<str>, W: Write>(&mut self, source: A, output: W) -> Result<()> {
        let mut session = self.open_synchronization_session()?;
        let source = source.as_ref();

        let adb_stat_response = session.stat_with_explicit_ids(source)?;

        if adb_stat_response.file_perm == 0 {
            return Err(RustADBError::UnknownResponseType(
                "mode is 0: source file does not exist".to_string(),
            ));
        }

        self.get_transport_mut().write_message_with_timeout(
            ADBTransportMessage::try_new(
                MessageCommand::Okay,
                session.local_id(),
                session.remote_id(),
                &[],
            )?,
            std::time::Duration::from_secs(4),
        )?;

        let recv_buffer = MessageSubcommand::Recv.with_arg(u32::try_from(source.len())?);
        session.send_and_expect_okay(ADBTransportMessage::try_new(
            MessageCommand::Write,
            session.local_id(),
            session.remote_id(),
            &recv_buffer.encode(),
        )?)?;
        session.send_and_expect_okay(ADBTransportMessage::try_new(
            MessageCommand::Write,
            session.local_id(),
            session.remote_id(),
            source.as_bytes(),
        )?)?;

        session.recv_file(output)?;
        self.end_transaction(&mut session)?;
        Ok(())
    }
}
