use byteorder::{ByteOrder, LittleEndian};

use crate::{
    BinaryDecodable, Result, RustADBError,
    message_devices::{message_commands::MessageCommand, utils::BinaryEncodable},
};

pub const AUTH_TOKEN: u32 = 1;
pub const AUTH_SIGNATURE: u32 = 2;
pub const AUTH_RSAPUBLICKEY: u32 = 3;

#[derive(Debug)]
pub struct ADBTransportMessage {
    header: ADBTransportMessageHeader,
    payload: Vec<u8>,
}

#[derive(Debug)]
#[repr(C)]
pub struct ADBTransportMessageHeader {
    command: MessageCommand, /* command identifier constant      */
    arg0: u32,               /* first argument                   */
    arg1: u32,               /* second argument                  */
    data_length: u32,        /* length of payload (0 is allowed) */
    data_crc32: u32,         /* crc32 of data payload            */
    magic: u32,              /* command ^ 0xffffffff             */
}

impl ADBTransportMessageHeader {
    pub fn try_new(command: MessageCommand, arg0: u32, arg1: u32, data: &[u8]) -> Result<Self> {
        Ok(Self {
            command,
            arg0,
            arg1,
            data_length: u32::try_from(data.len())?,
            data_crc32: Self::compute_crc32(data),
            magic: Self::compute_magic(command),
        })
    }

    pub const fn command(&self) -> MessageCommand {
        self.command
    }

    pub const fn arg0(&self) -> u32 {
        self.arg0
    }

    pub const fn arg1(&self) -> u32 {
        self.arg1
    }

    pub const fn data_length(&self) -> u32 {
        self.data_length
    }

    pub const fn data_crc32(&self) -> u32 {
        self.data_crc32
    }

    pub(crate) fn compute_crc32(data: &[u8]) -> u32 {
        data.iter().map(|&x| u32::from(x)).sum()
    }

    const fn compute_magic(command: MessageCommand) -> u32 {
        let command_u32 = command as u32;
        command_u32 ^ 0xFFFF_FFFF
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.encode()
    }
}

impl BinaryEncodable for ADBTransportMessageHeader {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.command.encode());
        bytes.extend_from_slice(&self.arg0.to_le_bytes());
        bytes.extend_from_slice(&self.arg1.to_le_bytes());
        bytes.extend_from_slice(&self.data_length.to_le_bytes());
        bytes.extend_from_slice(&self.data_crc32.to_le_bytes());
        bytes.extend_from_slice(&self.magic.to_le_bytes());
        bytes
    }
}

impl BinaryDecodable for ADBTransportMessageHeader {
    fn decode(data: &[u8]) -> Result<Self>
    where
        Self: Sized,
    {
        if data.len() != std::mem::size_of::<Self>() {
            return Err(RustADBError::ConversionError);
        }

        Ok(Self {
            command: MessageCommand::try_from(LittleEndian::read_u32(&data[0..4]))
                .map_err(|_| RustADBError::ConversionError)?,
            arg0: LittleEndian::read_u32(&data[4..8]),
            arg1: LittleEndian::read_u32(&data[8..12]),
            data_length: LittleEndian::read_u32(&data[12..16]),
            data_crc32: LittleEndian::read_u32(&data[16..20]),
            magic: LittleEndian::read_u32(&data[20..24]),
        })
    }
}

impl ADBTransportMessage {
    pub fn try_new(command: MessageCommand, arg0: u32, arg1: u32, data: &[u8]) -> Result<Self> {
        Ok(Self {
            header: ADBTransportMessageHeader::try_new(command, arg0, arg1, data)?,
            payload: data.to_vec(),
        })
    }

    pub const fn from_header_and_payload(
        header: ADBTransportMessageHeader,
        payload: Vec<u8>,
    ) -> Self {
        Self { header, payload }
    }

    pub fn check_message_integrity(&self) -> bool {
        ADBTransportMessageHeader::compute_magic(self.header.command) == self.header.magic
            && ADBTransportMessageHeader::compute_crc32(&self.payload) == self.header.data_crc32
    }

    pub fn assert_command(&self, expected_command: MessageCommand) -> Result<()> {
        let our_command = self.header().command();
        if expected_command == our_command {
            return Ok(());
        }

        Err(RustADBError::WrongResponseReceived(
            our_command.to_string(),
            expected_command.to_string(),
        ))
    }

    pub const fn header(&self) -> &ADBTransportMessageHeader {
        &self.header
    }

    pub const fn payload(&self) -> &Vec<u8> {
        &self.payload
    }

    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}

impl TryFrom<[u8; 24]> for ADBTransportMessageHeader {
    type Error = RustADBError;

    fn try_from(value: [u8; 24]) -> Result<Self> {
        Self::decode(&value)
    }
}
