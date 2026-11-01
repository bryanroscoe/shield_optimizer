//! Private, no-clobber storage for the desktop application's own ADB identity.
use std::path::{Path, PathBuf};

pub fn ensure_app_identity(app_data: &Path) -> Result<PathBuf, String> {
    ensure_with(
        app_data,
        shield_direct_adb::validate_identity_pem,
        shield_direct_adb::generate_identity_pem,
        std::io::Write::write_all,
    )
}

pub fn validate_existing_identity(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        unix::validate_path(path, shield_direct_adb::validate_identity_pem)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("Secure native ADB identity storage is not yet supported on this platform.".into())
    }
}

fn ensure_with<V, G, W>(
    app_data: &Path,
    validate: V,
    generate: G,
    write: W,
) -> Result<PathBuf, String>
where
    V: Fn(&[u8]) -> Result<(), String>,
    G: FnOnce() -> Result<Vec<u8>, String>,
    W: FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
{
    #[cfg(unix)]
    {
        unix::ensure(app_data, validate, generate, write)
    }
    #[cfg(not(unix))]
    {
        let _ = (app_data, validate, generate, write);
        Err("Secure native ADB identity storage is not yet supported on this platform.".into())
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        ffi::CString,
        fs::{File, OpenOptions},
        io::{Read, Seek, SeekFrom},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        },
        sync::atomic::{AtomicU64, Ordering},
    };
    const MAX_KEY_BYTES: u64 = 32768;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn error(action: &str, error: std::io::Error) -> String {
        format!("Native ADB identity {action} failed: {error}")
    }
    fn name(value: &str) -> CString {
        CString::new(value).expect("fixed identity filename has no NUL")
    }
    fn uid() -> u32 {
        // geteuid has no preconditions and performs no identity/configuration change.
        unsafe { libc::geteuid() }
    }
    fn secure_metadata(
        metadata: &std::fs::Metadata,
        directory: bool,
        private: bool,
    ) -> Result<(), String> {
        let kind = if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        };
        let mode = metadata.mode() & 0o7777;
        let permissions = if private {
            mode == if directory { 0o700 } else { 0o600 }
        } else {
            mode & 0o022 == 0
        };
        if !kind || metadata.uid() != uid() || !permissions {
            return Err("Native ADB identity path must be owned by this user, regular, and privately permissioned; existing files were not changed.".into());
        }
        Ok(())
    }
    fn open_at(dir: &File, file: &CString, flags: i32, mode: u32) -> std::io::Result<File> {
        // dir is an owned open directory; file is a single NUL-terminated component.
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                file.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                mode as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // Successful openat transferred ownership of this descriptor to us.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    fn read_file<V: Fn(&[u8]) -> Result<(), String>>(
        mut file: File,
        validate: &V,
    ) -> Result<std::fs::Metadata, String> {
        let metadata = file.metadata().map_err(|e| error("metadata", e))?;
        secure_metadata(&metadata, false, true)?;
        if metadata.len() > MAX_KEY_BYTES {
            return Err("Native ADB identity file is too large; it was not changed.".into());
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(MAX_KEY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| error("read", e))?;
        if bytes.len() as u64 > MAX_KEY_BYTES {
            return Err(
                "Native ADB identity file grew beyond its limit; it was not changed.".into(),
            );
        }
        validate(&bytes).map_err(|_| {
            "Native ADB identity is not a valid 2048-bit RSA PKCS#8 PEM; it was not changed."
                .to_owned()
        })?;
        Ok(metadata)
    }
    pub(super) fn validate_path<V: Fn(&[u8]) -> Result<(), String>>(
        path: &Path,
        validate: V,
    ) -> Result<(), String> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(|e| error("open", e))?;
        let metadata = read_file(file, &validate)?;
        let current = std::fs::symlink_metadata(path).map_err(|e| error("path verification", e))?;
        if !current.is_file() || !same_file(&metadata, &current) {
            return Err(
                "Native ADB identity path changed while being read; nothing was replaced.".into(),
            );
        }
        Ok(())
    }
    struct Staging {
        dir: File,
        name: CString,
    }
    impl Drop for Staging {
        fn drop(&mut self) {
            // Only our create_new staging entry inside the held private directory is removed.
            unsafe {
                libc::unlinkat(self.dir.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
    fn existing<V: Fn(&[u8]) -> Result<(), String>>(
        dir: &File,
        key: &CString,
        validate: &V,
    ) -> Result<bool, String> {
        match open_at(dir, key, libc::O_RDONLY, 0) {
            Ok(file) => {
                let original = read_file(file, validate)?;
                let current = open_at(dir, key, libc::O_RDONLY, 0)
                    .map_err(|e| error("existing identity recheck", e))?;
                if !same_file(
                    &original,
                    &current
                        .metadata()
                        .map_err(|e| error("existing identity metadata", e))?,
                ) {
                    return Err(
                        "Native ADB identity changed while being read; nothing was replaced."
                            .into(),
                    );
                }
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(error("existing identity open", e)),
        }
    }
    pub(super) fn ensure<V, G, W>(
        app_data: &Path,
        validate: V,
        generate: G,
        write: W,
    ) -> Result<PathBuf, String>
    where
        V: Fn(&[u8]) -> Result<(), String>,
        G: FnOnce() -> Result<Vec<u8>, String>,
        W: FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
    {
        if !app_data.is_absolute()
            || app_data.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err("Native ADB identity requires an absolute application-data directory; no fallback path was used.".into());
        }
        match std::fs::DirBuilder::new().mode(0o700).create(app_data) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(error("application directory creation", e)),
        }
        let root = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(app_data)
            .map_err(|e| error("application directory open", e))?;
        secure_metadata(
            &root
                .metadata()
                .map_err(|e| error("application directory metadata", e))?,
            true,
            false,
        )?;
        let directory = name("native-adb");
        // mkdirat creates only our fixed child, without following a substituted parent path.
        if unsafe { libc::mkdirat(root.as_raw_fd(), directory.as_ptr(), 0o700) } != 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error("private directory creation", e));
            }
        }
        root.sync_all()
            .map_err(|e| error("application directory sync", e))?;
        let dir = open_at(&root, &directory, libc::O_RDONLY | libc::O_DIRECTORY, 0)
            .map_err(|e| error("private directory open", e))?;
        secure_metadata(
            &dir.metadata()
                .map_err(|e| error("private directory metadata", e))?,
            true,
            true,
        )?;
        let key = name("identity.pkcs8.pem");
        let path = app_data.join("native-adb/identity.pkcs8.pem");
        let check_anchor = || -> Result<(), String> {
            for (held, path) in [
                (&root, app_data.to_path_buf()),
                (&dir, app_data.join("native-adb")),
            ] {
                let current = std::fs::symlink_metadata(path)
                    .map_err(|e| error("directory path verification", e))?;
                if !current.is_dir()
                    || !same_file(
                        &held
                            .metadata()
                            .map_err(|e| error("held directory verification", e))?,
                        &current,
                    )
                {
                    return Err(
                        "Native ADB identity directory changed; no identity was replaced.".into(),
                    );
                }
            }
            let current = std::fs::symlink_metadata(&path)
                .map_err(|e| error("identity path verification", e))?;
            let held = open_at(&dir, &key, libc::O_RDONLY, 0)
                .map_err(|e| error("identity handle verification", e))?;
            if !current.is_file()
                || !same_file(
                    &held
                        .metadata()
                        .map_err(|e| error("identity handle metadata", e))?,
                    &current,
                )
            {
                return Err("Native ADB identity path changed; no identity was replaced.".into());
            }
            Ok(())
        };

        if existing(&dir, &key, &validate)? {
            validate_path(&path, &validate)?;
            check_anchor()?;
            return Ok(path);
        }
        let bytes = generate().map_err(|_| {
            "Native ADB identity generation failed; existing identities were not changed."
                .to_owned()
        })?;
        validate(&bytes).map_err(|_| {
            "Generated native ADB identity was invalid; nothing was published.".to_owned()
        })?;
        let staging_name = name(&format!(
            ".identity-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut staging = open_at(
            &dir,
            &staging_name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
        .map_err(|e| error("staging creation", e))?;
        let cleanup = Staging {
            dir: dir
                .try_clone()
                .map_err(|e| error("staging directory retain", e))?,
            name: staging_name,
        };
        secure_metadata(
            &staging
                .metadata()
                .map_err(|e| error("staging metadata", e))?,
            false,
            true,
        )?;
        write(&mut staging, &bytes).map_err(|e| error("staging write", e))?;
        if staging
            .metadata()
            .map_err(|e| error("staging length", e))?
            .len()
            != bytes.len() as u64
        {
            return Err(
                "Native ADB identity staging was incomplete; nothing was published.".into(),
            );
        }
        staging
            .seek(SeekFrom::Start(0))
            .map_err(|e| error("staging validation seek", e))?;
        read_file(
            staging
                .try_clone()
                .map_err(|e| error("staging validation handle", e))?,
            &validate,
        )?;
        staging.sync_all().map_err(|e| error("staging sync", e))?;
        // linkat is atomic and refuses to replace a concurrent winner, including symlinks.
        if unsafe {
            libc::linkat(
                dir.as_raw_fd(),
                cleanup.name.as_ptr(),
                dir.as_raw_fd(),
                key.as_ptr(),
                0,
            )
        } != 0
        {
            let e = std::io::Error::last_os_error();
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error("publication", e));
            }
        }
        dir.sync_all().map_err(|e| error("publication sync", e))?;
        drop(cleanup);
        dir.sync_all()
            .map_err(|e| error("staging cleanup sync", e))?;
        if !existing(&dir, &key, &validate)? {
            return Err(
                "Native ADB identity disappeared after publication; it was not recreated.".into(),
            );
        }
        validate_path(&path, &validate)?;
        check_anchor()?;
        Ok(path)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        os::unix::fs::{symlink, MetadataExt, PermissionsExt},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Barrier,
        },
    };
    fn fixture() -> Vec<u8> {
        let source =
            include_str!("../../../vendor/adb_client/src/message_devices/models/adb_rsa_key.rs");
        let start = source.find("-----BEGIN PRIVATE KEY-----").unwrap();
        let end = start
            + source[start..].find("-----END PRIVATE KEY-----").unwrap()
            + "-----END PRIVATE KEY-----".len();
        source.as_bytes()[start..end].to_vec()
    }
    fn create(root: &Path) -> Result<PathBuf, String> {
        ensure_with(
            root,
            shield_direct_adb::validate_identity_pem,
            || Ok(fixture()),
            |file, bytes| file.write_all(bytes),
        )
    }
    #[test]
    fn creates_private_complete_fixture_once_and_reuses_without_generation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        assert_eq!(fs::read(&path).unwrap(), fixture());
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(root.join("native-adb")).unwrap().mode() & 0o777,
            0o700
        );
        let inode = fs::metadata(&path).unwrap().ino();
        let reused = ensure_with(
            &root,
            shield_direct_adb::validate_identity_pem,
            || panic!("must never rotate existing identity"),
            |_, _| panic!("must never rewrite existing identity"),
        )
        .unwrap();
        assert_eq!(reused, path);
        assert_eq!(fs::metadata(path).unwrap().ino(), inode);
    }
    #[test]
    fn concurrent_complete_publications_reuse_one_winner_without_partial_reads() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let barrier = Arc::new(Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    ensure_with(
                        &root,
                        shield_direct_adb::validate_identity_pem,
                        || {
                            barrier.wait();
                            Ok(fixture())
                        },
                        |file, bytes| file.write_all(bytes),
                    )
                    .unwrap()
                })
            })
            .collect();
        let paths: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert!(paths.iter().all(|path| path == &paths[0]));
        assert_eq!(fs::read(&paths[0]).unwrap(), fixture());
        assert_eq!(fs::read_dir(root.join("native-adb")).unwrap().count(), 1);
    }
    #[test]
    fn corrupt_empty_wrong_format_existing_identity_is_never_replaced() {
        for bytes in [
            &b"broken"[..],
            b"",
            b"-----BEGIN RSA PRIVATE KEY-----\nnot-pkcs8\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            let path = create(&root).unwrap();
            fs::write(&path, bytes).unwrap();
            assert!(ensure_with(
                &root,
                shield_direct_adb::validate_identity_pem,
                || panic!("corrupt identity must not rotate"),
                |_, _| panic!("existing identity must not rewrite")
            )
            .is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
    #[test]
    fn refuses_symlink_directory_key_and_unsafe_permissions_without_changing_them() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(create(&root).is_err());
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(&path).unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"do not change").unwrap();
        symlink(&outside, &path).unwrap();
        assert!(create(&root).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"do not change");
        fs::remove_file(&path).unwrap();
        fs::remove_dir(root.join("native-adb")).unwrap();
        symlink(temp.path(), root.join("native-adb")).unwrap();
        assert!(create(&root).is_err());
        let alias = temp.path().join("alias");
        symlink(&root, &alias).unwrap();
        assert!(create(&alias).is_err());
    }
    #[test]
    fn partial_write_error_and_silent_short_write_never_publish() {
        for fail in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            assert!(ensure_with(
                &root,
                shield_direct_adb::validate_identity_pem,
                || Ok(fixture()),
                |file, bytes| {
                    file.write_all(&bytes[..20])?;
                    if fail {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "fixture storage failure",
                        ))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err());
            assert!(!root.join("native-adb/identity.pkcs8.pem").exists());
            assert_eq!(fs::read_dir(root.join("native-adb")).unwrap().count(), 0);
            assert_eq!(fs::read(create(&root).unwrap()).unwrap(), fixture());
        }
    }
    #[test]
    fn abandoned_crash_staging_does_not_become_identity_or_get_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        fs::remove_file(&path).unwrap();
        let abandoned = root.join("native-adb/.identity-abandoned.tmp");
        fs::write(&abandoned, b"partial old fixture").unwrap();
        assert_eq!(create(&root).unwrap(), path);
        assert_eq!(fs::read(path).unwrap(), fixture());
        assert_eq!(fs::read(abandoned).unwrap(), b"partial old fixture");
    }
    #[test]
    fn disappearing_existing_identity_returns_error_without_regeneration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        let removed = AtomicBool::new(false);
        assert!(ensure_with(
            &root,
            |bytes| {
                shield_direct_adb::validate_identity_pem(bytes)?;
                if !removed.swap(true, Ordering::Relaxed) {
                    fs::remove_file(&path).unwrap();
                }
                Ok(())
            },
            || panic!("disappeared existing identity must not regenerate"),
            |_, _| panic!("must not rewrite")
        )
        .is_err());
        assert!(!path.exists());
    }
    #[test]
    fn rejects_relative_path_world_writable_root_and_nonregular_identity() {
        assert!(create(Path::new(".")).is_err());
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(create(&root).is_err());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let path = create(&root).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(create(&root).is_err());
    }
    #[test]
    fn malformed_generated_bytes_never_publish() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        assert!(ensure_with(
            &root,
            shield_direct_adb::validate_identity_pem,
            || Ok(b"not a key".to_vec()),
            |_, _| panic!("invalid bytes must not write")
        )
        .is_err());
        assert!(!root.join("native-adb/identity.pkcs8.pem").exists());
    }
}
