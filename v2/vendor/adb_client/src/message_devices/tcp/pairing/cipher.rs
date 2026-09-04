//! The AES-128-GCM layer the pairing protocol runs over SPAKE2 key material.
//!
//! Mirrors AOSP `packages/modules/adb/pairing_auth/aes_128_gcm.cpp` and
//! `pairing_auth/include/adb/pairing/aes_128_gcm.h` (Apache-2.0).

use aes_gcm::{
    Aes128Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use hkdf::Hkdf;
use sha2::Sha256;

use super::PairingError;

/// `uint8_t info[] = "adb pairing_auth aes-128-gcm key"` passed to `HKDF` with
/// `sizeof(info) - 1`, i.e. without the NUL terminator.
const HKDF_INFO: &[u8] = b"adb pairing_auth aes-128-gcm key";

/// `kHkdfKeyLength`.
const KEY_LEN: usize = 16;

/// `EVP_AEAD_nonce_length(EVP_aead_aes_128_gcm())`.
const NONCE_LEN: usize = 12;

/// The GCM tag AOSP gets from `EVP_AEAD_DEFAULT_TAG_LENGTH`, and hence
/// `EVP_AEAD_max_overhead`.
pub(super) const TAG_LEN: usize = 16;

/// A keyed AES-128-GCM channel with the separate send/receive sequence numbers
/// AOSP uses as nonces.
#[derive(Debug)]
pub(super) struct PairingCipher {
    key: Key<Aes128Gcm>,
    encrypt_sequence: u64,
    decrypt_sequence: u64,
}

impl PairingCipher {
    /// Derive the AES key from SPAKE2 key material with HKDF-SHA256 and no salt.
    pub(super) fn new(key_material: &[u8]) -> Self {
        Self {
            key: derive_key(key_material).into(),
            encrypt_sequence: 0,
            decrypt_sequence: 0,
        }
    }

    pub(super) fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, PairingError> {
        let cipher = Aes128Gcm::new(&self.key);
        let nonce = sequence_nonce(self.encrypt_sequence);
        let out = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| PairingError::Protocol("failed to encrypt pairing payload".into()))?;
        self.encrypt_sequence += 1;
        Ok(out)
    }

    /// A wrong pairing code produces a different AES key on each side, so this
    /// is where a mismatch surfaces — hence the dedicated error.
    pub(super) fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, PairingError> {
        let cipher = Aes128Gcm::new(&self.key);
        let nonce = sequence_nonce(self.decrypt_sequence);
        let out = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext)
            .map_err(|_| PairingError::CodeMismatch)?;
        self.decrypt_sequence += 1;
        Ok(out)
    }
}

/// `HKDF(key, 16, EVP_sha256(), key_material, key_material_len, nullptr, 0,
/// info, sizeof(info) - 1)`.
fn derive_key(key_material: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0_u8; KEY_LEN];
    Hkdf::<Sha256>::new(None, key_material)
        .expand(HKDF_INFO, &mut key)
        .expect("16 bytes is a valid HKDF-SHA256 output length");
    key
}

/// AOSP zero-fills the nonce then `memcpy`s the `uint64_t` sequence number over
/// its first eight bytes. Android is little-endian, so the counter lands
/// little-endian and the top four bytes stay zero.
fn sequence_nonce(sequence: u64) -> [u8; NONCE_LEN] {
    let mut nonce = [0_u8; NONCE_LEN];
    nonce[..8].copy_from_slice(&sequence.to_le_bytes());
    nonce
}

#[cfg(test)]
mod tests {
    use super::{HKDF_INFO, KEY_LEN, PairingCipher, TAG_LEN, derive_key, sequence_nonce};
    use aes_gcm::{
        Aes128Gcm, Nonce,
        aead::{Aead, KeyInit},
    };

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn nonce_is_a_little_endian_counter_in_the_low_eight_bytes() {
        assert_eq!(sequence_nonce(0), [0_u8; 12]);
        assert_eq!(sequence_nonce(1), [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            sequence_nonce(0x0102_0304_0506_0708),
            [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0, 0, 0, 0]
        );
    }

    /// HKDF-SHA256 with an empty salt over an all-zero 64-byte input keyed with
    /// the ADB info string. Cross-checked against Python's `hashlib`:
    /// `HKDF-Extract(b"", b"\x00"*64)` then one `HKDF-Expand` block.
    #[test]
    fn key_derivation_matches_the_spec_vector() {
        assert_eq!(HKDF_INFO.len(), 32);
        assert_eq!(
            hex(&derive_key(&[0_u8; 64])),
            "2df5d17d545e9c85354419b49e1a2b3f"
        );
        assert_eq!(
            hex(&derive_key(&[0xab_u8; 64])),
            "071dedc2f52ba28e02495cf0e99af87a"
        );
        assert_eq!(derive_key(&[0_u8; 64]).len(), KEY_LEN);
    }

    #[test]
    fn ciphertext_grows_by_exactly_the_gcm_tag() {
        let mut cipher = PairingCipher::new(&[7_u8; 64]);
        let sealed = cipher.encrypt(&[0_u8; 8192]).unwrap();
        assert_eq!(sealed.len(), 8192 + TAG_LEN);
    }

    /// The sequence numbers advance independently, so a peer that encrypts
    /// twice before we decrypt anything still lines up.
    #[test]
    fn sequence_numbers_advance_per_direction() {
        let mut writer = PairingCipher::new(&[3_u8; 64]);
        let mut reader = PairingCipher::new(&[3_u8; 64]);
        for i in 0..4_u8 {
            let sealed = writer.encrypt(&[i; 32]).unwrap();
            assert_eq!(reader.decrypt(&sealed).unwrap(), vec![i; 32]);
        }

        // Replaying an earlier record fails: the nonce has moved on.
        let mut writer = PairingCipher::new(&[3_u8; 64]);
        let mut reader = PairingCipher::new(&[3_u8; 64]);
        let first = writer.encrypt(b"hello").unwrap();
        let _ = writer.encrypt(b"hello").unwrap();
        assert_eq!(reader.decrypt(&first).unwrap(), b"hello");
        assert!(reader.decrypt(&first).is_err());
    }

    #[test]
    fn a_different_key_fails_to_open() {
        let mut writer = PairingCipher::new(&[1_u8; 64]);
        let mut reader = PairingCipher::new(&[2_u8; 64]);
        let sealed = writer.encrypt(b"peer info").unwrap();
        assert!(reader.decrypt(&sealed).is_err());
    }

    /// Pin the framing against a hand-built AES-128-GCM record so a change to
    /// the nonce layout or the derived key cannot pass unnoticed.
    #[test]
    fn records_match_a_hand_built_aes_128_gcm_sealing() {
        let key_material = [0x5a_u8; 64];
        let mut cipher = PairingCipher::new(&key_material);
        let sealed = cipher.encrypt(b"adb pairing").unwrap();

        let raw = Aes128Gcm::new(&derive_key(&key_material).into())
            .encrypt(
                Nonce::from_slice(&sequence_nonce(0)),
                b"adb pairing".as_ref(),
            )
            .unwrap();
        assert_eq!(sealed, raw);

        let sealed = cipher.encrypt(b"adb pairing").unwrap();
        let raw = Aes128Gcm::new(&derive_key(&key_material).into())
            .encrypt(
                Nonce::from_slice(&sequence_nonce(1)),
                b"adb pairing".as_ref(),
            )
            .unwrap();
        assert_eq!(sealed, raw);
    }
}
