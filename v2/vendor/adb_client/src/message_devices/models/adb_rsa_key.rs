use crate::{Result, RustADBError};
use base64::{Engine, engine::general_purpose::STANDARD};
use num_bigint::{BigUint, ModInverse};
use num_traits::FromPrimitive;
use num_traits::cast::ToPrimitive;
use rsa::pkcs8::DecodePrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{Pkcs1v15Sign, RsaPrivateKey};
use std::fmt::Write;
use std::fs::read_to_string;
use std::path::Path;

const ADB_PRIVATE_KEY_SIZE: usize = 2048;
const ANDROID_PUBKEY_MODULUS_SIZE_WORDS: u32 = 64;

#[repr(C)]
#[derive(Debug, Default)]
/// Internal ADB representation of a public key
struct ADBRsaInternalPublicKey {
    pub modulus_size_words: u32,
    pub n0inv: u32,
    pub modulus: BigUint,
    pub rr: Vec<u8>,
    pub exponent: u32,
}

impl ADBRsaInternalPublicKey {
    pub fn new(exponent: &BigUint, modulus: &BigUint) -> Result<Self> {
        Ok(Self {
            modulus_size_words: ANDROID_PUBKEY_MODULUS_SIZE_WORDS,
            exponent: exponent.to_u32().ok_or(RustADBError::ConversionError)?,
            modulus: modulus.clone(),
            ..Default::default()
        })
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.append(&mut self.modulus_size_words.to_le_bytes().to_vec());
        bytes.append(&mut self.n0inv.to_le_bytes().to_vec());
        bytes.append(&mut self.modulus.to_bytes_le());
        bytes.append(&mut self.rr);
        bytes.append(&mut self.exponent.to_le_bytes().to_vec());

        bytes
    }
}

#[derive(Debug, Clone)]
pub struct ADBRsaKey {
    private_key: RsaPrivateKey,
}

impl ADBRsaKey {
    pub fn new_random() -> Result<Self> {
        Ok(Self {
            private_key: RsaPrivateKey::new(&mut rsa::rand_core::OsRng, ADB_PRIVATE_KEY_SIZE)?,
        })
    }

    pub fn new_from_pkcs8(pkcs8_content: &str) -> Result<Self> {
        Ok(Self {
            private_key: RsaPrivateKey::from_pkcs8_pem(pkcs8_content)?,
        })
    }

    pub fn android_pubkey_encode(&self) -> Result<String> {
        // Helped from project: https://github.com/hajifkd/webadb
        // Source code: https://android.googlesource.com/platform/system/core/+/refs/heads/main/libcrypto_utils/android_pubkey.cpp
        // Useful function `android_pubkey_encode()`
        let mut adb_rsa_pubkey =
            ADBRsaInternalPublicKey::new(self.private_key.e(), self.private_key.n())?;

        // r32 = 2 ^ 32
        let r32 = BigUint::from_u64(1 << 32).ok_or(RustADBError::ConversionError)?;

        // r = 2 ^ rsa_size = 2 ^ 2048
        let r = set_bit(ADB_PRIVATE_KEY_SIZE)?;

        // rr = r ^ 2 mod N
        let rr = r.modpow(&BigUint::from(2u32), &adb_rsa_pubkey.modulus);
        adb_rsa_pubkey.rr = rr.to_bytes_le();

        // rem = N[0]
        let rem = &adb_rsa_pubkey.modulus % &r32;

        // n0inv = -1 / rem mod r32
        let n0inv = rem
            .mod_inverse(&r32)
            .and_then(|v| v.to_biguint())
            .ok_or(RustADBError::ConversionError)?;

        // BN_sub(n0inv, r32, n0inv)
        adb_rsa_pubkey.n0inv = (r32 - n0inv)
            .to_u32()
            .ok_or(RustADBError::ConversionError)?;

        Self::encode_public_key(adb_rsa_pubkey.into_bytes())
    }

    fn encode_public_key(pub_key: Vec<u8>) -> Result<String> {
        let mut encoded = STANDARD.encode(pub_key);
        encoded.push(' ');
        write!(encoded, "adb_client@{}", env!("CARGO_PKG_VERSION"))
            .map_err(|_| RustADBError::ConversionError)?;

        Ok(encoded)
    }

    pub fn sign(&self, msg: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        Ok(self
            .private_key
            .sign(Pkcs1v15Sign::new::<sha1::Sha1>(), msg.as_ref())?)
    }
}

pub fn read_adb_private_key<P: AsRef<Path>>(private_key_path: P) -> Result<Option<ADBRsaKey>> {
    // Try to read the private key file from given path
    // If the file is not found, return None
    // If there is another error while reading the file, return this error
    // Else, return the private key content
    let pk = match read_to_string(private_key_path.as_ref()) {
        Ok(pk) => pk,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    match ADBRsaKey::new_from_pkcs8(&pk) {
        Ok(pk) => Ok(Some(pk)),
        Err(e) => Err(e),
    }
}

fn set_bit(n: usize) -> Result<BigUint> {
    BigUint::parse_bytes(
        &{
            let mut bits = vec![b'1'];
            bits.append(&mut vec![b'0'; n]);
            bits
        }[..],
        2,
    )
    .ok_or(RustADBError::ConversionError)
}

#[test]
fn test_pubkey_gen() {
    const DEFAULT_PRIV_KEY: &str = r"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4Dyn85cxDJnjM
uYXQl/w469MDKdlGdviLfmFMWeYLVfL2Mz1AVyvKqscrtlhbbgMQ/M+3lDvEdHS0
14RIGAwWRtrlTTmhLvM2/IO+eSKSYeCrCVc4KLG3E3WRryUXbs2ynA29xjTJVw+Z
xYxDyn/tAYPEyMm4v+HIJHcOtRzxtO2vjMJ2vBT/ywYxjhncXbFSO09q2E4XrHli
SIPyO82hZgCkpzTZRp+nyA17TYuV9++mvUr9lWH9RbC+o8EF3yitlBsE2uXr97EV
i2Qy8CE7FIxsihXlukppwKRuz+1rJrvmZPTn49ZS+sIS99WE9GoCpsyQvTpvehrM
SIDRsVZPAgMBAAECggEAWNXAzzXeS36zCSR1yILCknqHotw86Pyc4z7BGUe+dzQp
itiaNIaeNTgN3zQoGyDSzA0o+BLMcfo/JdVrHBy3IL1cAxYtvXTaoGxp7bGrlPk2
pXZhqVJCy/jRYtokzdWF5DHbk/+pFJA3kGE/XKzM54g2n/DFI61A/QdUiz2w1ZtI
vc5cM08EM8B/TSI3SeWB8zkh5SlIuLsFO2J2+tCak6PdFfKOVIrFv9dKJYLxx+59
+edZamw2EvNlnl/sewgUk0gaZvQKVf4ivHyM+KSHuV4RFfiLvGuVcyA6XhSjztsG
EA++jDHP5ib/Izes7UK09v9y7kow+z6vUtnDDQOvgQKBgQD8WWAn7FQt9aziCw19
gZynzHG1bXI7uuEVSneuA3UwJImmDu8W+Qb9YL9Dc2nV0M5pGGdXKi2jzq8gPar6
GPAmy7TOlov6Nm0pbMXTAfuovG+gIXxelp3US3FvyRupi0/7UQRRwvetFYbDFwJX
ydF5uEtZdGSHAjPeU5FLq6tBwQKBgQC6uN0JwwZn+eaxguyKOXvp0KykhFI0HI1A
MBDZ1uuKt6OW5+r9NeQtTLctGlNKVQ8wz+Wr0C/nLGIIv4lySS9WFyc5/FnFhDdy
LsEi6whcca4vq3jsMOukvQGFnERsou4LqBEI1Es7jjeeEq+/8WnNTi6Y1flZ6UAp
YAOeFI98DwKBgQDvyfHgHeajwZalOQF5qGb24AOQ9c4dyefGNnvhA/IgbCfMftZc
iwhETuGQM6R3A7KQFRtlrXOu+2BYD6Ffg8D37IwD3vRmL7+tJGoapwC/B0g+7nLi
4tZY+9Nv+LbrdbDry8GB+/UkKJdk3IFicCk4M5KOD1bTH5mwAtLHB/p1QQKBgDHi
k8M45GxA+p4wMUvYgb987bLiWyfq/N3KOaZJYhJkb4MwoLpXfIeRuFqHbvsr8GwF
DwIxE6s6U1KtAWaUIN5qPyOhxMYdRcbusNDIZCp2gKfhsuO/SiVwDYkJr8oqWVip
5SsrtJHLtBY6PdQVBkRAf/h7KiwYQfkL2suQCKmHAoGBAJAkYImBYPHuRcnSXikn
xGDK/moPvzs0CjdPlRcEN+Myy/G0FUrOaC0FcpNoJOdQSYz3F6URA4nX+zj6Ie7G
CNkECiepaGyquQaffwR1CAi8dH6biJjlTQWQPFcCLA0hvernWo3eaSfiL7fHyym+
ile69MHFENUePSpuRSiF3Z02
-----END PRIVATE KEY-----";
    let priv_key =
        ADBRsaKey::new_from_pkcs8(DEFAULT_PRIV_KEY).expect("cannot create rsa key from data");
    let pub_key = priv_key
        .android_pubkey_encode()
        .expect("cannot encode public key");
    let pub_key_adb = "\
QAAAAFH/pU9PVrHRgEjMGnpvOr2QzKYCavSE1fcSwvpS1uPn9GTmuyZr7c9up\
MBpSrrlFYpsjBQ7IfAyZIsVsffr5doEG5StKN8FwaO+sEX9YZX9Sr2m7/eVi0\
17Dcinn0bZNKekAGahzTvyg0hieawXTthqTztSsV3cGY4xBsv/FLx2woyv7bT\
xHLUOdyTI4b+4ycjEgwHtf8pDjMWZD1fJNMa9DZyyzW4XJa+RdRO3sSg4Vwmr\
4GGSInm+g/w28y6hOU3l2kYWDBhIhNe0dHTEO5S3z/wQA25bWLYrx6rKK1dAP\
TP28lUL5llMYX6L+HZG2SkD0+s4/JfQhbnMeCZDzOX8KQ+4ThLy/gDTqCSTjj\
ic8BykdUIqYPwAjBMgQwLOLY5WNJMpjGlFINRcCGhvFFZ73sJTLerECuV/Oae\
nFRcORwnGIRgMrYXj4tjmxJC7sq3cfNX96YIcSCDE9SZFdlKXVK8Jc4YMLGF3\
MI8k1KoTby34uaIyxPJvwM1WR4Rdj60fwMyikFXNaOR2fPteZ3UMBA7CMrOEm\
9iYjntyEppA4hQXIO1TWTzkA/Kfl1i67k5NuLIQdhPFEc5ox5IYVHusauPQ7d\
Awu6BlgK37TUn0JdK0Z6Z4RaEIaNiEI0d5CoP6zQKV2QQnlscYpdsaUW5t9/F\
LioVXPwrz0tx35JyIUZPPYwEAAQA= ";
    assert_eq!(&pub_key[..pub_key_adb.len()], pub_key_adb);
}
