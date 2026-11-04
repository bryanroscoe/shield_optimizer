//! Moving an existing install over from the Shield Optimizer names.
//!
//! Every release before 2.5.0 used the identifier `com.shieldoptimizer.app`
//! and the `ShieldOptimizer` data folder. The identifier decides where the
//! WebView keeps its profile (every frontend setting is in its localStorage),
//! so both have to be carried over or the update looks like a fresh install.
//!
//! The rules:
//! - Copy, never move or delete, so the old data stays for a retry or a
//!   downgrade.
//! - Build each copy in a temporary sibling, write a completion marker into
//!   it, then rename it into place. A folder without the marker is not a
//!   finished copy, whoever made it.
//! - If the data folder cannot be copied, run this session on the old one,
//!   so nothing creates the new folder and the next launch really retries.
//! - Copy the WebView profile once. The data folder records that it was done,
//!   so a profile the user or an uninstaller deletes later is not refilled
//!   with the old settings.
//! - Every failure is logged, never a reason not to start.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

/// The app data folder, under the OS's local data directory.
pub const DATA_DIR_NAME: &str = "ATVOptimizer";
pub const LEGACY_DATA_DIR_NAME: &str = "ShieldOptimizer";
/// `identifier` in `tauri.conf.json`.
pub const IDENTIFIER: &str = "com.atvoptimizer.app";
pub const LEGACY_IDENTIFIER: &str = "com.shieldoptimizer.app";

pub const ADB_ENV: &str = "ATV_OPTIMIZER_ADB";
pub const LEGACY_ADB_ENV: &str = "SHIELD_OPTIMIZER_ADB";
pub const SUBNET_ENV: &str = "ATV_OPTIMIZER_SUBNET";
pub const LEGACY_SUBNET_ENV: &str = "SHIELD_OPTIMIZER_SUBNET";

/// Written into a copy just before it is renamed into place.
const MARKER: &str = ".atvoptimizer-migrated";
/// Written into the data folder once the WebView profile has been copied.
const PROFILE_DONE: &str = ".webview-profile-migrated";

/// Daily log files written before the rename start with this.
const LEGACY_LOG_PREFIX: &str = "shield-optimizer.";

/// Not copied: a download the app can fetch again, which a running adb
/// server keeps open on Windows. The old copy stays usable where it is (see
/// `adb::install`). `native-adb/` holds the native ADB identity key and is
/// copied.
const DATA_DIR_SKIP: &[&str] = &["platform-tools"];

/// WebView profile entries that are caches or locks, at any depth. Chromium
/// (WebView2), leveldb and WebKitGTK rebuild them; copying them only costs
/// time and, for a lock file, can fail outright.
const PROFILE_SKIP: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "ShaderCache",
    "GrShaderCache",
    "GraphiteDawnCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "Crashpad",
    "lockfile",
    "LOCK",
    "WebKitCache",
];

const TEMP_SUFFIX: &str = ".migrating-";
const SET_ASIDE_SUFFIX: &str = ".unmigrated-";

/// Waits between tries of the final rename. Antivirus scanners and the
/// Windows indexer briefly hold new files open.
const RENAME_BACKOFF_MS: &[u64] = &[100, 200, 400, 800, 1600];
/// How long to wait before copying again when the source changed under us.
const RECOPY_DELAY: Duration = Duration::from_secs(1);

/// `name`, or the variable's pre-rename spelling when `name` is unset.
fn env_with_legacy(name: &str, legacy: &str) -> Option<OsString> {
    std::env::var_os(name).or_else(|| std::env::var_os(legacy))
}

/// The user's adb override: `ATV_OPTIMIZER_ADB`, else `SHIELD_OPTIMIZER_ADB`.
pub fn adb_override() -> Option<PathBuf> {
    env_with_legacy(ADB_ENV, LEGACY_ADB_ENV).map(PathBuf::from)
}

/// The user's scan subnet: `ATV_OPTIMIZER_SUBNET`, else the old name.
pub fn subnet_override() -> Option<String> {
    env_with_legacy(SUBNET_ENV, LEGACY_SUBNET_ENV).and_then(|v| v.into_string().ok())
}

static SESSION_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The data folder this session uses: the new one, or the old one when the
/// copy failed. Set once by `run()`.
pub fn set_session_data_dir(dir: PathBuf) {
    let _ = SESSION_DATA_DIR.set(dir);
}

/// The session's data folder, or the new one when `run()` has not chosen.
pub fn session_data_dir() -> Option<PathBuf> {
    SESSION_DATA_DIR
        .get()
        .cloned()
        .or_else(|| dirs::data_local_dir().map(|d| d.join(DATA_DIR_NAME)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Data,
    Profile,
}

/// Where the old and new copies live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub data_old: PathBuf,
    pub data_new: PathBuf,
    pub profile_old: PathBuf,
    pub profile_new: PathBuf,
}

/// The folders to carry over on `os`.
///
/// `data_local` is `dirs::data_local_dir()`: `~/Library/Application Support`,
/// `%LOCALAPPDATA%` or `$XDG_DATA_HOME` (`~/.local/share`). The WebView
/// profile follows the identifier. Tauri 2.11 points WebView2 and WebKitGTK at
/// `<data_local>/<identifier>` (WebView2 adds `EBWebView` inside it); WKWebView
/// on macOS keeps it in `~/Library/WebKit/<bundle identifier>`, with the HTTP
/// cache separately in `~/Library/Caches`, which is not copied.
pub fn layout(os: Os, data_local: &Path, home: &Path) -> Layout {
    let profile_root = match os {
        Os::Mac => home.join("Library").join("WebKit"),
        Os::Windows | Os::Linux => data_local.to_path_buf(),
    };
    Layout {
        data_old: data_local.join(LEGACY_DATA_DIR_NAME),
        data_new: data_local.join(DATA_DIR_NAME),
        profile_old: profile_root.join(LEGACY_IDENTIFIER),
        profile_new: profile_root.join(IDENTIFIER),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No old copy: a fresh install.
    NothingToMigrate,
    /// Done on an earlier launch; nothing was touched.
    AlreadyMigrated,
    Copied(Stats),
    /// The new data folder already existed without a completion marker. Old
    /// entries it lacked were copied in, and the old `telemetry.json` won.
    Merged(Stats),
    /// The new profile already existed without a completion marker (the
    /// WebView made it after a failed copy). It was renamed to `set_aside`
    /// and the old profile copied again.
    Recopied {
        set_aside: PathBuf,
        stats: Stats,
    },
    /// Nothing usable was created; the old data is unchanged and the next
    /// launch tries again. For the data folder this session uses the old one.
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: u64,
    pub bytes: u64,
    /// Skipped by rule, plus non-essential entries that could not be read.
    pub skipped: Vec<String>,
}

#[derive(Debug)]
pub struct Migration {
    /// The data folder this session should use.
    pub data_dir: PathBuf,
    pub data: (PathBuf, PathBuf, Outcome),
    pub profile: (PathBuf, PathBuf, Outcome),
}

impl Migration {
    /// Write the outcome to the log. Called once logging is up, which is
    /// after the migration so the logs land in the session's data folder.
    pub fn log(&self) {
        for (label, (old, new, outcome)) in [
            ("data folder", &self.data),
            ("webview profile", &self.profile),
        ] {
            match outcome {
                Outcome::NothingToMigrate | Outcome::AlreadyMigrated => {
                    tracing::debug!(item = label, outcome = ?outcome, "rename migration")
                }
                Outcome::Copied(stats) => tracing::info!(
                    item = label,
                    from = %old.display(),
                    to = %new.display(),
                    files = stats.files,
                    bytes = stats.bytes,
                    skipped = ?stats.skipped,
                    "rename migration copied"
                ),
                Outcome::Merged(stats) => tracing::warn!(
                    item = label,
                    from = %old.display(),
                    to = %new.display(),
                    files = stats.files,
                    skipped = ?stats.skipped,
                    "rename migration: the new folder existed without a completion marker; merged the old entries it lacked"
                ),
                Outcome::Recopied { set_aside, stats } => tracing::warn!(
                    item = label,
                    from = %old.display(),
                    to = %new.display(),
                    set_aside = %set_aside.display(),
                    files = stats.files,
                    skipped = ?stats.skipped,
                    "rename migration: the new profile existed without a completion marker; set it aside and copied again"
                ),
                Outcome::Failed(error) => tracing::warn!(
                    item = label,
                    from = %old.display(),
                    to = %new.display(),
                    session_data_dir = %self.data_dir.display(),
                    error = %error,
                    "rename migration failed; the old data is untouched and it will be tried again next launch"
                ),
            }
        }
    }
}

/// Carry the data folder, then the profile, over. Never panics and never
/// fails the launch.
pub fn migrate(layout: &Layout) -> Migration {
    let (data_outcome, data_dir) = migrate_data(&layout.data_old, &layout.data_new);
    let record = (data_dir == layout.data_new).then_some(data_dir.as_path());
    let profile_outcome = migrate_profile(&layout.profile_old, &layout.profile_new, record);
    Migration {
        data: (
            layout.data_old.clone(),
            layout.data_new.clone(),
            data_outcome,
        ),
        profile: (
            layout.profile_old.clone(),
            layout.profile_new.clone(),
            profile_outcome,
        ),
        data_dir,
    }
}

/// The host's folders, for `run()`.
pub fn migrate_host() -> Migration {
    let Some(data_local) = dirs::data_local_dir() else {
        let data_dir = PathBuf::from(".").join(DATA_DIR_NAME);
        return Migration {
            data: (PathBuf::new(), data_dir.clone(), Outcome::NothingToMigrate),
            profile: (PathBuf::new(), PathBuf::new(), Outcome::NothingToMigrate),
            data_dir,
        };
    };
    let home = dirs::home_dir().unwrap_or_else(|| data_local.clone());
    migrate(&layout(Os::host(), &data_local, &home))
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn marked(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

/// Returns the outcome and the data folder the session should use.
fn migrate_data(old: &Path, new: &Path) -> (Outcome, PathBuf) {
    if !old.is_dir() {
        return (Outcome::NothingToMigrate, new.to_path_buf());
    }
    if marked(new) {
        remove_stale_temps(new);
        return (Outcome::AlreadyMigrated, new.to_path_buf());
    }
    if exists(new) {
        return match merge_into(old, new) {
            Ok(stats) => (Outcome::Merged(stats), new.to_path_buf()),
            Err(e) => (Outcome::Failed(e.to_string()), old.to_path_buf()),
        };
    }
    match copy_fresh(Kind::Data, old, new) {
        Ok(stats) => (Outcome::Copied(stats), new.to_path_buf()),
        Err(e) => (Outcome::Failed(e.to_string()), old.to_path_buf()),
    }
}

/// `record` is the new data folder when this session uses it; the profile's
/// completion is noted there.
fn migrate_profile(old: &Path, new: &Path, record: Option<&Path>) -> Outcome {
    if record.is_some_and(|data| data.join(PROFILE_DONE).is_file()) {
        return Outcome::AlreadyMigrated;
    }
    if !old.is_dir() {
        return Outcome::NothingToMigrate;
    }
    let note_done = || {
        if let Some(data) = record {
            let _ = std::fs::create_dir_all(data);
            let _ = std::fs::write(data.join(PROFILE_DONE), marker_text(old));
        }
    };
    if marked(new) {
        remove_stale_temps(new);
        note_done();
        return Outcome::AlreadyMigrated;
    }
    let set_aside = if exists(new) {
        let aside = set_aside_path(new);
        if let Err(e) = rename_with_retry(new, &aside) {
            return Outcome::Failed(StepError::new("set aside", new, e).to_string());
        }
        Some(aside)
    } else {
        None
    };
    match copy_fresh(Kind::Profile, old, new) {
        Ok(stats) => {
            note_done();
            match set_aside {
                Some(set_aside) => Outcome::Recopied { set_aside, stats },
                None => Outcome::Copied(stats),
            }
        }
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

fn marker_text(old: &Path) -> String {
    format!(
        "copied from {} by ATV Optimizer {} at {}\n",
        old.display(),
        env!("CARGO_PKG_VERSION"),
        chrono::Utc::now().to_rfc3339()
    )
}

fn set_aside_path(new: &Path) -> PathBuf {
    let name = new
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut candidate = new.with_file_name(format!("{name}{SET_ASIDE_SUFFIX}{stamp}"));
    let mut n = 2;
    while exists(&candidate) {
        candidate = new.with_file_name(format!("{name}{SET_ASIDE_SUFFIX}{stamp}-{n}"));
        n += 1;
    }
    candidate
}

/// Copy `old` to a new `new`, through a temp sibling, retrying once if the
/// source changed during the copy.
fn copy_fresh(kind: Kind, old: &Path, new: &Path) -> Result<Stats, StepError> {
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StepError::new("create", parent, e))?;
    }
    let temp = temp_path(new, std::process::id());
    let mut last = None;
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(RECOPY_DELAY);
        }
        match copy_once(kind, old, new, &temp) {
            Ok(stats) => {
                remove_stale_temps(new);
                return Ok(stats);
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&temp);
                let retry = e.step == "verify";
                last = Some(e);
                if !retry {
                    break;
                }
            }
        }
    }
    Err(last.expect("at least one attempt"))
}

fn copy_once(kind: Kind, old: &Path, new: &Path, temp: &Path) -> Result<Stats, StepError> {
    // Only this process's own leftover: another process could be mid-copy in
    // a temp of its own.
    if exists(temp) {
        std::fs::remove_dir_all(temp).map_err(|e| StepError::new("clear", temp, e))?;
    }
    let before = fingerprint(kind, old);
    let mut stats = Stats::default();
    std::fs::create_dir(temp).map_err(|e| StepError::new("create", temp, e))?;
    copy_dir_permissions(old, temp);
    copy_tree(kind, old, temp, Path::new(""), false, &mut stats)?;
    if fingerprint(kind, old) != before {
        return Err(StepError::new(
            "verify",
            old,
            io::Error::other("the old folder changed while it was being copied"),
        ));
    }
    std::fs::write(temp.join(MARKER), marker_text(old))
        .map_err(|e| StepError::new("mark", temp, e))?;
    rename_with_retry(temp, new).map_err(|e| StepError::new("rename", temp, e))?;
    Ok(stats)
}

/// Fill a new data folder that exists without a marker: copy the old entries
/// it lacks, take the old `telemetry.json` (so the install id and the user's
/// opt-out survive), and mark it done.
fn merge_into(old: &Path, new: &Path) -> Result<Stats, StepError> {
    let before = fingerprint(Kind::Data, old);
    let mut stats = Stats::default();
    copy_tree(Kind::Data, old, new, Path::new(""), true, &mut stats)?;
    let telemetry = old.join("telemetry.json");
    if telemetry.is_file() {
        let staged = new.join(format!("telemetry.json{TEMP_SUFFIX}{}", std::process::id()));
        std::fs::copy(&telemetry, &staged).map_err(|e| StepError::new("copy", &telemetry, e))?;
        keep_modified_time(&telemetry, &staged);
        rename_with_retry(&staged, &new.join("telemetry.json")).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            StepError::new("rename", &staged, e)
        })?;
    }
    if fingerprint(Kind::Data, old) != before {
        return Err(StepError::new(
            "verify",
            old,
            io::Error::other("the old folder changed while it was being merged"),
        ));
    }
    std::fs::write(new.join(MARKER), marker_text(old))
        .map_err(|e| StepError::new("mark", new, e))?;
    Ok(stats)
}

fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut delays = RENAME_BACKOFF_MS.iter();
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) => match delays.next() {
                Some(ms) => std::thread::sleep(Duration::from_millis(*ms)),
                None => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
}

/// Access denied or a sharing violation (Windows error 32): a scanner has
/// the file open for a moment.
fn is_transient(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied || (cfg!(windows) && e.raw_os_error() == Some(32))
}

#[derive(Debug)]
pub struct StepError {
    step: &'static str,
    path: PathBuf,
    source: io::Error,
}

impl StepError {
    fn new(step: &'static str, path: &Path, source: io::Error) -> Self {
        Self {
            step,
            path: path.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.step, self.path.display(), self.source)
    }
}

fn is_skipped(kind: Kind, rel: &Path, name: &str) -> bool {
    match kind {
        Kind::Data => rel.as_os_str().is_empty() && DATA_DIR_SKIP.contains(&name),
        Kind::Profile => PROFILE_SKIP.contains(&name),
    }
}

/// What the copy cannot do without: losing any of these loses the user's
/// snapshots, report id and opt-out, launcher bookkeeping or settings.
fn is_essential(kind: Kind, rel: &Path) -> bool {
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    match kind {
        Kind::Data => match parts.first().map(String::as_str) {
            Some("snapshots") => true,
            Some("telemetry.json" | "disabled-home-handlers.json") => parts.len() == 1,
            _ => false,
        },
        Kind::Profile => {
            parts.iter().any(|p| {
                let p = p.to_ascii_lowercase();
                p == "localstorage" || p == "local storage"
            }) || parts.ends_with(&["WebsiteData".into(), "Default".into(), "salt".into()])
        }
    }
}

/// Every file to be copied, with its size and modification time.
fn fingerprint(kind: Kind, root: &Path) -> BTreeMap<PathBuf, (u64, Option<SystemTime>)> {
    fn walk(
        kind: Kind,
        root: &Path,
        rel: &Path,
        out: &mut BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
    ) {
        let Ok(entries) = std::fs::read_dir(root.join(rel)) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if is_skipped(kind, rel, &name.to_string_lossy()) {
                continue;
            }
            let child = rel.join(&name);
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                walk(kind, root, &child, out);
            } else if meta.is_file() {
                out.insert(child, (meta.len(), meta.modified().ok()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(kind, root, Path::new(""), &mut out);
    out
}

/// Copy the directory `from_root/rel` into `to_root/rel'`. In merge mode an
/// existing destination file is kept and an existing directory is merged
/// into. Symlinks and special files are skipped rather than followed, so
/// nothing outside the old folder is read. A failure on an essential entry
/// fails the copy; any other is skipped and listed.
fn copy_tree(
    kind: Kind,
    from_root: &Path,
    to_root: &Path,
    rel: &Path,
    merge: bool,
    stats: &mut Stats,
) -> Result<(), StepError> {
    let from = from_root.join(rel);
    let entries = match std::fs::read_dir(&from) {
        Ok(entries) => entries,
        Err(e) => return tolerate(kind, rel, StepError::new("read", &from, e), stats),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tolerate(kind, rel, StepError::new("read", &from, e), stats)?;
                continue;
            }
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy().into_owned();
        let child = rel.join(&name);
        if is_skipped(kind, rel, &name_str) {
            stats.skipped.push(display(&child));
            continue;
        }
        let src = entry.path();
        let dst = to_root.join(rel).join(dest_name(kind, rel, &name));
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                tolerate(kind, &child, StepError::new("stat", &src, e), stats)?;
                continue;
            }
        };
        if file_type.is_dir() {
            if !(merge && dst.is_dir()) {
                if let Err(e) = std::fs::create_dir(&dst) {
                    tolerate(kind, &child, StepError::new("create", &dst, e), stats)?;
                    continue;
                }
                copy_dir_permissions(&src, &dst);
            }
            copy_tree(kind, from_root, to_root, &child, merge, stats)?;
        } else if file_type.is_file() {
            if merge && exists(&dst) {
                continue;
            }
            match std::fs::copy(&src, &dst) {
                Ok(bytes) => {
                    keep_modified_time(&src, &dst);
                    stats.files += 1;
                    stats.bytes += bytes;
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&dst);
                    tolerate(kind, &child, StepError::new("copy", &src, e), stats)?;
                }
            }
        } else {
            stats.skipped.push(display(&child));
        }
    }
    Ok(())
}

/// Fail on an essential entry; skip and list anything else.
fn tolerate(kind: Kind, rel: &Path, error: StepError, stats: &mut Stats) -> Result<(), StepError> {
    if is_essential(kind, rel) || rel.as_os_str().is_empty() {
        Err(error)
    } else {
        stats
            .skipped
            .push(format!("{} ({})", display(rel), error.source));
        Ok(())
    }
}

/// `logs/shield-optimizer.2026-11-04.log` is copied as
/// `logs/atv-optimizer.2026-11-04.log`, so the appender's keep-seven rotation
/// covers the old files too.
fn dest_name(kind: Kind, rel: &Path, name: &std::ffi::OsStr) -> OsString {
    let in_logs =
        kind == Kind::Data && rel == Path::new(crate::commands::diagnostics::LOG_DIR_NAME);
    match name
        .to_str()
        .and_then(|n| n.strip_prefix(LEGACY_LOG_PREFIX))
    {
        Some(rest) if in_logs => {
            format!("{}.{rest}", crate::commands::diagnostics::LOG_FILE_PREFIX).into()
        }
        _ => name.to_os_string(),
    }
}

/// `native-adb/` is private to its owner; a copy must not be readable by
/// anyone the original was not.
fn copy_dir_permissions(src: &Path, dst: &Path) {
    if let Ok(meta) = std::fs::metadata(src) {
        let _ = std::fs::set_permissions(dst, meta.permissions());
    }
}

/// The newest-log lookup in diagnostics goes by modification time, and on
/// Linux a copy would otherwise make every old log look brand new.
fn keep_modified_time(src: &Path, dst: &Path) {
    let Ok(modified) = std::fs::metadata(src).and_then(|m| m.modified()) else {
        return;
    };
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(dst) {
        let _ = file.set_modified(modified);
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn temp_path(new: &Path, pid: u32) -> PathBuf {
    let name = new
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    new.with_file_name(format!("{name}{TEMP_SUFFIX}{pid}"))
}

/// Temps another launch left behind when it crashed. Only removed once the
/// new folder is complete, since until then one may belong to a launch that
/// is still copying.
fn remove_stale_temps(new: &Path) {
    let (Some(parent), Some(name)) = (new.parent(), new.file_name()) else {
        return;
    };
    let prefix = format!("{}{TEMP_SUFFIX}", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(pid) = entry_name.to_str().and_then(|n| n.strip_prefix(&prefix)) else {
            continue;
        };
        if !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// What happened to a macOS bundle still named for the old product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleRename {
    /// The executable is not inside `X.app/Contents/MacOS`.
    NotABundle,
    /// The bundle already has the new name, or one the user chose.
    NotLegacyName,
    /// Gatekeeper runs quarantined apps from a randomized read-only copy.
    Translocated,
    /// Homebrew records this bundle by its old name; renaming it would make
    /// `brew upgrade` fail to find it. Brew replaces it on upgrade instead.
    HomebrewManaged,
    SiblingExists(PathBuf),
    /// The folder holding the bundle is not writable by this user.
    NotWritable,
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },
    Failed(String),
}

pub const LEGACY_BUNDLE_NAME: &str = "Shield Optimizer.app";
pub const BUNDLE_NAME: &str = "ATV Optimizer.app";

/// Where the host's Homebrew lives (Apple silicon, then Intel). A GUI app
/// does not see `HOMEBREW_PREFIX`.
pub const CASKROOMS: &[&str] = &["/opt/homebrew/Caskroom", "/usr/local/Caskroom"];

/// Rename `Shield Optimizer.app` to `ATV Optimizer.app` beside it.
///
/// Runs as the app exits, never at startup: the running process keeps its
/// launch path for `current_exe`, resources and the updater, so renaming
/// under it would break all three until the next launch. Not run on the way
/// to an update restart, which relaunches from the old path.
pub fn rename_legacy_bundle(exe: &Path, home: &Path, caskrooms: &[&Path]) -> BundleRename {
    let Some(bundle) = bundle_of(exe) else {
        return BundleRename::NotABundle;
    };
    if bundle.file_name().and_then(|n| n.to_str()) != Some(LEGACY_BUNDLE_NAME) {
        return BundleRename::NotLegacyName;
    }
    if bundle
        .components()
        .any(|c| c.as_os_str() == "AppTranslocation")
    {
        return BundleRename::Translocated;
    }
    let Some(parent) = bundle.parent() else {
        return BundleRename::NotABundle;
    };
    if homebrew_manages(parent, home, caskrooms) {
        return BundleRename::HomebrewManaged;
    }
    let target = parent.join(BUNDLE_NAME);
    if std::fs::symlink_metadata(&target).is_ok() {
        return BundleRename::SiblingExists(target);
    }
    // The rename is the writability check: it is atomic, so a folder this
    // user cannot write leaves everything exactly as it was.
    match std::fs::rename(&bundle, &target) {
        Ok(()) => BundleRename::Renamed {
            from: bundle,
            to: target,
        },
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(30) => {
            BundleRename::NotWritable
        }
        Err(e) => BundleRename::Failed(e.to_string()),
    }
}

fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let named = |p: &Path, n: &str| p.file_name().and_then(|f| f.to_str()) == Some(n);
    let is_app = bundle
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("app"));
    (named(macos, "MacOS") && named(contents, "Contents") && is_app).then(|| bundle.to_path_buf())
}

/// A cask for this app is installed and the bundle sits where casks put apps.
fn homebrew_manages(parent: &Path, home: &Path, caskrooms: &[&Path]) -> bool {
    let in_appdir = parent == Path::new("/Applications") || parent == home.join("Applications");
    in_appdir
        && caskrooms.iter().any(|root| {
            std::fs::read_dir(root).is_ok_and(|entries| {
                entries.flatten().any(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("shield-optimizer") || name.starts_with("atv-optimizer")
                })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    /// Every file under `dir`, relative, with its contents.
    fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                let rel = display(path.strip_prefix(root).unwrap());
                if entry.file_type().unwrap().is_dir() {
                    out.push((rel + "/", Vec::new()));
                    walk(root, &path, out);
                } else {
                    out.push((rel, std::fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    fn host(root: &Path) -> Layout {
        layout(Os::Mac, &root.join("data"), &root.join("home"))
    }

    const OLD_TELEMETRY: &str = "{\"enabled\":false,\"install_id\":\"abc\"}";
    const MAC_STORE: &str = "WebsiteData/Default/h/h/LocalStorage/localstorage.sqlite3";

    fn old_install(l: &Layout) {
        let old = &l.data_old;
        write(
            &old.join("snapshots/living-room.json"),
            "{\"schema_version\":3}",
        );
        write(&old.join("telemetry.json"), OLD_TELEMETRY);
        write(&old.join("support-tiers.json"), "{}");
        write(&old.join("disabled-home-handlers.json"), "{}");
        write(&old.join("screenshots/TV.png"), "png");
        write(
            &old.join("logs/shield-optimizer.2026-11-03.log"),
            "old log\n",
        );
        write(&old.join("logs/sessions/s.jsonl"), "{}\n");
        write(&old.join("platform-tools/adb"), "binary");
        write(&old.join("native-adb/identity.pkcs8.pem"), "key");
        write(&l.profile_old.join(MAC_STORE), "settings");
        write(&l.profile_old.join("WebsiteData/Default/salt"), "salt");
    }

    /// What the app writes into a data folder on a launch that did not copy.
    fn fresh_app_data(dir: &Path) {
        write(
            &dir.join("telemetry.json"),
            "{\"enabled\":true,\"install_id\":\"new\"}",
        );
        write(&dir.join("logs/atv-optimizer.2026-11-04.log"), "new log\n");
    }

    fn no_temps(root: &Path) -> bool {
        std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().contains(TEMP_SUFFIX))
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    /// Root reads everything, so permission-based failures cannot be staged.
    #[cfg(unix)]
    fn unreadable(path: &Path) -> bool {
        set_mode(path, 0o000);
        std::fs::read(path).is_err()
    }

    #[test]
    fn the_identifier_matches_the_tauri_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], IDENTIFIER);
    }

    #[test]
    fn layouts_follow_each_os() {
        let data = Path::new("/d");
        let home = Path::new("/h");
        let mac = layout(Os::Mac, data, home);
        assert_eq!(mac.data_old, Path::new("/d/ShieldOptimizer"));
        assert_eq!(mac.data_new, Path::new("/d/ATVOptimizer"));
        assert_eq!(
            mac.profile_old,
            Path::new("/h/Library/WebKit/com.shieldoptimizer.app")
        );
        assert_eq!(
            mac.profile_new,
            Path::new("/h/Library/WebKit/com.atvoptimizer.app")
        );
        for os in [Os::Windows, Os::Linux] {
            let other = layout(os, data, home);
            assert_eq!(other.profile_old, Path::new("/d/com.shieldoptimizer.app"));
            assert_eq!(other.profile_new, Path::new("/d/com.atvoptimizer.app"));
        }
    }

    #[test]
    fn a_fresh_install_has_nothing_to_migrate() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        let m = migrate(&l);
        assert_eq!(m.data.2, Outcome::NothingToMigrate);
        assert_eq!(m.profile.2, Outcome::NothingToMigrate);
        assert_eq!(m.data_dir, l.data_new);
        assert!(!l.data_new.exists());
        assert!(!l.profile_new.exists());
    }

    #[test]
    fn an_old_install_is_copied_marked_and_left_untouched() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        let before = (tree(&l.data_old), tree(&l.profile_old));

        let m = migrate(&l);

        let Outcome::Copied(stats) = &m.data.2 else {
            panic!("{:?}", m.data.2);
        };
        assert_eq!(stats.files, 8);
        assert_eq!(stats.skipped, ["platform-tools"]);
        assert!(
            matches!(m.profile.2, Outcome::Copied(_)),
            "{:?}",
            m.profile.2
        );
        assert_eq!(m.data_dir, l.data_new);
        assert_eq!((tree(&l.data_old), tree(&l.profile_old)), before);

        let new = &l.data_new;
        assert_eq!(read(&new.join("telemetry.json")), OLD_TELEMETRY);
        for kept in [
            "snapshots/living-room.json",
            "support-tiers.json",
            "disabled-home-handlers.json",
            "screenshots/TV.png",
            "logs/sessions/s.jsonl",
            "logs/atv-optimizer.2026-11-03.log",
            "native-adb/identity.pkcs8.pem",
            MARKER,
            PROFILE_DONE,
        ] {
            assert!(new.join(kept).is_file(), "{kept} missing");
        }
        assert!(!new.join("logs/shield-optimizer.2026-11-03.log").exists());
        assert!(!new.join("platform-tools").exists());
        assert_eq!(read(&l.profile_new.join(MAC_STORE)), "settings");
        assert!(l.profile_new.join(MARKER).is_file());
        assert!(no_temps(&root.path().join("data")));
    }

    #[test]
    fn a_marked_copy_is_never_touched_again() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        migrate(&l);
        write(&l.data_new.join("telemetry.json"), "{\"changed\":true}");
        let before = (tree(&l.data_new), tree(&l.profile_new));

        let m = migrate(&l);

        assert_eq!(m.data.2, Outcome::AlreadyMigrated);
        assert_eq!(m.profile.2, Outcome::AlreadyMigrated);
        assert_eq!((tree(&l.data_new), tree(&l.profile_new)), before);
    }

    #[test]
    fn a_deleted_profile_is_not_refilled_once_migrated() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        migrate(&l);
        // "Delete app data", or the user clearing settings by hand.
        std::fs::remove_dir_all(&l.profile_new).unwrap();

        let m = migrate(&l);

        assert_eq!(m.profile.2, Outcome::AlreadyMigrated);
        assert!(!l.profile_new.exists(), "old settings came back");
    }

    #[test]
    fn an_unmarked_data_folder_is_merged_and_keeps_the_old_report_id() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        fresh_app_data(&l.data_new);

        let m = migrate(&l);

        assert!(matches!(m.data.2, Outcome::Merged(_)), "{:?}", m.data.2);
        assert_eq!(m.data_dir, l.data_new);
        let new = &l.data_new;
        // The old opt-out and install id win over the fresh defaults.
        assert_eq!(read(&new.join("telemetry.json")), OLD_TELEMETRY);
        assert_eq!(
            read(&new.join("logs/atv-optimizer.2026-11-04.log")),
            "new log\n"
        );
        assert!(new.join("logs/atv-optimizer.2026-11-03.log").is_file());
        assert!(new.join("snapshots/living-room.json").is_file());
        assert!(new.join(MARKER).is_file());
        assert!(no_temps(new));
        assert_eq!(migrate(&l).data.2, Outcome::AlreadyMigrated);
    }

    #[test]
    fn an_unmarked_profile_is_set_aside_and_copied_again() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        // The WebView made this after an earlier copy failed.
        write(&l.profile_new.join("WebsiteData/Default/salt"), "fresh");

        let m = migrate(&l);

        let Outcome::Recopied { set_aside, .. } = &m.profile.2 else {
            panic!("{:?}", m.profile.2);
        };
        assert_eq!(read(&set_aside.join("WebsiteData/Default/salt")), "fresh");
        assert_eq!(read(&l.profile_new.join(MAC_STORE)), "settings");
        assert_eq!(
            read(&l.profile_new.join("WebsiteData/Default/salt")),
            "salt"
        );
        assert!(l.data_new.join(PROFILE_DONE).is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_data_copy_runs_the_session_on_the_old_folder_and_retries() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        let snapshot = l.data_old.join("snapshots/living-room.json");
        if !unreadable(&snapshot) {
            return;
        }

        let m = migrate(&l);

        assert!(matches!(m.data.2, Outcome::Failed(_)), "{:?}", m.data.2);
        assert_eq!(
            m.data_dir, l.data_old,
            "the session must use the old folder"
        );
        assert!(!l.data_new.exists());
        assert!(no_temps(&root.path().join("data")));
        assert_eq!(read(&l.data_old.join("telemetry.json")), OLD_TELEMETRY);
        // The profile is copied, but not recorded in the old folder.
        assert!(
            matches!(m.profile.2, Outcome::Copied(_)),
            "{:?}",
            m.profile.2
        );
        assert!(!l.data_old.join(PROFILE_DONE).exists());

        set_mode(&snapshot, 0o644);
        let m = migrate(&l);
        assert!(matches!(m.data.2, Outcome::Copied(_)), "{:?}", m.data.2);
        assert_eq!(m.data_dir, l.data_new);
        assert_eq!(read(&l.data_new.join("telemetry.json")), OLD_TELEMETRY);
        // The profile's own marker is noted in the new folder now.
        assert_eq!(m.profile.2, Outcome::AlreadyMigrated);
        assert!(l.data_new.join(PROFILE_DONE).is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_profile_copy_is_retried_after_the_webview_made_a_fresh_one() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        let store = l.profile_old.join(MAC_STORE);
        if !unreadable(&store) {
            return;
        }

        let m = migrate(&l);
        assert!(
            matches!(m.profile.2, Outcome::Failed(_)),
            "{:?}",
            m.profile.2
        );
        assert!(!l.profile_new.exists());
        assert!(!l.data_new.join(PROFILE_DONE).exists());
        // What WebKit does on that launch: start an empty profile.
        write(&l.profile_new.join("WebsiteData/Default/salt"), "fresh");

        set_mode(&store, 0o644);
        let m = migrate(&l);
        assert!(
            matches!(m.profile.2, Outcome::Recopied { .. }),
            "{:?}",
            m.profile.2
        );
        assert_eq!(read(&l.profile_new.join(MAC_STORE)), "settings");
        assert!(l.data_new.join(PROFILE_DONE).is_file());
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_extra_file_is_skipped_not_fatal() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        let log = l.data_old.join("logs/shield-optimizer.2026-11-03.log");
        if !unreadable(&log) {
            return;
        }

        let m = migrate(&l);

        set_mode(&log, 0o644);
        let Outcome::Copied(stats) = &m.data.2 else {
            panic!("{:?}", m.data.2);
        };
        assert!(
            stats
                .skipped
                .iter()
                .any(|s| s.starts_with("logs/shield-optimizer.2026-11-03.log (")),
            "{:?}",
            stats.skipped
        );
        assert!(l.data_new.join("snapshots/living-room.json").is_file());
        assert!(!l
            .data_new
            .join("logs/atv-optimizer.2026-11-03.log")
            .exists());
    }

    #[test]
    fn essentials_are_named() {
        for rel in [
            "snapshots",
            "snapshots/a.json",
            "telemetry.json",
            "disabled-home-handlers.json",
        ] {
            assert!(is_essential(Kind::Data, Path::new(rel)), "{rel}");
        }
        for rel in [
            "logs/x.log",
            "screenshots/a.png",
            "native-adb/key",
            "x/telemetry.json",
        ] {
            assert!(!is_essential(Kind::Data, Path::new(rel)), "{rel}");
        }
        for rel in [
            "EBWebView/Default/Local Storage/leveldb/000003.log",
            "WebsiteData/Default/h/h/LocalStorage/localstorage.sqlite3",
            "WebsiteData/Default/salt",
            "localstorage/tauri_localhost_0.localstorage",
        ] {
            assert!(is_essential(Kind::Profile, Path::new(rel)), "{rel}");
        }
        assert!(!is_essential(
            Kind::Profile,
            Path::new("EBWebView/Default/Preferences")
        ));
    }

    #[test]
    fn copied_files_keep_their_modification_time() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        let log = l.data_old.join("logs/shield-optimizer.2026-11-03.log");
        let then =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&log)
            .unwrap()
            .set_modified(then)
            .unwrap();

        migrate(&l);

        let copied = l.data_new.join("logs/atv-optimizer.2026-11-03.log");
        assert_eq!(std::fs::metadata(copied).unwrap().modified().unwrap(), then);
    }

    #[test]
    fn a_change_to_the_old_folder_shows_in_its_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        let before = fingerprint(Kind::Data, &l.data_old);
        assert!(!before.keys().any(|k| k.starts_with("platform-tools")));
        write(
            &l.data_old.join("logs/shield-optimizer.2026-11-03.log"),
            "old log, longer\n",
        );
        assert_ne!(fingerprint(Kind::Data, &l.data_old), before);
    }

    #[test]
    fn a_temp_left_by_a_crash_does_not_block_the_retry() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        // A crashed launch, and this process's own pid from an earlier run.
        let crashed = temp_path(&l.data_new, 999_999);
        write(&crashed.join("snapshots/half.json"), "{");
        let own = temp_path(&l.data_new, std::process::id());
        write(&own.join("partial"), "x");

        let m = migrate(&l);

        assert!(matches!(m.data.2, Outcome::Copied(_)), "{:?}", m.data.2);
        assert!(!l.data_new.join("snapshots/half.json").exists());
        assert!(!l.data_new.join("partial").exists());
        assert!(l.data_new.join("snapshots/living-room.json").is_file());
        assert!(no_temps(&root.path().join("data")));
    }

    #[test]
    fn a_stale_temp_is_cleared_once_the_new_folder_is_done() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        migrate(&l);
        write(&temp_path(&l.data_new, 4242).join("x"), "x");
        // Not ours: a different suffix is never removed.
        let notes = root.path().join("data/ATVOptimizer.migrating-notes/x");
        write(&notes, "x");

        assert_eq!(migrate(&l).data.2, Outcome::AlreadyMigrated);
        assert!(!temp_path(&l.data_new, 4242).exists());
        assert!(notes.exists());
    }

    #[test]
    fn the_profile_copy_skips_caches_and_locks_at_any_depth() {
        let root = tempfile::tempdir().unwrap();
        let l = layout(Os::Windows, &root.path().join("local"), root.path());
        let ebwebview = l.profile_old.join("EBWebView");
        write(
            &ebwebview.join("Default/Local Storage/leveldb/000003.log"),
            "ls",
        );
        write(&ebwebview.join("Default/Local Storage/leveldb/LOCK"), "");
        write(&ebwebview.join("Default/Cache/Cache_Data/data_0"), "cache");
        write(&ebwebview.join("Default/Code Cache/js/index"), "cache");
        write(&ebwebview.join("GrShaderCache/data_0"), "cache");
        write(&ebwebview.join("lockfile"), "");
        write(&ebwebview.join("Local State"), "{}");

        let Outcome::Copied(stats) = migrate(&l).profile.2 else {
            panic!("profile not copied");
        };
        assert_eq!(stats.files, 2);
        let new = l.profile_new.join("EBWebView");
        assert!(new
            .join("Default/Local Storage/leveldb/000003.log")
            .is_file());
        assert!(new.join("Local State").is_file());
        for gone in [
            "Default/Local Storage/leveldb/LOCK",
            "Default/Cache",
            "Default/Code Cache",
            "GrShaderCache",
            "lockfile",
        ] {
            assert!(!new.join(gone).exists(), "{gone} copied");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_and_private_folders_stay_private() {
        let root = tempfile::tempdir().unwrap();
        let l = host(root.path());
        old_install(&l);
        write(&root.path().join("elsewhere/secret"), "outside");
        std::os::unix::fs::symlink(root.path().join("elsewhere"), l.data_old.join("link")).unwrap();
        set_mode(&l.data_old.join("native-adb"), 0o700);

        let Outcome::Copied(stats) = migrate(&l).data.2 else {
            panic!("data not copied");
        };

        assert!(stats.skipped.contains(&"link".to_string()));
        assert!(!l.data_new.join("link").exists());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(l.data_new.join("native-adb"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_rename_onto_a_missing_parent_is_not_retried_forever() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("a");
        std::fs::create_dir(&from).unwrap();
        let started = std::time::Instant::now();
        assert!(rename_with_retry(&from, &root.path().join("missing/b")).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    fn fake_bundle(parent: &Path, name: &str) -> PathBuf {
        let exe = parent.join(name).join("Contents/MacOS/atv-optimizer");
        write(&exe, "bin");
        exe
    }

    #[test]
    fn an_old_bundle_is_renamed_beside_itself() {
        let root = tempfile::tempdir().unwrap();
        let exe = fake_bundle(root.path(), LEGACY_BUNDLE_NAME);

        let outcome = rename_legacy_bundle(&exe, root.path(), &[]);

        let new = root.path().join(BUNDLE_NAME);
        assert_eq!(
            outcome,
            BundleRename::Renamed {
                from: root.path().join(LEGACY_BUNDLE_NAME),
                to: new.clone(),
            }
        );
        assert!(new.join("Contents/MacOS/atv-optimizer").is_file());
        assert!(!root.path().join(LEGACY_BUNDLE_NAME).exists());
    }

    #[test]
    fn a_bundle_is_left_alone_when_the_new_name_is_taken() {
        let root = tempfile::tempdir().unwrap();
        let exe = fake_bundle(root.path(), LEGACY_BUNDLE_NAME);
        fake_bundle(root.path(), BUNDLE_NAME);

        assert_eq!(
            rename_legacy_bundle(&exe, root.path(), &[]),
            BundleRename::SiblingExists(root.path().join(BUNDLE_NAME))
        );
        assert!(exe.is_file());
    }

    #[test]
    fn other_bundles_and_layouts_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let renamed = fake_bundle(root.path(), BUNDLE_NAME);
        let custom = fake_bundle(root.path(), "My Copy.app");
        assert_eq!(
            rename_legacy_bundle(&renamed, root.path(), &[]),
            BundleRename::NotLegacyName
        );
        assert_eq!(
            rename_legacy_bundle(&custom, root.path(), &[]),
            BundleRename::NotLegacyName
        );
        assert_eq!(
            rename_legacy_bundle(
                &root.path().join("target/debug/atv-optimizer"),
                root.path(),
                &[]
            ),
            BundleRename::NotABundle
        );
    }

    #[test]
    fn a_translocated_bundle_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let exe = fake_bundle(
            &root.path().join("AppTranslocation/1234-ABCD/d"),
            LEGACY_BUNDLE_NAME,
        );
        assert_eq!(
            rename_legacy_bundle(&exe, root.path(), &[]),
            BundleRename::Translocated
        );
        assert!(exe.is_file());
    }

    #[test]
    fn a_homebrew_bundle_in_the_appdir_is_left_for_brew() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let exe = fake_bundle(&home.join("Applications"), LEGACY_BUNDLE_NAME);
        let caskroom = root.path().join("Caskroom");
        std::fs::create_dir_all(caskroom.join("shield-optimizer@2.0.0")).unwrap();

        assert_eq!(
            rename_legacy_bundle(&exe, &home, &[&caskroom]),
            BundleRename::HomebrewManaged
        );
        assert!(exe.is_file());

        // The same cask, but the bundle was copied somewhere brew does not
        // manage: renamed.
        let elsewhere = fake_bundle(&root.path().join("Downloads"), LEGACY_BUNDLE_NAME);
        assert!(matches!(
            rename_legacy_bundle(&elsewhere, &home, &[&caskroom]),
            BundleRename::Renamed { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_folder_is_reported_and_unchanged() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("ro");
        let exe = fake_bundle(&parent, LEGACY_BUNDLE_NAME);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();

        let outcome = rename_legacy_bundle(&exe, root.path(), &[]);

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        if outcome == BundleRename::NotWritable {
            assert!(exe.is_file());
        } else {
            // Root can write anywhere.
            assert!(
                matches!(outcome, BundleRename::Renamed { .. }),
                "{outcome:?}"
            );
        }
    }
}
