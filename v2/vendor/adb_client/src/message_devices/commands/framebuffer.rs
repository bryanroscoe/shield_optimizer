use std::io::Read;

use image::{ImageBuffer, Rgba};

use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
        adb_service::ADBService,
    },
    models::{ADBLocalCommand, FrameBufferInfoV1, FrameBufferInfoV2},
};

/// Bound on a frame's declared size: 8K RGBA is about 133 MB.
const MAX_FRAMEBUFFER_BYTES: u32 = 256 * 1024 * 1024;

const STREAM: &str = "the framebuffer stream";

fn read_exact<T: ADBMessageTransport>(stream: &mut ADBService<T>, buf: &mut [u8]) -> Result<()> {
    stream
        .read_exact(buf)
        .map_err(|error| stream.read_error(error, STREAM))
}

fn frame_pixels<T: ADBMessageTransport>(stream: &mut ADBService<T>, size: u32) -> Result<Vec<u8>> {
    if size > MAX_FRAMEBUFFER_BYTES {
        return Err(RustADBError::FramebufferConversionError);
    }
    let mut pixels = vec![0_u8; size as usize];
    read_exact(stream, &mut pixels)?;
    Ok(pixels)
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn framebuffer_inner(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        let mut stream = self.open_service(&ADBLocalCommand::FrameBuffer, DEFAULT_READ_TIMEOUT)?;
        let mut version = [0_u8; 4];
        read_exact(&mut stream, &mut version)?;
        let version = u32::from_le_bytes(version);

        let (width, height, pixels) = match version {
            // RGBA_8888
            1 => {
                let mut buf = [0u8; std::mem::size_of::<FrameBufferInfoV1>()];
                read_exact(&mut stream, &mut buf)?;
                let info: FrameBufferInfoV1 = buf.try_into()?;
                (
                    info.width,
                    info.height,
                    frame_pixels(&mut stream, info.size)?,
                )
            }
            // RGBX_8888
            2 => {
                let mut buf = [0u8; std::mem::size_of::<FrameBufferInfoV2>()];
                read_exact(&mut stream, &mut buf)?;
                let info: FrameBufferInfoV2 = buf.try_into()?;
                (
                    info.width,
                    info.height,
                    frame_pixels(&mut stream, info.size)?,
                )
            }
            v => return Err(RustADBError::UnimplementedFramebufferImageVersion(v)),
        };

        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(width, height, pixels)
            .ok_or(RustADBError::FramebufferConversionError)
    }
}
