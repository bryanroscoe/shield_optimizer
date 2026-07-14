use crate::Result;

pub trait BinaryEncodable {
    fn encode(&self) -> Vec<u8>;
}

/// Internal trait representing binary decoding capabilities.
pub trait BinaryDecodable {
    /// Decode binary data into a struct.
    fn decode(data: &[u8]) -> Result<Self>
    where
        Self: Sized;
}

#[cfg(test)]
mod tests {
    use crate::{
        BinaryDecodable,
        message_devices::{
            message_commands::{MessageSubcommand, SubcommandWithArg},
            utils::BinaryEncodable,
        },
    };

    #[test]
    fn test_bincode_serialize_deserialize_to_vec() {
        let quit_buffer = MessageSubcommand::Quit.with_arg(42u32);

        let serialized = &quit_buffer.encode();
        let deserialized: SubcommandWithArg =
            SubcommandWithArg::decode(serialized).expect("cannot deserialize struct");

        assert_eq!(
            quit_buffer, deserialized,
            "serialized data does not match deserialized"
        );
    }

    #[test]
    fn test_bincode_serialize_data_format() {
        let quit_buffer = MessageSubcommand::Quit.with_arg(42u32);

        let serialized = &quit_buffer.encode();

        // First 4 bytes should be the command value as u32
        assert_eq!(
            u32::from_le_bytes(
                serialized[0..(u32::BITS / 8) as usize]
                    .try_into()
                    .expect("invalid slice length")
            ),
            MessageSubcommand::Quit as u32,
            "invalid serialized command value"
        );

        // Next 4 bytes should be the argument value as u32
        assert_eq!(
            serialized[(u32::BITS / 8) as usize..(u32::BITS / 8) as usize + 4],
            42u32.to_le_bytes(),
            "invalid serialized argument value"
        );

        assert_eq!(serialized.len(), 8, "invalid serialized data length");
    }
}
