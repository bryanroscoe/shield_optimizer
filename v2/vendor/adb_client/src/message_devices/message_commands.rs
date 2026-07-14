use byteorder::{ByteOrder, LittleEndian};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::Display;

use crate::{
    RustADBError,
    message_devices::utils::{BinaryDecodable, BinaryEncodable},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum MessageCommand {
    /// Connect to a device
    Cnxn = 0x4E58_4E43,
    /// Close connection to a device
    Clse = 0x4553_4C43,
    /// Device ask for authentication
    Auth = 0x4854_5541,
    /// Open a data connection
    Open = 0x4E45_504F,
    /// Write data to connection
    Write = 0x4554_5257,
    /// Server understood the message
    Okay = 0x5941_4B4F,
    /// Start a connection using TLS
    Stls = 0x534C_5453,
}

impl BinaryEncodable for MessageCommand {
    fn encode(&self) -> Vec<u8> {
        u32::from(*self).to_le_bytes().to_vec()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum MessageSubcommand {
    Stat = 0x5441_5453,
    Send = 0x444E_4553,
    Recv = 0x5643_4552,
    Quit = 0x5449_5551,
    Fail = 0x4C49_4146,
    Done = 0x454E_4F44,
    Data = 0x4154_4144,
    List = 0x5453_494C,
}

impl BinaryEncodable for MessageSubcommand {
    fn encode(&self) -> Vec<u8> {
        u32::from(*self).to_le_bytes().to_vec()
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct SubcommandWithArg {
    subcommand: MessageSubcommand,
    arg: u32,
}

impl MessageSubcommand {
    pub const fn with_arg(self, arg: u32) -> SubcommandWithArg {
        SubcommandWithArg {
            subcommand: self,
            arg,
        }
    }
}

impl BinaryEncodable for SubcommandWithArg {
    fn encode(&self) -> Vec<u8> {
        let sc: u32 = self.subcommand as u32;
        let mut buffer = Vec::new();
        buffer.extend(sc.to_le_bytes());
        buffer.extend(self.arg.to_le_bytes());
        buffer
    }
}

impl BinaryDecodable for SubcommandWithArg {
    fn decode(data: &[u8]) -> crate::Result<Self>
    where
        Self: Sized,
    {
        if data.len() < std::mem::size_of::<Self>() {
            return Err(RustADBError::ConversionError);
        }

        Ok(Self {
            subcommand: MessageSubcommand::try_from(LittleEndian::read_u32(&data[0..4]))
                .map_err(|_| RustADBError::ConversionError)?,
            arg: LittleEndian::read_u32(&data[4..8]),
        })
    }
}

impl Display for MessageCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cnxn => write!(f, "CNXN"),
            Self::Clse => write!(f, "CLSE"),
            Self::Auth => write!(f, "AUTH"),
            Self::Open => write!(f, "OPEN"),
            Self::Write => write!(f, "WRTE"),
            Self::Okay => write!(f, "OKAY"),
            Self::Stls => write!(f, "STLS"),
        }
    }
}
