//! Wire framing for the Android 11+ ADB pairing protocol.
//!
//! Mirrors `PairingPacketHeader` and `PeerInfo` from AOSP
//! `packages/modules/adb/pairing_connection/pairing_connection.cpp` and
//! `pairing_connection/include/adb/pairing/pairing_connection.h` (Apache-2.0).
//! Only the wire layout is reproduced here; no AOSP code is copied.

use super::PairingError;

/// Only header version 1 has ever been defined, and AOSP rejects anything else
/// (`kMinSupportedKeyHeaderVersion == kMaxSupportedKeyHeaderVersion == 1`).
pub(super) const HEADER_VERSION: u8 = 1;

/// `sizeof(PairingPacketHeader)`: `u8` version, `u8` type, `u32` payload,
/// declared `__attribute__((packed))`.
pub(super) const HEADER_LEN: usize = 6;

/// `kMaxPeerInfoSize`, and also `sizeof(PeerInfo)` — the struct is a `u8` type
/// plus a `u8 data[kMaxPeerInfoSize - 1]` and is sent whole, never truncated.
pub(super) const PEER_INFO_SIZE: usize = 8192;

/// `kMaxPayloadSize = kMaxPeerInfoSize * 2`.
pub(super) const MAX_PAYLOAD_SIZE: u32 = 8192 * 2;

/// `PeerInfoType::ADB_RSA_PUB_KEY`.
pub(super) const PEER_INFO_RSA_PUB_KEY: u8 = 0;

/// `adb::proto::PairingPacket::Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PacketType {
    Spake2Msg = 0,
    PeerInfo = 1,
}

impl PacketType {
    fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Spake2Msg),
            1 => Some(Self::PeerInfo),
            _ => None,
        }
    }
}

/// A decoded `PairingPacketHeader`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PairingPacketHeader {
    pub(super) version: u8,
    pub(super) packet_type: PacketType,
    pub(super) payload: u32,
}

impl PairingPacketHeader {
    pub(super) fn new(packet_type: PacketType, payload: u32) -> Self {
        Self {
            version: HEADER_VERSION,
            packet_type,
            payload,
        }
    }

    /// The payload length is written big-endian: AOSP applies `htonl` to the
    /// `payload` field of an otherwise host-layout packed struct.
    pub(super) fn to_bytes(self) -> [u8; HEADER_LEN] {
        let payload = self.payload.to_be_bytes();
        [
            self.version,
            self.packet_type as u8,
            payload[0],
            payload[1],
            payload[2],
            payload[3],
        ]
    }

    /// Applies the same three validations as AOSP's `ReadHeader`: supported
    /// version, known packet type, and a non-zero payload within
    /// `kMaxPayloadSize`.
    pub(super) fn parse(bytes: [u8; HEADER_LEN]) -> Result<Self, PairingError> {
        let version = bytes[0];
        if version != HEADER_VERSION {
            return Err(PairingError::Protocol(format!(
                "unsupported pairing header version {version}"
            )));
        }
        let packet_type = PacketType::from_wire(bytes[1]).ok_or_else(|| {
            PairingError::Protocol(format!("unknown pairing packet type {}", bytes[1]))
        })?;
        let payload = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        if payload == 0 || payload > MAX_PAYLOAD_SIZE {
            return Err(PairingError::Protocol(format!(
                "pairing payload size {payload} out of range"
            )));
        }
        Ok(Self {
            version,
            packet_type,
            payload,
        })
    }
}

/// Build the fixed-size `PeerInfo` struct: one type byte followed by
/// `data[kMaxPeerInfoSize - 1]`, zero-filled so `data` stays NUL-terminated.
///
/// AOSP's host client asserts the payload fits with room for that terminator
/// (`CHECK_LE(public_key.size(), sizeof(system_info.data) - 1)`).
pub(super) fn encode_peer_info(kind: u8, data: &[u8]) -> Result<Vec<u8>, PairingError> {
    if data.len() > PEER_INFO_SIZE - 2 {
        return Err(PairingError::Protocol(format!(
            "peer info payload of {} bytes does not fit",
            data.len()
        )));
    }
    let mut out = vec![0_u8; PEER_INFO_SIZE];
    out[0] = kind;
    out[1..=data.len()].copy_from_slice(data);
    Ok(out)
}

/// The peer's `PeerInfo`, with `data` cut at its NUL terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// `PeerInfoType` discriminant. A device answers `ADB_DEVICE_GUID` (1).
    pub kind: u8,
    /// `data`, truncated at the first NUL byte.
    pub data: Vec<u8>,
}

impl PeerInfo {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, PairingError> {
        if bytes.len() != PEER_INFO_SIZE {
            return Err(PairingError::Protocol(format!(
                "peer info is {} bytes, expected {PEER_INFO_SIZE}",
                bytes.len()
            )));
        }
        let data = &bytes[1..];
        let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
        Ok(Self {
            kind: bytes[0],
            data: data[..end].to_vec(),
        })
    }

    /// `data` as a UTF-8 string, lossily decoded.
    #[must_use]
    pub fn data_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HEADER_LEN, MAX_PAYLOAD_SIZE, PEER_INFO_SIZE, PacketType, PairingPacketHeader, PeerInfo,
        encode_peer_info,
    };

    #[test]
    fn header_encodes_version_type_and_big_endian_payload() {
        let header = PairingPacketHeader::new(PacketType::Spake2Msg, 32);
        assert_eq!(header.to_bytes(), [1, 0, 0, 0, 0, 32]);

        let header = PairingPacketHeader::new(PacketType::PeerInfo, 8208);
        assert_eq!(header.to_bytes(), [1, 1, 0, 0, 0x20, 0x10]);
    }

    #[test]
    fn header_roundtrips() {
        for (packet_type, payload) in [
            (PacketType::Spake2Msg, 1_u32),
            (PacketType::Spake2Msg, 32),
            (PacketType::PeerInfo, 8208),
            (PacketType::PeerInfo, MAX_PAYLOAD_SIZE),
        ] {
            let header = PairingPacketHeader::new(packet_type, payload);
            let parsed = PairingPacketHeader::parse(header.to_bytes()).unwrap();
            assert_eq!(parsed, header);
        }
    }

    #[test]
    fn header_rejects_bad_version_type_and_payload() {
        assert!(PairingPacketHeader::parse([0, 0, 0, 0, 0, 32]).is_err());
        assert!(PairingPacketHeader::parse([2, 0, 0, 0, 0, 32]).is_err());
        assert!(PairingPacketHeader::parse([1, 2, 0, 0, 0, 32]).is_err());
        assert!(PairingPacketHeader::parse([1, 0, 0, 0, 0, 0]).is_err());
        let too_big = (MAX_PAYLOAD_SIZE + 1).to_be_bytes();
        assert!(
            PairingPacketHeader::parse([1, 0, too_big[0], too_big[1], too_big[2], too_big[3]])
                .is_err()
        );
        assert_eq!(HEADER_LEN, 6);
    }

    #[test]
    fn peer_info_is_fixed_size_and_nul_padded() {
        let encoded = encode_peer_info(0, b"QAAAAB key user@host").unwrap();
        assert_eq!(encoded.len(), PEER_INFO_SIZE);
        assert_eq!(encoded[0], 0);
        assert_eq!(&encoded[1..21], b"QAAAAB key user@host");
        assert!(encoded[21..].iter().all(|b| *b == 0));

        let parsed = PeerInfo::parse(&encoded).unwrap();
        assert_eq!(parsed.kind, 0);
        assert_eq!(parsed.data, b"QAAAAB key user@host");
        assert_eq!(parsed.data_lossy(), "QAAAAB key user@host");
    }

    #[test]
    fn peer_info_rejects_oversized_and_short_buffers() {
        assert!(encode_peer_info(0, &vec![b'x'; PEER_INFO_SIZE - 2]).is_ok());
        assert!(encode_peer_info(0, &vec![b'x'; PEER_INFO_SIZE - 1]).is_err());
        assert!(PeerInfo::parse(&[0_u8; 16]).is_err());
    }
}
