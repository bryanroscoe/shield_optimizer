//! ADB identities held only in memory that a connection offers before its
//! key file, so a device that trusts any one of them needs no new approval.

use std::{fmt, sync::Arc};

use rsa::{
    RsaPrivateKey,
    pkcs1::DecodeRsaPrivateKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
    traits::PublicKeyParts,
};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

use super::tls_identity::{TlsIdentity, client_identity_from_der};
use crate::{Result, RustADBError, message_devices::models::ADBRsaKey};

const PKCS8_HEADER: &str = "-----BEGIN PRIVATE KEY-----";
const PKCS1_HEADER: &str = "-----BEGIN RSA PRIVATE KEY-----";
const KEY_BITS: usize = 2048;

/// Identities a connection offers before its key file, in this order.
///
/// Legacy AUTH signs the device's first token with the first key, the next
/// token with the next key and the key file's after all of these, as AOSP's
/// host signs with each of its keys in turn. When every signature was
/// refused, the first key's public key is the one offered for approval. Over
/// TLS the certificate is the key the device names in its CA hints, else the
/// first key, and a refused certificate is tried again once with each other
/// key. Pairing registers the first key.
///
/// With none, every connection behaves exactly as with the key file alone.
/// Nothing here is ever written to disk, and `Debug` shows only the count.
#[derive(Clone, Default)]
pub struct PreferredKeys(Arc<Vec<PreferredKey>>);

pub(crate) struct PreferredKey {
    pub(crate) auth: ADBRsaKey,
    pub(crate) public_key: String,
    pub(crate) chain: Vec<CertificateDer<'static>>,
    pub(crate) pkcs8: PrivatePkcs8KeyDer<'static>,
    pub(crate) tls: TlsIdentity,
}

impl PreferredKeys {
    /// No extra identities.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Parse identities from PEM text, in the order given. Each must be an
    /// unencrypted RSA 2048 key, PKCS#8 (`BEGIN PRIVATE KEY`, what Google's
    /// adb writes to `adbkey`) or PKCS#1 (`BEGIN RSA PRIVATE KEY`). The first
    /// that is unusable fails the whole set with
    /// [`RustADBError::UnusableKey`], whose text never contains key material.
    pub fn from_pems<'a, I: IntoIterator<Item = &'a str>>(pems: I) -> Result<Self> {
        let keys = pems
            .into_iter()
            .map(PreferredKey::from_pem)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self(Arc::new(keys)))
    }

    /// How many identities there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn keys(&self) -> &[PreferredKey] {
        &self.0
    }
}

impl fmt::Debug for PreferredKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreferredKeys")
            .field("count", &self.len())
            .finish()
    }
}

fn unusable(reason: &str) -> RustADBError {
    RustADBError::UnusableKey(reason.to_string())
}

impl PreferredKey {
    fn from_pem(pem: &str) -> Result<Self> {
        let header = pem.trim_start();
        let private_key = if header.starts_with(PKCS8_HEADER) {
            RsaPrivateKey::from_pkcs8_pem(pem).map_err(|_| unusable("not an RSA PKCS#8 key"))?
        } else if header.starts_with(PKCS1_HEADER) {
            RsaPrivateKey::from_pkcs1_pem(pem).map_err(|_| unusable("not an RSA PKCS#1 key"))?
        } else {
            return Err(unusable(
                "not an unencrypted PKCS#8 or PKCS#1 PEM private key",
            ));
        };
        if private_key.n().bits() != KEY_BITS {
            return Err(unusable("not a 2048-bit RSA key"));
        }
        private_key
            .validate()
            .map_err(|_| unusable("the RSA key is inconsistent"))?;
        let pkcs8 = private_key
            .to_pkcs8_der()
            .map_err(|_| unusable("the RSA key cannot be encoded"))?;
        let pkcs8 = PrivatePkcs8KeyDer::from(pkcs8.as_bytes().to_vec());
        let (chain, pkcs8) = client_identity_from_der(&pkcs8)
            .map_err(|_| unusable("no TLS certificate can be made from the key"))?;
        let tls = TlsIdentity::new(chain.clone(), pkcs8.clone_key())
            .map_err(|_| unusable("no TLS certificate can be made from the key"))?;
        let auth = ADBRsaKey::from_private_key(private_key);
        let public_key = auth
            .android_pubkey_encode()
            .map_err(|_| unusable("the public key cannot be encoded for adbd"))?;
        Ok(Self {
            auth,
            public_key,
            chain,
            pkcs8,
            tls,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::PreferredKeys;
    use crate::RustADBError;
    use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
    use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey};

    /// The fixed key from `adb_rsa_key.rs`, distinct from `shared_test_key`.
    pub(crate) fn second_test_pem() -> &'static str {
        let source = include_str!("../models/adb_rsa_key.rs");
        let begin = source.find("-----BEGIN PRIVATE KEY-----").unwrap();
        let end = source[begin..].find("-----END PRIVATE KEY-----").unwrap()
            + begin
            + "-----END PRIVATE KEY-----".len();
        &source[begin..end]
    }

    #[test]
    fn pkcs8_and_pkcs1_keys_load_and_debug_shows_only_the_count() {
        let pkcs8 = second_test_pem();
        let rsa = rsa::RsaPrivateKey::from_pkcs8_pem(pkcs8).unwrap();
        let pkcs1 = rsa.to_pkcs1_pem(LineEnding::LF).unwrap();
        assert!(pkcs1.starts_with("-----BEGIN RSA PRIVATE KEY-----"));
        let keys = PreferredKeys::from_pems([pkcs8, pkcs1.as_str()]).unwrap();
        assert_eq!(keys.len(), 2);
        // Both encodings are the same key.
        assert_eq!(keys.keys()[0].public_key, keys.keys()[1].public_key);
        assert_eq!(
            keys.keys()[0].tls.fingerprint,
            keys.keys()[1].tls.fingerprint
        );
        assert_eq!(format!("{keys:?}"), "PreferredKeys { count: 2 }");
        assert!(PreferredKeys::none().is_empty());
    }

    #[test]
    fn unusable_keys_are_refused_without_echoing_them() {
        let small = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).unwrap();
        let small = small.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let truncated = &second_test_pem()[..200];
        let encrypted =
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n";
        let ec = "-----BEGIN EC PRIVATE KEY-----\nAAAA\n-----END EC PRIVATE KEY-----\n";
        for pem in [small.as_str(), truncated, encrypted, ec, "", "garbage"] {
            let error = PreferredKeys::from_pems([pem]).unwrap_err();
            assert!(matches!(error, RustADBError::UnusableKey(_)), "{error:?}");
            let text = error.to_string();
            assert!(!text.contains("MII") && !text.contains("AAAA"), "{text}");
        }
    }
}
