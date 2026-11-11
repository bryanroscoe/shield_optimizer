//! Import from the phone (`EXPORT-DISCOVERABILITY.md`, phase 2): Upload to TV in Files and
//! Install APK.
//!
//! Android's open-document picker (Storage Access Framework, through the atv-adb Kotlin plugin)
//! copies the documents the user picks into a fresh `<cache>/imports/import-<n>/` folder, under
//! [`MAX_IMPORT_FILES`] / [`MAX_IMPORT_FILE_BYTES`] / [`MAX_IMPORT_TOTAL_BYTES`]. No storage
//! permission and no persistable URI grant is involved, and no URI outlives its copy. The
//! frontend gets an opaque batch id, never a path, and every later step looks the batch up here.
//!
//! - **Upload to TV** pushes one staged file at a time into the current TV folder, `/sdcard`
//!   confined like delete. An existing file is replaced only after the user said so; a folder of
//!   the same name is never replaced. The pushed size is checked on the TV when it can be read.
//! - **Install APK** installs a staged `.apk` set with `install -r` / `install-multiple -r` and
//!   core's install check, the path Backups restore uses ([`install_local_apks`]).
//!
//! Staged copies are deleted when the batch is discarded or installed, after [`IMPORT_TTL`], and
//! at the first start of the process.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use atv_optimizer_core::commands::files::validate_sdcard_path;
use atv_optimizer_core::commands::AppState;
use atv_optimizer_core::engine::apk_abis;
use atv_optimizer_core::engine::install_outcome::decode_install_error;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_atv_adb::{AdbExt, PickedDocument};

use crate::file_commands::{install_local_apks, quote_path};

/// The folder under the app cache dir that holds staged imports.
pub const IMPORTS_DIR: &str = "imports";
const BATCH_PREFIX: &str = "import-";
/// At most this many documents per pick (a split APK set fits easily).
pub const MAX_IMPORT_FILES: u32 = 32;
/// One picked document, and all of one pick together: the same 2 GiB as one pull from the TV.
pub const MAX_IMPORT_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_IMPORT_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// A batch nobody used or discarded is dropped after this.
pub const IMPORT_TTL: Duration = Duration::from_secs(60 * 60);
/// The picker can stay open while the user browses, and the copy can be slow from a cloud
/// provider; past this the pick is given up and its folder removed.
const PICK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Longest staged name kept from a provider's display name, in bytes.
const MAX_NAME_BYTES: usize = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportPurpose {
    /// Files › Upload to TV: any file.
    Upload,
    /// Install APK: `.apk` files only.
    Apk,
}

/// One staged copy of a picked document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// One pick. Its folder is removed when the last reference goes, so a discard while an upload
/// is still reading a file waits for that upload.
#[derive(Debug)]
pub struct StagedBatch {
    dir: PathBuf,
    purpose: ImportPurpose,
    files: Vec<StagedFile>,
    created: Instant,
}

impl Drop for StagedBatch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The live batches, and one picker at a time.
#[derive(Default)]
pub struct ImportStore {
    batches: Mutex<HashMap<String, Arc<StagedBatch>>>,
    picking: AtomicBool,
    counter: AtomicU64,
}

pub struct PickBusy<'a>(&'a AtomicBool);

impl Drop for PickBusy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

const BATCH_GONE: &str = "Those picked files are no longer staged. Pick them again.";

impl ImportStore {
    fn try_begin_pick(&self) -> Option<PickBusy<'_>> {
        self.picking
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| PickBusy(&self.picking))
    }

    fn insert(&self, id: String, batch: StagedBatch) {
        self.lock().insert(id, Arc::new(batch));
    }

    fn get(&self, id: &str) -> Result<Arc<StagedBatch>, String> {
        self.lock()
            .get(id)
            .cloned()
            .ok_or_else(|| BATCH_GONE.into())
    }

    /// Remove a batch; its files go once nothing is using them.
    fn take(&self, id: &str) -> Option<Arc<StagedBatch>> {
        self.lock().remove(id)
    }

    /// Drop batches older than `ttl`.
    fn prune_expired(&self, ttl: Duration, now: Instant) -> usize {
        let mut batches = self.lock();
        let before = batches.len();
        batches.retain(|_, batch| now.saturating_duration_since(batch.created) < ttl);
        before - batches.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<StagedBatch>>> {
        self.batches
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// What the frontend sees of a staged file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportedFile {
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ImportPick {
    Picked {
        batch: String,
        files: Vec<ImportedFile>,
    },
    Canceled,
}

/// A provider's display name made safe as one file name: no path separators or control
/// characters, not `.`/`..`, bounded in length (keeping the extension). Empty becomes
/// `file-<n>`.
pub fn safe_import_name(display: &str, index: usize) -> String {
    let cleaned: String = display
        .chars()
        .map(|c| match c {
            '/' | '\\' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        return format!("file-{}", index + 1);
    }
    if cleaned.len() <= MAX_NAME_BYTES {
        return cleaned.to_string();
    }
    let ext = cleaned
        .rfind('.')
        .map(|i| &cleaned[i..])
        .filter(|ext| ext.len() <= 16)
        .unwrap_or("");
    let mut end = MAX_NAME_BYTES - ext.len();
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ext}", &cleaned[..end])
}

/// `name`, or `stem (n).ext` for the first `n` that no name in `taken` already uses.
fn distinct_name<'a>(name: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    let clash = |candidate: &str| taken.clone().any(|t| t.eq_ignore_ascii_case(candidate));
    if !clash(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !clash(candidate))
        .expect("an unused suffix exists")
}

fn is_apk_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("apk"))
}

/// Check what the picker wrote and give each copy its final name, `<NNN>-<safe name>`. Only
/// `<index>.part` regular files directly inside `dir` are accepted, with the size the picker
/// reported, so a reply can't point the import at another file.
pub fn stage_picked(
    dir: &Path,
    picked: &[PickedDocument],
    purpose: ImportPurpose,
) -> Result<Vec<StagedFile>, String> {
    if picked.is_empty() || picked.len() > MAX_IMPORT_FILES as usize {
        return Err(format!(
            "Pick between 1 and {MAX_IMPORT_FILES} files. Nothing was imported."
        ));
    }
    let root = dir
        .canonicalize()
        .map_err(|e| format!("open import folder: {e}"))?;
    let mut total: u64 = 0;
    let mut staged: Vec<StagedFile> = Vec::with_capacity(picked.len());
    for (index, document) in picked.iter().enumerate() {
        // Two picks with one name would land on the same TV path, so later ones get ` (2)`,
        // ` (3)`, … (compared without case, as `/sdcard` is).
        let name = distinct_name(
            &safe_import_name(&document.name, index),
            staged.iter().map(|f| f.name.as_str()),
        );
        if purpose == ImportPurpose::Apk && !is_apk_name(&name) {
            return Err(format!(
                "{name} isn't an .apk file. Pick only .apk files to install. Nothing was imported."
            ));
        }
        let expected = root.join(format!("{index}.part"));
        let reported = PathBuf::from(&document.file);
        let meta = std::fs::symlink_metadata(&reported)
            .map_err(|_| format!("The copy of {name} is missing. Nothing was imported."))?;
        if !meta.is_file()
            || reported.canonicalize().ok().as_deref() != Some(expected.as_path())
            || meta.len() != document.size
        {
            return Err(format!(
                "The copy of {name} doesn't match what was picked. Nothing was imported."
            ));
        }
        if meta.len() > MAX_IMPORT_FILE_BYTES {
            return Err(format!(
                "{name} is over the 2 GiB limit. Nothing was imported."
            ));
        }
        total = total.saturating_add(meta.len());
        if total > MAX_IMPORT_TOTAL_BYTES {
            return Err(
                "These files are over the 2 GiB limit together. Nothing was imported.".into(),
            );
        }
        if purpose == ImportPurpose::Apk && meta.len() == 0 {
            return Err(format!("{name} is empty. Nothing was imported."));
        }
        let path = root.join(format!("{index:03}-{name}"));
        std::fs::rename(&expected, &path).map_err(|e| format!("stage {name}: {e}"))?;
        staged.push(StagedFile {
            name,
            path,
            size_bytes: meta.len(),
        });
    }
    Ok(staged)
}

/// A fresh, empty `<imports>/import-<millis>-<n>` folder, and its name (the batch id).
fn new_batch_dir(imports: &Path, store: &ImportStore) -> Result<(String, PathBuf), String> {
    std::fs::create_dir_all(imports).map_err(|e| format!("prepare import: {e}"))?;
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    (0..100)
        .map(|_| {
            let n = store.counter.fetch_add(1, Ordering::Relaxed);
            format!("{BATCH_PREFIX}{stamp}-{n}")
        })
        .find_map(|id| {
            let dir = imports.join(&id);
            std::fs::create_dir(&dir).ok().map(|_| (id, dir))
        })
        .ok_or_else(|| "Couldn't prepare the import. Try again.".into())
}

/// Remove every staged import folder (`import-*`) under `imports`. Symlinks are never followed.
pub fn remove_staged_imports(imports: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(imports) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(BATCH_PREFIX))
        })
        .filter(|entry| std::fs::symlink_metadata(entry.path()).is_ok_and(|meta| meta.is_dir()))
        .filter(|entry| std::fs::remove_dir_all(entry.path()).is_ok())
        .count()
}

/// [`remove_staged_imports`] at the first start of this process only: on Android the entry
/// point can run again in a live process (activity recreation) while an import is in use.
pub fn clean_at_startup(cache_dir: &Path) -> Option<usize> {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let mut result = None;
    ONCE.call_once(|| result = Some(remove_staged_imports(&cache_dir.join(IMPORTS_DIR))));
    result
}

async fn blocking<T: Send + 'static>(
    timeout: Duration,
    late: &str,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        Ok(Err(join)) => Err(join.to_string()),
        Err(_) => Err(late.to_string()),
    }
}

/// `import_pick` — open Android's file picker and stage the documents picked. Nothing reaches
/// the TV here.
#[tauri::command]
pub async fn import_pick(
    app: AppHandle,
    store: State<'_, ImportStore>,
    purpose: ImportPurpose,
) -> Result<ImportPick, String> {
    let _busy = store
        .try_begin_pick()
        .ok_or("The file picker is already open.")?;
    store.prune_expired(IMPORT_TTL, Instant::now());
    let imports = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("prepare import: {e}"))?
        .join(IMPORTS_DIR);
    let (id, dir) = new_batch_dir(&imports, &store)?;
    // Removes the folder on every early return; disarmed once the batch owns it.
    let guard = DirGuard(Some(dir.clone()));
    let handle = app.clone();
    let target = dir.to_string_lossy().into_owned();
    let reply = blocking(
        PICK_TIMEOUT,
        "The file picker didn't finish in time. Nothing was imported.",
        move || {
            handle
                .adb()
                .pick_documents(
                    &target,
                    MAX_IMPORT_FILES,
                    MAX_IMPORT_FILE_BYTES,
                    MAX_IMPORT_TOTAL_BYTES,
                )
                .map_err(|e| e.to_string())
        },
    )
    .await?;
    if !reply.picked || reply.files.is_empty() {
        return Ok(ImportPick::Canceled);
    }
    let files = stage_picked(&dir, &reply.files, purpose)?;
    let shown = files
        .iter()
        .map(|f| ImportedFile {
            name: f.name.clone(),
            size_bytes: f.size_bytes,
        })
        .collect();
    guard.disarm();
    store.insert(
        id.clone(),
        StagedBatch {
            dir,
            purpose,
            files,
            created: Instant::now(),
        },
    );
    Ok(ImportPick::Picked {
        batch: id,
        files: shown,
    })
}

struct DirGuard(Option<PathBuf>);

impl DirGuard {
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        if let Some(dir) = &self.0 {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// `discard_import` — drop a staged batch. Unknown ids are fine: it may have expired.
#[tauri::command]
pub async fn discard_import(store: State<'_, ImportStore>, batch: String) -> Result<(), String> {
    store.take(&batch);
    Ok(())
}

/// `/sdcard`-confined `<dir>/<name>` for an upload: the same bound as delete, and never
/// `/sdcard` itself.
pub fn remote_target(remote_dir: &str, name: &str) -> Result<String, String> {
    let dir = validate_sdcard_path(remote_dir)?;
    let dir = dir.trim_end_matches('/');
    let dir = if dir.is_empty() { "/sdcard" } else { dir };
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(format!("Not a file name: {name:?}"));
    }
    let target = validate_sdcard_path(&format!("{dir}/{name}"))?;
    if target.trim_end_matches('/') == "/sdcard" {
        return Err("Pick a folder under /sdcard.".into());
    }
    Ok(target)
}

/// Something already on the TV where an upload would land.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UploadConflict {
    pub name: String,
    pub remote_path: String,
    /// A folder is never replaced; a file is replaced only when the user agrees.
    pub is_dir: bool,
}

/// One shell line that prints `d <path>` or `f <path>` for each target that exists.
fn existence_command(targets: &[String]) -> String {
    let quoted: Vec<String> = targets.iter().map(|t| quote_path(t)).collect();
    format!(
        "for f in {}; do if [ -d \"$f\" ]; then echo \"d $f\"; elif [ -e \"$f\" ] || [ -L \"$f\" ]; then echo \"f $f\"; fi; done",
        quoted.join(" ")
    )
}

/// The targets the existence check reported, as `(target, is_dir)`. Lines that aren't one of
/// the asked-for targets are ignored.
pub fn parse_existing(stdout: &str, targets: &[String]) -> Vec<(String, bool)> {
    stdout
        .lines()
        .filter_map(|line| {
            let (kind, path) = line.split_once(' ')?;
            let is_dir = match kind {
                "d" => true,
                "f" => false,
                _ => return None,
            };
            targets
                .iter()
                .find(|t| t.as_str() == path)
                .map(|t| (t.clone(), is_dir))
        })
        .collect()
}

async fn existing_on_tv(
    state: &AppState,
    serial: &str,
    targets: &[String],
) -> Result<Vec<(String, bool)>, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, &existence_command(targets))
        .await
        .map_err(|e| format!("check the TV folder: {e}"))?;
    Ok(parse_existing(&out.stdout, targets))
}

/// `upload_conflicts` — which picked files already exist in the TV folder.
#[tauri::command]
pub async fn upload_conflicts(
    state: State<'_, AppState>,
    store: State<'_, ImportStore>,
    serial: String,
    batch: String,
    remote_dir: String,
) -> Result<Vec<UploadConflict>, String> {
    upload_conflicts_impl(state.inner(), store.inner(), &serial, &batch, &remote_dir).await
}

pub async fn upload_conflicts_impl(
    state: &AppState,
    store: &ImportStore,
    serial: &str,
    batch: &str,
    remote_dir: &str,
) -> Result<Vec<UploadConflict>, String> {
    let batch = store.get(batch)?;
    let targets = batch
        .files
        .iter()
        .map(|f| remote_target(remote_dir, &f.name))
        .collect::<Result<Vec<_>, _>>()?;
    let existing = existing_on_tv(state, serial, &targets).await?;
    Ok(batch
        .files
        .iter()
        .zip(&targets)
        .filter_map(|(file, target)| {
            existing
                .iter()
                .find(|(path, _)| path == target)
                .map(|(_, is_dir)| UploadConflict {
                    name: file.name.clone(),
                    remote_path: target.clone(),
                    is_dir: *is_dir,
                })
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UploadOutcome {
    /// The TV accepted every byte. `verified` is true when the TV then reported the same size.
    Uploaded {
        name: String,
        remote_path: String,
        size_bytes: u64,
        verified: bool,
    },
    /// A file of that name is there and the user hasn't agreed to replace it. Nothing was sent.
    Exists { name: String, remote_path: String },
}

/// `upload_imported` — push one staged file into a TV folder.
// Keep the existing flat Tauri IPC arguments; the hardware identity is a required
// upload authorization bound, not an optional field hidden inside another argument.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn upload_imported(
    state: State<'_, AppState>,
    store: State<'_, ImportStore>,
    serial: String,
    expected_hardware_id: String,
    batch: String,
    index: usize,
    remote_dir: String,
    overwrite: bool,
) -> Result<UploadOutcome, String> {
    upload_imported_impl(
        state.inner(),
        store.inner(),
        &serial,
        &expected_hardware_id,
        &batch,
        index,
        &remote_dir,
        overwrite,
    )
    .await
}

// Mirrors the command boundary so tests exercise the same explicit request fields.
#[allow(clippy::too_many_arguments)]
pub async fn upload_imported_impl(
    state: &AppState,
    store: &ImportStore,
    serial: &str,
    expected_hardware_id: &str,
    batch: &str,
    index: usize,
    remote_dir: &str,
    overwrite: bool,
) -> Result<UploadOutcome, String> {
    let expected_hardware_id =
        atv_optimizer_core::commands::devices::verified_hardware_id(expected_hardware_id)
            .ok_or("The upload needs a verified TV hardware identity. Nothing was uploaded.")?;
    let batch = store.get(batch)?;
    let file = batch.files.get(index).ok_or(BATCH_GONE)?;
    let target = remote_target(remote_dir, &file.name)?;
    let adb = state.adb_snapshot().await;
    match adb
        .upload_file(
            serial,
            expected_hardware_id,
            &file.path,
            &target,
            file.size_bytes,
            overwrite,
        )
        .await
        .map_err(|error| error.to_string())?
    {
        atv_optimizer_core::adb::upload::UploadResult::Exists => Ok(UploadOutcome::Exists {
            name: file.name.clone(),
            remote_path: target,
        }),
        atv_optimizer_core::adb::upload::UploadResult::Uploaded { verified } => {
            Ok(UploadOutcome::Uploaded {
                name: file.name.clone(),
                remote_path: target,
                size_bytes: file.size_bytes,
                verified,
            })
        }
    }
}

/// What the staged APKs say about themselves before anything is installed. Not a signature
/// or safety check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApkSetInspection {
    pub files: Vec<ImportedFile>,
    /// Native-code ABIs across the set (`lib/<abi>/`). Empty with `abis_known` means none.
    pub abis: Vec<String>,
    /// False when an APK's ZIP directory couldn't be read; the ABIs are then not claimed.
    pub abis_known: bool,
    /// `ro.product.cpu.abilist`, when the TV answered.
    pub device_abis: Vec<String>,
    /// Core's rule: `Some(false)` only when both sides are known and nothing matches.
    pub abi_compatible: Option<bool>,
}

/// Read one APK's native ABIs from its ZIP central directory. `None` when it can't be read.
pub fn read_native_abis(path: &Path) -> Option<Vec<String>> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let tail_len = apk_abis::EOCD_SEARCH_BYTES.min(len);
    file.seek(SeekFrom::Start(len - tail_len)).ok()?;
    let mut tail = vec![0; tail_len as usize];
    file.read_exact(&mut tail).ok()?;
    let (offset, size) = apk_abis::central_directory_range(&tail, len)?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut directory = vec![0; size as usize];
    file.read_exact(&mut directory).ok()?;
    apk_abis::native_abis(&directory)
}

fn apk_batch(store: &ImportStore, batch: &str) -> Result<Arc<StagedBatch>, String> {
    let batch = store.get(batch)?;
    if batch.purpose != ImportPurpose::Apk || !batch.files.iter().all(|f| is_apk_name(&f.name)) {
        return Err("Those picked files aren't an APK set.".into());
    }
    Ok(batch)
}

/// `inspect_imported_apks` — the ABI check desktop's Install APK shows, for a staged set.
#[tauri::command]
pub async fn inspect_imported_apks(
    state: State<'_, AppState>,
    store: State<'_, ImportStore>,
    serial: String,
    batch: String,
) -> Result<ApkSetInspection, String> {
    inspect_imported_apks_impl(state.inner(), store.inner(), &serial, &batch).await
}

pub async fn inspect_imported_apks_impl(
    state: &AppState,
    store: &ImportStore,
    serial: &str,
    batch: &str,
) -> Result<ApkSetInspection, String> {
    let batch = apk_batch(store, batch)?;
    let paths: Vec<PathBuf> = batch.files.iter().map(|f| f.path.clone()).collect();
    let read = tokio::task::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| read_native_abis(p))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    let abis_known = read.iter().all(Option::is_some);
    let mut abis: Vec<String> = read.into_iter().flatten().flatten().collect();
    abis.sort();
    abis.dedup();
    let adb = state.adb_snapshot().await;
    let device_abis = match adb.shell(serial, "getprop ro.product.cpu.abilist").await {
        Ok(out) => apk_abis::parse_abilist(&out.stdout),
        Err(_) => Vec::new(),
    };
    let abi_compatible = if abis_known {
        apk_abis::abi_compatible(&abis, &device_abis)
    } else {
        None
    };
    Ok(ApkSetInspection {
        files: batch
            .files
            .iter()
            .map(|f| ImportedFile {
                name: f.name.clone(),
                size_bytes: f.size_bytes,
            })
            .collect(),
        abis,
        abis_known,
        device_abis,
        abi_compatible,
    })
}

/// The install's answer, as desktop's Install APK reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallApkResult {
    pub ok: bool,
    /// The package manager's reply (both streams).
    pub message: String,
    /// A plain-language reading of a known `INSTALL_FAILED_*` code.
    pub hint: Option<String>,
}

/// `install_imported_apks` — install a staged APK set on the TV. The batch is used up: its
/// files are deleted when the install ends, whatever the outcome.
#[tauri::command]
pub async fn install_imported_apks(
    state: State<'_, AppState>,
    store: State<'_, ImportStore>,
    serial: String,
    batch: String,
) -> Result<InstallApkResult, String> {
    install_imported_apks_impl(state.inner(), store.inner(), &serial, &batch).await
}

pub async fn install_imported_apks_impl(
    state: &AppState,
    store: &ImportStore,
    serial: &str,
    batch_id: &str,
) -> Result<InstallApkResult, String> {
    let batch = apk_batch(store, batch_id)?;
    store.take(batch_id);
    let paths: Vec<PathBuf> = batch.files.iter().map(|f| f.path.clone()).collect();
    let adb = state.adb_snapshot().await;
    let (ok, message) = install_local_apks(adb.as_ref(), serial, &paths)
        .await
        .map_err(|e| format!("install: {e}"))?;
    let hint = if ok {
        None
    } else {
        decode_install_error(&message)
    };
    Ok(InstallApkResult {
        ok,
        message: if !ok && message.is_empty() {
            "The TV did not confirm that the app was installed.".into()
        } else {
            message
        },
        hint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};

    fn output(stdout: &str) -> AdbResult<AdbOutput> {
        Ok(AdbOutput {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }

    /// Answers the existence check with `existing`, `stat` with `stat`, records every call,
    /// and replies to transfers with `transfer`.
    struct Tv {
        existing: String,
        stat: String,
        transfer: fn() -> AdbResult<AdbOutput>,
        abilist: String,
        mv_fails: bool,
        calls: Mutex<Vec<String>>,
    }

    impl Tv {
        fn new() -> Self {
            Tv {
                existing: String::new(),
                stat: String::new(),
                transfer: || output(""),
                abilist: "arm64-v8a,armeabi-v7a,armeabi\n".into(),
                mv_fails: false,
                calls: Mutex::default(),
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl AdbDriver for Tv {
        async fn upload_file(
            &self,
            serial: &str,
            _expected_hardware_id: &str,
            local: &Path,
            target: &str,
            size: u64,
            overwrite: bool,
        ) -> AdbResult<atv_optimizer_core::adb::upload::UploadResult> {
            struct Io<'a>(&'a Tv, &'a str);
            impl atv_optimizer_core::adb::upload::UploadIo for Io<'_> {
                fn shell(&mut self, command: &str) -> AdbResult<AdbOutput> {
                    self.0.calls.lock().unwrap().push(command.into());
                    if command.starts_with("case ") && !command.contains("cat ") {
                        output("atvopt-safe-shell")
                    } else if command.starts_with("if [ -d") {
                        output(if self.0.existing.starts_with("d ") {
                            "atvopt-directory"
                        } else if self.0.existing.starts_with("f ") {
                            "atvopt-exists"
                        } else {
                            "atvopt-absent"
                        })
                    } else if command.starts_with("mkdir ") {
                        output("atvopt-created")
                    } else if command.starts_with("stat ") {
                        output(&self.0.stat)
                    } else if command.starts_with("rm ") {
                        output("atvopt-clean")
                    } else {
                        output(if self.0.mv_fails {
                            ""
                        } else {
                            "atvopt-committed"
                        })
                    }
                }
                fn push(&mut self, local: &Path, remote: &str) -> AdbResult<()> {
                    self.0.calls.lock().unwrap().push(format!(
                        "-s {} push {} {remote}",
                        self.1,
                        local.display()
                    ));
                    (self.0.transfer)().map(|_| ())
                }
            }
            atv_optimizer_core::adb::upload::upload(
                &mut Io(self, serial),
                local,
                target,
                size,
                overwrite,
            )
        }
        async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            unreachable!()
        }
        async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
            self.calls.lock().unwrap().push(args.join(" "));
            (self.transfer)()
        }
        async fn shell(&self, _: &str, command: &str) -> AdbResult<AdbOutput> {
            self.calls.lock().unwrap().push(command.to_string());
            if command.starts_with("for f in") {
                output(&self.existing)
            } else if command.starts_with("stat -c") {
                output(&self.stat)
            } else if command.starts_with("mv -f") {
                output(if self.mv_fails {
                    "mv: Permission denied"
                } else {
                    "atvopt-moved\n"
                })
            } else if command.starts_with("rm -f") {
                output("atvopt-moved\n")
            } else if command == "getprop ro.product.cpu.abilist" {
                output(&self.abilist)
            } else {
                panic!("unexpected shell command: {command}")
            }
        }
    }

    /// A store holding one batch staged from `files` (name, bytes) the way `import_pick` does.
    fn staged(
        root: &Path,
        purpose: ImportPurpose,
        files: &[(&str, &[u8])],
    ) -> (ImportStore, String, PathBuf) {
        let store = ImportStore::default();
        let (id, dir) = new_batch_dir(&root.join(IMPORTS_DIR), &store).unwrap();
        let picked: Vec<PickedDocument> = files
            .iter()
            .enumerate()
            .map(|(i, (name, body))| {
                let file = dir.join(format!("{i}.part"));
                std::fs::write(&file, body).unwrap();
                PickedDocument {
                    name: name.to_string(),
                    size: body.len() as u64,
                    file: file.to_string_lossy().into_owned(),
                }
            })
            .collect();
        let files = stage_picked(&dir, &picked, purpose).unwrap();
        store.insert(
            id.clone(),
            StagedBatch {
                dir: dir.clone(),
                purpose,
                files,
                created: Instant::now(),
            },
        );
        (store, id, dir)
    }

    fn state(tv: Arc<Tv>, root: &Path) -> AppState {
        AppState::new(tv, Default::default(), root.to_path_buf())
    }

    #[test]
    fn display_names_become_one_safe_file_name() {
        assert_eq!(safe_import_name("movie.mkv", 0), "movie.mkv");
        assert_eq!(safe_import_name("../../etc/passwd", 0), "_.._etc_passwd");
        assert_eq!(safe_import_name("a/b\\c\n.txt", 0), "a_b_c_.txt");
        assert_eq!(safe_import_name("..", 2), "file-3");
        assert_eq!(safe_import_name("   ", 0), "file-1");
        assert_eq!(safe_import_name(".hidden", 0), "hidden");
        let long = format!("{}.apk", "é".repeat(200));
        let safe = safe_import_name(&long, 0);
        assert!(safe.len() <= MAX_NAME_BYTES, "{}", safe.len());
        assert!(safe.ends_with(".apk"));
    }

    #[test]
    fn staging_names_the_copies_and_refuses_anything_unexpected() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, dir) = staged(
            root.path(),
            ImportPurpose::Upload,
            &[("clip.mp4", b"video"), ("notes.txt", b"")],
        );
        let batch = store.get(&id).unwrap();
        assert_eq!(batch.files[0].name, "clip.mp4");
        assert_eq!(batch.files[0].size_bytes, 5);
        assert_eq!(batch.files[0].path.file_name().unwrap(), "000-clip.mp4");
        assert!(batch.files[1].path.starts_with(dir.canonicalize().unwrap()));

        let other = tempfile::tempdir().unwrap();
        let dir = other.path();
        std::fs::write(dir.join("0.part"), b"abc").unwrap();
        std::fs::write(dir.join("elsewhere"), b"abc").unwrap();
        let doc = |file: &Path, size| PickedDocument {
            name: "x.apk".into(),
            size,
            file: file.to_string_lossy().into_owned(),
        };
        // A file other than <index>.part, a wrong size, a missing copy.
        for picked in [
            doc(&dir.join("elsewhere"), 3),
            doc(&dir.join("0.part"), 4),
            doc(&dir.join("1.part"), 3),
        ] {
            assert!(stage_picked(dir, &[picked], ImportPurpose::Upload).is_err());
        }
        assert!(stage_picked(dir, &[], ImportPurpose::Upload).is_err());
        // Install accepts only non-empty .apk files.
        let named = |name: &str| PickedDocument {
            name: name.into(),
            size: 3,
            file: dir.join("0.part").to_string_lossy().into_owned(),
        };
        assert!(stage_picked(dir, &[named("photo.png")], ImportPurpose::Apk)
            .unwrap_err()
            .contains("isn't an .apk"));
        std::fs::write(dir.join("0.part"), b"").unwrap();
        let empty = PickedDocument {
            name: "base.APK".into(),
            size: 0,
            file: dir.join("0.part").to_string_lossy().into_owned(),
        };
        assert!(stage_picked(dir, &[empty], ImportPurpose::Apk)
            .unwrap_err()
            .contains("empty"));
    }

    #[test]
    fn picks_with_the_same_name_get_distinct_names() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, _) = staged(
            root.path(),
            ImportPurpose::Upload,
            &[
                ("notes.txt", b"a"),
                ("NOTES.txt", b"b"),
                ("notes.txt", b"c"),
                ("README", b"d"),
                ("readme", b"e"),
            ],
        );
        let names: Vec<String> = store
            .get(&id)
            .unwrap()
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(
            names,
            [
                "notes.txt",
                "NOTES (2).txt",
                "notes (3).txt",
                "README",
                "readme (2)"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_copy_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret"), b"key").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret"), dir.path().join("0.part")).unwrap();
        let picked = PickedDocument {
            name: "a.txt".into(),
            size: 3,
            file: dir.path().join("0.part").to_string_lossy().into_owned(),
        };
        assert!(stage_picked(dir.path(), &[picked], ImportPurpose::Upload).is_err());
    }

    #[test]
    fn a_batch_is_removed_on_discard_expiry_and_startup() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, dir) = staged(root.path(), ImportPurpose::Upload, &[("a.txt", b"a")]);
        // In use: the folder outlives the discard until the user lets go.
        let in_use = store.get(&id).unwrap();
        store.take(&id);
        assert!(dir.exists());
        drop(in_use);
        assert!(!dir.exists());
        assert_eq!(store.get(&id).unwrap_err(), BATCH_GONE);

        let (store, _, dir) = staged(root.path(), ImportPurpose::Upload, &[("b.txt", b"b")]);
        assert_eq!(store.prune_expired(IMPORT_TTL, Instant::now()), 0);
        assert!(dir.exists());
        assert_eq!(
            store.prune_expired(IMPORT_TTL, Instant::now() + IMPORT_TTL),
            1
        );
        assert!(!dir.exists());

        let imports = root.path().join(IMPORTS_DIR);
        std::fs::create_dir_all(imports.join("import-1-0")).unwrap();
        std::fs::write(imports.join("import-1-0/0.part"), b"x").unwrap();
        std::fs::create_dir_all(imports.join("keep")).unwrap();
        assert_eq!(remove_staged_imports(&imports), 1);
        assert!(imports.join("keep").exists());
        assert_eq!(remove_staged_imports(&root.path().join("missing")), 0);
    }

    #[test]
    fn startup_cleanup_runs_once_per_process() {
        let root = tempfile::tempdir().unwrap();
        let first = clean_at_startup(root.path());
        assert!(clean_at_startup(root.path()).is_none());
        // Another test may have used the process's one run; either way it ran at most once.
        assert!(first.is_none() || first == Some(0));
    }

    #[test]
    fn one_picker_at_a_time() {
        let store = ImportStore::default();
        let busy = store.try_begin_pick().unwrap();
        assert!(store.try_begin_pick().is_none());
        drop(busy);
        assert!(store.try_begin_pick().is_some());
    }

    #[test]
    fn uploads_stay_under_sdcard_like_delete() {
        assert_eq!(
            remote_target("/sdcard/Movies/", "clip.mp4").unwrap(),
            "/sdcard/Movies/clip.mp4"
        );
        assert_eq!(remote_target("/sdcard", "a.txt").unwrap(), "/sdcard/a.txt");
        for (dir, name) in [
            ("/storage/emulated/0", "a.txt"),
            ("/data/local/tmp", "a.txt"),
            ("/sdcard/../data", "a.txt"),
            ("/sdcard", ".."),
            ("/sdcard", ""),
            ("/sdcard", "a/b"),
            ("/sdcard", "a\nb"),
            ("relative", "a.txt"),
        ] {
            assert!(remote_target(dir, name).is_err(), "{dir} {name}");
        }
    }

    #[test]
    fn the_existence_check_reads_only_the_asked_for_targets() {
        let targets = vec!["/sdcard/a b.txt".to_string(), "/sdcard/dir".to_string()];
        let command = existence_command(&targets);
        assert!(command.contains("'/sdcard/a b.txt' '/sdcard/dir'"));
        assert_eq!(
            parse_existing(
                "f /sdcard/a b.txt\nd /sdcard/dir\nf /sdcard/other\nnoise\n",
                &targets
            ),
            vec![
                ("/sdcard/a b.txt".to_string(), false),
                ("/sdcard/dir".to_string(), true)
            ]
        );
    }

    #[tokio::test]
    async fn conflicts_name_existing_files_and_folders() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, _) = staged(
            root.path(),
            ImportPurpose::Upload,
            &[("a.txt", b"a"), ("b.txt", b"b"), ("Movies", b"c")],
        );
        let mut tv = Tv::new();
        tv.existing = "f /sdcard/a.txt\nd /sdcard/Movies\n".into();
        let state = state(Arc::new(tv), root.path());
        let conflicts = upload_conflicts_impl(&state, &store, "TV", &id, "/sdcard")
            .await
            .unwrap();
        assert_eq!(
            conflicts,
            vec![
                UploadConflict {
                    name: "a.txt".into(),
                    remote_path: "/sdcard/a.txt".into(),
                    is_dir: false
                },
                UploadConflict {
                    name: "Movies".into(),
                    remote_path: "/sdcard/Movies".into(),
                    is_dir: true
                },
            ]
        );
    }

    #[tokio::test]
    async fn an_upload_pushes_the_staged_copy_and_checks_its_size() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, _) = staged(
            root.path(),
            ImportPurpose::Upload,
            &[("clip.mp4", b"12345")],
        );
        let mut tv = Tv::new();
        tv.stat = "5\n".into();
        let tv = Arc::new(tv);
        let state = state(tv.clone(), root.path());
        let outcome = upload_imported_impl(
            &state,
            &store,
            "TV",
            "fixture-tv",
            &id,
            0,
            "/sdcard/Movies",
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            UploadOutcome::Uploaded {
                name: "clip.mp4".into(),
                remote_path: "/sdcard/Movies/clip.mp4".into(),
                size_bytes: 5,
                verified: true
            }
        );
        let calls = tv.calls();
        let push = calls.iter().position(|c| c.contains(" push ")).unwrap();
        assert!(calls[push].starts_with("-s TV push "));
        assert!(calls[push].contains("000-clip.mp4 /sdcard/Movies/.atvopt-upload-"));
        assert!(calls[push].ends_with("/payload"));
        assert!(calls[push + 1].starts_with("stat -c %s '/sdcard/Movies/.atvopt-upload-"));
        assert!(calls[push + 2].contains("( set -C;"));
        assert!(calls[push + 2].contains(" > '/sdcard/Movies/clip.mp4'"));
    }

    #[tokio::test]
    async fn an_existing_file_is_replaced_only_when_asked_and_a_folder_never() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, _) = staged(root.path(), ImportPurpose::Upload, &[("a.txt", b"a")]);
        let mut tv = Tv::new();
        tv.existing = "f /sdcard/a.txt\n".into();
        tv.stat = "1".into();
        let tv = Arc::new(tv);
        let state = state(tv.clone(), root.path());
        let outcome =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", false)
                .await
                .unwrap();
        assert!(matches!(outcome, UploadOutcome::Exists { .. }));
        assert!(!tv.calls().iter().any(|c| c.contains(" push ")));
        let outcome =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", true)
                .await
                .unwrap();
        assert!(matches!(
            outcome,
            UploadOutcome::Uploaded { verified: true, .. }
        ));

        let mut folder = Tv::new();
        folder.existing = "d /sdcard/a.txt\n".into();
        let folder = Arc::new(folder);
        let state = super::tests::state(folder.clone(), root.path());
        let error =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", true)
                .await
                .unwrap_err();
        assert!(error.contains("A folder named a.txt"), "{error}");
        assert!(!folder.calls().iter().any(|c| c.contains(" push ")));
    }

    #[tokio::test]
    async fn a_failed_short_or_unconfirmed_upload_is_reported_as_such() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, _) = staged(root.path(), ImportPurpose::Upload, &[("a.txt", b"abc")]);

        let mut short = Tv::new();
        short.stat = "2".into();
        let state = state(Arc::new(short), root.path());
        let error =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", false)
                .await
                .unwrap_err();
        assert!(error.contains("as 2 bytes instead of 3"), "{error}");
        assert!(
            error.contains("Nothing in the folder was replaced"),
            "{error}"
        );

        let mut unknown = Tv::new();
        unknown.stat = "stat: not found".into();
        let state = super::tests::state(Arc::new(unknown), root.path());
        let outcome =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", false)
                .await
                .unwrap();
        assert!(matches!(
            outcome,
            UploadOutcome::Uploaded {
                verified: false,
                ..
            }
        ));

        let mut uncertain = Tv::new();
        uncertain.transfer = || {
            Err(AdbError::Uncertain {
                operation: "file upload",
                detail: "the total deadline was reached".into(),
            })
        };
        let state = super::tests::state(Arc::new(uncertain), root.path());
        let error =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", false)
                .await
                .unwrap_err();
        assert!(error.contains("didn't finish uploading"), "{error}");
        assert!(
            error.contains("Nothing in the folder was replaced."),
            "{error}"
        );

        let mut stuck = Tv::new();
        stuck.stat = "3".into();
        stuck.mv_fails = true;
        let stuck = Arc::new(stuck);
        let state = super::tests::state(stuck.clone(), root.path());
        let error =
            upload_imported_impl(&state, &store, "TV", "fixture-tv", &id, 0, "/sdcard", true)
                .await
                .unwrap_err();
        assert!(
            error.contains("did not confirm putting the file in place"),
            "{error}"
        );
        assert!(stuck
            .calls()
            .iter()
            .any(|c| c.starts_with("rm -f '/sdcard/.atvopt-upload-")));

        let state = super::tests::state(Arc::new(Tv::new()), root.path());
        assert_eq!(
            upload_imported_impl(
                &state,
                &store,
                "TV",
                "fixture-tv",
                "import-unknown",
                0,
                "/sdcard",
                false
            )
            .await
            .unwrap_err(),
            BATCH_GONE
        );
        assert!(upload_imported_impl(
            &state,
            &store,
            "TV",
            "fixture-tv",
            &id,
            0,
            "/data/local/tmp",
            false
        )
        .await
        .is_err());
    }

    fn stored_zip(names: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for name in names {
            let offset = out.len() as u32;
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
            out.extend_from_slice(&[0; 22]);
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0; 2]);
            out.extend_from_slice(name.as_bytes());
            central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
            central.extend_from_slice(&[0; 24]);
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[tokio::test]
    async fn inspection_reads_abis_across_a_split_set_and_compares_with_the_tv() {
        let root = tempfile::tempdir().unwrap();
        let base = stored_zip(&["AndroidManifest.xml", "classes.dex"]);
        let split = stored_zip(&["lib/x86_64/libapp.so"]);
        let (store, id, _) = staged(
            root.path(),
            ImportPurpose::Apk,
            &[("base.apk", &base), ("split_config.x86_64.apk", &split)],
        );
        let state = state(Arc::new(Tv::new()), root.path());
        let inspection = inspect_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .unwrap();
        assert_eq!(inspection.abis, ["x86_64"]);
        assert!(inspection.abis_known);
        assert_eq!(
            inspection.device_abis,
            ["arm64-v8a", "armeabi-v7a", "armeabi"]
        );
        assert_eq!(inspection.abi_compatible, Some(false));
        assert_eq!(inspection.files.len(), 2);

        // An APK that isn't a readable ZIP: nothing is claimed.
        let (store, id, _) = staged(
            root.path(),
            ImportPurpose::Apk,
            &[("odd.apk", b"not a zip")],
        );
        let inspection = inspect_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .unwrap();
        assert!(!inspection.abis_known);
        assert_eq!(inspection.abi_compatible, None);

        // An upload batch is not an APK set.
        let (store, id, _) = staged(root.path(), ImportPurpose::Upload, &[("a.apk", b"x")]);
        assert!(inspect_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .is_err());
    }

    fn success() -> AdbResult<AdbOutput> {
        output("Success\n")
    }

    #[tokio::test]
    async fn install_uses_install_or_install_multiple_and_uses_up_the_batch() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, dir) = staged(
            root.path(),
            ImportPurpose::Apk,
            &[("base.apk", b"a"), ("split_config.arm64_v8a.apk", b"b")],
        );
        let mut tv = Tv::new();
        tv.transfer = success;
        let tv = Arc::new(tv);
        let state = state(tv.clone(), root.path());
        let result = install_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .unwrap();
        assert!(result.ok);
        assert_eq!(result.hint, None);
        let calls = tv.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].starts_with("-s TV install-multiple -r "));
        assert!(calls[0].contains("000-base.apk"));
        assert!(calls[0].contains("001-split_config.arm64_v8a.apk"));
        assert!(
            !dir.exists(),
            "the staged copies are deleted after the install"
        );
        assert_eq!(store.get(&id).unwrap_err(), BATCH_GONE);

        let (store, id, _) = staged(root.path(), ImportPurpose::Apk, &[("one.apk", b"a")]);
        install_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .unwrap();
        assert!(tv.calls()[1].starts_with("-s TV install -r "));
    }

    #[tokio::test]
    async fn a_refused_install_carries_the_reason_and_a_hint() {
        let root = tempfile::tempdir().unwrap();
        let (store, id, _) = staged(root.path(), ImportPurpose::Apk, &[("one.apk", b"a")]);
        let mut tv = Tv::new();
        tv.transfer = || {
            Err(AdbError::NonZeroExit {
                code: Some(1),
                stderr: "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]\n".into(),
            })
        };
        let state = state(Arc::new(tv), root.path());
        let result = install_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .unwrap();
        assert!(!result.ok);
        assert_eq!(result.message, "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]");
        assert!(result.hint.unwrap().contains("newer"));

        // Exit 0 without `Success` is not an install.
        let (store, id, _) = staged(root.path(), ImportPurpose::Apk, &[("one.apk", b"a")]);
        let mut silent = Tv::new();
        silent.transfer = || output("");
        let state = super::tests::state(Arc::new(silent), root.path());
        let result = install_imported_apks_impl(&state, &store, "TV", &id)
            .await
            .unwrap();
        assert!(!result.ok);
        assert_eq!(
            result.message,
            "The TV did not confirm that the app was installed."
        );
    }

    #[test]
    fn results_serialize_for_the_frontend() {
        assert_eq!(
            serde_json::to_value(ImportPick::Canceled).unwrap(),
            serde_json::json!({ "status": "canceled" })
        );
        assert_eq!(
            serde_json::to_value(UploadOutcome::Exists {
                name: "a".into(),
                remote_path: "/sdcard/a".into()
            })
            .unwrap(),
            serde_json::json!({ "status": "exists", "name": "a", "remote_path": "/sdcard/a" })
        );
        assert_eq!(
            serde_json::from_str::<ImportPurpose>(r#""apk""#).unwrap(),
            ImportPurpose::Apk
        );
    }
}
