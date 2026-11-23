//! File-manager guards and device-side file operations shared by desktop and
//! mobile. Both apps wrap these in their own Tauri commands because the local
//! side differs (a host folder on desktop, app-private storage on a phone),
//! but the device-side rules — what may be touched and how a search failure is
//! reported — must not drift between them.

use serde::Serialize;

use crate::adb::AdbDriver;
use crate::commands::apps::ActionResult;
use crate::commands::quote_shell_arg;

/// Where Android mounts removable (public) volumes such as USB drives and SD
/// cards: `/storage/<fs uuid>`.
pub const VOLUME_PARENT: &str = "/storage";

/// A removable volume's directory name under `/storage`: the filesystem UUID
/// Android names it by, in one of the shapes vold produces — `1A2B-3C4D`
/// (FAT/exFAT), 16 hex digits (NTFS) or a dashed 8-4-4-4-12 UUID (ext4, f2fs).
/// Nothing else counts: not `emulated` or `self` (internal storage's own
/// aliases), not `null` (what `sm` prints for a drive with no UUID) and not a
/// legacy name such as `sdcard0`.
pub fn is_volume_name(name: &str) -> bool {
    let groups: Vec<&str> = name.split('-').collect();
    let lengths: Vec<usize> = groups.iter().map(|g| g.len()).collect();
    groups
        .iter()
        .all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
        && matches!(lengths.as_slice(), [4, 4] | [16] | [8, 4, 4, 4, 12])
}

/// FAT32 stores sizes in 32 bits, so 4 GiB is the first size it can't hold.
pub const FAT32_MAX_FILE: u64 = 1 << 32;

/// Appended to a failed upload of `size` bytes to `target`: a file of 4 GiB or
/// more on a USB drive most likely hit FAT32's limit. Empty otherwise.
pub fn drive_size_hint(target: &str, size: u64) -> &'static str {
    let on_drive = user_storage_root(target).is_some_and(|root| root != "/sdcard");
    if on_drive && size >= FAT32_MAX_FILE {
        " USB drives formatted as FAT32 can't hold files of 4 GB or more; exFAT or NTFS can."
    } else {
        ""
    }
}

/// The user-storage root a path lives in: `/sdcard` (internal storage) or
/// `/storage/<uuid>` (a USB drive or SD card), or `None` outside both. Purely
/// lexical; callers have already refused `..` and control characters.
pub fn user_storage_root(path: &str) -> Option<String> {
    if path == "/sdcard" || path.starts_with("/sdcard/") {
        return Some("/sdcard".to_string());
    }
    let rest = path.strip_prefix("/storage/")?;
    let name = rest.split('/').next().unwrap_or("");
    is_volume_name(name).then(|| format!("{VOLUME_PARENT}/{name}"))
}

fn validate_path_syntax(path: &str) -> Result<String, String> {
    let p = path.trim();
    if !p.starts_with('/') {
        return Err(format!("Path must be absolute: {p:?}"));
    }
    if p.split('/').any(|seg| seg == "..") {
        return Err("Path traversal (`..`) is not allowed.".to_string());
    }
    if p.chars().any(|c| c.is_control()) {
        return Err("Path contains control characters.".to_string());
    }
    Ok(p.to_string())
}

/// Validate a device path for file-manager use. Always absolute, no `..`
/// traversal, no control characters (they'd corrupt the shell line even
/// quoted). When `allow_system` is false the path must also live in user
/// storage: `/sdcard` or a removable drive under `/storage/<uuid>`. Power-user
/// mode lifts only that boundary — the injection guards stay. Returns the
/// trimmed path.
pub fn validate_device_path(path: &str, allow_system: bool) -> Result<String, String> {
    let p = validate_path_syntax(path)?;
    if !allow_system && user_storage_root(&p).is_none() {
        return Err(format!(
            "Path must be under /sdcard or on a USB drive: {p:?}"
        ));
    }
    Ok(p)
}

/// User-storage validation: `/sdcard` or a removable drive. The bound for
/// uploads and for deletes outside power-user mode.
pub fn validate_user_storage_path(path: &str) -> Result<String, String> {
    validate_device_path(path, false)
}

/// `/sdcard`-confined validation — for the paths that are internal storage
/// only by design (the device-to-device copy and the backup finder).
pub fn validate_sdcard_path(path: &str) -> Result<String, String> {
    let p = validate_path_syntax(path)?;
    if p != "/sdcard" && !p.starts_with("/sdcard/") {
        return Err(format!("Path must be under /sdcard: {p:?}"));
    }
    Ok(p)
}

/// A removable volume the TV has mounted: a USB drive or SD card set up as
/// portable storage. Drives set up as internal storage merge into `/sdcard`
/// and are not listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorageVolume {
    /// `/storage/<uuid>`.
    pub path: String,
    /// The filesystem UUID Android names the volume by.
    pub uuid: String,
    pub read_only: bool,
    /// The filesystem as `/proc/mounts` names it (`vfat`, `exfat`, `ntfs`, …),
    /// or `None` when the TV didn't say.
    pub fs_type: Option<String>,
}

/// Parse `sm list-volumes public`: one `<id> <state> <fs uuid>` line per
/// volume, e.g. `public:8,1 mounted 1A2B-3C4D`. Only mounted volumes with a
/// usable name are kept.
pub fn parse_sm_volumes(out: &str) -> Vec<StorageVolume> {
    let mut volumes: Vec<StorageVolume> = Vec::new();
    for line in out.lines() {
        let mut fields = line.split_whitespace();
        let (Some(id), Some(state), Some(uuid)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if !id.starts_with("public:") || !is_volume_name(uuid) {
            continue;
        }
        let read_only = match state {
            "mounted" => false,
            "mounted_ro" => true,
            _ => continue,
        };
        if volumes.iter().any(|v| v.uuid == uuid) {
            continue;
        }
        volumes.push(StorageVolume {
            path: format!("{VOLUME_PARENT}/{uuid}"),
            uuid: uuid.to_string(),
            read_only,
            fs_type: None,
        });
    }
    volumes
}

/// Printed when `sm` itself is unavailable, so an empty volume list can be
/// told apart from a TV without the tool.
const NO_SM: &str = "__ATVOPT_NO_SM__";

/// The filesystem of the drive named `uuid`, from `/proc/mounts`. vold mounts
/// the real filesystem at `/mnt/media_rw/<uuid>` and exposes it at
/// `/storage/<uuid>` through FUSE or sdcardfs, so the former is the one that
/// says `vfat` or `exfat`; `/storage/<uuid>` is the fallback on TVs that mount
/// the drive there directly.
pub fn mount_fs_type(mounts: &str, uuid: &str) -> Option<String> {
    let media_rw = format!("/mnt/media_rw/{uuid}");
    let storage = format!("{VOLUME_PARENT}/{uuid}");
    let fs_at = |point: &str| {
        mounts.lines().find_map(|line| {
            let mut fields = line.split_whitespace();
            let (_, Some(mount), Some(fs)) = (fields.next(), fields.next(), fields.next()) else {
                return None;
            };
            (mount == point).then(|| fs.to_string())
        })
    };
    fs_at(&media_rw).or_else(|| {
        fs_at(&storage).filter(|fs| !matches!(fs.as_str(), "fuse" | "sdcardfs" | "esdfs"))
    })
}

/// FAT32 as Linux names it.
pub fn is_fat32(fs_type: Option<&str>) -> bool {
    matches!(fs_type, Some("vfat" | "msdos"))
}

/// Refuse, before any bytes move, a file FAT32 can't hold on a drive known to
/// be FAT32. A drive whose filesystem is unknown is left to the transfer
/// itself, whose failure carries [`drive_size_hint`].
pub fn fat32_refusal(name: &str, size: u64, fs_type: Option<&str>) -> Result<(), String> {
    if size >= FAT32_MAX_FILE && is_fat32(fs_type) {
        return Err(format!(
            "{name} is 4 GB or larger, and this USB drive is formatted as FAT32, which can't hold files that big. Nothing was copied. Reformat the drive as exFAT or NTFS, or use a smaller file."
        ));
    }
    Ok(())
}

/// The filesystem of the drive `path` is on, read from the TV's
/// `/proc/mounts`; `None` for internal storage or when it can't be read.
pub async fn drive_fs_type_with(adb: &dyn AdbDriver, serial: &str, path: &str) -> Option<String> {
    let root = user_storage_root(path)?;
    let uuid = root.strip_prefix(&format!("{VOLUME_PARENT}/"))?;
    let out = adb.shell(serial, "cat /proc/mounts").await.ok()?;
    mount_fs_type(&out.stdout, uuid)
}

/// Fill in each drive's filesystem from `/proc/mounts`. An unreadable file
/// leaves them unknown rather than failing the list.
async fn with_fs_types(
    adb: &dyn AdbDriver,
    serial: &str,
    mut volumes: Vec<StorageVolume>,
) -> Vec<StorageVolume> {
    if volumes.is_empty() {
        return volumes;
    }
    if let Ok(out) = adb.shell(serial, "cat /proc/mounts").await {
        for v in &mut volumes {
            v.fs_type = mount_fs_type(&out.stdout, &v.uuid);
        }
    }
    volumes
}

/// The removable drives mounted on the TV, with their filesystems. Reads
/// `sm list-volumes public`; where `sm` is missing, falls back to the volume
/// directories under `/storage` (read/write state then unknown, so reported
/// writable and left to the upload itself to fail).
pub async fn list_storage_volumes_with(
    adb: &dyn AdbDriver,
    serial: &str,
) -> Result<Vec<StorageVolume>, String> {
    let out = adb
        .shell(
            serial,
            &format!("sm list-volumes public 2>/dev/null || echo {NO_SM}"),
        )
        .await
        .map_err(|e| format!("list volumes: {e}"))?;
    if !out.stdout.contains(NO_SM) {
        return Ok(with_fs_types(adb, serial, parse_sm_volumes(&out.stdout)).await);
    }
    let out = adb
        .shell(serial, "ls -1 /storage 2>/dev/null; true")
        .await
        .map_err(|e| format!("list volumes: {e}"))?;
    let mut volumes: Vec<StorageVolume> = Vec::new();
    for name in out.stdout.lines().map(str::trim) {
        if is_volume_name(name) && !volumes.iter().any(|v| v.uuid == name) {
            volumes.push(StorageVolume {
                path: format!("{VOLUME_PARENT}/{name}"),
                uuid: name.to_string(),
                read_only: false,
                fs_type: None,
            });
        }
    }
    Ok(with_fs_types(adb, serial, volumes).await)
}

/// Normalise a device path for deleting: collapse repeated slashes and `.`
/// segments so `//system` and `/sdcard/.` are checked as `/system` and
/// `/sdcard`. A `..` segment is refused outright rather than resolved.
pub fn normalize_delete_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if !path.starts_with('/') {
        return Err(format!("Path must be absolute: {path:?}"));
    }
    let mut segments = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => return Err("Path traversal (`..`) is not allowed.".to_string()),
            seg => segments.push(seg),
        }
    }
    Ok(format!("/{}", segments.join("/")))
}

/// Refuse deleting the filesystem root, `/sdcard` itself, or a critical system
/// mount even in power-user mode — a recursive delete there could brick the
/// device. Subpaths are the user's call (and mostly permission-denied without
/// root). Other spellings of a protected path are checked in normalised form.
/// Returns the refusal message, or `None` if the path is deletable.
pub fn protected_delete_reason(path: &str) -> Option<String> {
    let normalized = match normalize_delete_path(path) {
        Ok(p) => p,
        Err(e) => return Some(format!("Refusing to delete {}: {e}", path.trim())),
    };
    let trimmed = normalized.as_str();
    // Every name Android gives the user's shared storage.
    const STORAGE: &[&str] = &[
        "/sdcard",
        "/storage",
        "/storage/emulated",
        "/storage/emulated/0",
        "/storage/self",
        "/storage/self/primary",
        "/mnt",
        "/mnt/sdcard",
        "/mnt/user",
        "/mnt/user/0",
        "/mnt/user/0/primary",
        "/data/media",
        "/data/media/0",
    ];
    if STORAGE.contains(&trimmed) {
        return Some(format!(
            "Refusing to delete {trimmed} itself: it is the TV's shared storage."
        ));
    }
    if user_storage_root(trimmed).as_deref() == Some(trimmed) {
        return Some(format!(
            "Refusing to delete {trimmed} itself: it is a whole drive. Delete the files on it instead."
        ));
    }
    const PROTECTED: &[&str] = &[
        "/", "/system", "/data", "/vendor", "/proc", "/sys", "/dev", "/boot", "/init", "/sbin",
        "/bin", "/etc",
    ];
    if PROTECTED.contains(&trimmed) {
        return Some(format!(
            "Refusing to delete a protected system path: {trimmed}"
        ));
    }
    None
}

/// Remove a file or directory (recursively). Confined to user storage
/// (`/sdcard` and removable drives) unless `allow_system` (desktop power-user
/// mode) is set; callers confirm with the user first, and
/// `protected_delete_reason` still blocks catastrophic targets.
pub async fn delete_path_with(
    adb: &dyn AdbDriver,
    serial: &str,
    path: &str,
    allow_system: bool,
) -> Result<ActionResult, String> {
    let path = validate_device_path(&normalize_delete_path(path)?, allow_system)?;
    if let Some(reason) = protected_delete_reason(&path) {
        return Err(reason);
    }
    let out = adb
        .shell(serial, &format!("rm -rf {}", quote_shell_arg(&path)))
        .await
        .map_err(|e| format!("rm: {e}"))?;
    let noise = out.combined().trim().to_string();
    // The desktop driver turns a nonzero exit into an error above; the mobile
    // transport hands it back as data, so it is checked here too.
    let exited_cleanly = matches!(out.exit_code, None | Some(0));
    if noise.is_empty() && exited_cleanly {
        Ok(ActionResult {
            ok: true,
            message: format!("Deleted {path}."), partial: false
        })
    } else if noise.is_empty() {
        Ok(ActionResult {
            ok: false,
            message: format!("The TV did not confirm that {path} was deleted."), partial: false
        })
    } else {
        Ok(ActionResult {
            ok: false,
            message: noise, partial: false
        })
    }
}

/// Filename patterns for `find -name`: glob stars and dots only — no slashes,
/// quotes, or anything the shell could reinterpret.
pub fn validate_find_pattern(pattern: &str) -> Result<(), String> {
    let ok = !pattern.is_empty()
        && pattern.len() <= 64
        && pattern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid search pattern: {pattern:?}"))
    }
}

/// Most hits one search returns.
pub const MAX_FIND_HITS: usize = 100;

/// Printed after `find` finishes. Its presence is the only proof the search
/// ran to the end: a missing directory makes `find` exit nonzero (which the
/// desktop driver reports as an error, indistinguishable from a dropped
/// connection), while a cut-off stream produces partial output with no error
/// at all.
const FIND_DONE: &str = "__ATVOPT_FIND_DONE__";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindResult {
    /// Matching file paths, capped at [`MAX_FIND_HITS`].
    pub hits: Vec<String>,
    /// Directories the search could not be run against at all, because the ADB
    /// call itself failed. A directory that simply does not exist is *not*
    /// listed here — that is a real "no matches". Without this split, a
    /// dropped connection would read as "export from the app first", telling
    /// the user to redo something that had worked (GitHub #86).
    pub unsearched: Vec<String>,
}

/// Locate files matching a name pattern under one or more `/sdcard`
/// directories. Powers the app-backup finder (e.g. Projectivy's `*.plbackup`
/// exports land wherever the user's file picker put them).
pub async fn find_files_with(
    adb: &dyn AdbDriver,
    serial: &str,
    dirs: &[String],
    pattern: &str,
) -> Result<FindResult, String> {
    validate_find_pattern(pattern)?;
    let dirs = dirs
        .iter()
        .map(|dir| validate_sdcard_path(dir))
        .collect::<Result<Vec<_>, _>>()?;
    let mut hits: Vec<String> = Vec::new();
    let mut unsearched = Vec::new();
    for dir in dirs {
        // A missing directory or a permission denial is expected for some
        // candidates, and `find` reports those on stderr — suppress them, since
        // an empty result is the honest answer. The trailing marker keeps the
        // exit status 0 and proves the search completed.
        let cmd = format!(
            "find {} -maxdepth 4 -type f -name '{pattern}' 2>/dev/null; echo {FIND_DONE}",
            quote_shell_arg(&dir)
        );
        let out = match adb.shell(serial, &cmd).await {
            Ok(out) if out.stdout.lines().any(|line| line.trim() == FIND_DONE) => out,
            _ => {
                unsearched.push(dir);
                continue;
            }
        };
        for line in out.stdout.lines() {
            if hits.len() >= MAX_FIND_HITS {
                break;
            }
            let line = line.trim();
            if line.starts_with("/sdcard/") && !hits.iter().any(|h| h == line) {
                hits.push(line.to_string());
            }
        }
    }
    Ok(FindResult { hits, unsearched })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::MockAdb;
    use pretty_assertions::assert_eq;

    fn dirs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn find_patterns_validated() {
        assert!(validate_find_pattern("*.plbackup").is_ok());
        assert!(validate_find_pattern("backup_*.zip").is_ok());
        assert!(validate_find_pattern("a/b").is_err());
        assert!(validate_find_pattern("x'y").is_err());
        assert!(validate_find_pattern("").is_err());
        assert!(validate_find_pattern(&"x".repeat(65)).is_err());
    }

    #[test]
    fn accepts_sdcard_paths() {
        assert_eq!(validate_sdcard_path("/sdcard").unwrap(), "/sdcard");
        assert_eq!(
            validate_sdcard_path("/sdcard/Download/file 1.mp4").unwrap(),
            "/sdcard/Download/file 1.mp4"
        );
    }

    #[test]
    fn rejects_escapes_and_system_paths() {
        assert!(validate_sdcard_path("/data/data/com.x").is_err());
        assert!(validate_sdcard_path("/sdcard/../data").is_err());
        assert!(validate_sdcard_path("/sdcardX/evil").is_err());
        assert!(validate_sdcard_path("/sdcard/a\nb").is_err());
        assert!(validate_sdcard_path("/storage/emulated/0/x").is_err());
        assert!(validate_sdcard_path("").is_err());
    }

    #[test]
    fn power_user_mode_allows_system_paths_but_keeps_injection_guards() {
        assert!(validate_device_path("/system/app", false).is_err());
        assert_eq!(
            validate_device_path("/system/app", true).unwrap(),
            "/system/app"
        );
        assert_eq!(validate_device_path("/", true).unwrap(), "/");
        assert!(validate_device_path("/system/../x", true).is_err());
        assert!(validate_device_path("/system/a\nb", true).is_err());
        assert!(validate_device_path("relative/path", true).is_err());
    }

    #[test]
    fn protected_paths_are_never_deletable() {
        for p in [
            "/", "/system", "/data", "/vendor", "/sdcard", "/system/", "/sdcard/",
        ] {
            assert!(protected_delete_reason(p).is_some(), "{p} must be refused");
        }
        assert!(protected_delete_reason("/sdcard/Download/old.zip").is_none());
        assert!(protected_delete_reason("/system/app/Bloat/Bloat.apk").is_none());
    }

    #[test]
    fn other_spellings_of_protected_paths_are_refused() {
        for p in [
            "/sdcard/.",
            "/sdcard/./",
            "//system",
            "/system//",
            "/sdcard//",
            "/sdcard/Download/..",
            "/sdcard/Download/../x",
            "/storage/emulated/0",
            "/storage/emulated/0/",
            "/storage/self/primary",
            "/storage/emulated",
            "/storage",
            "/mnt/sdcard",
            "/data/media/0",
        ] {
            assert!(protected_delete_reason(p).is_some(), "{p} must be refused");
        }
        assert!(protected_delete_reason("/storage/emulated/0/Download/old.zip").is_none());
        assert!(protected_delete_reason("/sdcard/.hidden").is_none());
        assert!(protected_delete_reason("/sdcard//Download").is_none());
        assert!(protected_delete_reason("//data/local/tmp/x").is_none());
    }

    #[test]
    fn removable_drives_are_user_storage() {
        for (path, root) in [
            ("/sdcard", "/sdcard"),
            ("/sdcard/Movies/a.mp4", "/sdcard"),
            ("/storage/1A2B-3C4D", "/storage/1A2B-3C4D"),
            ("/storage/1A2B-3C4D/Movies/a b.mkv", "/storage/1A2B-3C4D"),
            ("/storage/0123456789ABCDEF/x", "/storage/0123456789ABCDEF"),
            (
                "/storage/0a1b2c3d-4e5f-6789-abcd-ef0123456789/x",
                "/storage/0a1b2c3d-4e5f-6789-abcd-ef0123456789",
            ),
        ] {
            assert_eq!(user_storage_root(path).as_deref(), Some(root), "{path}");
            assert_eq!(validate_user_storage_path(path).unwrap(), path);
        }
        for path in [
            "/storage",
            "/storage/",
            "/storage/emulated/0/x",
            "/storage/self/primary",
            "/storage/-ABCD",
            "/storage/AB_CD",
            "/storage/AB CD",
            "/storage/null",
            "/storage/sdcard0/x",
            "/storage/usbdisk",
            "/storage/1A2B-3C4G",
            "/storage/1A2B3C4D",
            "/storage/1A2B-3C4D/../../data",
            "/data/local/tmp",
            "/sdcardX",
        ] {
            assert!(validate_user_storage_path(path).is_err(), "{path}");
        }
        // The backup finder and device-to-device copy stay on internal storage.
        assert!(validate_sdcard_path("/storage/1A2B-3C4D/x").is_err());
    }

    #[test]
    fn the_fat32_hint_is_for_big_files_on_drives() {
        let big = FAT32_MAX_FILE;
        assert!(drive_size_hint("/storage/1A2B-3C4D/m.mkv", big).contains("FAT32"));
        assert!(drive_size_hint("/storage/1A2B-3C4D/m.mkv", big - 1).is_empty());
        assert!(drive_size_hint("/sdcard/m.mkv", big).is_empty());
    }

    #[test]
    fn a_whole_drive_is_never_deletable() {
        for p in [
            "/storage/1A2B-3C4D",
            "/storage/1A2B-3C4D/",
            "/storage//1A2B-3C4D/.",
        ] {
            assert!(protected_delete_reason(p).is_some(), "{p} must be refused");
        }
        assert!(protected_delete_reason("/storage/1A2B-3C4D/Movies").is_none());
    }

    #[test]
    fn sm_volume_lines_are_parsed() {
        let out = "public:8,1 mounted 1A2B-3C4D\n\
                   public:8,17 mounted_ro 0123456789ABCDEF\n\
                   public:8,33 unmounted null\n\
                   public:8,49 checking 5555-6666\n\
                   public:8,1 mounted 1A2B-3C4D\n\
                   private mounted null\n\
                   public:8,65 mounted ../x\n\
                   public:8,81 mounted null\n";
        assert_eq!(
            parse_sm_volumes(out),
            vec![
                StorageVolume {
                    path: "/storage/1A2B-3C4D".into(),
                    uuid: "1A2B-3C4D".into(),
                    read_only: false,
                    fs_type: None,
                },
                StorageVolume {
                    path: "/storage/0123456789ABCDEF".into(),
                    uuid: "0123456789ABCDEF".into(),
                    read_only: true,
                    fs_type: None,
                },
            ]
        );
        assert!(parse_sm_volumes("").is_empty());
    }

    const MOUNTS: &str = "/dev/block/vold/public:8,1 /mnt/media_rw/1A2B-3C4D vfat rw,dirsync 0 0\n\
        /dev/fuse /storage/1A2B-3C4D fuse rw,nosuid 0 0\n\
        /dev/block/vold/public:8,17 /mnt/media_rw/0123456789ABCDEF ntfs rw 0 0\n\
        /dev/block/sda1 /storage/5555-6666 exfat rw 0 0\n\
        /dev/fuse /storage/7777-8888 fuse rw 0 0\n";

    #[test]
    fn drive_filesystems_come_from_proc_mounts() {
        assert_eq!(mount_fs_type(MOUNTS, "1A2B-3C4D").as_deref(), Some("vfat"));
        assert_eq!(
            mount_fs_type(MOUNTS, "0123456789ABCDEF").as_deref(),
            Some("ntfs")
        );
        assert_eq!(mount_fs_type(MOUNTS, "5555-6666").as_deref(), Some("exfat"));
        // Only the FUSE view is known: the real filesystem is unknown, not "fuse".
        assert_eq!(mount_fs_type(MOUNTS, "7777-8888"), None);
        assert_eq!(mount_fs_type(MOUNTS, "9999-AAAA"), None);
    }

    #[test]
    fn fat32_refuses_only_what_it_cannot_hold() {
        assert!(fat32_refusal("m.mkv", FAT32_MAX_FILE, Some("vfat")).is_err());
        assert!(fat32_refusal("m.mkv", FAT32_MAX_FILE - 1, Some("vfat")).is_ok());
        assert!(fat32_refusal("m.mkv", FAT32_MAX_FILE, Some("exfat")).is_ok());
        assert!(fat32_refusal("m.mkv", FAT32_MAX_FILE, None).is_ok());
    }

    #[tokio::test]
    async fn a_drive_filesystem_is_read_for_its_path() {
        let mock = MockAdb::default().on_shell("cat /proc/mounts", MOUNTS);
        assert_eq!(
            drive_fs_type_with(&mock, "tv", "/storage/1A2B-3C4D/Movies")
                .await
                .as_deref(),
            Some("vfat")
        );
        assert_eq!(
            drive_fs_type_with(&mock, "tv", "/sdcard/Movies").await,
            None
        );
    }

    #[tokio::test]
    async fn volumes_fall_back_to_the_storage_folder_without_sm() {
        let mock = MockAdb::default()
            .on_shell("sm list-volumes", &format!("{NO_SM}\n"))
            .on_shell("ls -1 /storage", "emulated\nself\nsdcard0\n1A2B-3C4D\n")
            .on_shell("cat /proc/mounts", MOUNTS);
        let volumes = list_storage_volumes_with(&mock, "tv").await.unwrap();
        assert_eq!(
            volumes,
            vec![StorageVolume {
                path: "/storage/1A2B-3C4D".into(),
                uuid: "1A2B-3C4D".into(),
                read_only: false,
                fs_type: Some("vfat".into()),
            }]
        );
        let none = MockAdb::default().on_shell("sm list-volumes", "");
        assert!(list_storage_volumes_with(&none, "tv")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn delete_on_a_drive_needs_no_power_user_mode() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        let r = delete_path_with(&mock, "tv", "/storage/1A2B-3C4D/old.mkv", false)
            .await
            .unwrap();
        assert!(r.ok, "{}", r.message);
        assert!(delete_path_with(&mock, "tv", "/storage/1A2B-3C4D", false)
            .await
            .is_err());
        assert_eq!(
            log.lock().unwrap().as_slice(),
            ["rm -rf '/storage/1A2B-3C4D/old.mkv'".to_string()]
        );
    }

    #[tokio::test]
    async fn delete_runs_on_the_normalised_path() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        for bad in ["//system", "/sdcard/.", "/sdcard//", "//data/../system"] {
            assert!(
                delete_path_with(&mock, "tv", bad, true).await.is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(log.lock().unwrap().is_empty());
        // The Files tab at `/` joins `/` and `data/...` into `//data/...`.
        let r = delete_path_with(&mock, "tv", "//data/local/tmp/x", true)
            .await
            .unwrap();
        assert!(r.ok, "{}", r.message);
        assert_eq!(
            log.lock().unwrap().as_slice(),
            ["rm -rf '/data/local/tmp/x'".to_string()]
        );
    }

    const SMARTTUBE_DIRS: &[&str] = &[
        "/sdcard/Documents/SmartTubeBackup",
        "/sdcard/SmartTubeBackup",
        "/sdcard/Android/data/com.teamsmart.videomanager.tv",
    ];

    #[tokio::test]
    async fn missing_directories_are_no_matches_not_unsearched() {
        let hit = "/sdcard/Documents/SmartTubeBackup/org.smarttube.stable_1.zip";
        let mock = MockAdb::default()
            .on_shell(
                "'/sdcard/Documents/SmartTubeBackup'",
                &format!("{hit}\n{FIND_DONE}\n"),
            )
            // `find` printed nothing for a directory that isn't there, but the
            // marker still ran — that is an honest empty search.
            .on_shell("'/sdcard/SmartTubeBackup'", &format!("{FIND_DONE}\n"))
            .on_shell("'/sdcard/Android/data/", &format!("{FIND_DONE}\n"));
        let log = mock.shell_log();
        let result = find_files_with(&mock, "tv", &dirs(SMARTTUBE_DIRS), "*.zip")
            .await
            .unwrap();
        assert_eq!(
            result,
            FindResult {
                hits: vec![hit.to_string()],
                unsearched: vec![],
            }
        );
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 3);
        assert!(log[0].contains("-name '*.zip'"));
    }

    #[tokio::test]
    async fn a_failed_call_or_a_cut_off_stream_is_unsearched() {
        let mock = MockAdb::default()
            .on_shell_err("'/sdcard/Documents/SmartTubeBackup'", "device offline")
            // Output without the completion marker: the stream ended early.
            .on_shell(
                "'/sdcard/SmartTubeBackup'",
                "/sdcard/SmartTubeBackup/partial.zip\n",
            )
            .on_shell("'/sdcard/Android/data/", &format!("{FIND_DONE}\n"));
        let result = find_files_with(&mock, "tv", &dirs(SMARTTUBE_DIRS), "*.zip")
            .await
            .unwrap();
        assert!(result.hits.is_empty());
        assert_eq!(
            result.unsearched,
            dirs(&SMARTTUBE_DIRS[..2]),
            "a search that never finished must not read as no matches"
        );
    }

    #[tokio::test]
    async fn find_rejects_bad_dirs_before_touching_the_device() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        assert!(
            find_files_with(&mock, "tv", &dirs(&["/sdcard/ok", "/data"]), "*.zip")
                .await
                .is_err()
        );
        assert!(
            find_files_with(&mock, "tv", &dirs(&["/sdcard"]), "*.zip'; rm")
                .await
                .is_err()
        );
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn hits_are_capped_and_deduplicated() {
        let mut many: String = (0..150)
            .map(|i| format!("/sdcard/Download/f{i}.zip\n"))
            .collect();
        many.push_str("/sdcard/Download/f0.zip\n");
        many.push_str(FIND_DONE);
        let mock = MockAdb::default().on_shell("find", &many);
        let result = find_files_with(
            &mock,
            "tv",
            &dirs(&["/sdcard/Download", "/sdcard"]),
            "*.zip",
        )
        .await
        .unwrap();
        assert_eq!(result.hits.len(), MAX_FIND_HITS);
        assert!(result.unsearched.is_empty());
    }

    #[tokio::test]
    async fn delete_is_confined_and_quoted() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        for bad in [
            "/sdcard",
            "/sdcard/",
            "/sdcard/../data",
            "/data/local/tmp/x",
            "/storage/emulated/0/x",
            "relative",
            "/sdcard/a\nb",
        ] {
            assert!(
                delete_path_with(&mock, "tv", bad, false).await.is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(log.lock().unwrap().is_empty());

        let r = delete_path_with(&mock, "tv", "/sdcard/Download/it's.zip", false)
            .await
            .unwrap();
        assert!(r.ok, "{}", r.message);
        assert_eq!(
            log.lock().unwrap().as_slice(),
            [r"rm -rf '/sdcard/Download/it'\''s.zip'".to_string()]
        );
    }

    #[tokio::test]
    async fn delete_reports_device_side_failures() {
        let mock = MockAdb::default()
            .on_shell("noisy", "rm: /sdcard/noisy: Permission denied")
            .on_shell_exit("silent", "", 1)
            .on_shell_err("gone", "device offline");
        let noisy = delete_path_with(&mock, "tv", "/sdcard/noisy", false)
            .await
            .unwrap();
        assert!(!noisy.ok);
        assert!(noisy.message.contains("Permission denied"));
        let silent = delete_path_with(&mock, "tv", "/sdcard/silent", false)
            .await
            .unwrap();
        assert!(!silent.ok);
        assert!(delete_path_with(&mock, "tv", "/sdcard/gone", false)
            .await
            .is_err());
    }
}
