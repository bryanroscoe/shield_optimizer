use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};
use image::{ImageBuffer, Rgba};

use crate::{
    Result, RustADBError,
    models::{ADBCommand, ADBLocalCommand, FrameBufferInfoV1, FrameBufferInfoV2},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Inner method requesting framebuffer from Android device
    pub(crate) fn framebuffer_inner(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        self.set_serial_transport()?;

        self.transport
            .send_adb_request(&ADBCommand::Local(ADBLocalCommand::FrameBuffer))?;

        let version = self
            .transport
            .get_raw_connection()?
            .read_u32::<LittleEndian>()?;

        match version {
            // RGBA_8888
            1 => {
                let mut buf = [0u8; std::mem::size_of::<FrameBufferInfoV1>()];

                self.transport.get_raw_connection()?.read_exact(&mut buf)?;

                let framebuffer_info: FrameBufferInfoV1 = buf.try_into()?;

                let mut data = vec![
                    0_u8;
                    framebuffer_info
                        .size
                        .try_into()
                        .map_err(|_| RustADBError::ConversionError)?
                ];
                self.transport.get_raw_connection()?.read_exact(&mut data)?;

                Ok(ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(
                    framebuffer_info.width,
                    framebuffer_info.height,
                    data,
                )
                .ok_or_else(|| RustADBError::FramebufferConversionError)?)
            }
            // RGBX_8888
            2 => {
                let mut buf = [0u8; std::mem::size_of::<FrameBufferInfoV2>()];

                self.transport.get_raw_connection()?.read_exact(&mut buf)?;

                let framebuffer_info: FrameBufferInfoV2 = buf.try_into()?;

                let mut data = vec![
                    0_u8;
                    framebuffer_info
                        .size
                        .try_into()
                        .map_err(|_| RustADBError::ConversionError)?
                ];
                self.transport.get_raw_connection()?.read_exact(&mut data)?;

                Ok(ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(
                    framebuffer_info.width,
                    framebuffer_info.height,
                    data,
                )
                .ok_or_else(|| RustADBError::FramebufferConversionError)?)
            }
            v => Err(RustADBError::UnimplementedFramebufferImageVersion(v)),
        }
    }
}
