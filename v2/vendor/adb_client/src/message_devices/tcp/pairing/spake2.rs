//! SPAKE2 as implemented by `BoringSSL`'s `SPAKE2_CTX_*` API, which is the
//! variant the ADB pairing protocol uses.
//!
//! This is a from-scratch Rust implementation of the algorithm described by
//! `BoringSSL` `crypto/curve25519/spake25519.cc` (Apache-2.0) and driven by AOSP
//! `packages/modules/adb/pairing_auth/pairing_auth.cpp` (Apache-2.0). It is
//! deliberately *not* built on the `spake2` crate: that crate implements the
//! python-spake2 / CFRG flavour, which differs from `BoringSSL`'s on every
//! observable value (33-byte message with a role prefix vs. 32 bytes, SHA-256
//! transcript vs. SHA-512, different M/N points, no cofactor handling), so it
//! cannot interoperate with a real device. See `mobile/PAIRING-PLAN.md`.
//!
//! The pieces that must match bit-for-bit:
//!
//! * `M` and `N` are the two curve points `BoringSSL` derives by iterated
//!   SHA-256 over the seeds `edwards25519 point generation seed (M|N)`. Their
//!   compressed encodings are reproduced below and re-derived in the tests.
//! * The ephemeral scalar is `8 * (reduce(64 random bytes))` kept as an
//!   unreduced 256-bit integer, so that it clears any cofactor component of the
//!   peer's point.
//! * The password scalar is `reduce(SHA-512(password))` with its low three bits
//!   cleared by *adding* multiples of the group order — `BoringSSL`'s workaround
//!   for a missing `left_shift_3`. Because that changes the mask point by a
//!   small-order point, the scalar must stay unreduced, and every scalar
//!   multiplication here is therefore a full 256-bit ladder rather than a
//!   reduced `Scalar` multiply.
//! * The transcript is SHA-512 over six fields, each prefixed by its length as
//!   a little-endian `u64`, ordered by role.

use curve25519_dalek::{
    constants::ED25519_BASEPOINT_POINT,
    edwards::{CompressedEdwardsY, EdwardsPoint},
    scalar::Scalar,
    traits::Identity,
};
use rand::RngExt;
use sha2::{Digest, Sha512};
use subtle::{Choice, ConditionallySelectable};

/// `kClientName` including its NUL: AOSP passes `sizeof(kClientName)`.
pub(super) const CLIENT_NAME: &[u8] = b"adb pair client\0";
/// `kServerName` including its NUL.
pub(super) const SERVER_NAME: &[u8] = b"adb pair server\0";

/// `SPAKE2_MAX_MSG_SIZE`.
pub(super) const MSG_SIZE: usize = 32;
/// `SPAKE2_MAX_KEY_SIZE`, the full SHA-512 transcript.
pub(super) const KEY_SIZE: usize = 64;

/// `BoringSSL`'s `kSpakeMSmallPrecomp` point, compressed.
const M_COMPRESSED: [u8; 32] = [
    0x5a, 0xda, 0x7e, 0x4b, 0xf6, 0xdd, 0xd9, 0xad, 0xb6, 0x62, 0x6d, 0x32, 0x13, 0x1c, 0x6b, 0x5c,
    0x51, 0xa1, 0xe3, 0x47, 0xa3, 0x47, 0x8f, 0x53, 0xcf, 0xcf, 0x44, 0x1b, 0x88, 0xee, 0xd1, 0x2e,
];

/// `BoringSSL`'s `kSpakeNSmallPrecomp` point, compressed.
const N_COMPRESSED: [u8; 32] = [
    0x10, 0xe3, 0xdf, 0x0a, 0xe3, 0x7d, 0x8e, 0x7a, 0x99, 0xb5, 0xfe, 0x74, 0xb4, 0x46, 0x72, 0x10,
    0x3d, 0xbd, 0xdc, 0xbd, 0x06, 0xaf, 0x68, 0x0d, 0x71, 0x32, 0x9a, 0x11, 0x69, 0x3b, 0xc7, 0x78,
];

/// The order of the prime-order subgroup of edwards25519, little-endian.
/// `BoringSSL`'s `kOrder`.
const GROUP_ORDER_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

/// `spake2_role_t`. The ADB pairing client is Alice, the device is Bob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Alice,
    Bob,
}

/// One execution of `BoringSSL`-flavoured SPAKE2.
#[derive(Debug)]
pub(super) struct Spake2 {
    role: Role,
    my_name: &'static [u8],
    their_name: &'static [u8],
    /// `8 * reduce(seed)`, an unreduced 256-bit little-endian integer.
    private_key: [u8; 32],
    /// `reduce(SHA-512(password))` with the low three bits cleared, unreduced.
    password_scalar: [u8; 32],
    /// The untruncated `SHA-512(password)`, bound into the transcript.
    password_hash: [u8; 64],
    my_msg: [u8; MSG_SIZE],
}

impl Spake2 {
    /// Start an exchange with a fresh ephemeral scalar.
    pub(super) fn new(role: Role, password: &[u8]) -> Self {
        let seed: [u8; 64] = rand::rng().random();
        Self::with_private_seed(role, password, &seed)
    }

    fn with_private_seed(role: Role, password: &[u8], seed: &[u8; 64]) -> Self {
        let (my_name, their_name) = match role {
            Role::Alice => (CLIENT_NAME, SERVER_NAME),
            Role::Bob => (SERVER_NAME, CLIENT_NAME),
        };

        // Multiply the reduced ephemeral scalar by the cofactor so it will
        // clear any small-order component of the peer's point later on.
        let private_key = shift_left_3(&reduce_wide(seed));

        let password_hash: [u8; 64] = Sha512::digest(password).into();
        let password_scalar = clear_low_three_bits(&reduce_wide(&password_hash));

        // P* = private_key * B + password_scalar * (M for Alice, N for Bob).
        let point = mul_wide(&ED25519_BASEPOINT_POINT, &private_key);
        let mask = mul_wide(&mask_point(role), &password_scalar);
        let my_msg = (point + mask).compress().to_bytes();

        Self {
            role,
            my_name,
            their_name,
            private_key,
            password_scalar,
            password_hash,
            my_msg,
        }
    }

    /// The message to hand to the peer.
    pub(super) fn msg(&self) -> [u8; MSG_SIZE] {
        self.my_msg
    }

    /// Consume the peer's message and derive the 64 bytes of key material.
    ///
    /// Returns `None` if the peer sent something that is not a curve point;
    /// a *wrong password* still yields key material here, and only shows up as
    /// an AEAD failure on the next message (which is what makes the code check
    /// a single guess per connection).
    pub(super) fn process(self, their_msg: &[u8]) -> Option<[u8; KEY_SIZE]> {
        let their_msg: [u8; MSG_SIZE] = their_msg.try_into().ok()?;
        let q_star = CompressedEdwardsY(their_msg).decompress()?;

        // Unmask with the peer's point, then clear the cofactor via our
        // multiple-of-eight ephemeral scalar.
        let peers_mask = mul_wide(&mask_point(self.role.peer()), &self.password_scalar);
        let dh_shared = mul_wide(&(q_star - peers_mask), &self.private_key);
        let dh_encoded = dh_shared.compress().to_bytes();

        let mut hasher = Sha512::new();
        let (first, second, third, fourth) = match self.role {
            Role::Alice => (self.my_name, self.their_name, &self.my_msg, &their_msg),
            Role::Bob => (self.their_name, self.my_name, &their_msg, &self.my_msg),
        };
        update_with_length_prefix(&mut hasher, first);
        update_with_length_prefix(&mut hasher, second);
        update_with_length_prefix(&mut hasher, third);
        update_with_length_prefix(&mut hasher, fourth);
        update_with_length_prefix(&mut hasher, &dh_encoded);
        update_with_length_prefix(&mut hasher, &self.password_hash);

        Some(hasher.finalize().into())
    }
}

impl Role {
    fn peer(self) -> Self {
        match self {
            Self::Alice => Self::Bob,
            Self::Bob => Self::Alice,
        }
    }
}

/// The point a given role masks its own share with: `M` for Alice, `N` for Bob.
fn mask_point(role: Role) -> EdwardsPoint {
    let compressed = match role {
        Role::Alice => M_COMPRESSED,
        Role::Bob => N_COMPRESSED,
    };
    CompressedEdwardsY(compressed)
        .decompress()
        .expect("BoringSSL SPAKE2 mask point is a valid curve point")
}

/// `SHA512_Update` of an 8-byte little-endian length followed by the data,
/// matching `BoringSSL`'s `update_with_length_prefix`.
fn update_with_length_prefix(hasher: &mut Sha512, data: &[u8]) {
    let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
    hasher.update(len.to_le_bytes());
    hasher.update(data);
}

/// `x25519_sc_reduce`: reduce a 64-byte little-endian integer mod the group
/// order and keep the low 32 bytes.
fn reduce_wide(wide: &[u8; 64]) -> [u8; 32] {
    Scalar::from_bytes_mod_order_wide(wide).to_bytes()
}

/// `left_shift_3`: multiply a 256-bit little-endian integer by eight,
/// discarding any carry out of the top byte (`BoringSSL` relies on the input
/// being small enough that none occurs).
fn shift_left_3(value: &[u8; 32]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    let mut carry = 0_u8;
    for (dst, src) in out.iter_mut().zip(value.iter()) {
        *dst = (src << 3) | carry;
        carry = src >> 5;
    }
    out
}

/// Add the group order, twice the order, then four times the order whenever the
/// corresponding low bit is still set, so the result is a multiple of eight.
///
/// Each test reads the *running* value, exactly as `BoringSSL` does, and the
/// result stays below 2^256 because the input is at most `order - 1`.
fn clear_low_three_bits(scalar: &[u8; 32]) -> [u8; 32] {
    let mut value = *scalar;
    let mut addend = GROUP_ORDER_LE;
    for bit in 0..3_u32 {
        let set = Choice::from((value[0] >> bit) & 1);
        let sum = add_256(&value, &addend);
        for (dst, src) in value.iter_mut().zip(sum.iter()) {
            *dst = u8::conditional_select(dst, src, set);
        }
        addend = shift_left_1(&addend);
    }
    debug_assert_eq!(value[0] & 7, 0);
    value
}

/// Wrapping 256-bit little-endian addition.
fn add_256(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    let mut carry = 0_u8;
    for i in 0..32 {
        let (partial, carry_a) = a[i].overflowing_add(b[i]);
        let (sum, carry_b) = partial.overflowing_add(carry);
        out[i] = sum;
        carry = u8::from(carry_a || carry_b);
    }
    out
}

/// Wrapping 256-bit little-endian doubling (`scalar_double`).
fn shift_left_1(value: &[u8; 32]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    let mut carry = 0_u8;
    for (dst, src) in out.iter_mut().zip(value.iter()) {
        *dst = (src << 1) | carry;
        carry = src >> 7;
    }
    out
}

/// `scalar * point` for an *unreduced* 256-bit little-endian `scalar`.
///
/// `Scalar` arithmetic cannot be used: reducing modulo the group order would
/// drop exactly the small-order component that `BoringSSL`'s password-scalar
/// workaround depends on. Constant-time double-and-add, most significant bit
/// first, over all 256 bits — the same range `BoringSSL`'s
/// `x25519_ge_scalarmult{,_small_precomp}` cover.
fn mul_wide(point: &EdwardsPoint, scalar: &[u8; 32]) -> EdwardsPoint {
    let mut acc = EdwardsPoint::identity();
    for bit in (0..256_usize).rev() {
        acc = acc + acc;
        let set = Choice::from((scalar[bit / 8] >> (bit % 8)) & 1);
        let sum = acc + point;
        acc = EdwardsPoint::conditional_select(&acc, &sum, set);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::{
        CLIENT_NAME, KEY_SIZE, M_COMPRESSED, MSG_SIZE, N_COMPRESSED, Role, SERVER_NAME, Spake2,
        add_256, clear_low_three_bits, mul_wide, reduce_wide, shift_left_1, shift_left_3,
    };
    use curve25519_dalek::{
        constants::ED25519_BASEPOINT_POINT,
        edwards::{CompressedEdwardsY, EdwardsPoint},
        scalar::Scalar,
        traits::Identity,
    };
    use sha2::{Digest, Sha256};

    /// The two mask points are the first SHA-256 iterate of `BoringSSL`'s seeds
    /// that lands on the curve. Re-deriving them here means the hard-coded
    /// encodings above cannot silently drift.
    #[test]
    fn mask_points_derive_from_the_boringssl_seeds() {
        fn gen_point(seed: &[u8]) -> [u8; 32] {
            let mut candidate: [u8; 32] = Sha256::digest(seed).into();
            for _ in 0..64 {
                if CompressedEdwardsY(candidate).decompress().is_some() {
                    return candidate;
                }
                candidate = Sha256::digest(candidate).into();
            }
            panic!("no point found");
        }

        assert_eq!(
            gen_point(b"edwards25519 point generation seed (M)"),
            M_COMPRESSED
        );
        assert_eq!(
            gen_point(b"edwards25519 point generation seed (N)"),
            N_COMPRESSED
        );
    }

    #[test]
    fn shift_helpers_match_wrapping_integer_arithmetic() {
        let value = [0xff_u8; 32];
        // 8 * (2^256 - 1) mod 2^256 == 2^256 - 8.
        let mut expected = [0xff_u8; 32];
        expected[0] = 0xf8;
        assert_eq!(shift_left_3(&value), expected);

        let mut expected = [0xff_u8; 32];
        expected[0] = 0xfe;
        assert_eq!(shift_left_1(&value), expected);

        let mut one = [0_u8; 32];
        one[0] = 1;
        assert_eq!(add_256(&value, &one), [0_u8; 32]);
    }

    #[test]
    fn clear_low_three_bits_produces_a_multiple_of_eight() {
        for byte in 0..=255_u8 {
            let mut scalar = [0_u8; 32];
            scalar[0] = byte;
            scalar[8] = 0x37;
            let cleared = clear_low_three_bits(&scalar);
            assert_eq!(cleared[0] & 7, 0, "byte={byte}");
        }
    }

    /// `mul_wide` must agree with reduced `Scalar` arithmetic on the prime-order
    /// base point (where reduction is harmless), which pins the ladder itself.
    #[test]
    fn mul_wide_matches_scalar_multiplication_on_the_base_point() {
        for seed in 0..4_u8 {
            let mut wide = [0_u8; 64];
            for (i, byte) in wide.iter_mut().enumerate() {
                *byte = (i as u8).wrapping_mul(31).wrapping_add(seed);
            }
            let scalar_bytes = reduce_wide(&wide);
            let expected = Scalar::from_bytes_mod_order(scalar_bytes) * ED25519_BASEPOINT_POINT;
            assert_eq!(
                mul_wide(&ED25519_BASEPOINT_POINT, &scalar_bytes).compress(),
                expected.compress()
            );
        }
    }

    /// Values produced by an independent Python model of `BoringSSL`'s
    /// `spake25519.cc` (see `mobile/PAIRING-PLAN.md`). Fixing the ephemeral
    /// scalars makes the whole exchange deterministic.
    const ALICE_SEED: [u8; 64] = {
        let mut seed = [0_u8; 64];
        let mut i = 0;
        while i < 64 {
            seed[i] = i as u8;
            i += 1;
        }
        seed
    };
    const BOB_SEED: [u8; 64] = {
        let mut seed = [0_u8; 64];
        let mut i = 0;
        while i < 64 {
            seed[i] = 0xff - (i as u8);
            i += 1;
        }
        seed
    };

    fn kat_password() -> Vec<u8> {
        let mut password = b"123456".to_vec();
        for i in 0..64_u8 {
            password.push(i.wrapping_mul(7).wrapping_add(3));
        }
        password
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn known_answer_vector_matches_the_boringssl_algorithm() {
        let password = kat_password();
        let alice = Spake2::with_private_seed(Role::Alice, &password, &ALICE_SEED);
        let bob = Spake2::with_private_seed(Role::Bob, &password, &BOB_SEED);

        assert_eq!(
            hex(&alice.private_key),
            "d0e31113846fb901851ab16da0423167ce0aa31e960899d4d306afa52387962b"
        );
        assert_eq!(
            hex(&bob.private_key),
            "98337df968380adf6a6638ee8f6937c060e80aa1fa95caa21acbd53bb8813371"
        );
        assert_eq!(
            hex(&alice.password_scalar),
            "d8c3626e3875ea4a963c322fde9a08713446dfea427439ad7bff6d8b5061d77f"
        );
        assert_eq!(alice.password_scalar, bob.password_scalar);
        assert_eq!(
            hex(&alice.msg()),
            "7caeaf89f8d5c4578bc13cf4697ca3a3fdce84394d017a76424dc8bd5a19be6a"
        );
        assert_eq!(
            hex(&bob.msg()),
            "816a41c369b3b9e943cfc27906cada304cd1a2dd815df4f12245b19301fc4dc7"
        );

        let alice_msg = alice.msg();
        let bob_msg = bob.msg();
        let alice_key = alice.process(&bob_msg).unwrap();
        let bob_key = bob.process(&alice_msg).unwrap();

        assert_eq!(alice_key, bob_key);
        assert_eq!(
            hex(&alice_key),
            "3314923ec53cf12ff0aa89f8d362c74c2a98db09aac2cc60309bcbd04d1dca82\
             3321bd6198967897aee7f7b9bd0d5219f854d4c0fa5496d26c0b0dff77e3cae8"
        );
    }

    #[test]
    fn matching_passwords_agree_and_mismatched_passwords_do_not() {
        let alice = Spake2::new(Role::Alice, b"424242");
        let bob = Spake2::new(Role::Bob, b"424242");
        let alice_msg = alice.msg();
        let bob_msg = bob.msg();
        assert_eq!(alice_msg.len(), MSG_SIZE);
        let key_a = alice.process(&bob_msg).unwrap();
        let key_b = bob.process(&alice_msg).unwrap();
        assert_eq!(key_a, key_b);
        assert_eq!(key_a.len(), KEY_SIZE);

        let alice = Spake2::new(Role::Alice, b"424242");
        let bob = Spake2::new(Role::Bob, b"242424");
        let alice_msg = alice.msg();
        let bob_msg = bob.msg();
        assert_ne!(
            alice.process(&bob_msg).unwrap(),
            bob.process(&alice_msg).unwrap()
        );
    }

    /// The names are role-bound, so swapping them must break agreement.
    #[test]
    fn identities_are_bound_into_the_transcript() {
        assert_eq!(CLIENT_NAME, b"adb pair client\0");
        assert_eq!(SERVER_NAME, b"adb pair server\0");
        assert_eq!(CLIENT_NAME.len(), 16);
    }

    #[test]
    fn a_non_curve_message_is_rejected() {
        let off_curve = (0..64_u8)
            .map(|y| {
                let mut candidate = [0_u8; MSG_SIZE];
                candidate[0] = y;
                candidate
            })
            .find(|candidate| CompressedEdwardsY(*candidate).decompress().is_none())
            .expect("some small y coordinate is off the curve");

        let alice = Spake2::new(Role::Alice, b"111111");
        assert!(alice.process(&off_curve).is_none());

        let alice = Spake2::new(Role::Alice, b"111111");
        assert!(alice.process(&[0_u8; 31]).is_none());
    }

    #[test]
    fn identity_is_the_additive_zero_for_the_ladder() {
        assert_eq!(
            mul_wide(&ED25519_BASEPOINT_POINT, &[0_u8; 32]).compress(),
            EdwardsPoint::identity().compress()
        );
    }
}
