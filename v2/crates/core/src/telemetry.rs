//! Anonymous device and error reports: the user's setting, a bounded queue,
//! the background uploader, and the support-tier refresh.
//!
//! Payloads are built and redacted by the pure `engine::telemetry`; this
//! module only stores, batches and sends them. Everything here is best
//! effort: no call blocks, fails or slows a user action, and every network
//! failure is silent.
//!
//! The endpoint is fixed at compile time by `ATVOPT_TELEMETRY_URL`. A build
//! without it never touches the network from here, whatever the setting says.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::commands::AppState;
use crate::engine::telemetry::{
    action_event, device_seen_event, encode_batches, error_event, telemetry_device, AppKind,
    EventContext, TelemetryAction, TelemetryDevice, TelemetryEvent,
};
use crate::engine::types::Device;
use crate::engine::{parse_support_rules, SupportRules};

/// The build-time variable that turns telemetry on for a build.
pub const URL_ENV: &str = "ATVOPT_TELEMETRY_URL";
const SETTINGS_FILE: &str = "telemetry.json";
const SUPPORT_CACHE_FILE: &str = "support-tiers.json";
/// Oldest events are dropped past this; a TV that's offline for a week can't
/// grow the queue without bound.
const MAX_QUEUE: usize = 200;
/// How long the uploader waits after the first event so several go in one
/// request.
const FLUSH_DELAY: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SUPPORT_DOC_BYTES: usize = 256 * 1024;
/// Error reports: a burst allowance, a per-session ceiling, and a cap on
/// repeats of one code, so a polling loop that fails every few seconds sends
/// a handful of reports rather than thousands.
const ERRORS_PER_MINUTE: u32 = 10;
const ERRORS_PER_SESSION: u32 = 100;
const REPEATS_PER_CODE: u32 = 5;

/// The base URL compiled into this build, without a trailing slash.
pub fn build_base_url() -> Option<String> {
    normalize_base_url(option_env!("ATVOPT_TELEMETRY_URL"))
}

fn normalize_base_url(raw: Option<&str>) -> Option<String> {
    let url = raw?.trim().trim_end_matches('/');
    (url.starts_with("https://") || url.starts_with("http://")).then(|| url.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSettings {
    enabled: bool,
    install_id: String,
    #[serde(default)]
    notice_seen: bool,
}

impl StoredSettings {
    fn fresh() -> Self {
        Self {
            enabled: true,
            install_id: new_install_id(),
            notice_seen: false,
        }
    }
}

fn new_install_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// What the settings screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySettings {
    /// The user's choice. On by default.
    pub enabled: bool,
    /// Whether this build can send anything at all. A build without
    /// `ATVOPT_TELEMETRY_URL` says so instead of implying it reports.
    pub available: bool,
    /// The one-time first-launch notice has been dismissed.
    pub notice_seen: bool,
}

pub struct TelemetryConfig {
    pub base_url: Option<String>,
    pub app: AppKind,
    pub app_version: String,
    pub os: String,
}

impl TelemetryConfig {
    /// Configuration for a shipped app: the compiled-in URL and the host OS.
    pub fn for_build(app: AppKind, app_version: &str) -> Self {
        Self {
            base_url: build_base_url(),
            app,
            app_version: app_version.to_string(),
            os: host_os(),
        }
    }
}

#[derive(Default)]
struct ErrorLimiter {
    window_start: Option<Instant>,
    in_window: u32,
    session: u32,
    per_code: HashMap<String, u32>,
}

impl ErrorLimiter {
    fn allow(&mut self, code: &str, now: Instant) -> bool {
        match self.window_start {
            Some(start) if now.duration_since(start) < Duration::from_secs(60) => {}
            _ => {
                self.window_start = Some(now);
                self.in_window = 0;
            }
        }
        let repeats = self.per_code.entry(code.to_string()).or_default();
        if self.in_window >= ERRORS_PER_MINUTE
            || self.session >= ERRORS_PER_SESSION
            || *repeats >= REPEATS_PER_CODE
        {
            return false;
        }
        self.in_window += 1;
        self.session += 1;
        *repeats += 1;
        true
    }
}

struct Inner {
    settings: StoredSettings,
    queue: VecDeque<TelemetryEvent>,
    /// TVs already reported this session. Keys stay in memory only.
    seen: HashSet<String>,
    /// Last profile per transport serial, so an action can say which kind of
    /// TV it ran on. In memory only; the serial is never sent.
    devices: HashMap<String, TelemetryDevice>,
    limiter: ErrorLimiter,
    /// Bumped on opt-out so a batch drained before it is never sent.
    generation: u64,
}

pub struct Telemetry {
    config: TelemetryConfig,
    settings_path: Option<PathBuf>,
    inner: Mutex<Inner>,
    wake: Notify,
    cancel: Notify,
    started: AtomicBool,
    client: OnceLock<Option<reqwest::Client>>,
}

impl Telemetry {
    /// A hub that never records or sends. The default in `AppState`, and
    /// what tests and the e2e harness get.
    pub fn disabled() -> Arc<Self> {
        Arc::new(Self::build(
            TelemetryConfig {
                base_url: None,
                app: AppKind::Desktop,
                app_version: String::new(),
                os: String::new(),
            },
            None,
            StoredSettings {
                enabled: false,
                install_id: String::new(),
                notice_seen: true,
            },
        ))
    }

    /// Load (or create) the persisted setting under `data_dir`. Does not
    /// start the uploader; call [`Telemetry::start`] once a runtime exists.
    pub fn new(config: TelemetryConfig, data_dir: &Path) -> Arc<Self> {
        let path = data_dir.join(SETTINGS_FILE);
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<StoredSettings>(&text).ok())
            .filter(|s| uuid::Uuid::parse_str(&s.install_id).is_ok());
        let fresh = stored.is_none();
        let settings = stored.unwrap_or_else(StoredSettings::fresh);
        let hub = Self::build(config, Some(path), settings);
        if fresh {
            hub.persist(&hub.lock().settings);
        }
        Arc::new(hub)
    }

    fn build(config: TelemetryConfig, settings_path: Option<PathBuf>, s: StoredSettings) -> Self {
        Self {
            config,
            settings_path,
            inner: Mutex::new(Inner {
                settings: s,
                queue: VecDeque::new(),
                seen: HashSet::new(),
                devices: HashMap::new(),
                limiter: ErrorLimiter::default(),
                generation: 0,
            }),
            wake: Notify::new(),
            cancel: Notify::new(),
            started: AtomicBool::new(false),
            client: OnceLock::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn persist(&self, settings: &StoredSettings) {
        let Some(path) = &self.settings_path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string(settings) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!(error = %e, "could not save telemetry setting");
                }
            }
            Err(e) => tracing::warn!(error = %e, "could not encode telemetry setting"),
        }
    }

    fn configured(&self) -> bool {
        self.config.base_url.is_some()
    }

    fn active(inner: &Inner, configured: bool) -> bool {
        configured && inner.settings.enabled
    }

    pub fn is_active(&self) -> bool {
        Self::active(&self.lock(), self.configured())
    }

    pub fn settings(&self) -> TelemetrySettings {
        let inner = self.lock();
        TelemetrySettings {
            enabled: inner.settings.enabled,
            available: self.configured(),
            notice_seen: inner.settings.notice_seen,
        }
    }

    /// Off discards everything queued and cancels an upload in flight. Back
    /// on starts over under a new random install id, so the two periods
    /// can't be linked.
    pub fn set_enabled(&self, enabled: bool) -> TelemetrySettings {
        {
            let mut inner = self.lock();
            if inner.settings.enabled != enabled {
                inner.settings.enabled = enabled;
                if enabled {
                    inner.settings.install_id = new_install_id();
                } else {
                    inner.queue.clear();
                    inner.generation += 1;
                }
                inner.seen.clear();
            }
            let snapshot = inner.settings.clone();
            drop(inner);
            self.persist(&snapshot);
        }
        if !enabled {
            self.cancel.notify_waiters();
        }
        self.settings()
    }

    pub fn dismiss_notice(&self) -> TelemetrySettings {
        let snapshot = {
            let mut inner = self.lock();
            inner.settings.notice_seen = true;
            inner.settings.clone()
        };
        self.persist(&snapshot);
        self.settings()
    }

    fn context(&self, inner: &Inner) -> EventContext {
        EventContext {
            install_id: inner.settings.install_id.clone(),
            app: self.config.app,
            app_version: self.config.app_version.clone(),
            os: self.config.os.clone(),
        }
    }

    fn enqueue(&self, inner: &mut Inner, event: TelemetryEvent) {
        inner.queue.push_back(event);
        while inner.queue.len() > MAX_QUEUE {
            inner.queue.pop_front();
        }
        self.wake.notify_one();
    }

    /// Called by the canonical device listing after a TV was profiled.
    /// Reports each TV once per session, keyed in memory by its verified
    /// hardware id (or its transport when it has none).
    pub fn device_profiled(&self, device: &Device) {
        if !self.configured() {
            return;
        }
        let Some(props) = device.properties.as_ref() else {
            return;
        };
        let snapshot = telemetry_device(
            props,
            device.device_type,
            device.tv_evidence,
            &device.support,
        );
        let hardware_id = props.serial_number.trim();
        let key = if hardware_id.is_empty() || hardware_id.eq_ignore_ascii_case("unknown") {
            format!("transport:{}", device.serial)
        } else {
            format!("hw:{hardware_id}")
        };
        let mut inner = self.lock();
        inner
            .devices
            .insert(device.serial.clone(), snapshot.clone());
        if !Self::active(&inner, true) || !inner.seen.insert(key) {
            return;
        }
        let event = device_seen_event(&self.context(&inner), &now(), snapshot);
        self.enqueue(&mut inner, event);
    }

    /// A finished user action. `serial` only selects which profiled TV to
    /// describe; it is never sent.
    pub fn record_action(
        &self,
        action: TelemetryAction,
        serial: Option<&str>,
        ok: bool,
        error_code: Option<&str>,
        message: Option<&str>,
    ) {
        let mut inner = self.lock();
        if !Self::active(&inner, self.configured()) {
            return;
        }
        let device = serial.and_then(|s| inner.devices.get(s).cloned());
        let event = action_event(
            &self.context(&inner),
            &now(),
            action,
            device,
            ok,
            error_code,
            message,
        );
        self.enqueue(&mut inner, event);
    }

    /// An unexpected failure. Rate limited.
    pub fn record_error(
        &self,
        action: TelemetryAction,
        serial: Option<&str>,
        error_code: &str,
        message: &str,
    ) {
        let mut inner = self.lock();
        self.record_error_locked(&mut inner, action, serial, error_code, message);
    }

    fn record_error_locked(
        &self,
        inner: &mut Inner,
        action: TelemetryAction,
        serial: Option<&str>,
        error_code: &str,
        message: &str,
    ) {
        if !Self::active(inner, self.configured()) {
            return;
        }
        let code = crate::engine::telemetry::sanitize_error_code(error_code);
        if !inner.limiter.allow(&code, Instant::now()) {
            return;
        }
        let device = serial.and_then(|s| inner.devices.get(s).cloned());
        let event = error_event(&self.context(inner), &now(), action, device, &code, message);
        self.enqueue(inner, event);
    }

    /// Start the background uploader. Idempotent, and a no-op for a build
    /// without an endpoint.
    pub fn start(self: &Arc<Self>) {
        if !self.configured() || self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let hub = Arc::clone(self);
        tauri::async_runtime::spawn(async move { hub.upload_loop().await });
    }

    async fn upload_loop(self: Arc<Self>) {
        loop {
            self.wake.notified().await;
            tokio::time::sleep(FLUSH_DELAY).await;
            self.flush().await;
        }
    }

    /// Send everything queued. Public for tests and for a best-effort flush.
    pub async fn flush(&self) {
        loop {
            let (events, generation) = {
                let mut inner = self.lock();
                if !Self::active(&inner, self.configured()) {
                    inner.queue.clear();
                    return;
                }
                let n = inner
                    .queue
                    .len()
                    .min(crate::engine::telemetry::MAX_BATCH_EVENTS);
                if n == 0 {
                    return;
                }
                let events: Vec<_> = inner.queue.drain(..n).collect();
                (events, inner.generation)
            };
            for body in encode_batches(&events) {
                if !self.still_sending(generation) {
                    return;
                }
                tokio::select! {
                    _ = self.post_events(body) => {}
                    _ = self.cancel.notified() => return,
                }
            }
        }
    }

    fn still_sending(&self, generation: u64) -> bool {
        let inner = self.lock();
        Self::active(&inner, self.configured()) && inner.generation == generation
    }

    fn client(&self) -> Option<&reqwest::Client> {
        self.client.get_or_init(build_client).as_ref()
    }

    async fn post_events(&self, body: String) {
        let (Some(base), Some(client)) = (self.config.base_url.as_deref(), self.client()) else {
            return;
        };
        let result = client
            .post(format!("{base}/v1/events"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => tracing::debug!(status = %resp.status(), "telemetry upload refused"),
            Err(e) => tracing::debug!(error = %e, "telemetry upload failed"),
        }
    }

    async fn fetch_support_rules(&self) -> Option<(String, SupportRules)> {
        let base = self.config.base_url.as_deref()?;
        if !self.is_active() {
            return None;
        }
        let client = self.client()?;
        let mut resp = client
            .get(format!("{base}/v1/support-tiers"))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.ok()? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_SUPPORT_DOC_BYTES {
                return None;
            }
        }
        let text = String::from_utf8(bytes).ok()?;
        let rules = parse_support_rules(&text).ok()?;
        Some((text, rules))
    }

    #[cfg(test)]
    fn queued(&self) -> Vec<TelemetryEvent> {
        self.lock().queue.iter().cloned().collect()
    }

    #[cfg(test)]
    fn install_id(&self) -> String {
        self.lock().settings.install_id.clone()
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn build_client() -> Option<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("ATV-Optimizer/", env!("CARGO_PKG_VERSION")));
    #[cfg(target_os = "android")]
    let builder = android_system_roots(builder);
    match builder.build() {
        Ok(client) => Some(client),
        Err(e) => {
            tracing::debug!(error = %e, "telemetry client unavailable");
            None
        }
    }
}

/// Android's own CA store. Newer releases keep it in the Conscrypt APEX; the
/// classic location is still populated on older ones.
#[cfg(target_os = "android")]
fn android_system_roots(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    add_pem_roots(
        builder,
        &[
            "/apex/com.android.conscrypt/cacerts",
            "/system/etc/security/cacerts",
        ],
    )
    .0
}

#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
const PEM_CERT_MARKER: &[u8] = b"-----BEGIN CERTIFICATE-----";

/// Trust every PEM certificate in the first directory that has any.
/// Compiled on every target so host tests cover what Android runs.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
fn add_pem_roots(
    mut builder: reqwest::ClientBuilder,
    dirs: &[&str],
) -> (reqwest::ClientBuilder, usize) {
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut added = 0;
        for entry in entries.flatten() {
            let Ok(pem) = std::fs::read(entry.path()) else {
                continue;
            };
            // reqwest parses lazily, so a stray non-certificate file would
            // only surface as a failed `build()` and no client at all.
            if !pem
                .windows(PEM_CERT_MARKER.len())
                .any(|w| w == PEM_CERT_MARKER)
            {
                continue;
            }
            if let Ok(cert) = reqwest::Certificate::from_pem(&pem) {
                builder = builder.add_root_certificate(cert);
                added += 1;
            }
        }
        if added > 0 {
            return (builder, added);
        }
    }
    (builder, 0)
}

/// Load a previously fetched rules document from `data_dir`, if it's still
/// valid. Local only; runs whatever the telemetry setting is.
pub fn load_cached_support_rules(state: &AppState) {
    let path = state.data_dir.join(SUPPORT_CACHE_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    match parse_support_rules(&text) {
        Ok(rules) => state.set_support_rules(Arc::new(rules)),
        Err(e) => tracing::debug!(error = %e, "ignoring cached support rules"),
    }
}

/// Fetch the current rules from the report server and cache them. Only when
/// reports are on: turning them off means the app doesn't contact that
/// server at all. Any failure keeps the rules already loaded.
pub async fn refresh_support_rules(state: &AppState) {
    let Some((text, rules)) = state.telemetry.fetch_support_rules().await else {
        return;
    };
    let path = state.data_dir.join(SUPPORT_CACHE_FILE);
    if let Err(e) = std::fs::write(&path, text) {
        tracing::debug!(error = %e, "could not cache support rules");
    }
    state.set_support_rules(Arc::new(rules));
}

/// Report Rust panics, then hand them to whatever hook was there before.
/// Never blocks: if the panic happened while the queue was locked on this
/// thread, the report is skipped.
pub fn install_panic_hook(telemetry: Arc<Telemetry>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut inner) = telemetry.inner.try_lock() {
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_string());
            let location = info
                .location()
                .map(|l| format!(" at {}:{}", l.file(), l.line()))
                .unwrap_or_default();
            telemetry.record_error_locked(
                &mut inner,
                TelemetryAction::Other,
                None,
                "panic",
                &format!("{payload}{location}"),
            );
        }
        previous(info);
    }));
}

/// `android 15`, `macos 15.1`, `windows`, `linux`. Version detail only where
/// the OS publishes it without spawning a console or reading anything
/// personal.
pub fn host_os() -> String {
    #[cfg(target_os = "android")]
    {
        let release = std::process::Command::new("getprop")
            .arg("ro.build.version.release")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() < 16);
        match release {
            Some(r) => format!("android {r}"),
            None => "android".to_string(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
            .ok()
            .and_then(|plist| macos_product_version(&plist))
            .map(|v| format!("macos {v}"))
            .unwrap_or_else(|| "macos".to_string())
    }
    #[cfg(not(any(target_os = "android", target_os = "macos")))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn macos_product_version(plist: &str) -> Option<String> {
    let after_key = plist.split("<key>ProductVersion</key>").nth(1)?;
    let value = after_key
        .trim_start()
        .strip_prefix("<string>")?
        .split("</string>")
        .next()?
        .trim();
    (!value.is_empty() && value.len() < 16 && value.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::telemetry::EventKind;
    use crate::engine::types::{ConnectionType, DeviceProperties, DeviceStatus};
    use crate::engine::{DeviceType, SupportInfo, SupportTier, TvEvidence};
    use pretty_assertions::assert_eq;

    fn config(base: Option<&str>) -> TelemetryConfig {
        TelemetryConfig {
            base_url: base.map(str::to_string),
            app: AppKind::Mobile,
            app_version: "0.1.0".into(),
            os: "android 15".into(),
        }
    }

    fn shield(serial: &str, hardware_id: &str) -> Device {
        Device {
            id: 1,
            serial: serial.into(),
            name: "Bedroom TV".into(),
            model: "Shield TV Pro (2019)".into(),
            device_type: DeviceType::Shield,
            tv_evidence: TvEvidence::Tv,
            status: DeviceStatus::Device,
            connection: ConnectionType::Network,
            properties: Some(DeviceProperties {
                friendly_name: Some("Bedroom TV".into()),
                manufacturer: "NVIDIA".into(),
                brand: "NVIDIA".into(),
                model: "SHIELD Android TV".into(),
                device_codename: "mdarcy".into(),
                sdk_level: "30".into(),
                android_release: "11".into(),
                serial_number: hardware_id.into(),
                ..Default::default()
            }),
            support: SupportInfo {
                tier: SupportTier::FullySupported,
                note: Some("n".into()),
            },
        }
    }

    #[test]
    fn first_launch_is_on_with_a_v4_install_id_that_persists() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        let s = hub.settings();
        assert!(s.enabled && s.available && !s.notice_seen);
        let id = hub.install_id();
        assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
        let again = Telemetry::new(config(Some("https://t.example")), dir.path());
        assert_eq!(again.install_id(), id);
    }

    #[test]
    fn a_build_without_an_endpoint_records_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(None), dir.path());
        assert!(!hub.settings().available);
        hub.device_profiled(&shield("10.0.0.2:5555", "1324619053514"));
        hub.record_action(TelemetryAction::Disable, None, false, Some("x"), Some("y"));
        hub.record_error(TelemetryAction::Other, None, "panic", "boom");
        assert!(hub.queued().is_empty());
        assert!(!Telemetry::disabled().settings().available);
    }

    #[test]
    fn opting_out_discards_the_queue_and_opting_in_rotates_the_id() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        let first_id = hub.install_id();
        hub.record_action(TelemetryAction::Scan, None, true, None, None);
        assert_eq!(hub.queued().len(), 1);

        assert!(!hub.set_enabled(false).enabled);
        assert!(hub.queued().is_empty());
        hub.record_action(TelemetryAction::Scan, None, true, None, None);
        assert!(hub.queued().is_empty(), "nothing is recorded while off");
        // The choice survives a restart.
        assert!(
            !Telemetry::new(config(Some("https://t.example")), dir.path())
                .settings()
                .enabled
        );

        assert!(hub.set_enabled(true).enabled);
        assert_ne!(hub.install_id(), first_id);
        // Re-sending the current setting is not a toggle.
        let id = hub.install_id();
        hub.set_enabled(true);
        assert_eq!(hub.install_id(), id);
    }

    #[test]
    fn device_seen_once_per_tv_per_session_without_identifiers() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        hub.device_profiled(&shield("10.0.0.2:5555", "1324619053514"));
        // Same TV reached a second way.
        hub.device_profiled(&shield(
            "adb-1324619053514-x._adb-tls-connect._tcp",
            "1324619053514",
        ));
        // A TV with no readable id is keyed by its transport.
        hub.device_profiled(&shield("10.0.0.3:5555", ""));
        hub.device_profiled(&shield("10.0.0.3:5555", ""));
        let events = hub.queued();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.kind == EventKind::DeviceSeen));
        let text = serde_json::to_string(&events).unwrap();
        for secret in ["1324619053514", "10.0.0", "Bedroom", "adb-"] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }

    #[test]
    fn actions_describe_the_profiled_tv() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        hub.device_profiled(&shield("10.0.0.2:5555", "1"));
        hub.record_action(
            TelemetryAction::Uninstall,
            Some("10.0.0.2:5555"),
            false,
            Some("pm_failure"),
            Some("Failure [DELETE_FAILED_INTERNAL_ERROR] for com.example.private"),
        );
        let e = hub.queued().pop().unwrap();
        assert_eq!(e.kind, EventKind::ActionResult);
        assert_eq!(e.device.unwrap().model, "SHIELD Android TV");
        assert_eq!(e.error_code.as_deref(), Some("pm_failure"));
        assert_eq!(
            e.message.as_deref(),
            Some("Failure [DELETE_FAILED_INTERNAL_ERROR] for <pkg>")
        );
    }

    #[test]
    fn error_reports_are_rate_limited() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        for _ in 0..20 {
            hub.record_error(TelemetryAction::Other, None, "same_code", "m");
        }
        assert_eq!(hub.queued().len(), REPEATS_PER_CODE as usize);
        for i in 0..20 {
            hub.record_error(TelemetryAction::Other, None, &format!("code_{i}"), "m");
        }
        assert_eq!(hub.queued().len(), ERRORS_PER_MINUTE as usize);
    }

    #[test]
    fn queue_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        for _ in 0..(MAX_QUEUE + 50) {
            hub.record_action(TelemetryAction::Scan, None, true, None, None);
        }
        assert_eq!(hub.queued().len(), MAX_QUEUE);
    }

    #[test]
    fn corrupt_settings_start_over_rather_than_fail() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SETTINGS_FILE), "{not json").unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        assert!(hub.settings().enabled);
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            r#"{"enabled":false,"install_id":"1324619053514"}"#,
        )
        .unwrap();
        let hub = Telemetry::new(config(Some("https://t.example")), dir.path());
        assert!(uuid::Uuid::parse_str(&hub.install_id()).is_ok());
    }

    #[test]
    fn base_url_must_be_http_and_loses_its_trailing_slash() {
        assert_eq!(
            normalize_base_url(Some(" https://t.example/ ")),
            Some("https://t.example".into())
        );
        assert_eq!(normalize_base_url(Some("")), None);
        assert_eq!(normalize_base_url(Some("ftp://x")), None);
        assert_eq!(normalize_base_url(None), None);
    }

    #[test]
    fn pem_roots_skip_missing_dirs_and_non_certificates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("junk.0"), "not a certificate").unwrap();
        let path = dir.path().to_str().unwrap();
        let (builder, added) = add_pem_roots(reqwest::Client::builder(), &["/nonexistent", path]);
        assert_eq!(added, 0);
        assert!(builder.build().is_ok());
    }

    #[test]
    fn macos_version_parses_from_system_version_plist() {
        let plist = "<dict>\n\t<key>ProductName</key>\n\t<string>macOS</string>\n\t<key>ProductVersion</key>\n\t<string>15.1</string>\n</dict>";
        assert_eq!(macos_product_version(plist), Some("15.1".into()));
        assert_eq!(macos_product_version("<dict></dict>"), None);
    }

    /// One HTTP request, read off a real socket, so the uploader is proven
    /// end to end: batching, the path, the body and the opt-out gate.
    async fn capture_one_post(listener: tokio::net::TcpListener) -> (String, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = sock.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(split) = text.find("\r\n\r\n") {
                let head = text[..split].to_ascii_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .map(|v| v.trim().parse().unwrap())
                    .unwrap_or(0);
                if buf.len() >= split + 4 + len {
                    sock.write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n")
                        .await
                        .unwrap();
                    let request_line = text.lines().next().unwrap().to_string();
                    return (request_line, text[split + 4..].to_string());
                }
            }
            if n == 0 {
                panic!("connection closed early");
            }
        }
    }

    #[tokio::test]
    async fn flush_posts_the_batch_to_v1_events() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let dir = tempfile::tempdir().unwrap();
        let hub = Telemetry::new(config(Some(&base)), dir.path());
        hub.device_profiled(&shield("10.0.0.2:5555", "1324619053514"));
        hub.record_action(
            TelemetryAction::Disable,
            Some("10.0.0.2:5555"),
            true,
            None,
            None,
        );

        let server = tokio::spawn(capture_one_post(listener));
        hub.flush().await;
        let (request_line, body) = server.await.unwrap();
        assert_eq!(request_line, "POST /v1/events HTTP/1.1");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let events = v["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["kind"], "device_seen");
        assert_eq!(events[0]["app"], "mobile");
        assert_eq!(events[1]["action"], "disable");
        assert!(!body.contains("1324619053514"));
        assert!(hub.queued().is_empty());
    }

    #[tokio::test]
    async fn opted_out_flush_sends_nothing() {
        let dir = tempfile::tempdir().unwrap();
        // Port 9 (discard) on loopback: a send attempt would fail, but the
        // point is that none is made and the queue is gone.
        let hub = Telemetry::new(config(Some("http://127.0.0.1:9")), dir.path());
        hub.record_action(TelemetryAction::Scan, None, true, None, None);
        hub.set_enabled(false);
        hub.flush().await;
        assert!(hub.queued().is_empty());
    }
}
