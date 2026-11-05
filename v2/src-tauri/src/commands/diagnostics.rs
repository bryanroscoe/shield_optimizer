//! Logging to disk, and the bug-report bundle.
//!
//! A packaged app has nowhere for stdout to go, so without a file every
//! `tracing` line is lost on the machines where it matters.
//! [`init_logging`] adds a rolling daily file under the app data dir and keeps
//! its filter behind a `reload` handle, so "Debug logging" can be switched on
//! from the UI without a restart — which is the only way someone reproducing a
//! bug can capture the run that failed.
//!
//! The bundle itself is formatted by the pure
//! [`atv_optimizer_core::engine::diagnostics`] module; everything here is
//! the I/O that feeds it.

use std::io::{Read, Seek, SeekFrom};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, State};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, reload, EnvFilter, Layer};

use atv_optimizer_core::adb::AdbDriver;
use atv_optimizer_core::commands::{devices, launcher, AppState};
use atv_optimizer_core::engine::diagnostics::{
    format_diagnostics, DeviceDiagnostics, DiagnosticsInput,
};
use atv_optimizer_core::engine::log_redact::{self, Scrub};
use atv_optimizer_core::engine::telemetry::{format_support_id, telemetry_device};
use atv_optimizer_core::engine::telemetry_diag::{
    adb_diag, mdns_diag, net_diag, transport_counts, AdbSource, Diag, Gateway, NetInterface,
};
use atv_optimizer_core::telemetry::ExcerptSource;

/// Subdirectory of the app data dir that holds the rolling log files.
pub(crate) const LOG_DIR_NAME: &str = "logs";
/// Daily log files are `<prefix>.<date>.log`.
pub(crate) const LOG_FILE_PREFIX: &str = "atv-optimizer";
/// One file per day, a week back. Long enough to cover "it broke sometime last
/// week", short enough that nobody's disk notices.
const KEEP_LOG_FILES: usize = 7;
/// Log lines carried in a bug report. Enough to hold a failed device scan and
/// everything around it, not so many that the report stops being readable.
const LOG_TAIL_LINES: usize = 200;

/// Quiet by default, verbose for our own code when debug logging is on.
/// Turning *everything* to `debug` buries the interesting lines under hyper
/// and reqwest internals, which makes the bundle worse, not better.
const INFO_FILTER: &str = "info";
const DEBUG_FILTER: &str =
    "info,atv_optimizer_lib=debug,atv_optimizer_core=debug,atv_optimizer=debug";

/// Managed state for the log file: where it is, whether debug is on, and the
/// switch that changes the running filter.
pub struct LogControl {
    dir: PathBuf,
    debug: AtomicBool,
    /// `None` when the file layer could not be created — the app still runs
    /// and still logs to stdout, the toggle just has nothing to switch.
    apply: Option<Box<dyn Fn(bool) -> bool + Send + Sync>>,
    /// Dropping this flushes the background writer and stops it, so it lives
    /// until `flush_and_close` on exit.
    guard: std::sync::Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>,
}

impl LogControl {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn debug_enabled(&self) -> bool {
        self.debug.load(Ordering::Relaxed)
    }

    /// Flush the log file and stop writing it. Lines logged afterwards reach
    /// stdout only. Called last thing on exit.
    pub fn flush_and_close(&self) {
        let guard = self.guard.lock().map(|mut g| g.take()).unwrap_or_default();
        drop(guard);
    }
}

/// Debug logging starts on in dev builds (`tauri dev`), where the person
/// running the app is the one testing it, and off in release builds.
const DEBUG_BY_DEFAULT: bool = cfg!(debug_assertions);

fn file_filter(debug: bool) -> EnvFilter {
    if debug {
        EnvFilter::new(DEBUG_FILTER)
    } else {
        base_filter()
    }
}

fn base_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(INFO_FILTER))
}

/// Install the stdout + rolling-file subscriber. Returns the handle the
/// `set_debug_logging` command drives. Best-effort throughout: a read-only or
/// missing data dir costs the log file, never the app.
pub fn init_logging(data_dir: &Path) -> LogControl {
    let dir = data_dir.join(LOG_DIR_NAME);
    let appender = match std::fs::create_dir_all(&dir).ok().and_then(|()| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix(LOG_FILE_PREFIX)
            .filename_suffix("log")
            .max_log_files(KEEP_LOG_FILES)
            .build(&dir)
            .ok()
    }) {
        Some(appender) => appender,
        None => {
            let _ = tracing_subscriber::registry()
                .with(fmt::layer().with_filter(base_filter()))
                .with(crate::session::event_layer())
                .try_init();
            return LogControl {
                dir,
                debug: AtomicBool::new(false),
                apply: None,
                guard: std::sync::Mutex::new(None),
            };
        }
    };

    let (writer, guard) = tracing_appender::non_blocking(appender);
    let (file_filter, handle) = reload::Layer::new(file_filter(DEBUG_BY_DEFAULT));
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(writer)
        .with_filter(file_filter);
    let stdout_layer = fmt::layer().with_filter(base_filter());

    let _ = tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .with(crate::session::event_layer())
        .try_init();

    LogControl {
        dir,
        debug: AtomicBool::new(DEBUG_BY_DEFAULT),
        apply: Some(Box::new(move |debug: bool| {
            let filter = EnvFilter::new(if debug { DEBUG_FILTER } else { INFO_FILTER });
            handle.reload(filter).is_ok()
        })),
        guard: std::sync::Mutex::new(Some(guard)),
    }
}

/// `set_debug_logging` — raise or lower the *file* layer's level at runtime.
/// Returns what the level actually is afterwards, so a failed reload cannot
/// leave the checkbox claiming something untrue.
#[tauri::command]
pub fn set_debug_logging(logs: State<'_, LogControl>, enabled: bool) -> Result<bool, String> {
    let Some(apply) = logs.apply.as_ref() else {
        return Err(
            "Logging to a file isn't available on this install, so there's nothing to turn up."
                .to_string(),
        );
    };
    if !apply(enabled) {
        return Err("The log filter could not be changed.".to_string());
    }
    logs.debug.store(enabled, Ordering::Relaxed);
    crate::session::set_recording(enabled);
    tracing::info!(debug_logging = enabled, "debug logging toggled");
    Ok(enabled)
}

/// `get_debug_logging` — is the file layer currently at debug?
#[tauri::command]
pub fn get_debug_logging(logs: State<'_, LogControl>) -> bool {
    logs.debug.load(Ordering::Relaxed)
}

const UI_LABEL_LIMIT: usize = 120;
const UI_PATH_LIMIT: usize = 200;

/// Drop control characters and cap the length, so a breadcrumb is always one
/// readable JSON field.
fn clean_ui_text(text: &str, limit: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn valid_ui_event(event: &str) -> bool {
    (1..=24).contains(&event.len()) && event.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// `log_ui_event` — a UI breadcrumb (route change, tab switch, button label)
/// for the session recording. A no-op while recording is off.
#[tauri::command]
pub fn log_ui_event(event: String, label: String, path: Option<String>) -> Result<(), String> {
    if !valid_ui_event(&event) {
        return Err(format!("unknown UI event {event:?}"));
    }
    let label = clean_ui_text(&label, UI_LABEL_LIMIT);
    let path = path.map(|p| clean_ui_text(&p, UI_PATH_LIMIT));
    crate::session::record_ui(&event, &label, path.as_deref());
    Ok(())
}

/// `log_dir_path` — where the rolling log files live, for the UI to show.
#[tauri::command]
pub fn log_dir_path(logs: State<'_, LogControl>) -> String {
    logs.dir().display().to_string()
}

/// `open_log_dir` — reveal the log folder in the system file manager.
#[tauri::command]
pub fn open_log_dir<R: tauri::Runtime>(
    app: AppHandle<R>,
    logs: State<'_, LogControl>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = logs.dir().to_path_buf();
    // A folder that was never created cannot be opened; make it rather than
    // handing back an error the user can do nothing about.
    let _ = std::fs::create_dir_all(&dir);
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// The most recently written log file in `dir`.
///
/// Only files directly in `dir`: the `sessions/` recordings below it are
/// local-only and must never reach the bug-report bundle.
///
/// Picked by modification time rather than by formatting today's date:
/// `tracing-appender` names files by UTC day, so a machine west of Greenwich
/// would look for a file that does not exist yet for part of every evening.
fn newest_log_file(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let is_newer = match newest.as_ref() {
            Some((best, _)) => modified > *best,
            None => true,
        };
        if is_newer {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

fn log_tail(dir: &Path, lines: usize) -> Vec<String> {
    let Some(path) = newest_log_file(dir) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| (*l).to_string())
        .collect()
}

/// Parse `HOME_HANDLER_QUERY`'s `cmd package query-activities` output down to
/// the packages it reported. Reuses the launcher tab's own parser rather than
/// a second one: Android's ResolveInfo dump exposes the class under `name=`
/// and the package separately under `packageName=` — `name=` has no slash, so
/// treating it as a flattened `pkg/activity` component would drop every real
/// handler.
fn home_handler_components(stdout: &str) -> Vec<String> {
    launcher::parse_home_handler_packages(stdout)
}

/// `(package, enabled)` for each catalogued setup helper that is installed.
/// Read from the package lists because a disabled one never answers the HOME
/// query; `None` when they could not be read.
async fn setup_helper_states(
    adb: &dyn atv_optimizer_core::adb::AdbDriver,
    serial: &str,
) -> Option<Vec<(String, bool)>> {
    let cmd = atv_optimizer_core::adb::checked_batch_command(&[
        "pm list packages",
        "pm list packages -d",
    ]);
    let out = adb.shell(serial, &cmd).await.ok()?;
    let sections = atv_optimizer_core::adb::parse_checked_batch(&out.stdout, 2, &[0, 1]).ok()?;
    setup_helper_states_from(&sections[0], &sections[1])
}

/// An Android device always has packages, so an empty installed list is an
/// unreadable answer, not proof the helper is absent.
fn setup_helper_states_from(installed: &str, disabled: &str) -> Option<Vec<(String, bool)>> {
    let installed = atv_optimizer_core::adb::parse_installed_packages_output(installed);
    if installed.is_empty() {
        return None;
    }
    let disabled = atv_optimizer_core::adb::parse_disabled_packages_output(disabled);
    Some(
        atv_optimizer_core::commands::loader::launchers()
            .transient_home_holders
            .iter()
            .filter(|p| installed.contains(p))
            .map(|p| (p.clone(), !disabled.contains(p)))
            .collect(),
    )
}

/// `collect_diagnostics` — the text a user pastes into a bug report.
///
/// Nothing is sent anywhere: this command returns a string and the UI puts it
/// on the clipboard. Deliberately narrow — host facts, one device's identity
/// and TV evidence, its HOME handlers, and the tail of the log. No package
/// inventory.
#[tauri::command]
pub async fn collect_diagnostics(
    state: State<'_, AppState>,
    logs: State<'_, LogControl>,
    serial: Option<String>,
) -> Result<String, String> {
    let adb = state.adb_snapshot().await;
    let adb_path = if adb.uses_host_server() {
        crate::adb::cached_adb_binary().map(|p| p.display().to_string())
    } else {
        Some("(not applicable: direct network transport, no host binary)".into())
    };
    let adb_version = adb
        .raw(&["version"])
        .await
        .ok()
        .map(|out| out.stdout.trim().to_string());

    // Resolve the device first so a failure there is reported as "no device
    // section" rather than failing the whole bundle — the host half is still
    // worth having when the device is the thing that is broken.
    let mut unreadable: Option<String> = None;
    let device = match serial.as_deref() {
        None => None,
        Some(serial) => match devices::device_profile_impl(state.inner(), serial).await {
            Ok(device) => {
                let handlers = adb
                    .shell(serial, launcher::HOME_HANDLER_QUERY)
                    .await
                    .ok()
                    .map(|out| home_handler_components(&out.stdout))
                    .unwrap_or_default();
                let current_home = launcher::read_current_home(&*adb, serial).await.ok();
                let helpers = setup_helper_states(&*adb, serial).await;
                Some((device, handlers, current_home, helpers))
            }
            Err(e) => {
                tracing::warn!(serial, error = %e, "diagnostics: device unavailable");
                unreadable = Some(e);
                None
            }
        },
    };

    let tail = log_tail(logs.dir(), LOG_TAIL_LINES);
    let support_id = state.telemetry.settings().support_id;

    Ok(format_diagnostics(&DiagnosticsInput {
        app_version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        adb_path: adb_path.as_deref(),
        adb_version: adb_version.as_deref(),
        device: device
            .as_ref()
            .map(
                |(device, handlers, current_home, helpers)| DeviceDiagnostics {
                    serial: &device.serial,
                    connection: device.connection,
                    properties: device.properties.as_ref(),
                    tv_evidence: device.tv_evidence,
                    device_type: device.device_type,
                    home_handlers: handlers,
                    current_home: current_home.as_ref(),
                    setup_helpers: helpers.as_deref(),
                },
            ),
        unreadable_device: match (serial.as_deref(), unreadable.as_deref()) {
            (Some(serial), Some(error)) => Some((serial, error)),
            _ => None,
        },
        log_tail: &tail,
        support_id: support_id.as_deref(),
    }))
}

/// Where the adb in use came from, by the same priority discovery uses.
pub(crate) fn adb_source() -> AdbSource {
    let Some(path) = crate::adb::cached_adb_binary() else {
        return AdbSource::None;
    };
    let custom = crate::rename::adb_override();
    let sdk_roots: Vec<PathBuf> = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    let managed: Vec<PathBuf> = [
        crate::adb::adb_path_in_install_root(),
        crate::adb::install::adb_path_in_legacy_install_root(),
    ]
    .into_iter()
    .flatten()
    .collect();
    classify_adb_source(&path, custom.as_deref(), &managed, &sdk_roots)
}

fn classify_adb_source(
    path: &Path,
    custom: Option<&Path>,
    managed: &[PathBuf],
    sdk_roots: &[PathBuf],
) -> AdbSource {
    if custom == Some(path) {
        AdbSource::Custom
    } else if managed.iter().any(|m| m == path) {
        AdbSource::Bundled
    } else if sdk_roots.iter().any(|root| path.starts_with(root)) {
        AdbSource::Sdk
    } else {
        AdbSource::System
    }
}

/// The local interfaces, names and addresses included. Only counts and
/// prefix lengths leave this process, through `net_diag`.
fn local_interfaces() -> Vec<(String, IpAddr, u8)> {
    match if_addrs::get_if_addrs() {
        Ok(list) => list
            .into_iter()
            .map(|iface| {
                let (addr, prefix) = match &iface.addr {
                    if_addrs::IfAddr::V4(v4) => (IpAddr::V4(v4.ip), v4.prefixlen),
                    if_addrs::IfAddr::V6(v6) => (IpAddr::V6(v6.ip), v6.prefixlen),
                };
                (iface.name, addr, prefix)
            })
            .collect(),
        Err(e) => {
            tracing::debug!(error = %e, "could not list network interfaces");
            Vec::new()
        }
    }
}

/// What a connection report says about this computer: where adb came from
/// and whether its server answers, the transports it holds (counted by
/// state), what mDNS sees, and the network interfaces (counted). Everything
/// identifying is dropped by the pure builders.
pub(crate) async fn host_diag(
    adb: &dyn AdbDriver,
    swept: Option<[u8; 3]>,
    gateway: Option<Gateway>,
) -> Diag {
    let ifaces = local_interfaces();
    let ifaces: Vec<NetInterface> = ifaces
        .iter()
        .map(|(name, addr, prefix)| NetInterface {
            name,
            addr: *addr,
            prefix: *prefix,
        })
        .collect();
    if !adb.uses_host_server() {
        return Diag {
            adb: Some(adb_diag(AdbSource::Builtin, None, None)),
            net: Some(net_diag(&ifaces, gateway, swept)),
            ..Default::default()
        };
    }
    let version = adb.raw(&["version"]).await.ok().map(|o| o.combined());
    let devices = adb
        .raw(&["devices"])
        .await
        .map(|o| o.stdout)
        .map_err(|e| e.to_string());
    let check = adb
        .raw(&["mdns", "check"])
        .await
        .map(|o| o.combined())
        .map_err(|e| e.to_string());
    let services = adb
        .raw(&["mdns", "services"])
        .await
        .ok()
        .map(|o| crate::adb::parse_mdns_services(&o.stdout))
        .unwrap_or_default();
    let types: Vec<&str> = services.iter().map(|s| s.service.as_str()).collect();
    Diag {
        adb: Some(adb_diag(
            adb_source(),
            version.as_deref(),
            Some(devices.as_ref().map(|_| ()).map_err(String::as_str)),
        )),
        transports: devices.as_deref().ok().map(transport_counts),
        mdns: Some(mdns_diag(
            Some(check.as_deref().map_err(String::as_str)),
            &types,
        )),
        net: Some(net_diag(&ifaces, gateway, swept)),
        ..Default::default()
    }
}

/// The home directory and user name, removed from anything sent. A path is
/// already dropped whole by the redaction rules; the name on its own (in a
/// message, a hostname) is not.
fn host_identifiers() -> (Vec<String>, Vec<String>) {
    let mut known = Vec::new();
    let mut words = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let text = home.display().to_string();
        known.push(text.replace('\\', "/"));
        known.push(text);
        if let Some(name) = home.file_name().and_then(|n| n.to_str()) {
            words.push(name.to_string());
        }
    }
    for var in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(name) = std::env::var(var) {
            if !words.contains(&name) {
                words.push(name);
            }
        }
    }
    (known, words)
}

fn scrub_with<T>(session: &[String], f: impl FnOnce(&Scrub) -> T) -> T {
    let (mut known, words) = host_identifiers();
    known.extend(session.iter().cloned());
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    f(&Scrub {
        known: &known,
        words: &words,
    })
}

/// The last `max_bytes` of the newest log files in `dir`, oldest first.
/// Only the rolling app logs; the `sessions/` recordings never leave.
fn recent_log_text(dir: &Path, max_bytes: u64) -> String {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut parts: Vec<String> = Vec::new();
    let mut remaining = max_bytes;
    for (_, path) in files {
        if remaining == 0 {
            break;
        }
        let Some(text) = read_tail(&path, remaining) else {
            continue;
        };
        remaining = remaining.saturating_sub(text.len() as u64);
        parts.push(text);
    }
    parts.reverse();
    parts.join("\n")
}

/// The last `max_bytes` of a file, starting at a line boundary.
fn read_tail(path: &Path, max_bytes: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start == 0 {
        return Some(text);
    }
    Some(match text.find('\n') {
        Some(i) => text[i + 1..].to_string(),
        None => String::new(),
    })
}

/// Raw bytes read before redaction, for an excerpt and for an upload. The
/// redacted results are capped by `log_redact`.
const EXCERPT_READ_BYTES: u64 = 256 * 1024;
const UPLOAD_READ_BYTES: u64 = 1024 * 1024;

/// The desktop's excerpt builder for automatic reports: the newest log's
/// adb and connection lines, redacted.
pub fn excerpt_source(dir: PathBuf) -> ExcerptSource {
    Box::new(move |session: &[String]| {
        let text = recent_log_text(&dir, EXCERPT_READ_BYTES);
        scrub_with(session, |scrub| {
            log_redact::connection_excerpt(&text, scrub)
        })
    })
}

#[derive(Serialize)]
pub struct DiagnosticsUploadResult {
    pub support_id: String,
    pub bytes: usize,
}

#[derive(Serialize)]
struct UploadBody<'a> {
    schema: u32,
    install_id: &'a str,
    app: atv_optimizer_core::engine::telemetry::AppKind,
    app_version: &'a str,
    os: &'a str,
    ts: String,
    reports_enabled: bool,
    text: &'a str,
}

const UPLOAD_SCHEMA: u32 = 1;
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const UPLOAD_HEADER_MAX_BYTES: usize = 16 * 1024;

/// The serial column of `adb devices [-l]` output, whatever each row's state.
fn transport_serials(devices_output: &str) -> Vec<String> {
    devices_output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("List of devices") && !l.starts_with('*'))
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// One line per adb transport: state, transport and the TV's public build
/// properties. No serial or name.
fn device_lines(list: &[atv_optimizer_core::engine::types::Device]) -> Vec<String> {
    list.iter()
        .map(|d| {
            let status = serde_json::to_value(d.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            let connection = serde_json::to_value(d.connection)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            match d.properties.as_ref() {
                Some(props) => {
                    let t = telemetry_device(props, d.device_type, d.tv_evidence, &d.support);
                    format!(
                        "- {connection}, {status}: {} {} ({}), Android {} (SDK {}), profile {:?}, tier {:?}",
                        t.manufacturer,
                        t.model,
                        t.device,
                        t.android_release,
                        t.sdk.map_or("?".to_string(), |n| n.to_string()),
                        t.profile,
                        t.support_tier,
                    )
                }
                None => format!("- {connection}, {status}: no details"),
            }
        })
        .collect()
}

/// `upload_diagnostics` — Send logs to developer.
///
/// The user pressed a button that says it sends their logs, so this runs
/// whether or not automatic reports are on, under the current install id
/// (never a new one) so the id it returns finds the upload. Everything sent
/// is redacted by `engine::log_redact`: a summary of this computer and its
/// TVs, the connection diagnostics, and the last 256 KiB of the app log.
#[tauri::command]
pub async fn upload_diagnostics(
    state: State<'_, AppState>,
    logs: State<'_, LogControl>,
) -> Result<DiagnosticsUploadResult, String> {
    let telemetry = state.telemetry.clone();
    let (Some(target), Some(client)) = (telemetry.upload_target(), telemetry.http_client()) else {
        return Err("Log upload isn't available in this build.".to_string());
    };
    let reports_enabled = telemetry.settings().enabled;

    let adb = state.adb_snapshot().await;
    let gateway = if crate::adb::subnet_overridden() {
        Gateway::Override
    } else {
        Gateway::Detected
    };
    let swept = crate::adb::local_subnet_prefix().await;
    let diag = host_diag(
        &*adb,
        swept,
        Some(if swept.is_some() {
            gateway
        } else {
            Gateway::NotDetected
        }),
    )
    .await;
    let devices_l = adb
        .raw(&["devices", "-l"])
        .await
        .map(|o| o.stdout)
        .unwrap_or_else(|e| format!("(adb devices failed: {e})"));
    let devices = devices::list_devices_impl(state.inner())
        .await
        .map(|list| device_lines(&list))
        .unwrap_or_else(|e| vec![format!("- (could not list devices: {e})")]);

    // Our own values, written before redaction so the id survives it.
    let mut text = String::new();
    text.push_str("## ATV Optimizer logs (Send logs)\n\n");
    text.push_str(&format!(
        "- Support ID: {} (`{}`)\n",
        format_support_id(&target.install_id),
        target.install_id
    ));
    text.push_str(&format!("- App version: {}\n", target.app_version));
    text.push_str(&format!(
        "- OS: {} ({})\n",
        target.os,
        std::env::consts::ARCH
    ));
    text.push_str(&format!(
        "- Automatic reports: {}\n",
        if reports_enabled { "on" } else { "off" }
    ));

    let mut header = String::new();
    header.push_str("\n### Devices\n\n");
    for line in &devices {
        header.push_str(line);
        header.push('\n');
    }
    header.push_str("\n### adb devices -l\n\n```\n");
    header.push_str(devices_l.trim());
    header.push_str("\n```\n\n### Connection diagnostics\n\n```json\n");
    header.push_str(&serde_json::to_string_pretty(&diag).unwrap_or_default());
    header.push_str("\n```\n\n### Recent log\n\n```\n");

    let raw_log = recent_log_text(logs.dir(), UPLOAD_READ_BYTES);
    // Every transport adb lists, even one the device listing never reached,
    // so `adb devices -l` can't carry a serial the redaction rules can't see.
    let mut session = telemetry.known_identifiers();
    session.extend(transport_serials(&devices_l));
    scrub_with(&session, |scrub| {
        text.push_str(&log_redact::redact_text(
            &header,
            scrub,
            UPLOAD_HEADER_MAX_BYTES,
        ));
        text.push('\n');
        text.push_str(&log_redact::upload_log(&raw_log, scrub));
        text.push_str("\n```\n");
    });

    let body = UploadBody {
        schema: UPLOAD_SCHEMA,
        install_id: &target.install_id,
        app: target.app,
        app_version: &target.app_version,
        os: &target.os,
        ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        reports_enabled,
        text: &text,
    };
    let body = serde_json::to_string(&body).map_err(|e| format!("Log upload failed: {e}"))?;
    let response = client
        .post(&target.url)
        .timeout(UPLOAD_TIMEOUT)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Log upload failed: {}", without_url(e)))?;
    match response.status() {
        s if s.is_success() => {
            tracing::info!(bytes = text.len(), "logs sent to the developer");
            Ok(DiagnosticsUploadResult {
                support_id: target.install_id,
                bytes: text.len(),
            })
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => Err(
            "Log upload limit reached for today. Try again tomorrow, or attach the bug report instead."
                .to_string(),
        ),
        s => Err(format!("Log upload failed: the server answered {s}.")),
    }
}

/// reqwest errors name the URL; the user needs the reason.
fn without_url(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "the request timed out.".to_string()
    } else if e.is_connect() {
        "couldn't reach the server. Check your internet connection.".to_string()
    } else {
        e.without_url().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_installed_list_is_unknown_not_absent() {
        assert_eq!(setup_helper_states_from("", ""), None);
        assert_eq!(setup_helper_states_from("\n", "package:a\n"), None);
        let some = setup_helper_states_from("package:com.android.settings\n", "");
        assert_eq!(some, Some(vec![]));
    }

    #[test]
    fn home_handlers_are_read_from_the_real_resolveinfo_shape() {
        // `name=` is the bare class (no slash) and `packageName=` is the
        // separate field that actually names the package — the shape Android
        // emits, not a flattened `pkg/activity` component.
        let stdout = "Activity #0:\n  \
                      Priority=0 PreferredOrder=0 Match=0x108000 Specific=null\n  \
                      ActivityInfo:\n    \
                      name=com.spocky.projengmenu.ui.home.MainActivity\n    \
                      packageName=com.spocky.projengmenu\n    \
                      labelRes=0x7f0e0000\n\n\
                      Activity #1:\n  \
                      ActivityInfo:\n    \
                      name=com.google.android.tvlauncher.MainActivity\n    \
                      packageName=com.google.android.tvlauncher\n";

        assert_eq!(
            home_handler_components(stdout),
            vec![
                "com.spocky.projengmenu".to_string(),
                "com.google.android.tvlauncher".to_string(),
            ]
        );
    }

    #[test]
    fn session_recordings_never_reach_the_bug_report_tail() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join(crate::session::SESSION_DIR_NAME);
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("2026-09-30T14-03-22Z.jsonl"),
            "session line\n",
        )
        .unwrap();
        assert!(log_tail(dir.path(), 200).is_empty());

        std::fs::write(
            dir.path().join("atv-optimizer.2026-09-30.log"),
            "log line\n",
        )
        .unwrap();
        // Written after the log file, so it would win on modification time if
        // the directory walk ever descended into `sessions/`.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            sessions.join("2026-09-30T15-00-00Z.jsonl"),
            "newer session\n",
        )
        .unwrap();
        assert_eq!(log_tail(dir.path(), 200), vec!["log line".to_string()]);
    }

    #[test]
    fn ui_breadcrumbs_are_one_clean_bounded_line() {
        assert_eq!(clean_ui_text("Save\n  snapshot\t", 120), "Save snapshot");
        assert_eq!(clean_ui_text(&"a".repeat(300), 120).len(), 120);
        assert!(valid_ui_event("route"));
        assert!(valid_ui_event("click"));
        assert!(!valid_ui_event(""));
        assert!(!valid_ui_event("Route\n"));
        assert!(log_ui_event("bad event".into(), "x".into(), None).is_err());
        assert!(log_ui_event("tab".into(), "Apps".into(), Some("/devices/x".into())).is_ok());
    }

    #[test]
    fn every_listed_transport_is_scrubbed_from_an_upload() {
        let out = "List of devices attached\nQWERTYUIOPAS           device usb:1-1 product:mdarcy model:SHIELD_Android_TV device:mdarcy transport_id:2\nZXCVBNMLKJHG\tunauthorized usb:1-2 transport_id:3\n";
        let serials = transport_serials(out);
        assert!(serials.iter().any(|s| s == "QWERTYUIOPAS"), "{serials:?}");
        assert!(serials.iter().any(|s| s == "ZXCVBNMLKJHG"), "{serials:?}");
        let text = scrub_with(&serials, |scrub| log_redact::redact_text(out, scrub, 4096));
        assert!(
            !text.contains("QWERTYUIOPAS") && !text.contains("ZXCVBNMLKJHG"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn native_diagnostics_do_not_probe_sessions_or_a_host_server() {
        struct Native;
        #[async_trait::async_trait]
        impl AdbDriver for Native {
            fn uses_host_server(&self) -> bool {
                false
            }
            async fn raw(
                &self,
                _: &[&str],
            ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput>
            {
                panic!("diagnostics must not probe native sessions")
            }
            async fn shell(
                &self,
                _: &str,
                _: &str,
            ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput>
            {
                panic!("diagnostics must not read a TV")
            }
        }
        let diag = host_diag(&Native, None, Some(Gateway::NotDetected)).await;
        assert_eq!(diag.adb.unwrap().source, AdbSource::Builtin);
        assert!(diag.mdns.is_none());
        assert!(diag.transports.is_none());
        assert!(diag.net.is_some());
    }

    #[test]
    fn adb_source_follows_discovery_priority() {
        let managed = vec![
            PathBuf::from("/data/ATVOptimizer/platform-tools/adb"),
            PathBuf::from("/data/ShieldOptimizer/platform-tools/adb"),
        ];
        let custom = PathBuf::from("/opt/custom/adb");
        let sdk = vec![PathBuf::from("/sdk")];
        let classify = |p: &str| classify_adb_source(Path::new(p), Some(&custom), &managed, &sdk);
        assert_eq!(classify("/opt/custom/adb"), AdbSource::Custom);
        assert_eq!(
            classify("/data/ATVOptimizer/platform-tools/adb"),
            AdbSource::Bundled
        );
        assert_eq!(
            classify("/data/ShieldOptimizer/platform-tools/adb"),
            AdbSource::Bundled
        );
        assert_eq!(classify("/sdk/platform-tools/adb"), AdbSource::Sdk);
        assert_eq!(classify("/opt/homebrew/bin/adb"), AdbSource::System);
    }

    #[test]
    fn recent_logs_span_files_newest_last_and_skip_sessions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(crate::session::SESSION_DIR_NAME)).unwrap();
        std::fs::write(
            dir.path()
                .join(crate::session::SESSION_DIR_NAME)
                .join("s.jsonl"),
            "session secret\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("atv-optimizer.2026-11-01.log"),
            "old 1\nold 2\n",
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            dir.path().join("atv-optimizer.2026-11-02.log"),
            "new 1\nnew 2\n",
        )
        .unwrap();
        let all = recent_log_text(dir.path(), 1024);
        assert!(
            all.find("old 1").unwrap() < all.find("new 2").unwrap(),
            "{all}"
        );
        assert!(!all.contains("session"), "{all}");
        // A partial first line is dropped rather than sent half-cut.
        let tail = recent_log_text(dir.path(), 9);
        assert_eq!(tail.trim(), "new 2");
    }

    #[test]
    fn device_summaries_carry_no_serial_or_name() {
        use atv_optimizer_core::engine::types::{
            ConnectionType, Device, DeviceProperties, DeviceStatus,
        };
        use atv_optimizer_core::engine::{DeviceType, SupportInfo, TvEvidence};
        let list = vec![
            Device {
                id: 1,
                serial: "192.168.1.20:5555".into(),
                name: "Living Room".into(),
                model: "Shield".into(),
                device_type: DeviceType::Shield,
                tv_evidence: TvEvidence::Tv,
                status: DeviceStatus::Device,
                connection: ConnectionType::Network,
                properties: Some(DeviceProperties {
                    friendly_name: Some("Living Room".into()),
                    manufacturer: "NVIDIA".into(),
                    model: "SHIELD Android TV".into(),
                    serial_number: "1324619053514".into(),
                    android_release: "11".into(),
                    sdk_level: "30".into(),
                    ..Default::default()
                }),
                support: SupportInfo::default(),
            },
            Device {
                id: 2,
                serial: "QWERTYUIOPAS".into(),
                name: "QWERTYUIOPAS".into(),
                model: String::new(),
                device_type: DeviceType::Unknown,
                tv_evidence: TvEvidence::Unknown,
                status: DeviceStatus::Unauthorized,
                connection: ConnectionType::Usb,
                properties: None,
                support: SupportInfo::default(),
            },
        ];
        let lines = device_lines(&list).join("\n");
        assert!(
            lines.contains("network, device: NVIDIA SHIELD Android TV"),
            "{lines}"
        );
        assert!(lines.contains("usb, unauthorized: no details"), "{lines}");
        for secret in ["192.168", "Living Room", "1324619053514", "QWERTYUIOPAS"] {
            assert!(!lines.contains(secret), "{secret} leaked: {lines}");
        }
    }

    #[test]
    fn the_tail_is_the_last_n_lines_and_an_absent_log_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(log_tail(dir.path(), 200).is_empty());

        let body: String = (1..=300).map(|n| format!("line {n}\n")).collect();
        std::fs::write(dir.path().join("atv-optimizer.2026-09-19.log"), body).unwrap();

        let tail = log_tail(dir.path(), 200);
        assert_eq!(tail.len(), 200);
        assert_eq!(tail.first().unwrap(), "line 101");
        assert_eq!(tail.last().unwrap(), "line 300");
    }
}
