//! Win32 storage only. No ACL repair, key migration, or implicit overwrite.
use super::identity_policy::{self, AcePolicy, ParentAcePolicy};
use std::{
    ffi::c_void,
    fs::File,
    io::{Read, Seek, SeekFrom},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{
        SystemServices::{ACCESS_ALLOWED_ACE_TYPE, FILE_PERSISTENT_ACLS},
        Threading::*,
    },
};
const MAX_BYTES: u64 = 32768;
static NEXT: AtomicU64 = AtomicU64::new(0);
fn failure(action: &str) -> String {
    format!(
        "Windows native identity {action} failed: {}",
        std::io::Error::last_os_error()
    )
}
fn wide(path: &Path) -> Result<Vec<u16>, String> {
    let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) || value.len() >= 260 {
        return Err(
            "Windows identity path is invalid or exceeds the supported local path length.".into(),
        );
    }
    value.push(0);
    Ok(value)
}
fn raw(file: &File) -> HANDLE {
    file.as_raw_handle()
}
struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
struct UserSid {
    storage: Vec<usize>,
}
impl UserSid {
    fn ptr(&self) -> PSID {
        self.storage.as_ptr().cast_mut().cast()
    }
    fn current() -> Result<Self, String> {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(failure("token open"));
        }
        // OpenProcessToken returned an owned non-null handle.
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut needed = 0;
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut needed);
        }
        if needed < size_of::<TOKEN_USER>() as u32 || needed > 65536 {
            return Err("Windows identity token size is invalid.".into());
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(failure("token user query"));
        }
        // Vec<usize> provides TOKEN_USER alignment; retain its backing memory throughout SID copying.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let start = buffer.as_ptr() as usize;
        let end = start + buffer.len() * size_of::<usize>();
        let address = sid as usize;
        if address < start || address.checked_add(8).map_or(true, |end_sid| end_sid > end) {
            return Err("Windows token SID is outside its buffer.".into());
        }
        let length = 8usize + 4 * usize::from(unsafe { *sid.cast::<u8>().add(1) });
        if address
            .checked_add(length)
            .map_or(true, |end_sid| end_sid > end)
            || unsafe { IsValidSid(sid) } == 0
        {
            return Err("Windows token SID is invalid.".into());
        }
        let mut storage = vec![0usize; length.div_ceil(size_of::<usize>())];
        unsafe {
            std::ptr::copy_nonoverlapping(
                sid.cast::<u8>(),
                storage.as_mut_ptr().cast::<u8>(),
                length,
            );
        }
        Ok(Self { storage })
    }
}
fn trusted_installer_sid() -> Result<&'static UserSid, String> {
    static SID: std::sync::OnceLock<Result<UserSid, String>> = std::sync::OnceLock::new();
    SID.get_or_init(|| {
        // Microsoft documents this service SID: https://learn.microsoft.com/en-us/archive/msdn-magazine/2008/november/access-control-understanding-windows-file-and-registry-permissions
        let text: Vec<u16> = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut sid = null_mut();
        if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) } == 0 {
            return Err(failure("TrustedInstaller SID decoding"));
        }
        let _allocation = Allocation(sid);
        if unsafe { IsValidSid(sid) } == 0 {
            return Err("Windows TrustedInstaller SID is invalid.".into());
        }
        let length = unsafe { GetLengthSid(sid) } as usize;
        if !(8..=68).contains(&length) {
            return Err("Windows TrustedInstaller SID size is invalid.".into());
        }
        let mut storage = vec![0usize; length.div_ceil(size_of::<usize>())];
        unsafe {
            std::ptr::copy_nonoverlapping(
                sid.cast::<u8>(),
                storage.as_mut_ptr().cast::<u8>(),
                length,
            );
        }
        Ok(UserSid { storage })
    })
    .as_ref()
    .map_err(Clone::clone)
}
struct PrivateSecurity {
    descriptor: Allocation,
}
impl PrivateSecurity {
    fn new(user: &UserSid) -> Result<Self, String> {
        let mut text = null_mut();
        if unsafe { ConvertSidToStringSidW(user.ptr(), &mut text) } == 0 {
            return Err(failure("SID conversion"));
        }
        let allocation = Allocation(text.cast());
        let mut length = 0;
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        if length == 256 {
            return Err("Windows SID string is invalid.".into());
        }
        let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
            .map_err(|_| "Windows SID string is invalid.".to_owned())?;
        drop(allocation);
        let sddl: Vec<u16> = format!("O:{sid}D:P(A;;FA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(failure("private security descriptor creation"));
        }
        Ok(Self {
            descriptor: Allocation(descriptor),
        })
    }
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: 0,
        }
    }
}
struct OpenError {
    code: u32,
    message: String,
}
fn open_code(
    path: &Path,
    access: u32,
    disposition: u32,
    security: Option<&PrivateSecurity>,
    directory: bool,
) -> Result<File, OpenError> {
    let path = wide(path).map_err(|message| OpenError {
        code: ERROR_INVALID_NAME,
        message,
    })?;
    let attributes = security.map(PrivateSecurity::attributes);
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ,
            attributes
                .as_ref()
                .map_or(null(), |value| value as *const _),
            disposition,
            flags,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let code = unsafe { GetLastError() };
        return Err(OpenError {
            code,
            message: format!(
                "Windows identity handle open failed: {}",
                std::io::Error::from_raw_os_error(code as i32)
            ),
        });
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}
fn open(
    path: &Path,
    access: u32,
    disposition: u32,
    security: Option<&PrivateSecurity>,
    directory: bool,
) -> Result<File, String> {
    open_code(path, access, disposition, security, directory).map_err(|error| error.message)
}

fn attributes(file: &File, directory: bool) -> Result<(), String> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            raw(file),
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    } == 0
    {
        return Err(failure("handle attributes"));
    }
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
        || unsafe { GetFileType(raw(file)) } != FILE_TYPE_DISK
    {
        return Err(
            "Windows identity path must be a regular local object without reparse points.".into(),
        );
    }
    let mut flags = 0;
    if unsafe {
        GetVolumeInformationByHandleW(
            raw(file),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        )
    } == 0
    {
        return Err(failure("filesystem ACL capability query"));
    }
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err("Windows identity storage requires persistent filesystem ACLs.".into());
    }
    Ok(())
}
fn acl_snapshot(
    file: &File,
    user: &UserSid,
) -> Result<(bool, bool, bool, bool, Vec<DecodedAce>), String> {
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let result = unsafe {
        GetSecurityInfo(
            raw(file),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(format!(
            "Windows identity security query failed with code {result}."
        ));
    }
    let _allocation = Allocation(descriptor);
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
        return Err(failure("security control query"));
    }
    let owner_is_user = !owner.is_null()
        && unsafe { IsValidSid(owner) } != 0
        && unsafe { EqualSid(owner, user.ptr()) } != 0;
    let owner_is_trusted = owner_is_user
        || (!owner.is_null()
            && unsafe { IsValidSid(owner) } != 0
            && (unsafe { IsWellKnownSid(owner, WinLocalSystemSid) } != 0
                || unsafe { IsWellKnownSid(owner, WinBuiltinAdministratorsSid) } != 0));
    let installer = trusted_installer_sid()?;
    let owner_is_trusted = owner_is_trusted
        || (!owner.is_null()
            && unsafe { IsValidSid(owner) } != 0
            && unsafe { EqualSid(owner, installer.ptr()) } != 0);
    let mut aces = Vec::new();
    if !dacl.is_null() {
        if unsafe { IsValidAcl(dacl) } == 0 {
            return Err("Windows identity DACL is invalid.".into());
        }
        let count = unsafe { (*dacl).AceCount };
        if count > 16 {
            return Err("Windows identity DACL contains unexpected entries.".into());
        }
        for index in 0..u32::from(count) {
            let mut pointer = null_mut();
            if unsafe { GetAce(dacl, index, &mut pointer) } == 0 || pointer.is_null() {
                return Err(failure("ACL entry query"));
            }
            let header = unsafe { &*pointer.cast::<ACE_HEADER>() };
            let sid_offset = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
            if header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8
                || usize::from(header.AceSize) < sid_offset + 8
            {
                return Err("Windows identity DACL contains an unsupported entry.".into());
            }
            let ace = unsafe { &*pointer.cast::<ACCESS_ALLOWED_ACE>() };
            let sid = unsafe { pointer.cast::<u8>().add(sid_offset) };
            let length = 8 + 4 * usize::from(unsafe { *sid.add(1) });
            if sid_offset + length > usize::from(header.AceSize)
                || unsafe { IsValidSid(sid.cast()) } == 0
            {
                return Err("Windows identity DACL SID is invalid.".into());
            }
            let current = unsafe { EqualSid(sid.cast(), user.ptr()) } != 0;
            let trusted = current
                || unsafe { IsWellKnownSid(sid.cast(), WinLocalSystemSid) } != 0
                || unsafe { IsWellKnownSid(sid.cast(), WinBuiltinAdministratorsSid) } != 0;
            let installer = unsafe { EqualSid(sid.cast(), installer.ptr()) } != 0;
            aces.push(DecodedAce {
                current,
                trusted,
                installer,
                flags: header.AceFlags,
                mask: ace.Mask,
            });
        }
    }
    Ok((
        owner_is_user,
        owner_is_trusted,
        control & SE_DACL_PROTECTED != 0,
        !dacl.is_null(),
        aces,
    ))
}
struct DecodedAce {
    current: bool,
    trusted: bool,
    installer: bool,
    flags: u8,
    mask: u32,
}
fn private_acl(file: &File, user: &UserSid) -> Result<(), String> {
    let (owner, _, protected, present, entries) = acl_snapshot(file, user)?;
    let entries: Vec<_> = entries
        .into_iter()
        .map(|entry| AcePolicy {
            allow_current_user: entry.current,
            flags: entry.flags,
            mask: entry.mask,
        })
        .collect();
    if !identity_policy::private_acl(owner, protected, present, &entries) {
        return Err("Windows identity must have a protected, explicit current-user-only full-control DACL; existing permissions were not changed.".into());
    }
    Ok(())
}
fn safe_parent_acl(file: &File, user: &UserSid) -> Result<(), String> {
    let (owner, _, _, present, entries) = acl_snapshot(file, user)?;
    let entries: Vec<_> = entries
        .into_iter()
        .map(|entry| ParentAcePolicy {
            trusted_writer: entry.trusted,
            flags: entry.flags,
            mask: entry.mask,
        })
        .collect();
    if !identity_policy::parent_acl(owner, present, &entries) {
        return Err("Windows application-data directory grants mutation rights to an untrusted identity, has no DACL, or has a different owner; permissions were not changed.".into());
    }
    Ok(())
}
fn safe_ancestor_acl(file: &File, user: &UserSid) -> Result<(), String> {
    let (_, owner_trusted, _, present, entries) = acl_snapshot(file, user)?;
    let entries: Vec<_> = entries
        .into_iter()
        .map(|entry| ParentAcePolicy {
            trusted_writer: entry.trusted || entry.installer,
            flags: entry.flags,
            mask: entry.mask,
        })
        .collect();
    if !identity_policy::ancestor_acl(owner_trusted, present, &entries) {
        return Err("Windows identity ancestor grants replacement or security-control rights to an untrusted identity; permissions were not changed.".into());
    }
    Ok(())
}
fn same_file(a: &File, b: &File) -> Result<bool, String> {
    fn id(file: &File) -> Result<FILE_ID_INFO, String> {
        let mut value = FILE_ID_INFO::default();
        if unsafe {
            GetFileInformationByHandleEx(
                raw(file),
                FileIdInfo,
                (&mut value as *mut FILE_ID_INFO).cast(),
                size_of::<FILE_ID_INFO>() as u32,
            )
        } == 0
        {
            return Err(failure("file identity query"));
        }
        Ok(value)
    }
    let (a, b) = (id(a)?, id(b)?);
    Ok(a.VolumeSerialNumber == b.VolumeSerialNumber && a.FileId.Identifier == b.FileId.Identifier)
}
fn validate_handle<V: Fn(&[u8]) -> Result<(), String>>(
    file: &mut File,
    user: &UserSid,
    validate: &V,
) -> Result<(), String> {
    attributes(file, false)?;
    private_acl(file, user)?;
    let length = file
        .metadata()
        .map_err(|error| format!("Windows identity file length query failed: {error}"))?
        .len();
    if length > MAX_BYTES {
        return Err("Windows identity exceeds the key size limit.".into());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("Windows identity identity seek failed: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Windows identity identity read failed: {error}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Windows identity exceeds the key size limit.".into());
    }
    validate(&bytes).map_err(|_| {
        "Windows identity is not a valid RSA2048 PKCS#8 key; it was not changed.".into()
    })
}
fn existing<V: Fn(&[u8]) -> Result<(), String>>(
    path: &Path,
    user: &UserSid,
    validate: &V,
) -> Result<Option<File>, String> {
    for attempt in 0..50 {
        match open_code(
            path,
            GENERIC_READ | READ_CONTROL,
            OPEN_EXISTING,
            None,
            false,
        ) {
            Ok(mut file) => {
                validate_handle(&mut file, user, validate)?;
                return Ok(Some(file));
            }
            Err(error) => {
                let code = error.code;
                if code == ERROR_FILE_NOT_FOUND {
                    return Ok(None);
                }
                if code == ERROR_SHARING_VIOLATION && attempt < 49 {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                return Err(error.message);
            }
        }
    }
    unreachable!()
}
fn fixed_drive(root: &Path) -> Result<(), String> {
    let root = wide(root)?;
    if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
        return Err("Windows identity requires a fixed local drive; mapped network and other drive types are refused.".into());
    }
    Ok(())
}
fn anchored_directories(
    app_data: &Path,
    user: &UserSid,
    security: &PrivateSecurity,
) -> Result<Vec<File>, String> {
    let text = app_data
        .to_str()
        .ok_or_else(|| "Windows identity path must be valid Unicode.".to_owned())?;
    let (root, parts) = identity_policy::local_components(text)?;
    let mut path = PathBuf::from(root);
    fixed_drive(&path)?;
    let mut handles = Vec::new();
    let root_handle = open(
        &path,
        READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE,
        OPEN_EXISTING,
        None,
        true,
    )?;
    attributes(&root_handle, true)?;
    safe_ancestor_acl(&root_handle, user)?;
    handles.push(root_handle);
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        if index + 1 == parts.len() {
            let wide = wide(&path)?;
            let security_attributes = security.attributes();
            if unsafe { CreateDirectoryW(wide.as_ptr(), &security_attributes) } == 0
                && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
            {
                return Err(failure("application directory creation"));
            }
        }
        let file = open(
            &path,
            READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE,
            OPEN_EXISTING,
            None,
            true,
        )?;
        attributes(&file, true)?;
        if index + 1 == parts.len() {
            safe_parent_acl(&file, user)?;
        } else {
            safe_ancestor_acl(&file, user)?;
        }
        handles.push(file);
    }
    let path = app_data.join("native-adb");
    let wide = wide(&path)?;
    let security_attributes = security.attributes();
    if unsafe { CreateDirectoryW(wide.as_ptr(), &security_attributes) } == 0
        && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
    {
        return Err(failure("private directory creation"));
    }
    let private = open(
        &path,
        READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE,
        OPEN_EXISTING,
        None,
        true,
    )?;
    attributes(&private, true)?;
    private_acl(&private, user)?;
    handles.push(private);
    Ok(handles)
}
struct Staging {
    file: File,
    keep: bool,
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.keep {
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            unsafe {
                SetFileInformationByHandle(
                    raw(&self.file),
                    FileDispositionInfo,
                    (&info as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                );
            }
        }
    }
}
fn publish(file: &File, directory: &File) -> Result<(), String> {
    let name: Vec<u16> = "identity.pkcs8.pem".encode_utf16().collect();
    let bytes = offset_of!(FILE_RENAME_INFO, FileName) + name.len() * 2;
    let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = raw(directory);
        (*info).FileNameLength = u32::try_from(name.len() * 2).unwrap();
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
    }
    if unsafe {
        SetFileInformationByHandle(
            raw(file),
            FileRenameInfo,
            buffer.as_ptr().cast(),
            u32::try_from(bytes).unwrap(),
        )
    } == 0
    {
        return Err(failure("no-replace publication"));
    }
    Ok(())
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
    let user = UserSid::current()?;
    let security = PrivateSecurity::new(&user)?;
    let directories = anchored_directories(app_data, &user, &security)?;
    let directory = directories.last().unwrap();
    let path = app_data.join("native-adb/identity.pkcs8.pem");
    if existing(&path, &user, &validate)?.is_some() {
        return Ok(path);
    }
    let bytes = generate().map_err(|_| {
        "Windows identity generation failed; existing identities were not changed.".to_owned()
    })?;
    validate(&bytes)
        .map_err(|_| "Generated Windows identity was invalid; nothing was published.".to_owned())?;
    let temporary = app_data.join("native-adb").join(format!(
        ".identity-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut stage = Staging {
        file: open(
            &temporary,
            GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL,
            CREATE_NEW,
            Some(&security),
            false,
        )?,
        keep: false,
    };
    write(&mut stage.file, &bytes)
        .map_err(|error| format!("Windows identity staging write failed: {error}"))?;
    if stage
        .file
        .metadata()
        .map_err(|error| format!("Windows identity staging size query failed: {error}"))?
        .len()
        != bytes.len() as u64
    {
        return Err("Windows identity staging was incomplete; nothing was published.".into());
    }
    validate_handle(&mut stage.file, &user, &validate)?;
    stage
        .file
        .sync_all()
        .map_err(|error| format!("Windows identity staging flush failed: {error}"))?;
    stage.keep = true;
    if let Err(error) = publish(&stage.file, directory) {
        if let Some(winner) = existing(&path, &user, &validate)? {
            if !same_file(&stage.file, &winner)? {
                stage.keep = false;
            }
            return Ok(path);
        }
        return Err(error);
    }
    stage
        .file
        .sync_all()
        .map_err(|error| format!("Windows identity post-publication flush failed: {error}"))?;
    drop(stage);
    if existing(&path, &user, &validate)?.is_none() {
        return Err("Windows identity disappeared after publication; it was not recreated.".into());
    }
    Ok(path)
}
pub(super) fn validate_path<V: Fn(&[u8]) -> Result<(), String>>(
    path: &Path,
    validate: V,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Windows identity requires an absolute local path.".to_owned())?;
    identity_policy::local_components(
        path.to_str()
            .ok_or_else(|| "Windows identity path must be valid Unicode.".to_owned())?,
    )?;
    let user = UserSid::current()?;
    let text = parent.to_str().unwrap();
    let (root, parts) = identity_policy::local_components(text)?;
    let mut current = PathBuf::from(root);
    fixed_drive(&current)?;
    let root_handle = open(
        &current,
        READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE,
        OPEN_EXISTING,
        None,
        true,
    )?;
    attributes(&root_handle, true)?;
    safe_ancestor_acl(&root_handle, &user)?;
    let mut held = vec![root_handle];
    for part in parts {
        current.push(part);
        let directory = open(
            &current,
            READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE,
            OPEN_EXISTING,
            None,
            true,
        )?;
        attributes(&directory, true)?;
        safe_ancestor_acl(&directory, &user)?;
        held.push(directory);
    }
    if existing(path, &user, &validate)?.is_none() {
        return Err("Windows identity does not exist; nothing was created.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        sync::{Arc, Barrier},
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
        ensure(
            root,
            shield_direct_adb::validate_identity_pem,
            || Ok(fixture()),
            std::io::Write::write_all,
        )
    }
    #[test]
    fn windows_fixture_identity_is_complete_private_and_reused_without_generation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), fixture());
        let user = UserSid::current().unwrap();
        let file = open(
            &path,
            GENERIC_READ | READ_CONTROL,
            OPEN_EXISTING,
            None,
            false,
        )
        .unwrap();
        private_acl(&file, &user).unwrap();
        drop(file);
        assert_eq!(
            ensure(
                &root,
                shield_direct_adb::validate_identity_pem,
                || panic!("existing identity must not rotate"),
                |_, _| panic!("existing identity must not rewrite")
            )
            .unwrap(),
            path
        );
    }
    #[test]
    fn windows_fixture_concurrent_complete_winner_is_reused() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let barrier = Arc::new(Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    ensure(
                        &root,
                        shield_direct_adb::validate_identity_pem,
                        || {
                            barrier.wait();
                            Ok(fixture())
                        },
                        std::io::Write::write_all,
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
        assert_eq!(std::fs::read(&paths[0]).unwrap(), fixture());
    }
    #[test]
    fn windows_fixture_partial_write_never_publishes_or_replaces_identity() {
        for fail in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("app");
            assert!(ensure(
                &root,
                shield_direct_adb::validate_identity_pem,
                || Ok(fixture()),
                |file, bytes| {
                    file.write_all(&bytes[..20])?;
                    if fail {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "fixture storage error",
                        ))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err());
            assert!(!root.join("native-adb/identity.pkcs8.pem").exists());
            assert_eq!(std::fs::read(create(&root).unwrap()).unwrap(), fixture());
        }
    }
    #[test]
    fn windows_fixture_corrupt_existing_key_and_directory_are_never_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        std::fs::write(&path, b"corrupt fixture").unwrap();
        assert!(ensure(
            &root,
            shield_direct_adb::validate_identity_pem,
            || panic!("corrupt must not regenerate"),
            |_, _| panic!("must not overwrite")
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt fixture");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(create(&root).is_err());
        assert!(path.is_dir());
    }
    #[test]
    fn windows_fixture_broad_created_dacl_is_refused_without_acl_repair() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        std::fs::remove_file(&path).unwrap();
        let sddl: Vec<u16> = "D:P(A;;FA;;;WD)".encode_utf16().chain(Some(0)).collect();
        let mut descriptor = null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    null_mut(),
                )
            },
            0
        );
        let broad = PrivateSecurity {
            descriptor: Allocation(descriptor),
        };
        let mut file = open(
            &path,
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
            CREATE_NEW,
            Some(&broad),
            false,
        )
        .unwrap();
        file.write_all(&fixture()).unwrap();
        drop(file);
        assert!(create(&root).is_err());
        assert_eq!(std::fs::read(path).unwrap(), fixture());
    }
    #[test]
    fn windows_fixture_reader_prevents_deletion_while_validating() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let path = create(&root).unwrap();
        let result = ensure(
            &root,
            |bytes| {
                shield_direct_adb::validate_identity_pem(bytes)?;
                assert!(std::fs::remove_file(&path).is_err());
                Ok(())
            },
            || panic!("must reuse"),
            |_, _| panic!("must not write"),
        );
        assert_eq!(result.unwrap(), path);
        assert_eq!(std::fs::read(path).unwrap(), fixture());
    }
}
