//! Google platform-tools' own ADB key, which the built-in engine offers
//! before the app's key, so a TV that already allows platform-tools connects
//! without a new prompt. It is only ever read: never created, written,
//! copied, rotated or re-permissioned, and its contents are never logged.
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

use atv_direct_adb::PreferredKeys;

/// An `adbkey` is under 2 KiB; anything this large is not one.
const MAX_KEY_BYTES: u64 = 32 * 1024;

/// Where Google's adb keeps its user key, found the way adb finds it
/// (`adb_get_homedir_path` and `adb_get_android_dir_path` in `adb_utils.cpp`,
/// `get_user_key_path` in `client/auth.cpp`): `<home>/.android/adbkey`, where
/// home is `$HOME` on macOS and Linux, taken as is even when empty, or the
/// user database's home folder when `HOME` is unset, and the user profile
/// folder on Windows. adb reads no other variable for it. `None` when there is
/// no home folder at all.
pub fn platform_tools_key_path() -> Option<PathBuf> {
    key_path(|name| std::env::var_os(name), dirs::home_dir)
}

/// [`platform_tools_key_path`] with the environment and the OS home lookup
/// passed in, so tests never touch the real home folder.
fn key_path(
    var: impl Fn(&str) -> Option<OsString>,
    os_home: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // adb asks the shell for the profile folder (CSIDL_PROFILE) and never
        // reads HOME or USERPROFILE; `dirs` asks for the same folder.
        let _ = var;
        os_home().map(|profile| profile.join(".android").join("adbkey"))
    }
    #[cfg(not(windows))]
    {
        let mut path = match var("HOME") {
            Some(home) => home,
            None => os_home()?.into_os_string(),
        };
        path.push("/.android/adbkey");
        Some(PathBuf::from(path))
    }
}

/// Read the platform-tools key at `path` for one engine start. Missing,
/// unreadable, oversized or unusable means no preferred key, with a debug log
/// that names the path and the reason only. This never fails the engine.
pub fn load_platform_tools_key(path: Option<&Path>) -> PreferredKeys {
    let Some(path) = path else {
        tracing::debug!(
            "no home folder, so the built-in engine has no platform-tools key to offer"
        );
        return PreferredKeys::none();
    };
    match read_key(path) {
        Ok(keys) => {
            tracing::debug!(path = %path.display(), "the built-in engine offers the platform-tools key first");
            keys
        }
        Err(reason) => {
            tracing::debug!(path = %path.display(), %reason, "the built-in engine does not use the platform-tools key");
            PreferredKeys::none()
        }
    }
}

fn read_key(path: &Path) -> Result<PreferredKeys, String> {
    let file = open_read_only(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => "there is no key there".to_string(),
        _ => format!("it could not be opened: {error}"),
    })?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("it could not be inspected: {error}"))?;
    if !metadata.is_file() {
        return Err("it is not a regular file".into());
    }
    if metadata.len() > MAX_KEY_BYTES {
        return Err("it is larger than 32 KiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_KEY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("it could not be read: {error}"))?;
    let keys = atv_direct_adb::load_external_identity_pem(&bytes);
    bytes.fill(0);
    keys
}

#[cfg(unix)]
fn open_read_only(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    // Non-blocking, so a FIFO at that path cannot hold up engine start; a
    // regular file ignores the flag. Symlinks are followed, as adb does.
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_read_only(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey};
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};

    /// One RSA 2048 key per test binary, PKCS#1. Never the user's adbkey.
    fn pkcs1_pem() -> &'static str {
        static PEM: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        PEM.get_or_init(|| {
            rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
                .unwrap()
                .to_pkcs1_pem(LineEnding::LF)
                .unwrap()
                .to_string()
        })
    }

    fn pkcs8_pem() -> String {
        rsa::RsaPrivateKey::from_pkcs1_pem(pkcs1_pem())
            .unwrap()
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .to_string()
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), OsString::from(value)))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.clone())
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn the_key_is_found_where_adb_looks_for_it() {
        let never = || -> Option<PathBuf> { panic!("HOME is set, so adb does not ask the OS") };
        assert_eq!(
            key_path(env(&[("HOME", "/fixture/home")]), never),
            Some(PathBuf::from("/fixture/home/.android/adbkey"))
        );
        // adb ignores these entirely.
        assert_eq!(
            key_path(
                env(&[
                    ("HOME", "/fixture/home"),
                    ("ANDROID_USER_HOME", "/fixture/user-home"),
                    ("ANDROID_SDK_HOME", "/fixture/sdk-home"),
                    ("ANDROID_HOME", "/fixture/sdk"),
                ]),
                never
            ),
            Some(PathBuf::from("/fixture/home/.android/adbkey"))
        );
        // An empty HOME is used as is, as adb does.
        assert_eq!(
            key_path(env(&[("HOME", "")]), never),
            Some(PathBuf::from("/.android/adbkey"))
        );
        // Without HOME adb asks the user database.
        assert_eq!(
            key_path(env(&[]), || Some(PathBuf::from("/fixture/passwd-home"))),
            Some(PathBuf::from("/fixture/passwd-home/.android/adbkey"))
        );
        assert_eq!(key_path(env(&[]), || None), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_key_is_found_where_adb_looks_for_it() {
        assert_eq!(
            key_path(
                env(&[
                    ("HOME", r"C:\fixture\home"),
                    ("USERPROFILE", r"C:\fixture\other")
                ]),
                || Some(PathBuf::from(r"C:\fixture\profile"))
            ),
            Some(PathBuf::from(r"C:\fixture\profile\.android\adbkey"))
        );
        assert_eq!(key_path(env(&[]), || None), None);
    }

    /// Load `bytes` from a fresh `.android/adbkey` and prove it was only read.
    fn load_bytes(bytes: &[u8]) -> PreferredKeys {
        let dir = tempfile::tempdir().unwrap();
        let android = dir.path().join(".android");
        std::fs::create_dir(&android).unwrap();
        let path = android.join("adbkey");
        std::fs::write(&path, bytes).unwrap();
        let before = std::fs::metadata(&path).unwrap();
        let keys = load_platform_tools_key(Some(&path));
        let after = std::fs::metadata(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        assert_eq!(before.permissions(), after.permissions());
        let names: Vec<_> = std::fs::read_dir(&android)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            vec![OsString::from("adbkey")],
            "nothing else is created"
        );
        keys
    }

    #[test]
    fn a_platform_tools_key_loads_from_pkcs8_or_pkcs1() {
        assert_eq!(load_bytes(pkcs8_pem().as_bytes()).len(), 1);
        assert_eq!(load_bytes(pkcs1_pem().as_bytes()).len(), 1);
    }

    #[test]
    fn an_unusable_or_missing_key_is_skipped_without_failing() {
        let small = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024)
            .unwrap()
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap();
        let mut oversized = pkcs8_pem().into_bytes();
        oversized.resize(32 * 1024 + 1, b'\n');
        let cases: [&[u8]; 6] = [
            small.as_bytes(),
            oversized.as_slice(),
            b"-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n",
            b"not a key",
            b"",
            &[0xff, 0xfe, 0xfd],
        ];
        for bytes in cases {
            assert!(load_bytes(bytes).is_empty());
        }

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join(".android").join("adbkey");
        assert!(load_platform_tools_key(Some(&missing)).is_empty());
        assert!(
            !dir.path().join(".android").exists(),
            "adb's folder is never created"
        );
        assert!(load_platform_tools_key(None).is_empty());

        // A folder where the key should be.
        std::fs::create_dir_all(&missing).unwrap();
        assert!(load_platform_tools_key(Some(&missing)).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_at_the_key_path_does_not_hold_up_engine_start() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("adbkey");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success());
        let started = std::time::Instant::now();
        assert!(load_platform_tools_key(Some(&fifo)).is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
