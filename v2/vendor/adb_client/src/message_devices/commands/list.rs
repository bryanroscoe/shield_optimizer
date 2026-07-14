use byteorder::ByteOrder;
use byteorder::LittleEndian;
use std::str;

use crate::Result;
use crate::message_devices::adb_message_device::ADBMessageDevice;
use crate::message_devices::adb_message_transport::ADBMessageTransport;
use crate::message_devices::adb_session::ADBSession;
use crate::message_devices::adb_transport_message::ADBTransportMessage;
use crate::message_devices::message_commands::MessageCommand;
use crate::message_devices::message_commands::MessageSubcommand;
use crate::message_devices::utils::BinaryEncodable;
use crate::models::{ADBListItem, ADBListItemType};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// List the entries in the given directory on the device.
    /// note: path uses internal file paths, so Documents is at /storage/emulated/0/Documents
    pub(crate) fn list<A: AsRef<str>>(&mut self, path: A) -> Result<Vec<ADBListItemType>> {
        let mut session = self.open_synchronization_session()?;

        let output = self.handle_list(&mut session, path);

        self.end_transaction(&mut session)?;
        output
    }

    /// Request amount of bytes from transport, potentially across payloads
    ///
    /// This automatically request a new payload by sending back "Okay" and waiting for the next payload
    /// It reads the request bytes across the existing payload, and if there is not enough bytes left,
    /// reads the rest from the next payload
    ///
    ///   Current index
    /// ┼───────────────┼   Requested
    ///                 ┌─────────────┐
    /// ┌───────────────┼───────┐     │
    /// └───────────────────────┘
    ///     Current             └─────┘
    ///     payload          Wanted in
    ///                      Next payload
    fn read_bytes_from_transport(
        session: &mut ADBSession<T>,
        requested_bytes: usize,
        current_index: &mut usize,
        payload: &mut Vec<u8>,
    ) -> Result<Vec<u8>> {
        if *current_index + requested_bytes <= payload.len() {
            // if there is enough bytes in this payload
            // Copy from existing payload
            let slice = &payload[*current_index..*current_index + requested_bytes];
            *current_index += requested_bytes;
            Ok(slice.to_vec())
        } else {
            // Read the rest of the existing payload, then continue with the next message
            let mut slice = Vec::new();
            let bytes_read_from_existing_payload = payload.len() - *current_index;
            slice.extend_from_slice(
                &payload[*current_index..*current_index + bytes_read_from_existing_payload],
            );

            let local_id = session.local_id();
            let remote_id = session.remote_id();

            // Request the next message
            session
                .get_transport_mut()
                .write_message(ADBTransportMessage::try_new(
                    MessageCommand::Okay,
                    local_id,
                    remote_id,
                    &[],
                )?)?;

            *payload = session.get_transport_mut().read_message()?.into_payload();

            let bytes_read_from_new_payload = requested_bytes - bytes_read_from_existing_payload;
            slice.extend_from_slice(&payload[..bytes_read_from_new_payload]);
            *current_index = bytes_read_from_new_payload;
            Ok(slice)
        }
    }

    fn handle_list<A: AsRef<str>>(
        &mut self,
        session: &mut ADBSession<T>,
        path: A,
    ) -> Result<Vec<ADBListItemType>> {
        // TODO: use LIS2 to support files over 2.14 GB in size.
        // SEE: https://github.com/cstyan/adbDocumentation?tab=readme-ov-file#adb-list
        {
            let mut len_buf = Vec::from([0_u8; 4]);
            LittleEndian::write_u32(&mut len_buf, u32::try_from(path.as_ref().len())?);

            let subcommand_data = MessageSubcommand::List;

            let mut serialized_message = subcommand_data.encode();

            serialized_message.append(&mut len_buf);
            let mut path_bytes: Vec<u8> = Vec::from(path.as_ref().as_bytes());
            serialized_message.append(&mut path_bytes);

            session.send_and_expect_okay(ADBTransportMessage::try_new(
                MessageCommand::Write,
                session.local_id(),
                session.remote_id(),
                &serialized_message,
            )?)?;
        }

        let mut list_items = Vec::new();

        let transport = self.get_transport_mut();
        let mut payload = transport.read_message()?.into_payload();
        let mut current_index = 0;
        loop {
            // Loop though the response for all the entries
            const STATUS_CODE_LENGTH_IN_BYTES: usize = 4;
            let status_code = Self::read_bytes_from_transport(
                session,
                STATUS_CODE_LENGTH_IN_BYTES,
                &mut current_index,
                &mut payload,
            )?;
            match str::from_utf8(&status_code)? {
                "DENT" => {
                    // Read the file mode, size, mod time and name length in one go, since all their sizes are predictable
                    const U32_SIZE_IN_BYTES: usize = 4;
                    const SIZE_OF_METADATA: usize = U32_SIZE_IN_BYTES * 4;
                    let metadata = Self::read_bytes_from_transport(
                        session,
                        SIZE_OF_METADATA,
                        &mut current_index,
                        &mut payload,
                    )?;
                    let mode = metadata[..U32_SIZE_IN_BYTES].to_vec();
                    let size = metadata[U32_SIZE_IN_BYTES..2 * U32_SIZE_IN_BYTES].to_vec();
                    let time = metadata[2 * U32_SIZE_IN_BYTES..3 * U32_SIZE_IN_BYTES].to_vec();
                    let name_len = metadata[3 * U32_SIZE_IN_BYTES..4 * U32_SIZE_IN_BYTES].to_vec();

                    let mode = LittleEndian::read_u32(&mode);
                    let size = LittleEndian::read_u32(&size);
                    let time = LittleEndian::read_u32(&time);
                    let name_len = LittleEndian::read_u32(&name_len) as usize;
                    // Read the file name, since it requires the length from the name_len
                    let name_buf = Self::read_bytes_from_transport(
                        session,
                        name_len,
                        &mut current_index,
                        &mut payload,
                    )?;
                    let name = String::from_utf8(name_buf)?;

                    // First 9 bits are the file permissions
                    let permissions = mode & 0b1_1111_1111;

                    let entry = ADBListItem {
                        name,
                        time,
                        permissions,
                        size,
                    };

                    list_items.push(ADBListItemType::from_mode_and_entry(mode, entry));
                }
                "DONE" => {
                    return Ok(list_items);
                }
                x => log::error!("Got an unknown response {x}"),
            }
        }
    }
}
