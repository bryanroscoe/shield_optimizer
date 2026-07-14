use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};
use image::{ImageBuffer, Rgba};

use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
        message_commands::MessageCommand,
    },
    models::{ADBLocalCommand, FrameBufferInfoV1, FrameBufferInfoV2},
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn framebuffer_inner(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        let mut session = self.open_session(&ADBLocalCommand::FrameBuffer)?;

        let response = session.recv_and_reply_okay()?;

        let mut payload_cursor = Cursor::new(response.payload());

        let version = payload_cursor.read_u32::<LittleEndian>()?;

        let img = match version {
            // RGBA_8888
            1 => {
                let mut buf = [0u8; std::mem::size_of::<FrameBufferInfoV1>()];

                payload_cursor.read_exact(&mut buf)?;

                let framebuffer_info: FrameBufferInfoV1 = buf.try_into()?;

                let mut framebuffer_data = Vec::new();
                payload_cursor.read_to_end(&mut framebuffer_data)?;

                loop {
                    if u32::try_from(framebuffer_data.len())? == framebuffer_info.size {
                        break;
                    }

                    let response = session.recv_and_reply_okay()?;

                    framebuffer_data.extend_from_slice(&response.into_payload());

                    log::debug!(
                        "received framebuffer data. new size {}",
                        framebuffer_data.len()
                    );
                }

                ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(
                    framebuffer_info.width,
                    framebuffer_info.height,
                    framebuffer_data,
                )
                .ok_or_else(|| RustADBError::FramebufferConversionError)?
            }
            // RGBX_8888
            2 => {
                let mut buf = [0u8; std::mem::size_of::<FrameBufferInfoV2>()];

                payload_cursor.read_exact(&mut buf)?;

                let framebuffer_info: FrameBufferInfoV2 = buf.try_into()?;

                let mut framebuffer_data = Vec::new();
                payload_cursor.read_to_end(&mut framebuffer_data)?;

                loop {
                    if u32::try_from(framebuffer_data.len())? == framebuffer_info.size {
                        break;
                    }

                    let response = session.recv_and_reply_okay()?;

                    framebuffer_data.extend_from_slice(&response.into_payload());

                    log::debug!(
                        "received framebuffer data. new size {}",
                        framebuffer_data.len()
                    );
                }

                ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(
                    framebuffer_info.width,
                    framebuffer_info.height,
                    framebuffer_data,
                )
                .ok_or_else(|| RustADBError::FramebufferConversionError)?
            }
            v => return Err(RustADBError::UnimplementedFramebufferImageVersion(v)),
        };

        session
            .get_transport_mut()
            .read_message()
            .and_then(|message| message.assert_command(MessageCommand::Clse))?;

        Ok(img)
    }
}
