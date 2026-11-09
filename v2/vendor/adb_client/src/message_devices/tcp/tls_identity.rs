// Derived in part from Kadb (Apache-2.0): cert/CertUtils.kt.

use std::{fs::read_to_string, path::Path, sync::Arc};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
    PKCS_RSA_SHA256, PublicKeyData, SerialNumber,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject},
    sign::CertifiedKey,
};
use sha2::{Digest, Sha256};

use crate::Result;

/// The self-signed TLS client certificate and its key, derived from the ADB
/// RSA key at `private_key_path`. Pairing and the TLS connection both use
/// this, so the key the device trusts after pairing is the key the later
/// connection presents.
///
/// adbd only compares the certificate's public key with its trusted keys, but
/// the certificate otherwise follows AOSP's `GenerateX509Certificate`
/// (`C=US, O=Android, CN=Adb`, serial 1, a CA with `keyCertSign`, `cRLSign`
/// and `digitalSignature`), the shape every adbd has been tested against.
/// The validity window stays rcgen's wide default instead of AOSP's ten years
/// from now, so a phone with a wrong clock cannot produce a certificate that is
/// not yet valid.
pub(crate) fn client_identity(
    private_key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivatePkcs8KeyDer<'static>)> {
    let pem = read_to_string(private_key_path)?;
    let key_pair = KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &PKCS_RSA_SHA256)?;
    let certificate = certificate_params().self_signed(&key_pair)?;
    let private_key = PrivatePkcs8KeyDer::from_pem_slice(pem.as_bytes())?;
    Ok((vec![certificate.der().to_owned()], private_key))
}

/// [`client_identity`] for a PKCS#8 key already in memory.
pub(crate) fn client_identity_from_der(
    pkcs8: &PrivatePkcs8KeyDer<'_>,
) -> Result<(Vec<CertificateDer<'static>>, PrivatePkcs8KeyDer<'static>)> {
    let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(pkcs8, &PKCS_RSA_SHA256)?;
    let certificate = certificate_params().self_signed(&key_pair)?;
    Ok((vec![certificate.der().to_owned()], pkcs8.clone_key()))
}

/// One certificate a TLS connection can present, with the SHA-256 of its
/// `SubjectPublicKeyInfo`: the fingerprint adbd names in its CA hints.
#[derive(Clone, Debug)]
pub(crate) struct TlsIdentity {
    pub(crate) certified: Arc<CertifiedKey>,
    pub(crate) fingerprint: [u8; 32],
}

impl TlsIdentity {
    pub(crate) fn new(
        chain: Vec<CertificateDer<'static>>,
        key: PrivatePkcs8KeyDer<'static>,
    ) -> Result<Self> {
        let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(&key, &PKCS_RSA_SHA256)?;
        let fingerprint = Sha256::digest(key_pair.subject_public_key_info()).into();
        let signing_key =
            rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(key))?;
        Ok(Self {
            certified: Arc::new(CertifiedKey::new(chain, signing_key)),
            fingerprint,
        })
    }

    pub(crate) fn from_path(private_key_path: &Path) -> Result<Self> {
        let (chain, key) = client_identity(private_key_path)?;
        Self::new(chain, key)
    }
}

/// The key fingerprints adbd advertises in its `CertificateRequest`. AOSP's
/// `adbd_tls_client_ca_list` names each trusted key as a CA whose subject is
/// `O=AdbKey-0, CN=<uppercase hex SHA-256 of the key's SubjectPublicKeyInfo>`
/// (`tls/adb_ca_list.cpp`); the host's `adb_tls_set_certificate` picks its
/// key from them. Any other name is ignored, as AOSP does.
pub(crate) fn hinted_fingerprints(root_hint_subjects: &[&[u8]]) -> Vec<[u8; 32]> {
    root_hint_subjects
        .iter()
        .filter_map(|name| adb_key_fingerprint(name))
        .collect()
}

const OID_ORGANIZATION: &[u8] = &[0x55, 0x04, 0x0a];
const OID_COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
const ADB_KEY_IDENTIFIER_V0: &[u8] = b"AdbKey-0";

fn adb_key_fingerprint(name: &[u8]) -> Option<[u8; 32]> {
    // A DER `Name` is SEQUENCE OF SET OF SEQUENCE { type OID, value }. The
    // hint may or may not keep its outer SEQUENCE header.
    let mut rdns = match der(name)? {
        (0x30, content, []) => content,
        _ => name,
    };
    let mut organization = None;
    let mut common_name = None;
    while !rdns.is_empty() {
        let (tag, mut set, rest) = der(rdns)?;
        rdns = rest;
        if tag != 0x31 {
            return None;
        }
        while !set.is_empty() {
            let (tag, attribute, rest) = der(set)?;
            set = rest;
            if tag != 0x30 {
                return None;
            }
            let (oid_tag, oid, value) = der(attribute)?;
            let (_string_tag, value, []) = der(value)? else {
                return None;
            };
            if oid_tag != 0x06 {
                return None;
            }
            if oid == OID_ORGANIZATION {
                organization = Some(value);
            } else if oid == OID_COMMON_NAME {
                common_name = Some(value);
            }
        }
    }
    if organization? != ADB_KEY_IDENTIFIER_V0 {
        return None;
    }
    decode_hex_sha256(common_name?)
}

/// One DER element: its tag, its content and what follows it.
fn der(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = match first {
        0x00..=0x7f => (usize::from(first), rest),
        0x81 => {
            let (&len, rest) = rest.split_first()?;
            (usize::from(len), rest)
        }
        0x82 => {
            let (len, rest) = rest.split_at_checked(2)?;
            (usize::from(u16::from_be_bytes([len[0], len[1]])), rest)
        }
        _ => return None,
    };
    let (content, rest) = rest.split_at_checked(len)?;
    Some((tag, content, rest))
}

fn decode_hex_sha256(text: &[u8]) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut digest = [0_u8; 32];
    let (pairs, _) = text.as_chunks::<2>();
    for (byte, [high, low]) in digest.iter_mut().zip(pairs) {
        let digit = |c: u8| char::from(c).to_digit(16);
        *byte = u8::try_from(digit(*high)? * 16 + digit(*low)?).ok()?;
    }
    Some(digest)
}

fn certificate_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CountryName, "US");
    name.push(DnType::OrganizationName, "Android");
    name.push(DnType::CommonName, "Adb");
    params.distinguished_name = name;
    params.serial_number = Some(SerialNumber::from(1_u64));
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
}

/// The CA hint adbd sends for a key with `fingerprint`, DER-encoded with its
/// outer SEQUENCE as `BoringSSL` puts it on the wire.
#[cfg(test)]
pub(crate) fn adb_ca_hint(fingerprint: &[u8; 32]) -> Vec<u8> {
    use std::fmt::Write as _;
    let mut hex = String::new();
    for byte in fingerprint {
        write!(hex, "{byte:02X}").unwrap();
    }
    let mut name = DistinguishedName::new();
    name.push(DnType::OrganizationName, "AdbKey-0");
    name.push(DnType::CommonName, hex);
    let mut params = CertificateParams::default();
    params.distinguished_name = name;
    // rcgen exposes a Name's encoding only inside a certificate; take the
    // subject from a throwaway one rather than hand-writing DER here.
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let certificate = params.self_signed(&key).unwrap();
    subject_of(certificate.der()).to_vec()
}

/// The subject `Name` of a DER certificate.
#[cfg(test)]
fn subject_of(certificate: &[u8]) -> &[u8] {
    let (_, cert, _) = der(certificate).unwrap();
    let (_, tbs, _) = der(cert).unwrap();
    let mut fields = tbs;
    // version [0], serial, signature algorithm, issuer, validity, subject
    let mut skipped = 0;
    loop {
        let (tag, _, rest) = der(fields).unwrap();
        if tag == 0xa0 {
            fields = rest;
            continue;
        }
        if skipped == 4 {
            let consumed = fields.len() - rest.len();
            return &fields[..consumed];
        }
        skipped += 1;
        fields = rest;
    }
}

#[cfg(test)]
mod tests {
    use super::{TlsIdentity, adb_ca_hint, client_identity, hinted_fingerprints};
    use crate::message_devices::tcp::test_support::shared_test_key;
    use rcgen::{KeyPair, PKCS_RSA_SHA256, PublicKeyData};
    use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
    use sha2::Digest;

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn certificate_carries_the_adb_key_and_the_aosp_subject() {
        let key_path = shared_test_key();
        let (chain, _key) = client_identity(key_path).unwrap();
        assert_eq!(chain.len(), 1);
        let der = chain[0].as_ref();

        let pem = std::fs::read_to_string(key_path).unwrap();
        let key_pair = KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &PKCS_RSA_SHA256).unwrap();
        assert!(contains(der, &key_pair.subject_public_key_info()));
        for part in [&b"US"[..], b"Android", b"Adb"] {
            assert!(contains(der, part), "{}", String::from_utf8_lossy(part));
        }
        assert!(!contains(der, b"rcgen self signed cert"));
    }

    #[test]
    fn a_missing_key_is_an_error_not_a_new_identity() {
        let missing = std::env::temp_dir().join("adb_client_tls_identity_missing.pem");
        assert!(client_identity(&missing).is_err());
    }

    /// adbd hashes `i2d_RSA_PUBKEY`, the standard `SubjectPublicKeyInfo` with
    /// rsaEncryption and NULL parameters. rcgen's encoding must be that one.
    #[test]
    fn fingerprint_is_the_sha256_of_the_standard_public_key_info() {
        let pem = std::fs::read_to_string(shared_test_key()).unwrap();
        let rsa = rsa::RsaPrivateKey::from_pkcs8_pem(&pem).unwrap();
        let spki = rsa.to_public_key().to_public_key_der().unwrap();
        let identity = TlsIdentity::from_path(shared_test_key()).unwrap();
        let expected: [u8; 32] = sha2::Sha256::digest(spki.as_bytes()).into();
        assert_eq!(identity.fingerprint, expected);
    }

    #[test]
    fn adb_key_hints_are_read_and_other_names_ignored() {
        let fingerprint = [0xab_u8; 32];
        let hint = adb_ca_hint(&fingerprint);
        assert_eq!(hinted_fingerprints(&[&hint]), vec![fingerprint]);
        // Without its outer SEQUENCE header the same name still parses.
        let (_, inner, _) = super::der(&hint).unwrap();
        assert_eq!(hinted_fingerprints(&[inner]), vec![fingerprint]);

        let mut lower = hint.clone();
        let upper = b"AB".repeat(32);
        let at = lower
            .windows(upper.len())
            .position(|window| window == upper.as_slice())
            .unwrap();
        lower[at..at + upper.len()].make_ascii_lowercase();
        assert_eq!(hinted_fingerprints(&[&lower]), vec![fingerprint]);

        let mut other_version = hint.clone();
        let at = other_version
            .windows(8)
            .position(|window| window == b"AdbKey-0")
            .unwrap();
        other_version[at + 7] = b'1';
        for ignored in [
            &other_version[..],
            &[],
            &[0x30, 0x05, 0x31],
            &hint[..hint.len() - 1],
        ] {
            assert!(hinted_fingerprints(&[ignored]).is_empty(), "{ignored:?}");
        }
    }
}
