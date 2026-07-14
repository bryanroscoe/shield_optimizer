use std::{iter::Map, slice::ChunksExact};

use byteorder::{ByteOrder, LittleEndian};

use crate::{Result, RustADBError};

type U32ChunkIter<'a> = Map<ChunksExact<'a, u8>, fn(&[u8]) -> Result<u32>>;

fn read_next(chunks: &mut U32ChunkIter) -> Result<u32> {
    chunks
        .next()
        .ok_or(RustADBError::FramebufferConversionError)?
}

#[derive(Debug)]
pub struct FrameBufferInfoV1 {
    pub _bpp: u32,
    pub size: u32,
    pub width: u32,
    pub height: u32,
    pub _red_offset: u32,
    pub _red_length: u32,
    pub _blue_offset: u32,
    pub _blue_length: u32,
    pub _green_offset: u32,
    pub _green_length: u32,
    pub _alpha_offset: u32,
    pub _alpha_length: u32,
}

impl TryFrom<[u8; std::mem::size_of::<Self>()]> for FrameBufferInfoV1 {
    type Error = RustADBError;

    fn try_from(
        value: [u8; std::mem::size_of::<Self>()],
    ) -> std::result::Result<Self, Self::Error> {
        let mut chunks: U32ChunkIter = value.chunks_exact(4).map(|v| Ok(LittleEndian::read_u32(v)));

        Ok(Self {
            _bpp: read_next(&mut chunks)?,
            size: read_next(&mut chunks)?,
            width: read_next(&mut chunks)?,
            height: read_next(&mut chunks)?,
            _red_offset: read_next(&mut chunks)?,
            _red_length: read_next(&mut chunks)?,
            _blue_offset: read_next(&mut chunks)?,
            _blue_length: read_next(&mut chunks)?,
            _green_offset: read_next(&mut chunks)?,
            _green_length: read_next(&mut chunks)?,
            _alpha_offset: read_next(&mut chunks)?,
            _alpha_length: read_next(&mut chunks)?,
        })
    }
}

#[derive(Debug)]
pub struct FrameBufferInfoV2 {
    pub _bpp: u32,
    pub _color_space: u32,
    pub size: u32,
    pub width: u32,
    pub height: u32,
    pub _red_offset: u32,
    pub _red_length: u32,
    pub _blue_offset: u32,
    pub _blue_length: u32,
    pub _green_offset: u32,
    pub _green_length: u32,
    pub _alpha_offset: u32,
    pub _alpha_length: u32,
}

impl TryFrom<[u8; std::mem::size_of::<Self>()]> for FrameBufferInfoV2 {
    type Error = RustADBError;

    fn try_from(
        value: [u8; std::mem::size_of::<Self>()],
    ) -> std::result::Result<Self, Self::Error> {
        let mut chunks: U32ChunkIter = value.chunks_exact(4).map(|v| Ok(LittleEndian::read_u32(v)));

        Ok(Self {
            _bpp: read_next(&mut chunks)?,
            _color_space: read_next(&mut chunks)?,
            size: read_next(&mut chunks)?,
            width: read_next(&mut chunks)?,
            height: read_next(&mut chunks)?,
            _red_offset: read_next(&mut chunks)?,
            _red_length: read_next(&mut chunks)?,
            _blue_offset: read_next(&mut chunks)?,
            _blue_length: read_next(&mut chunks)?,
            _green_offset: read_next(&mut chunks)?,
            _green_length: read_next(&mut chunks)?,
            _alpha_offset: read_next(&mut chunks)?,
            _alpha_length: read_next(&mut chunks)?,
        })
    }
}
