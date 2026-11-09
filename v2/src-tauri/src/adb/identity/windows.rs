use std::{
    ffi::c_void,
    fs::File,
    io::{Read, Seek, SeekFrom},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::{
        LocalFree, ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_NO_TOKEN, GENERIC_ALL,
        GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        EqualSid, GetAce, GetSecurityDescriptorControl, GetTokenInformation, IsValidAcl,
        IsValidSid, IsWellKnownSid, TokenUser, WinBuiltinAdministratorsSid, WinLocalSystemSid,
        ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE,
        OWNER_SECURITY_INFORMATION, PSID, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, FileDispositionInfo, FileRenameInfo,
        GetFileInformationByHandle, GetFileType, SetFileInformationByHandle,
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD,
        FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_TYPE_DISK, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA,
        OPEN_EXISTING, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
    },
    System::{
        SystemServices::{ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE},
        Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
    },
};

const MAX_KEY_BYTES: u64 = 32768;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn error(action: &str, error: std::io::Error) -> String {
    format!("Native ADB identity {action} failed: {error}")
}

fn last_error(action: &str) -> String {
    error(action, std::io::Error::last_os_error())
}

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // These allocations are returned by Win32 APIs that require LocalFree.
        unsafe { LocalFree(self.0) };
    }
}

struct CurrentUser(Vec<usize>);
impl CurrentUser {
    fn load() -> Result<Self, String> {
        let mut raw = null_mut();
        // Query the effective token without changing impersonation or privileges.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut raw) } == 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(ERROR_NO_TOKEN as i32) {
                return Err(error("thread token query", e));
            }
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
                return Err(last_error("process token query"));
            }
        }
        // OpenThreadToken/OpenProcessToken returned an owned, valid handle.
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut length = 0;
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length)
        };
        if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&length) {
            return Err("Native ADB identity token has an invalid size.".into());
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(last_error("user identity query"));
        }
        let user = Self(buffer);
        if unsafe { IsValidSid(user.sid()) } == 0 {
            return Err("Native ADB identity token has an invalid user SID.".into());
        }
        Ok(user)
    }

    fn sid(&self) -> PSID {
        // The aligned buffer owns the successful TOKEN_USER query and its SID.
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    fn descriptor(&self) -> Result<LocalAllocation, String> {
        let mut text = null_mut();
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
            return Err(last_error("user SID formatting"));
        }
        let _text_owner = LocalAllocation(text.cast());
        let mut length = 0;
        // A converted SID is a bounded, NUL-terminated string owned by LocalAlloc.
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        if length == 256 {
            return Err("Native ADB identity SID text exceeded its limit.".into());
        }
        let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
            .map_err(|_| "Native ADB identity SID text is invalid.".to_owned())?;
        descriptor(&format!("O:{sid}D:P(A;;FA;;;{sid})"))
    }
}

fn descriptor(sddl: &str) -> Result<LocalAllocation, String> {
    let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut raw = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut raw,
            null_mut(),
        )
    } == 0
    {
        return Err(last_error("private security descriptor creation"));
    }
    Ok(LocalAllocation(raw))
}

fn security_attributes(descriptor: &LocalAllocation) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    }
}

fn wide(path: &Path) -> Result<Vec<u16>, String> {
    let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) || value.len() > 32766 {
        return Err("Native ADB identity path is invalid or too long.".into());
    }
    value.push(0);
    Ok(value)
}

fn local_absolute(path: &Path) -> Result<(), String> {
    let mut components = path.components();
    let disk = matches!(
        components.next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    );
    if !disk
        || components.next() != Some(Component::RootDir)
        || components.any(|part| match part {
            Component::Normal(value) => {
                let value: Vec<u16> = value.encode_wide().collect();
                value.contains(&(b':' as u16))
                    || value
                        .last()
                        .is_some_and(|c| *c == b'.' as u16 || *c == b' ' as u16)
            }
            _ => true,
        })
    {
        return Err("Native ADB identity requires an absolute local-drive path without aliases or alternate streams; no fallback path was used.".into());
    }
    Ok(())
}

fn open(path: &Path, directory: bool, access: u32) -> Result<File, String> {
    let path = wide(path)?;
    // Directory handles deny delete sharing, pinning every path component while
    // publication/read verification runs. Files share with concurrent readers and
    // publishers; their held file ID is checked against the final name afterward.
    let share = if directory {
        FILE_SHARE_READ | FILE_SHARE_WRITE
    } else {
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    };
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            share,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(last_error("open"));
    }
    // CreateFileW returned an owned handle.
    let file = unsafe { File::from_raw_handle(raw) };
    metadata(&file, directory)?;
    Ok(file)
}

fn metadata(file: &File, directory: bool) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileType(file.as_raw_handle()) } != FILE_TYPE_DISK
        || unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
    {
        return Err(last_error("regular filesystem handle query"));
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
    {
        return Err("Native ADB identity path must be regular and contain no reparse points; existing files were not changed.".into());
    }
    Ok(info)
}

fn same_file(a: &BY_HANDLE_FILE_INFORMATION, b: &BY_HANDLE_FILE_INFORMATION) -> bool {
    a.dwVolumeSerialNumber == b.dwVolumeSerialNumber
        && a.nFileIndexHigh == b.nFileIndexHigh
        && a.nFileIndexLow == b.nFileIndexLow
}

fn allowed_ace(ace: &[u8]) -> Option<(u32, &[u8])> {
    let header = ace.get(..4)?;
    if u32::from(header[0]) != ACCESS_ALLOWED_ACE_TYPE {
        return None;
    }
    let declared = usize::from(u16::from_le_bytes([header[2], header[3]]));
    let ace = ace.get(..declared)?;
    let sid_start = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
    let sid_header = ace.get(sid_start..sid_start + 8)?;
    let sid_length = 8 + 4 * usize::from(sid_header[1]);
    let sid = ace.get(sid_start..sid_start + sid_length)?;
    let mask = u32::from_le_bytes(ace.get(4..8)?.try_into().ok()?);
    Some((mask, sid))
}

fn check_security(file: &File, user: &CurrentUser, private: bool) -> Result<(), String> {
    let mut owner = null_mut();
    let mut acl: *mut ACL = null_mut();
    let mut raw = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut raw,
        )
    };
    if status != 0 {
        return Err(error(
            "permissions query",
            std::io::Error::from_raw_os_error(status as i32),
        ));
    }
    let _descriptor = LocalAllocation(raw);
    let unsafe_permissions = || {
        "Native ADB identity storage must be owned by this user with a private protected ACL; existing permissions were not changed.".to_owned()
    };
    if owner.is_null()
        || acl.is_null()
        || unsafe { IsValidSid(owner) } == 0
        || unsafe { EqualSid(owner, user.sid()) } == 0
        || unsafe { IsValidAcl(acl) } == 0
    {
        return Err(unsafe_permissions());
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(raw, &mut control, &mut revision) } == 0 {
        return Err(last_error("permissions inheritance query"));
    }
    if private && control & SE_DACL_PROTECTED == 0 {
        return Err(unsafe_permissions());
    }
    let unsafe_write = GENERIC_ALL
        | GENERIC_WRITE
        | FILE_WRITE_DATA
        | FILE_APPEND_DATA
        | FILE_WRITE_EA
        | FILE_WRITE_ATTRIBUTES
        | FILE_DELETE_CHILD
        | DELETE
        | WRITE_DAC
        | WRITE_OWNER;
    for index in 0..unsafe { (*acl).AceCount } {
        let mut raw_ace = null_mut();
        if unsafe { GetAce(acl, u32::from(index), &mut raw_ace) } == 0 {
            return Err(last_error("ACL entry query"));
        }
        // IsValidAcl/GetAce establish the ACE allocation, not the length of
        // its embedded SID. Bound both before any Win32 SID function reads it.
        let acl_bytes =
            unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), usize::from((*acl).AclSize)) };
        let offset = (raw_ace as usize)
            .checked_sub(acl as usize)
            .ok_or_else(unsafe_permissions)?;
        let remaining = acl_bytes.get(offset..).ok_or_else(unsafe_permissions)?;
        let header = remaining.get(..4).ok_or_else(unsafe_permissions)?;
        let length = usize::from(u16::from_le_bytes([header[2], header[3]]));
        let ace = remaining.get(..length).ok_or_else(unsafe_permissions)?;
        if u32::from(header[1]) & INHERIT_ONLY_ACE != 0
            || u32::from(header[0]) == ACCESS_DENIED_ACE_TYPE
        {
            continue;
        }
        let (mask, sid_bytes) = allowed_ace(ace).ok_or_else(unsafe_permissions)?;
        let sid = sid_bytes.as_ptr().cast_mut().cast();
        if unsafe { IsValidSid(sid) } == 0 {
            return Err(unsafe_permissions());
        }
        let trusted = unsafe {
            EqualSid(sid, user.sid()) != 0
                || IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
        };
        if !trusted && (private && mask != 0 || mask & unsafe_write != 0) {
            return Err(unsafe_permissions());
        }
    }
    Ok(())
}

fn anchor_directories(path: &Path) -> Result<Vec<File>, String> {
    local_absolute(path)?;
    let mut paths: Vec<_> = path
        .ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    paths.reverse();
    paths
        .into_iter()
        .map(|path| open(path, true, FILE_READ_ATTRIBUTES))
        .collect()
}

fn create_directory(path: &Path, descriptor: &LocalAllocation) -> Result<(), String> {
    let name = wide(path)?;
    let attributes = security_attributes(descriptor);
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error("directory creation", e));
        }
    }
    Ok(())
}

fn read_file<V: Fn(&[u8]) -> Result<(), String>>(
    mut file: File,
    user: &CurrentUser,
    validate: &V,
) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
    let before = metadata(&file, false)?;
    check_security(&file, user, true)?;
    let length = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if length > MAX_KEY_BYTES {
        return Err("Native ADB identity file is too large; it was not changed.".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_KEY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error("read", e))?;
    if bytes.len() as u64 > MAX_KEY_BYTES {
        return Err("Native ADB identity file grew beyond its limit; it was not changed.".into());
    }
    validate(&bytes).map_err(|_| {
        "Native ADB identity is not a valid 2048-bit RSA PKCS#8 PEM; it was not changed.".to_owned()
    })?;
    Ok(before)
}

fn validate_held_path<V: Fn(&[u8]) -> Result<(), String>>(
    path: &Path,
    user: &CurrentUser,
    validate: &V,
) -> Result<(), String> {
    let file = open(path, false, GENERIC_READ | READ_CONTROL)?;
    let before = read_file(file, user, validate)?;
    let current = open(path, false, GENERIC_READ | READ_CONTROL)?;
    check_security(&current, user, true)?;
    if !same_file(&before, &metadata(&current, false)?) {
        return Err("Native ADB identity changed while being read; nothing was replaced.".into());
    }
    Ok(())
}

pub(super) fn validate_path<V: Fn(&[u8]) -> Result<(), String>>(
    path: &Path,
    validate: V,
) -> Result<(), String> {
    local_absolute(path)?;
    let parent = path
        .parent()
        .ok_or("Native ADB identity has no parent directory.")?;
    let _anchors = anchor_directories(parent)?;
    validate_held_path(path, &CurrentUser::load()?, &validate)
}

struct Staging {
    file: File,
    published: bool,
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.published {
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            // Delete only the file opened with CREATE_NEW, never a path that may
            // have been substituted. A crash can leave a private staging file.
            unsafe {
                SetFileInformationByHandle(
                    self.file.as_raw_handle(),
                    FileDispositionInfo,
                    std::ptr::from_ref(&disposition).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                );
            }
        }
    }
}

fn publish(file: &File, path: &Path) -> Result<bool, String> {
    let name = wide(path)?;
    let bytes = offset_of!(FILE_RENAME_INFO, FileName) + name.len() * size_of::<u16>();
    let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // FILE_RENAME_INFO has a flexible UTF-16 tail; the aligned zeroed allocation
    // covers that tail and ReplaceIfExists remains false for no-clobber publication.
    unsafe {
        (*info).FileNameLength = ((name.len() - 1) * size_of::<u16>()) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        if SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            bytes as u32,
        ) == 0
        {
            let e = std::io::Error::last_os_error();
            if matches!(e.raw_os_error(), Some(code) if code == ERROR_ALREADY_EXISTS as i32 || code == ERROR_FILE_EXISTS as i32)
            {
                return Ok(false);
            }
            return Err(error("no-clobber publication", e));
        }
    }
    Ok(true)
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
    local_absolute(app_data)?;
    let parent = app_data
        .parent()
        .ok_or("Native ADB identity has no application parent.")?;
    let _parents = anchor_directories(parent)?;
    let user = CurrentUser::load()?;
    let descriptor = user.descriptor()?;
    create_directory(app_data, &descriptor)?;
    let root = open(app_data, true, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    check_security(&root, &user, false)?;
    let directory = app_data.join("native-adb");
    create_directory(&directory, &descriptor)?;
    let dir = open(&directory, true, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    check_security(&dir, &user, true)?;
    let path = directory.join("identity.pkcs8.pem");
    // Only absence permits generation. Corrupt, unsafe or inaccessible files
    // fail visibly rather than being removed, rewritten or treated as missing.
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            validate_held_path(&path, &user, &validate)?;
            return Ok(path);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(error("existing identity metadata", e)),
    }
    let bytes = generate().map_err(|_| {
        "Native ADB identity generation failed; existing identities were not changed.".to_owned()
    })?;
    validate(&bytes).map_err(|_| {
        "Generated native ADB identity was invalid; nothing was published.".to_owned()
    })?;
    let staging_path = directory.join(format!(
        ".identity-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let staging_name = wide(&staging_path)?;
    let attributes = security_attributes(&descriptor);
    // The owner-only, inheritance-protected descriptor is supplied at creation;
    // there is no interval with inherited/public permissions before hardening.
    let raw = unsafe {
        CreateFileW(
            staging_name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL | DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(last_error("staging creation"));
    }
    let mut staging = Staging {
        file: unsafe { File::from_raw_handle(raw) },
        published: false,
    };
    metadata(&staging.file, false)?;
    check_security(&staging.file, &user, true)?;
    write(&mut staging.file, &bytes).map_err(|e| error("staging write", e))?;
    if staging
        .file
        .metadata()
        .map_err(|e| error("staging metadata", e))?
        .len()
        != bytes.len() as u64
    {
        return Err("Native ADB identity staging was incomplete; nothing was published.".into());
    }
    staging
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|e| error("staging seek", e))?;
    read_file(
        staging
            .file
            .try_clone()
            .map_err(|e| error("staging handle", e))?,
        &user,
        &validate,
    )?;
    staging
        .file
        .sync_all()
        .map_err(|e| error("staging sync", e))?;
    staging.published = publish(&staging.file, &path)?;
    if staging.published {
        staging
            .file
            .sync_all()
            .map_err(|e| error("publication sync", e))?;
    }
    drop(staging);
    validate_held_path(&path, &user, &validate)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        sync::{Arc, Condvar, Mutex},
    };
    use windows_sys::Win32::Security::{
        Authorization::SetNamedSecurityInfoW, GetSecurityDescriptorDacl,
        PROTECTED_DACL_SECURITY_INFORMATION,
    };

    fn fixture() -> Vec<u8> {
        let source =
            include_str!("../../../../vendor/adb_client/src/message_devices/models/adb_rsa_key.rs");
        let start = source.find("-----BEGIN PRIVATE KEY-----").unwrap();
        let end = start
            + source[start..].find("-----END PRIVATE KEY-----").unwrap()
            + "-----END PRIVATE KEY-----".len();
        source.as_bytes()[start..end].to_vec()
    }

    fn create(root: &Path) -> Result<PathBuf, String> {
        ensure(
            root,
            atv_direct_adb::validate_identity_pem,
            || Ok(fixture()),
            |file, bytes| file.write_all(bytes),
        )
    }

    fn set_acl(path: &Path, sddl: &str) {
        let descriptor = descriptor(sddl).unwrap();
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl = null_mut();
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
            },
            0
        );
        assert_ne!(present, 0);
        let mut path = wide(path).unwrap();
        assert_eq!(
            unsafe {
                SetNamedSecurityInfoW(
                    path.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    acl,
                    null_mut(),
                )
            },
            0
        );
    }

    fn allowed_ace_fixture() -> Vec<u8> {
        let mut ace = vec![0u8; 20];
        ace[0] = ACCESS_ALLOWED_ACE_TYPE as u8;
        ace[2..4].copy_from_slice(&20u16.to_le_bytes());
        ace[4..8].copy_from_slice(&GENERIC_READ.to_le_bytes());
        ace[8] = 1;
        ace[9] = 1;
        ace[15] = 5;
        ace[16..20].copy_from_slice(&18u32.to_le_bytes());
        ace
    }

    #[test]
    fn ace_sid_parser_accepts_only_the_sid_contained_in_its_declared_entry() {
        let mut ace = allowed_ace_fixture();
        let (mask, sid) = allowed_ace(&ace).unwrap();
        assert_eq!(mask, GENERIC_READ);
        assert_eq!(sid, &ace[8..20]);
        ace.extend_from_slice(&[0xa5; 32]);
        assert_eq!(allowed_ace(&ace).unwrap().1.len(), 12);
        // Bytes belonging to the next ACE must not complete an oversized SID.
        for count in [2, 15, 255] {
            ace[9] = count;
            assert!(allowed_ace(&ace).is_none());
        }
        ace[9] = 1;
        ace[2..4].copy_from_slice(&16u16.to_le_bytes());
        assert!(allowed_ace(&ace).is_none());
        ace[2..4].copy_from_slice(&100u16.to_le_bytes());
        assert!(allowed_ace(&ace).is_none());
    }

    #[test]
    fn ace_sid_parser_rejects_truncated_headers_and_wrong_entry_types() {
        let ace = allowed_ace_fixture();
        for length in 0..ace.len() {
            let mut truncated = ace[..length].to_vec();
            if length >= 4 {
                truncated[2..4].copy_from_slice(&(length as u16).to_le_bytes());
            }
            assert!(allowed_ace(&truncated).is_none(), "accepted {length} bytes");
        }
        let mut wrong_type = ace;
        wrong_type[0] = ACCESS_DENIED_ACE_TYPE as u8;
        assert!(allowed_ace(&wrong_type).is_none());
    }

    #[test]
    fn creates_private_complete_identity_once_and_reuses_without_generation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        let user = CurrentUser::load().unwrap();
        let key = open(&path, false, GENERIC_READ | READ_CONTROL).unwrap();
        let original = metadata(&key, false).unwrap();
        check_security(&key, &user, true).unwrap();
        let directory = open(
            &root.join("native-adb"),
            true,
            READ_CONTROL | FILE_READ_ATTRIBUTES,
        )
        .unwrap();
        check_security(&directory, &user, true).unwrap();
        let path_again = ensure(
            &root,
            atv_direct_adb::validate_identity_pem,
            || panic!("must never rotate existing identity"),
            |_, _| panic!("must never rewrite existing identity"),
        )
        .unwrap();
        assert_eq!(path_again, path);
        assert_eq!(fs::read(&path).unwrap(), fixture());
        assert!(same_file(
            &original,
            &metadata(&open(&path, false, GENERIC_READ).unwrap(), false).unwrap()
        ));
        validate_path(&path, atv_direct_adb::validate_identity_pem).unwrap();
    }

    #[test]
    fn concurrent_publications_reuse_one_complete_winner() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let gate = Arc::new((Mutex::new(0usize), Condvar::new()));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let root = root.clone();
                let gate = gate.clone();
                std::thread::spawn(move || {
                    ensure(
                        &root,
                        atv_direct_adb::validate_identity_pem,
                        || {
                            let (count, condition) = &*gate;
                            let mut arrived = count.lock().unwrap();
                            *arrived += 1;
                            condition.notify_all();
                            let (arrived, _) = condition
                                .wait_timeout_while(
                                    arrived,
                                    std::time::Duration::from_secs(5),
                                    |arrived| *arrived < 4,
                                )
                                .unwrap();
                            assert_eq!(*arrived, 4, "publisher did not reach the generation gate");
                            Ok(fixture())
                        },
                        |file, bytes| file.write_all(bytes),
                    )
                    .unwrap()
                })
            })
            .collect();
        let paths: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert!(paths.iter().all(|p| p == &paths[0]));
        assert_eq!(fs::read(&paths[0]).unwrap(), fixture());
        assert_eq!(fs::read_dir(root.join("native-adb")).unwrap().count(), 1);
    }

    #[test]
    fn publication_never_replaces_a_concurrent_valid_or_corrupt_winner() {
        for corrupt in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            let original = std::cell::RefCell::new(None);
            let result = ensure(
                &root,
                atv_direct_adb::validate_identity_pem,
                || Ok(fixture()),
                |file, bytes| {
                    let winner = create(&root).unwrap();
                    if corrupt {
                        fs::write(&winner, b"concurrent identity must not rotate").unwrap();
                    }
                    *original.borrow_mut() = Some(
                        metadata(&open(&winner, false, GENERIC_READ).unwrap(), false).unwrap(),
                    );
                    file.write_all(bytes)
                },
            );
            assert_eq!(result.is_err(), corrupt);
            let winner = root.join("native-adb/identity.pkcs8.pem");
            let current = metadata(&open(&winner, false, GENERIC_READ).unwrap(), false).unwrap();
            assert!(same_file(original.borrow().as_ref().unwrap(), &current));
            if corrupt {
                assert_eq!(
                    fs::read(&winner).unwrap(),
                    b"concurrent identity must not rotate"
                );
            } else {
                assert_eq!(fs::read(&winner).unwrap(), fixture());
            }
            assert_eq!(fs::read_dir(root.join("native-adb")).unwrap().count(), 1);
        }
    }

    #[test]
    fn corrupt_oversized_and_directory_keys_are_never_replaced() {
        for bytes in [
            b"".to_vec(),
            b"broken".to_vec(),
            vec![b'x'; MAX_KEY_BYTES as usize + 1],
        ] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            let path = create(&root).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(ensure(
                &root,
                atv_direct_adb::validate_identity_pem,
                || panic!("invalid existing identity must not rotate"),
                |_, _| panic!("existing identity must not rewrite"),
            )
            .is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(create(&root).is_err());
        assert!(path.is_dir());
    }

    #[test]
    fn partial_writes_never_publish_and_private_staging_is_cleaned() {
        for fail in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            assert!(ensure(
                &root,
                atv_direct_adb::validate_identity_pem,
                || Ok(fixture()),
                |file, bytes| {
                    file.write_all(&bytes[..20])?;
                    if fail {
                        Err(std::io::Error::other("fixture write failure"))
                    } else {
                        Ok(())
                    }
                },
            )
            .is_err());
            assert!(!root.join("native-adb/identity.pkcs8.pem").exists());
            assert_eq!(fs::read_dir(root.join("native-adb")).unwrap().count(), 0);
            assert_eq!(fs::read(create(&root).unwrap()).unwrap(), fixture());
        }
    }

    #[test]
    fn unsafe_key_or_directory_acl_is_rejected_without_hardening_it() {
        for directory in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            let path = create(&root).unwrap();
            let target = if directory {
                root.join("native-adb")
            } else {
                path.clone()
            };
            // Only temporary fixtures receive this deliberately unsafe ACL.
            set_acl(&target, "D:P(A;;FA;;;WD)");
            assert!(create(&root).is_err());
            let handle = open(&target, directory, READ_CONTROL | FILE_READ_ATTRIBUTES).unwrap();
            assert!(check_security(&handle, &CurrentUser::load().unwrap(), true).is_err());
            assert_eq!(fs::read(&path).unwrap(), fixture());
        }
    }

    #[test]
    fn rejects_remote_relative_and_alternate_stream_paths_before_generation() {
        for path in [
            "relative",
            "C:relative",
            "\\\\server\\share\\app",
            "C:\\app:stream",
            "C:\\app.\\keys",
            "C:\\app\\..\\keys",
        ] {
            assert!(ensure(
                Path::new(path),
                |_| panic!("invalid path must not read identity"),
                || panic!("invalid path must not generate identity"),
                |_, _| panic!("invalid path must not create identity"),
            )
            .is_err());
        }
    }

    fn junction(link: &Path, target: &Path) {
        use windows_sys::Win32::System::{
            Ioctl::FSCTL_SET_REPARSE_POINT, SystemServices::IO_REPARSE_TAG_MOUNT_POINT,
            IO::DeviceIoControl,
        };
        fs::create_dir(link).unwrap();
        let file = open(link, true, GENERIC_WRITE).unwrap();
        let target = fs::canonicalize(target).unwrap();
        let mut print: Vec<u16> = target.as_os_str().encode_wide().collect();
        let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
        if print.starts_with(&verbatim) {
            print.drain(..verbatim.len());
        }
        let substitute: Vec<u16> = r"\??\"
            .encode_utf16()
            .chain(print.iter().copied())
            .collect();
        let path_bytes = (substitute.len() + print.len() + 2) * 2;
        let mut data = Vec::new();
        data.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        data.extend(((8 + path_bytes) as u16).to_le_bytes());
        data.extend(0u16.to_le_bytes());
        data.extend(0u16.to_le_bytes());
        data.extend(((substitute.len() * 2) as u16).to_le_bytes());
        data.extend((((substitute.len() + 1) * 2) as u16).to_le_bytes());
        data.extend(((print.len() * 2) as u16).to_le_bytes());
        for ch in substitute
            .into_iter()
            .chain(Some(0))
            .chain(print)
            .chain(Some(0))
        {
            data.extend(ch.to_le_bytes());
        }
        let mut returned = 0;
        // Mount-point junction fixtures do not require symbolic-link creation
        // privilege or Developer Mode; no process token/privilege is adjusted.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    FSCTL_SET_REPARSE_POINT,
                    data.as_ptr().cast(),
                    data.len() as u32,
                    null_mut(),
                    0,
                    &mut returned,
                    null_mut(),
                )
            },
            0,
            "junction fixture: {}",
            std::io::Error::last_os_error()
        );
    }

    #[test]
    fn rejects_junctions_in_ancestors_root_private_directory_and_key() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        let alias = temp.path().join("alias");
        junction(&alias, temp.path());
        assert!(create(&alias.join("app")).is_err());
        assert!(validate_path(
            &alias.join("app/native-adb/identity.pkcs8.pem"),
            atv_direct_adb::validate_identity_pem
        )
        .is_err());
        fs::remove_dir(&alias).unwrap();
        junction(&alias, &root);
        assert!(create(&alias).is_err());
        fs::remove_dir(&alias).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"do not change").unwrap();
        fs::remove_file(&path).unwrap();
        junction(&path, &outside);
        assert!(create(&root).is_err());
        fs::remove_dir(&path).unwrap();
        fs::remove_dir(root.join("native-adb")).unwrap();
        junction(&root.join("native-adb"), &outside);
        assert!(create(&root).is_err());
        assert_eq!(
            fs::read(outside.join("sentinel")).unwrap(),
            b"do not change"
        );
        assert!(!outside.join("identity.pkcs8.pem").exists());
        fs::remove_dir(root.join("native-adb")).unwrap();
    }

    #[test]
    fn failed_generation_and_abandoned_staging_do_not_rotate_or_delete_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        assert!(ensure(
            &root,
            atv_direct_adb::validate_identity_pem,
            || Err("fixture RNG failure".into()),
            |_, _| panic!("failed generation must not write"),
        )
        .is_err());
        let abandoned = root.join("native-adb/.identity-abandoned.tmp");
        fs::write(&abandoned, b"old partial fixture").unwrap();
        let path = create(&root).unwrap();
        assert_eq!(fs::read(path).unwrap(), fixture());
        assert_eq!(fs::read(abandoned).unwrap(), b"old partial fixture");
    }
}
