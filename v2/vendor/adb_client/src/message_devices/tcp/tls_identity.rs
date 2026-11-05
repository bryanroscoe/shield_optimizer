// Derived in part from Kadb (Apache-2.0): cert/CertUtils.kt.

use std::{fs::read_to_string, path::Path};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
    PKCS_RSA_SHA256, SerialNumber,
};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, pem::PemObject};

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

#[cfg(test)]
mod tests {
    use super::client_identity;
    use crate::message_devices::tcp::test_support::shared_test_key;
    use rcgen::{KeyPair, PKCS_RSA_SHA256, PublicKeyData};

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
}
