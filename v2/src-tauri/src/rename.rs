//! Moving an existing install over from the Shield Optimizer names.
//!
//! Every release before this one used the identifier `com.shieldoptimizer.app`
//! and the `ShieldOptimizer` data folder. The identifier decides where the
//! WebView keeps its profile (every frontend setting is in its localStorage),
//! so both have to be carried over or the update looks like a fresh install.
//!
//! The rules: act only when the old copy exists and the new one does not;
//! copy, never move or delete, so the old data stays for a retry or a
//! downgrade; build each copy in a temporary sibling and rename it into place,
//! so a crash never leaves a half-filled folder that would block the retry;
//! and treat every failure as something to log, never a reason not to start.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

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

/// Daily log files written before the rename start with this.
const LEGACY_LOG_PREFIX: &str = "shield-optimizer.";

/// Data-folder entries that are not copied. Both are downloads the app can
/// fetch again, and a running adb server keeps files open in them on Windows.
/// The old platform-tools stay usable where they are (see `adb::install`).
const DATA_DIR_SKIP: &[&str] = &["platform-tools", "native-adb"];

/// WebView profile entries that are caches or locks, at any depth. Chromium
/// (WebView2) and WebKitGTK rebuild them; copying them only costs time and,
/// for a lock file, can fail outright.
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
    "WebKitCache",
];

const TEMP_SUFFIX: &str = ".migrating-";

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

/// One folder to carry over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: &'static str,
    pub old: PathBuf,
    pub new: PathBuf,
    /// Skipped when found directly inside `old`.
    skip_top: &'static [&'static str],
    /// Skipped wherever they are found.
    skip_any: &'static [&'static str],
    /// Rename `logs/shield-optimizer.*` so log rotation keeps counting them.
    rename_logs: bool,
}

/// The folders to carry over on `os`.
///
/// `data_local` is `dirs::data_local_dir()`: `~/Library/Application Support`,
/// `%LOCALAPPDATA%` or `$XDG_DATA_HOME` (`~/.local/share`). The WebView
/// profile follows the identifier. Tauri 2.11 points WebView2 and WebKitGTK at
/// `<data_local>/<identifier>` (WebView2 adds `EBWebView` inside it); WKWebView
/// on macOS keeps it in `~/Library/WebKit/<bundle identifier>`, with the HTTP
/// cache separately in `~/Library/Caches`, which is not copied.
pub fn items(os: Os, data_local: &Path, home: &Path) -> Vec<Item> {
    let profile_root = match os {
        Os::Mac => home.join("Library").join("WebKit"),
        Os::Windows | Os::Linux => data_local.to_path_buf(),
    };
    vec![
        Item {
            label: "data folder",
            old: data_local.join(LEGACY_DATA_DIR_NAME),
            new: data_local.join(DATA_DIR_NAME),
            skip_top: DATA_DIR_SKIP,
            skip_any: &[],
            rename_logs: true,
        },
        Item {
            label: "webview profile",
            old: profile_root.join(LEGACY_IDENTIFIER),
            new: profile_root.join(IDENTIFIER),
            skip_top: &[],
            skip_any: PROFILE_SKIP,
            rename_logs: false,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No old copy: a fresh install.
    NothingToMigrate,
    /// The new folder exists already; nothing was touched.
    AlreadyPresent,
    Copied {
        files: u64,
        bytes: u64,
        skipped: Vec<String>,
    },
    /// The new folder was not created; the old one is unchanged.
    Failed(String),
}

#[derive(Debug, Default)]
pub struct Report {
    pub entries: Vec<(Item, Outcome)>,
}

impl Report {
    /// Write the report to the log. Called once logging is up, which is
    /// after the migration so the logs land in the new folder.
    pub fn log(&self) {
        for (item, outcome) in &self.entries {
            match outcome {
                Outcome::NothingToMigrate | Outcome::AlreadyPresent => tracing::debug!(
                    item = item.label,
                    outcome = ?outcome,
                    "rename migration"
                ),
                Outcome::Copied {
                    files,
                    bytes,
                    skipped,
                } => tracing::info!(
                    item = item.label,
                    from = %item.old.display(),
                    to = %item.new.display(),
                    files,
                    bytes,
                    skipped = ?skipped,
                    "rename migration copied"
                ),
                Outcome::Failed(error) => tracing::warn!(
                    item = item.label,
                    from = %item.old.display(),
                    to = %item.new.display(),
                    error = %error,
                    "rename migration failed; the old data is untouched and it will be tried again next launch"
                ),
            }
        }
    }
}

/// Carry every item over. Never panics and never fails the launch.
pub fn migrate(items: &[Item]) -> Report {
    Report {
        entries: items
            .iter()
            .map(|item| (item.clone(), migrate_item(item)))
            .collect(),
    }
}

/// The host's folders, for `run()`.
pub fn migrate_host() -> Report {
    match (dirs::data_local_dir(), dirs::home_dir()) {
        (Some(data_local), Some(home)) => migrate(&items(Os::host(), &data_local, &home)),
        _ => Report::default(),
    }
}

fn migrate_item(item: &Item) -> Outcome {
    if !item.old.is_dir() {
        return Outcome::NothingToMigrate;
    }
    if std::fs::symlink_metadata(&item.new).is_ok() {
        remove_stale_temps(&item.new);
        return Outcome::AlreadyPresent;
    }
    let temp = temp_path(&item.new, std::process::id());
    match copy_into_place(item, &temp) {
        Ok(stats) => {
            remove_stale_temps(&item.new);
            Outcome::Copied {
                files: stats.files,
                bytes: stats.bytes,
                skipped: stats.skipped,
            }
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp);
            Outcome::Failed(e.to_string())
        }
    }
}

fn copy_into_place(item: &Item, temp: &Path) -> Result<Stats, StepError> {
    if let Some(parent) = item.new.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StepError::new("create", parent, e))?;
    }
    // Only this process's own leftover: another process could be mid-copy in
    // a temp of its own.
    if std::fs::symlink_metadata(temp).is_ok() {
        std::fs::remove_dir_all(temp).map_err(|e| StepError::new("clear", temp, e))?;
    }
    let mut stats = Stats::default();
    std::fs::create_dir(temp).map_err(|e| StepError::new("create", temp, e))?;
    copy_tree(&item.old, temp, item, true, &mut stats)?;
    if item.rename_logs {
        rename_legacy_logs(&temp.join(crate::commands::diagnostics::LOG_DIR_NAME));
    }
    std::fs::rename(temp, &item.new).map_err(|e| StepError::new("rename", temp, e))?;
    Ok(stats)
}

#[derive(Debug, Default)]
struct Stats {
    files: u64,
    bytes: u64,
    skipped: Vec<String>,
}

#[derive(Debug)]
struct StepError {
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

/// Copy `from` into the existing directory `to`. Symlinks and special files
/// are skipped rather than followed, so nothing outside `from` is read.
fn copy_tree(
    from: &Path,
    to: &Path,
    item: &Item,
    top: bool,
    stats: &mut Stats,
) -> Result<(), StepError> {
    let entries = std::fs::read_dir(from).map_err(|e| StepError::new("read", from, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| StepError::new("read", from, e))?;
        let name = entry.file_name();
        let src = entry.path();
        let dst = to.join(&name);
        let name_str = name.to_string_lossy();
        if (top && item.skip_top.iter().any(|s| *s == name_str))
            || item.skip_any.iter().any(|s| *s == name_str)
        {
            stats.skipped.push(display_rel(&item.old, &src));
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|e| StepError::new("stat", &src, e))?;
        if file_type.is_dir() {
            std::fs::create_dir(&dst).map_err(|e| StepError::new("create", &dst, e))?;
            copy_tree(&src, &dst, item, false, stats)?;
        } else if file_type.is_file() {
            let bytes = std::fs::copy(&src, &dst).map_err(|e| StepError::new("copy", &src, e))?;
            keep_modified_time(&src, &dst);
            stats.files += 1;
            stats.bytes += bytes;
        } else {
            stats.skipped.push(display_rel(&item.old, &src));
        }
    }
    Ok(())
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

fn display_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// `shield-optimizer.2026-11-04.log` becomes `atv-optimizer.2026-11-04.log`,
/// so the appender's keep-seven rotation covers the old files too. A name
/// already taken keeps the old spelling.
fn rename_legacy_logs(logs: &Path) {
    let Ok(entries) = std::fs::read_dir(logs) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(rest) = name
            .to_str()
            .and_then(|n| n.strip_prefix(LEGACY_LOG_PREFIX))
        else {
            continue;
        };
        let renamed = logs.join(format!(
            "{}.{rest}",
            crate::commands::diagnostics::LOG_FILE_PREFIX
        ));
        if std::fs::symlink_metadata(&renamed).is_err() {
            let _ = std::fs::rename(entry.path(), renamed);
        }
    }
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

    fn data_item(root: &Path) -> Item {
        items(Os::Linux, root, root).remove(0)
    }

    /// Every file under `dir`, relative, with its contents.
    fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    out.push((display_rel(root, &path) + "/", Vec::new()));
                    walk(root, &path, out);
                } else {
                    out.push((display_rel(root, &path), std::fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    fn old_install(root: &Path) -> PathBuf {
        let old = root.join(LEGACY_DATA_DIR_NAME);
        write(
            &old.join("snapshots/living-room.json"),
            "{\"schema_version\":3}",
        );
        write(&old.join("telemetry.json"), "{\"install_id\":\"abc\"}");
        write(&old.join("support-tiers.json"), "{}");
        write(&old.join("disabled-home-handlers.json"), "{}");
        write(&old.join("screenshots/TV.png"), "png");
        write(
            &old.join("logs/shield-optimizer.2026-11-03.log"),
            "old log\n",
        );
        write(&old.join("logs/sessions/s.jsonl"), "{}\n");
        write(&old.join("platform-tools/adb"), "binary");
        write(&old.join("native-adb/key"), "secret");
        old
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
        let mac = items(Os::Mac, data, home);
        assert_eq!(mac[0].old, Path::new("/d/ShieldOptimizer"));
        assert_eq!(mac[0].new, Path::new("/d/ATVOptimizer"));
        assert_eq!(
            mac[1].old,
            Path::new("/h/Library/WebKit/com.shieldoptimizer.app")
        );
        assert_eq!(
            mac[1].new,
            Path::new("/h/Library/WebKit/com.atvoptimizer.app")
        );
        for os in [Os::Windows, Os::Linux] {
            let other = items(os, data, home);
            assert_eq!(other[1].old, Path::new("/d/com.shieldoptimizer.app"));
            assert_eq!(other[1].new, Path::new("/d/com.atvoptimizer.app"));
        }
    }

    #[test]
    fn a_fresh_install_has_nothing_to_migrate() {
        let root = tempfile::tempdir().unwrap();
        let report = migrate(&items(Os::Linux, root.path(), root.path()));
        assert!(report
            .entries
            .iter()
            .all(|(_, o)| *o == Outcome::NothingToMigrate));
        assert!(!root.path().join(DATA_DIR_NAME).exists());
    }

    #[test]
    fn an_old_install_is_copied_and_left_untouched() {
        let root = tempfile::tempdir().unwrap();
        let old = old_install(root.path());
        let before = tree(&old);

        let outcome = migrate_item(&data_item(root.path()));

        let Outcome::Copied { files, skipped, .. } = &outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(*files, 7);
        let mut skipped = skipped.clone();
        skipped.sort();
        assert_eq!(skipped, ["native-adb", "platform-tools"]);
        assert_eq!(tree(&old), before, "the old folder changed");

        let new = root.path().join(DATA_DIR_NAME);
        assert_eq!(
            std::fs::read_to_string(new.join("telemetry.json")).unwrap(),
            "{\"install_id\":\"abc\"}"
        );
        for kept in [
            "snapshots/living-room.json",
            "support-tiers.json",
            "disabled-home-handlers.json",
            "screenshots/TV.png",
            "logs/sessions/s.jsonl",
            "logs/atv-optimizer.2026-11-03.log",
        ] {
            assert!(new.join(kept).is_file(), "{kept} missing");
        }
        assert!(!new.join("logs/shield-optimizer.2026-11-03.log").exists());
        assert!(!new.join("platform-tools").exists());
        assert!(!new.join("native-adb").exists());
        assert!(no_temps(root.path()));
    }

    #[test]
    fn copied_files_keep_their_modification_time() {
        let root = tempfile::tempdir().unwrap();
        let old = old_install(root.path());
        let log = old.join("logs/shield-optimizer.2026-11-03.log");
        let then =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&log)
            .unwrap()
            .set_modified(then)
            .unwrap();

        migrate_item(&data_item(root.path()));

        let copied = root
            .path()
            .join(DATA_DIR_NAME)
            .join("logs/atv-optimizer.2026-11-03.log");
        assert_eq!(std::fs::metadata(copied).unwrap().modified().unwrap(), then);
    }

    #[test]
    fn nothing_happens_when_both_exist() {
        let root = tempfile::tempdir().unwrap();
        old_install(root.path());
        let new = root.path().join(DATA_DIR_NAME);
        write(&new.join("telemetry.json"), "{\"install_id\":\"new\"}");
        let before = tree(&new);

        assert_eq!(
            migrate_item(&data_item(root.path())),
            Outcome::AlreadyPresent
        );
        assert_eq!(tree(&new), before);
    }

    #[test]
    fn a_temp_left_by_a_crash_does_not_block_the_retry() {
        let root = tempfile::tempdir().unwrap();
        old_install(root.path());
        let new = root.path().join(DATA_DIR_NAME);
        // A crashed launch, and this process's own pid from an earlier run.
        let crashed = temp_path(&new, 999_999);
        write(&crashed.join("snapshots/half.json"), "{");
        let own = temp_path(&new, std::process::id());
        write(&own.join("partial"), "x");

        let outcome = migrate_item(&data_item(root.path()));

        assert!(matches!(outcome, Outcome::Copied { .. }), "{outcome:?}");
        assert!(!new.join("snapshots/half.json").exists());
        assert!(!new.join("partial").exists());
        assert!(new.join("snapshots/living-room.json").is_file());
        assert!(no_temps(root.path()));
    }

    #[test]
    fn a_stale_temp_is_cleared_once_the_new_folder_exists() {
        let root = tempfile::tempdir().unwrap();
        old_install(root.path());
        let new = root.path().join(DATA_DIR_NAME);
        std::fs::create_dir_all(&new).unwrap();
        write(&temp_path(&new, 4242).join("x"), "x");
        // Not ours: a different suffix is never removed.
        write(&root.path().join("ATVOptimizer.migrating-notes/x"), "x");

        assert_eq!(
            migrate_item(&data_item(root.path())),
            Outcome::AlreadyPresent
        );
        assert!(!temp_path(&new, 4242).exists());
        assert!(root.path().join("ATVOptimizer.migrating-notes/x").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_failure_leaves_no_new_folder_and_the_old_one_intact() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let old = old_install(root.path());
        let locked = old.join("snapshots/living-room.json");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&locked).is_ok() {
            // Running as root: permissions cannot make the copy fail.
            return;
        }

        let outcome = migrate_item(&data_item(root.path()));

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        assert!(!root.path().join(DATA_DIR_NAME).exists());
        assert!(no_temps(root.path()));
        assert_eq!(
            std::fs::read_to_string(old.join("telemetry.json")).unwrap(),
            "{\"install_id\":\"abc\"}"
        );
        assert!(old.join("platform-tools/adb").is_file());

        // The next launch, with the problem gone, succeeds.
        assert!(matches!(
            migrate_item(&data_item(root.path())),
            Outcome::Copied { .. }
        ));
    }

    #[test]
    fn the_profile_copy_skips_caches_and_locks_at_any_depth() {
        let root = tempfile::tempdir().unwrap();
        let item = items(Os::Windows, root.path(), root.path()).remove(1);
        let ebwebview = item.old.join("EBWebView");
        write(
            &ebwebview.join("Default/Local Storage/leveldb/000003.log"),
            "ls",
        );
        write(&ebwebview.join("Default/Cache/Cache_Data/data_0"), "cache");
        write(&ebwebview.join("Default/Code Cache/js/index"), "cache");
        write(&ebwebview.join("GrShaderCache/data_0"), "cache");
        write(&ebwebview.join("lockfile"), "");
        write(&ebwebview.join("Local State"), "{}");

        let outcome = migrate_item(&item);

        let Outcome::Copied { files, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(files, 2);
        let new = item.new.join("EBWebView");
        assert!(new
            .join("Default/Local Storage/leveldb/000003.log")
            .is_file());
        assert!(new.join("Local State").is_file());
        assert!(!new.join("Default/Cache").exists());
        assert!(!new.join("Default/Code Cache").exists());
        assert!(!new.join("GrShaderCache").exists());
        assert!(!new.join("lockfile").exists());
    }

    #[test]
    fn mac_profile_is_copied_whole() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let item = items(Os::Mac, &root.path().join("data"), &home).remove(1);
        let ls = "WebsiteData/Default/abc/abc/LocalStorage/localstorage.sqlite3";
        write(&item.old.join(ls), "sqlite");
        write(&item.old.join("WebsiteData/Default/salt"), "salt");

        assert!(matches!(
            migrate_item(&item),
            Outcome::Copied { files: 2, .. }
        ));
        assert_eq!(
            std::fs::read_to_string(item.new.join(ls)).unwrap(),
            "sqlite"
        );
        assert!(home.join("Library/WebKit/com.atvoptimizer.app").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let old = old_install(root.path());
        write(&root.path().join("elsewhere/secret"), "outside");
        std::os::unix::fs::symlink(root.path().join("elsewhere"), old.join("link")).unwrap();

        let outcome = migrate_item(&data_item(root.path()));

        let Outcome::Copied { skipped, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert!(skipped.contains(&"link".to_string()));
        assert!(!root.path().join(DATA_DIR_NAME).join("link").exists());
    }

    fn no_temps(root: &Path) -> bool {
        std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().contains(TEMP_SUFFIX))
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
