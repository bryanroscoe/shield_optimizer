//! Telemetry payloads: what an anonymous report contains, and what it never
//! contains.
//!
//! Pure. Building and redacting happen here so the privacy promise is unit
//! tested; queueing and HTTP live in `crate::telemetry`.
//!
//! Never sent: `ro.serialno` or any other hardware id or fingerprint, IP
//! addresses or ports, TV names, installed package lists, file paths,
//! license data, snapshot contents. [`TelemetryDevice`] has no field that
//! could carry the first three, and every free-text `message` goes through
//! [`redact_message`].

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::detection::{is_fire_tv, DeviceType, TvEvidence};
use super::support::{SupportInfo, SupportTier};
use super::telemetry_diag::{Diag, DiagReason};
use super::types::DeviceProperties;

/// 2 added the `diagnostic` kind and the `diag` and `log_excerpt` fields.
pub const TELEMETRY_SCHEMA: u32 = 2;
pub const MAX_BATCH_EVENTS: usize = 50;
pub const MAX_BATCH_BYTES: usize = 64 * 1024;
pub const MAX_MESSAGE_CHARS: usize = 500;
const MAX_FIELD_CHARS: usize = 64;
const MAX_CODE_CHARS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppKind {
    Desktop,
    Mobile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    DeviceSeen,
    ActionResult,
    Error,
    /// A scan or device read that succeeded but found nothing usable. Carries
    /// a [`Diag`] and the reason as `error_code`.
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryAction {
    Connect,
    Scan,
    Disable,
    Enable,
    Uninstall,
    Optimize,
    SnapshotApply,
    Launcher,
    SettingsWrite,
    Pairing,
    Other,
}

impl TelemetryAction {
    /// Anything unrecognised is `other`, never an error: the frontend's list
    /// can grow ahead of this enum.
    pub fn parse(s: &str) -> Self {
        serde_json::from_value(serde_json::Value::String(s.trim().to_ascii_lowercase()))
            .unwrap_or(Self::Other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceProfile {
    Shield,
    Googletv,
    Androidtv,
    Fire,
    Unknown,
}

/// The device half of an event. Deliberately has no serial, fingerprint,
/// build id, friendly name or address field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryDevice {
    pub manufacturer: String,
    pub brand: String,
    pub model: String,
    pub device: String,
    pub product: String,
    pub android_release: String,
    /// `None` (sent as `null`) when the device didn't report a number.
    pub sdk: Option<u32>,
    pub profile: DeviceProfile,
    pub support_tier: SupportTier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub schema: u32,
    pub install_id: String,
    pub app: AppKind,
    pub app_version: String,
    pub os: String,
    pub ts: String,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<TelemetryDevice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<TelemetryAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diag: Option<Diag>,
    /// Desktop only: the redacted tail of the app's adb and connection log
    /// lines (`engine::log_redact::connection_excerpt`). The phone app never
    /// sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_excerpt: Option<String>,
}

/// The per-install fields every event repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContext {
    pub install_id: String,
    pub app: AppKind,
    pub app_version: String,
    pub os: String,
}

/// Which family to report the device as. Uses the canonical detection
/// output, plus the Fire TV check that lives beside it.
pub fn profile_for(
    props: &DeviceProperties,
    device_type: DeviceType,
    evidence: TvEvidence,
) -> DeviceProfile {
    if is_fire_tv(props) {
        return DeviceProfile::Fire;
    }
    match device_type {
        DeviceType::Shield => DeviceProfile::Shield,
        DeviceType::GoogleTv => DeviceProfile::Googletv,
        DeviceType::Unknown if evidence == TvEvidence::Tv => DeviceProfile::Androidtv,
        DeviceType::Unknown => DeviceProfile::Unknown,
    }
}

fn bounded(s: &str) -> String {
    // Model strings are public product names, but they are still free text
    // the device chose, so they get the same scrubbing as a message.
    let cleaned = redact_message(s);
    cleaned.chars().take(MAX_FIELD_CHARS).collect()
}

pub fn telemetry_device(
    props: &DeviceProperties,
    device_type: DeviceType,
    evidence: TvEvidence,
    support: &SupportInfo,
) -> TelemetryDevice {
    TelemetryDevice {
        manufacturer: bounded(&props.manufacturer),
        brand: bounded(&props.brand),
        model: bounded(&props.model),
        device: bounded(&props.device_codename),
        product: bounded(&props.product),
        android_release: bounded(&props.android_release),
        sdk: props.sdk_level.trim().parse().ok(),
        profile: profile_for(props, device_type, evidence),
        support_tier: support.tier,
    }
}

/// `short_snake_case`, or `unknown` when nothing usable is left.
pub fn sanitize_error_code(raw: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = true;
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
        if out.len() >= MAX_CODE_CHARS {
            break;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() || !out.starts_with(|c: char| c.is_ascii_lowercase()) {
        "unknown".to_string()
    } else {
        out
    }
}

/// Compile a pattern written in this crate. Shared by the redaction and
/// diagnostics tables; a bad pattern is a bug the tests hit at once.
pub(super) fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static pattern")
}

static URL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b[a-z][a-z0-9+.-]*://\S+"));
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"));
static MDNS: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\badb-[A-Za-z0-9-]+(?:\._[A-Za-z0-9-]+\._(?:tcp|udp)\.?(?:local\.?)?)?|\S+\._(?:tcp|udp)\b\S*",
    )
});
static MAC: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b[0-9a-f]{2}(?:[:-][0-9a-f]{2}){5}\b"));
// Leading group stands in for the lookbehind the regex crate lacks: an IPv6
// literal starts at a non-word, non-colon boundary, so `std::io` survives.
static IPV6: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(^|[^0-9a-z_:])(\[?[0-9a-f]{1,4}(?::[0-9a-f]{0,4}){2,7}(?:%[0-9a-z]+)?\]?(?::\d{1,5})?)",
    )
});
static IPV4: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d{1,3}(?:\.\d{1,3}){3}(?::\d{1,5})?\b"));
// A private subnet prefix on its own (`192.168.42`, `10.0.5.x`), as the scan
// names the /24 it swept. Also takes a version number that starts with 10,
// which only loses context.
static PRIVATE_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\b(?:192\.168|10\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01]))\.\d{1,3}(?:\.[x*])?\b")
});
static HOST_PORT: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*:\d{2,5}\b"));
// `Bryans-MacBook.local`, `living-room-tv.local.`: Bonjour host names carry
// the owner's or the TV's name.
static LOCAL_HOST: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b[a-z0-9](?:[a-z0-9_-]*[a-z0-9])?(?:\.[a-z0-9_-]+)*\.local\b\.?"));
// A path runs to a quote, bracket, newline, `: ` or the end of the text, so
// one with spaces in it (`C:\Users\Jane Doe\…`) is removed whole. That
// also swallows any unquoted words after a path, which only loses context.
static UNIX_PATH: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(^|[\s'"(=,\[])~?/(?:[^'"\n:)\]]|:\S)*"#));
static WINDOWS_PATH: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?i)\b[a-z]:\\(?:[^'"\n:)\]]|:\S)*|\\\\(?:[^'"\n:)\]]|:\S)+"#));
// A dotted name that can't be a package id because a label starts with a
// digit or holds a hyphen, under a letters-only top-level label: a Private
// DNS host such as `1a2b3c.dns.nextdns.io` or `one-one.example-dns.net`.
// Ones that could be package ids are taken by PACKAGE below.
static DOTTED_HOST: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Za-z0-9-]{1,63}(?:\.[A-Za-z0-9-]{1,63})*\.[A-Za-z]{2,24}\b"));

fn is_host_shaped(token: &str) -> bool {
    let labels: Vec<&str> = token.split('.').collect();
    labels.len() >= 2
        && labels
            .iter()
            .any(|l| l.contains('-') || l.starts_with(|c: char| c.is_ascii_digit()))
}

// Two segments are enough: `acme.player` is a valid package id. This also
// takes file names like `notes.txt`, which only loses context.
static PACKAGE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+\b"));
static ID_TOKEN: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Za-z0-9]{8,}\b"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));

/// Platform class names (`java.lang.SecurityException`) explain a failure and
/// name code, not an app. Only a capitalised last segment in a platform
/// namespace survives: `android.*` and `com.android.*` are also prefixes of
/// real installed packages, which are removed like any other.
const KEEP_NAMESPACES: [&str; 5] = ["java.", "javax.", "android.", "kotlin.", "dalvik."];

fn is_platform_class(token: &str) -> bool {
    KEEP_NAMESPACES.iter().any(|ns| token.starts_with(ns))
        && token
            .rsplit('.')
            .next()
            .and_then(|last| last.chars().next())
            .is_some_and(|c| c.is_ascii_uppercase())
}

/// System property names (`ro.product.model`, `persist.sys.locale`) say which
/// read failed and are not packages. Their values are handled elsewhere.
const PROPERTY_NAMESPACES: [&str; 5] = ["ro.", "persist.", "sys.", "init.svc.", "service.adb."];

fn is_property_name(token: &str) -> bool {
    PROPERTY_NAMESPACES.iter().any(|ns| token.starts_with(ns))
}

/// [`redact_message`], after first removing identifiers the caller already
/// knows: the serials and hardware ids of the TVs this session has seen. A
/// serial has no reliable shape (`ABCDEFABCDEF` is a valid one), so the
/// generic patterns can't be the only line of defence.
pub fn redact_message_with(raw: &str, known: &[&str]) -> String {
    let mut s = raw.to_string();
    let mut known: Vec<&str> = known
        .iter()
        .map(|k| k.trim())
        .filter(|k| k.len() >= 4)
        .collect();
    // Longest first, so a serial inside a longer mDNS name goes with it.
    known.sort_by_key(|k| std::cmp::Reverse(k.len()));
    for k in known {
        s = s.replace(k, "<id>");
    }
    redact_message(&s)
}

/// Strip anything identifying from free text and cap it at
/// [`MAX_MESSAGE_CHARS`]. Errs on the side of removing too much: a timestamp
/// that looks like an address loses nothing important.
pub fn redact_message(raw: &str) -> String {
    redact_unbounded(raw)
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .collect()
}

/// [`redact_message`] without the length cap, for log lines.
pub(crate) fn redact_unbounded(raw: &str) -> String {
    let mut s = URL.replace_all(raw, "<url>").into_owned();
    s = EMAIL.replace_all(&s, "<email>").into_owned();
    s = MDNS.replace_all(&s, "<mdns>").into_owned();
    s = MAC.replace_all(&s, "<mac>").into_owned();
    s = IPV4.replace_all(&s, "<ip>").into_owned();
    s = PRIVATE_PREFIX.replace_all(&s, "<ip>").into_owned();
    s = IPV6.replace_all(&s, "${1}<ip>").into_owned();
    s = HOST_PORT.replace_all(&s, "<host>").into_owned();
    s = LOCAL_HOST.replace_all(&s, "<host>").into_owned();
    s = DOTTED_HOST
        .replace_all(&s, |caps: &regex::Captures| {
            let token = &caps[0];
            if is_host_shaped(token) {
                "<host>".to_string()
            } else {
                token.to_string()
            }
        })
        .into_owned();
    s = WINDOWS_PATH.replace_all(&s, "<path>").into_owned();
    s = UNIX_PATH.replace_all(&s, "${1}<path>").into_owned();
    s = PACKAGE
        .replace_all(&s, |caps: &regex::Captures| {
            let token = &caps[0];
            if is_platform_class(token) || is_property_name(token) {
                token.to_string()
            } else {
                "<pkg>".to_string()
            }
        })
        .into_owned();
    s = ID_TOKEN
        .replace_all(&s, |caps: &regex::Captures| {
            let token = &caps[0];
            if token.chars().filter(char::is_ascii_digit).count() >= 3 {
                "<id>".to_string()
            } else {
                token.to_string()
            }
        })
        .into_owned();
    SPACES.replace_all(s.trim(), " ").into_owned()
}

fn base(ctx: &EventContext, ts: &str, kind: EventKind) -> TelemetryEvent {
    TelemetryEvent {
        schema: TELEMETRY_SCHEMA,
        install_id: ctx.install_id.clone(),
        app: ctx.app,
        app_version: bounded(&ctx.app_version),
        os: bounded(&ctx.os),
        ts: ts.to_string(),
        kind,
        device: None,
        action: None,
        ok: None,
        error_code: None,
        message: None,
        diag: None,
        log_excerpt: None,
    }
}

pub fn device_seen_event(ctx: &EventContext, ts: &str, device: TelemetryDevice) -> TelemetryEvent {
    TelemetryEvent {
        device: Some(device),
        ..base(ctx, ts, EventKind::DeviceSeen)
    }
}

/// A finished user action. A success carries no code or message.
#[allow(clippy::too_many_arguments)]
pub fn action_event(
    ctx: &EventContext,
    ts: &str,
    action: TelemetryAction,
    device: Option<TelemetryDevice>,
    ok: bool,
    error_code: Option<&str>,
    message: Option<&str>,
    known: &[&str],
) -> TelemetryEvent {
    let (error_code, message) = if ok {
        (None, None)
    } else {
        (
            Some(sanitize_error_code(error_code.unwrap_or("unknown"))),
            message
                .map(|m| redact_message_with(m, known))
                .filter(|m| !m.is_empty()),
        )
    };
    TelemetryEvent {
        device,
        action: Some(action),
        ok: Some(ok),
        error_code,
        message,
        ..base(ctx, ts, EventKind::ActionResult)
    }
}

/// An unexpected failure: a panic, an uncaught frontend error, or a command
/// failure outside the tracked actions.
pub fn error_event(
    ctx: &EventContext,
    ts: &str,
    action: TelemetryAction,
    device: Option<TelemetryDevice>,
    error_code: &str,
    message: &str,
    known: &[&str],
) -> TelemetryEvent {
    TelemetryEvent {
        device,
        action: Some(action),
        ok: Some(false),
        error_code: Some(sanitize_error_code(error_code)),
        message: Some(redact_message_with(message, known)).filter(|m| !m.is_empty()),
        ..base(ctx, ts, EventKind::Error)
    }
}

/// A scan or device read that found nothing usable. `diag` is built by
/// `engine::telemetry_diag`, which only ever holds counts, codes and versions.
pub fn diagnostic_event(
    ctx: &EventContext,
    ts: &str,
    reason: DiagReason,
    device: Option<TelemetryDevice>,
    diag: Diag,
) -> TelemetryEvent {
    TelemetryEvent {
        device,
        action: Some(reason.action()),
        error_code: Some(reason.code().to_string()),
        diag: Some(diag),
        ..base(ctx, ts, EventKind::Diagnostic)
    }
}

/// Log excerpts attached per install per UTC day, at most.
pub const EXCERPTS_PER_DAY: u32 = 20;
/// Error events per install per UTC day once each error has been sent once.
/// The first of each kind always goes (see [`DailyLedger::allow_error`]).
pub const ERRORS_PER_DAY: u32 = 30;
/// Bounds the ledger file. A full ledger sends no further excerpts that day,
/// and stops limiting first errors and once-a-day reports, which are then
/// bounded by the per-session limits alone.
const MAX_LEDGER_KEYS: usize = 128;

/// What one install has already sent today, so restarts don't resend it.
/// The host stores it in its settings file; nothing here is ever sent. Keys
/// are built from codes, actions and command names (and a TV's public
/// model name), never from a serial, address or message text.
///
/// `excerpt_day` and `excerpts_today` keep the names of the excerpt budget
/// they replaced, so a settings file from an older build keeps its count.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyLedger {
    #[serde(
        default,
        rename = "excerpt_day",
        skip_serializing_if = "String::is_empty"
    )]
    pub day: String,
    #[serde(default, rename = "excerpts_today")]
    pub excerpts: u32,
    #[serde(default, rename = "errors_today")]
    pub errors: u32,
    #[serde(default, rename = "sent_today", skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}

impl DailyLedger {
    fn roll(&mut self, today: &str) {
        if self.day != today {
            *self = Self {
                day: today.to_string(),
                ..Self::default()
            };
        }
    }

    /// Record `key` as sent today. False when it already was, or the ledger
    /// is full.
    fn insert(&mut self, today: &str, key: String) -> bool {
        self.roll(today);
        if self.keys.contains(&key) || self.keys.len() >= MAX_LEDGER_KEYS {
            return false;
        }
        self.keys.push(key);
        true
    }

    fn contains(&mut self, today: &str, key: &str) -> bool {
        self.roll(today);
        self.keys.iter().any(|k| k == key)
    }

    /// Spend one of today's excerpts on the first event today with this
    /// [`excerpt_key`]. A repeat sends its code and message without one.
    pub fn take_excerpt(&mut self, today: &str, key: &str) -> bool {
        self.roll(today);
        if self.excerpts >= EXCERPTS_PER_DAY || !self.insert(today, format!("x|{key}")) {
            return false;
        }
        self.excerpts += 1;
        true
    }

    /// Put back an excerpt [`Self::take_excerpt`] granted that never
    /// reached the server (the log was empty, or the upload failed), so
    /// the next occurrence can carry one.
    pub fn return_excerpt(&mut self, today: &str, key: &str) {
        if self.remove(today, &format!("x|{key}")) {
            self.excerpts = self.excerpts.saturating_sub(1);
        }
    }

    fn remove(&mut self, today: &str, key: &str) -> bool {
        if self.day != today {
            return false;
        }
        match self.keys.iter().position(|k| k == key) {
            Some(i) => {
                self.keys.remove(i);
                true
            }
            None => false,
        }
    }

    /// No error event with this key has been sent today. A key a full
    /// ledger couldn't record counts as unsent.
    pub fn first_error_today(&mut self, today: &str, key: &str) -> bool {
        !self.contains(today, &format!("e|{key}"))
    }

    /// The daily ceiling on error events. The first event today with each
    /// key always goes, so no error goes unreported because another one
    /// repeated all day; repeats stop at [`ERRORS_PER_DAY`].
    pub fn allow_error(&mut self, today: &str, key: &str) -> bool {
        if self.first_error_today(today, key) {
            self.insert(today, format!("e|{key}"));
        } else if self.errors >= ERRORS_PER_DAY {
            return false;
        }
        self.errors = self.errors.saturating_add(1);
        true
    }

    /// Whether a once-a-day report with this key was already sent today.
    pub fn sent_today(&mut self, today: &str, key: &str) -> bool {
        self.contains(today, &format!("d|{key}"))
    }

    /// Mark a once-a-day report as sent. False when the ledger is full and
    /// couldn't record it; the report still goes.
    pub fn mark_sent(&mut self, today: &str, key: &str) -> bool {
        self.insert(today, format!("d|{key}"))
    }

    /// Undo [`Self::mark_sent`] for a report whose upload failed.
    pub fn unmark_sent(&mut self, today: &str, key: &str) {
        self.remove(today, &format!("d|{key}"));
    }

    /// Forget which keys were sent, for a fresh install id. The excerpt
    /// count stays, so turning reports off and on can't raise the budget.
    pub fn forget_keys(&mut self) {
        self.keys.clear();
        self.errors = 0;
    }
}

fn action_name(action: Option<TelemetryAction>) -> String {
    action
        .and_then(|a| serde_json::to_value(a).ok())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The command a frontend-reported error names first (`app_usage_map: …`),
/// when it reads as one.
fn command_prefix(message: Option<&str>) -> Option<&str> {
    let (head, _) = message?.split_once(':')?;
    let ok = !head.is_empty()
        && head.len() <= MAX_FIELD_CHARS
        && head.starts_with(|c: char| c.is_ascii_lowercase())
        && head
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then_some(head)
}

/// Which events may carry a desktop log excerpt, and the key that limits it
/// to the first one per install per day: the kind, action and error code,
/// plus the command for an error event, since the frontend reports every
/// unrecognised command failure as `unknown`. Keying finer than the code
/// alone means no code ever loses its first excerpt to another. `None`: no
/// excerpt (device seen, a success, a diagnostic whose `diag` suffices).
pub fn excerpt_key(event: &TelemetryEvent) -> Option<String> {
    let code = event.error_code.as_deref().unwrap_or("unknown");
    let action = action_name(event.action);
    match event.kind {
        EventKind::DeviceSeen => None,
        EventKind::ActionResult if event.ok != Some(false) => None,
        EventKind::ActionResult => Some(format!("action|{action}|{code}")),
        EventKind::Error => Some(match command_prefix(event.message.as_deref()) {
            Some(command) => format!("error|{action}|{code}|{command}"),
            None => format!("error|{action}|{code}"),
        }),
        EventKind::Diagnostic => DiagReason::from_code(code)
            .filter(|r| r.carries_excerpt())
            .map(|r| format!("diag|{}", r.code())),
    }
}

/// The key that limits a once-a-day diagnostic: its reason and the TV model
/// it came from, so a second TV of another model is still reported.
pub fn once_a_day_key(reason: DiagReason, device: Option<&TelemetryDevice>) -> String {
    match device {
        Some(d) => format!("{}|{}|{}", reason.code(), d.manufacturer, d.model),
        None => reason.code().to_string(),
    }
}

/// The short form of an install id a user reads out: the first eight hex
/// digits, grouped (`7D0B-0B9E`). Same as `formatSupportId` in
/// `shared/telemetry.ts`; empty when there aren't eight.
pub fn format_support_id(id: &str) -> String {
    let hex: String = id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(8)
        .collect::<String>()
        .to_ascii_uppercase();
    if hex.len() == 8 {
        format!("{}-{}", &hex[..4], &hex[4..])
    } else {
        String::new()
    }
}

#[derive(Serialize)]
struct Batch<'a> {
    events: &'a [TelemetryEvent],
}

/// Split events into request bodies of at most [`MAX_BATCH_EVENTS`] events
/// and [`MAX_BATCH_BYTES`] bytes. An event too big to send alone (it can't
/// be, with the field caps above) is dropped rather than sent.
pub fn encode_batches(events: &[TelemetryEvent]) -> Vec<String> {
    let mut bodies = Vec::new();
    let mut start = 0;
    while start < events.len() {
        let mut end = (start + MAX_BATCH_EVENTS).min(events.len());
        loop {
            let body = serde_json::to_string(&Batch {
                events: &events[start..end],
            })
            .unwrap_or_default();
            if body.len() <= MAX_BATCH_BYTES {
                bodies.push(body);
                start = end;
                break;
            }
            if end - start == 1 {
                start = end;
                break;
            }
            end = start + (end - start) / 2;
        }
    }
    bodies
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn ctx() -> EventContext {
        EventContext {
            install_id: "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11".into(),
            app: AppKind::Desktop,
            app_version: "2.4.1".into(),
            os: "macos 15.1".into(),
        }
    }

    fn shield_props() -> DeviceProperties {
        DeviceProperties {
            friendly_name: Some("Living Room SHIELD".into()),
            brand: "NVIDIA".into(),
            model: "SHIELD Android TV".into(),
            device_codename: "mdarcy".into(),
            manufacturer: "NVIDIA".into(),
            android_release: "11".into(),
            sdk_level: "30".into(),
            build_id: "RQ1A.210105.003".into(),
            board_platform: "tegra".into(),
            characteristics: "tv".into(),
            serial_number: "1324619053514".into(),
            leanback: Some(true),
            product: "mdarcy".into(),
        }
    }

    #[test]
    fn device_seen_payload_has_exactly_the_contract_fields() {
        let support = SupportInfo {
            tier: SupportTier::FullySupported,
            note: Some("n".into()),
        };
        let device = telemetry_device(
            &shield_props(),
            DeviceType::Shield,
            TvEvidence::Tv,
            &support,
        );
        let event = device_seen_event(&ctx(), "2026-10-31T12:00:00Z", device);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "schema": 2,
                "install_id": "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11",
                "app": "desktop",
                "app_version": "2.4.1",
                "os": "macos 15.1",
                "ts": "2026-10-31T12:00:00Z",
                "kind": "device_seen",
                "device": {
                    "manufacturer": "NVIDIA",
                    "brand": "NVIDIA",
                    "model": "SHIELD Android TV",
                    "device": "mdarcy",
                    "product": "mdarcy",
                    "android_release": "11",
                    "sdk": 30,
                    "profile": "shield",
                    "support_tier": "fully_supported"
                }
            })
        );
        let text = json.to_string();
        for secret in ["1324619053514", "Living Room", "RQ1A", "tegra"] {
            assert!(!text.contains(secret), "{secret} leaked into {text}");
        }
    }

    #[test]
    fn diagnostic_payload_is_the_reason_and_the_diag() {
        use super::super::telemetry_diag::{transport_counts, AdbDiag, AdbSource};
        let diag = Diag {
            adb: Some(AdbDiag {
                source: AdbSource::Bundled,
                version: Some("1.0.41".into()),
                tools: None,
                server: None,
                server_error: None,
            }),
            transports: Some(transport_counts(
                "List of devices attached\n10.0.0.2:5555\tunauthorized\n",
            )),
            ..Default::default()
        };
        let e = diagnostic_event(
            &ctx(),
            "2026-11-02T12:00:00Z",
            DiagReason::ScanEmpty,
            None,
            diag,
        );
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "schema": 2,
                "install_id": "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11",
                "app": "desktop",
                "app_version": "2.4.1",
                "os": "macos 15.1",
                "ts": "2026-11-02T12:00:00Z",
                "kind": "diagnostic",
                "action": "scan",
                "error_code": "scan_empty",
                "diag": {
                    "adb": { "source": "bundled", "version": "1.0.41" },
                    "transports": { "device": 0, "unauthorized": 1, "offline": 0, "other": 0 }
                }
            })
        );
        assert!(!json.to_string().contains("10.0.0.2"));
    }

    #[test]
    fn support_ids_shorten_like_the_frontend() {
        assert_eq!(
            format_support_id("7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11"),
            "7D0B-0B9E"
        );
        assert_eq!(format_support_id("7d0b"), "");
        assert_eq!(format_support_id(""), "");
    }

    #[test]
    fn local_host_names_and_subnet_prefixes_are_removed() {
        for (input, want) in [
            ("ping Bryans-MacBook-Pro.local failed", "ping <host> failed"),
            ("resolved living-room.local. late", "resolved <host> late"),
            (
                "No devices on 192.168.42.x answered",
                "No devices on <ip> answered",
            ),
            ("subnet 10.0.5", "subnet <ip>"),
            (
                "read ro.product.model failed",
                "read ro.product.model failed",
            ),
            (
                "dns 1a2b3c.dns.nextdns.io unreachable",
                "dns <host> unreachable",
            ),
            (
                "dns abc123.dns.nextdns.io unreachable",
                "dns <pkg> unreachable",
            ),
            ("via 1dot1dot1dot1.cloudflare-dns.com", "via <host>"),
            ("tools 35.0.2-12147458 ok", "tools 35.0.2-<id> ok"),
        ] {
            assert_eq!(redact_message(input), want, "input: {input}");
        }
    }

    #[test]
    fn unreadable_sdk_is_null_not_zero() {
        let mut p = shield_props();
        p.sdk_level = String::new();
        let d = telemetry_device(
            &p,
            DeviceType::Shield,
            TvEvidence::Tv,
            &SupportInfo::default(),
        );
        assert_eq!(d.sdk, None);
        assert_eq!(
            serde_json::to_value(&d).unwrap()["sdk"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn profiles_follow_detection() {
        let mut fire = shield_props();
        fire.manufacturer = "Amazon".into();
        fire.model = "AFTKA".into();
        assert_eq!(
            profile_for(&fire, DeviceType::GoogleTv, TvEvidence::Tv),
            DeviceProfile::Fire
        );
        let p = DeviceProperties::default();
        assert_eq!(
            profile_for(&p, DeviceType::GoogleTv, TvEvidence::Tv),
            DeviceProfile::Googletv
        );
        assert_eq!(
            profile_for(&p, DeviceType::Unknown, TvEvidence::Tv),
            DeviceProfile::Androidtv
        );
        assert_eq!(
            profile_for(&p, DeviceType::Unknown, TvEvidence::Unknown),
            DeviceProfile::Unknown
        );
    }

    #[test]
    fn successful_action_carries_no_message() {
        let e = action_event(
            &ctx(),
            "t",
            TelemetryAction::Disable,
            None,
            true,
            Some("x"),
            Some("Disabled com.example.secretapp"),
            &[],
        );
        assert_eq!(e.ok, Some(true));
        assert_eq!(e.error_code, None);
        assert_eq!(e.message, None);
        let json = serde_json::to_value(&e).unwrap();
        assert!(json.get("device").is_none());
        assert_eq!(json["action"], "disable");
    }

    #[test]
    fn failed_action_is_redacted_and_coded() {
        let e = action_event(
            &ctx(),
            "t",
            TelemetryAction::Connect,
            None,
            false,
            Some("Connection Refused!"),
            Some("adb connect 192.168.1.20:5555: failed to connect to 192.168.1.20:5555"),
            &[],
        );
        assert_eq!(e.error_code.as_deref(), Some("connection_refused"));
        assert_eq!(
            e.message.as_deref(),
            Some("adb connect <ip>: failed to connect to <ip>")
        );
    }

    #[test]
    fn redaction_strips_every_identifier_class() {
        let cases = [
            ("connect 10.0.0.5:37123 refused", "connect <ip> refused"),
            ("host 192.168.1.2 down", "host <ip> down"),
            ("addr [fe80::1c2:3ff:fe4d:5e6f]:5555 gone", "addr <ip> gone"),
            ("addr fe80::1c2:3ff:fe4d:5e6f%en0 gone", "addr <ip> gone"),
            ("mac 3c:a6:f6:12:ab:cd seen", "mac <mac> seen"),
            ("mac 3C-A6-F6-12-AB-CD seen", "mac <mac> seen"),
            (
                "adb-1324619053514-AbCdEf._adb-tls-connect._tcp offline",
                "<mdns> offline",
            ),
            ("tv.local:5555 refused", "<host> refused"),
            (
                "serial 1324619053514 unauthorized",
                "serial <id> unauthorized",
            ),
            (
                "serial 4A281FDAQ000QB unauthorized",
                "serial <id> unauthorized",
            ),
            ("mail bryan@example.com now", "mail <email> now"),
            (
                "open /Users/bryan/Library/ShieldOptimizer/snapshots/a.json: denied",
                "open <path>: denied",
            ),
            (
                "open C:\\Users\\bryan\\AppData\\x.json: failed",
                "open <path>: failed",
            ),
            (
                "Not a file: C:\\Users\\Jane Doe\\private backup.apk",
                "Not a file: <path>",
            ),
            ("copy /Users/Jane Doe/My Files/a b.apk", "copy <path>"),
            ("no file ~/Downloads/app.apk", "no file <path>"),
            (
                "pull '/sdcard/Movies/home video.mp4' failed",
                "pull '<path>' failed",
            ),
            (
                "pm disable-user com.example.secretapp: Failure",
                "pm disable-user <pkg>: Failure",
            ),
            (
                "Failure [DELETE_FAILED_INTERNAL_ERROR] for acme.player",
                "Failure [DELETE_FAILED_INTERNAL_ERROR] for <pkg>",
            ),
            (
                "Shell cannot change component state for com.android.gallery3d",
                "Shell cannot change component state for <pkg>",
            ),
            (
                "disable android.autoinstalls.config failed",
                "disable <pkg> failed",
            ),
            ("see https://example.com/path?q=1 now", "see <url> now"),
        ];
        for (input, want) in cases {
            assert_eq!(redact_message(input), want, "input: {input}");
        }
    }

    #[test]
    fn known_serials_are_removed_whatever_their_shape() {
        let msg = "device_profile: device ABCDEFABCDEF not found";
        assert!(
            redact_message(msg).contains("ABCDEFABCDEF"),
            "generic rules can't know this one"
        );
        assert_eq!(
            redact_message_with(msg, &["ABCDEFABCDEF", "", "ab"]),
            "device_profile: device <id> not found"
        );
        let e = error_event(
            &ctx(),
            "t",
            TelemetryAction::Other,
            None,
            "x",
            msg,
            &["ABCDEFABCDEF"],
        );
        assert!(!e.message.unwrap().contains("ABCDEF"));
        let a = action_event(
            &ctx(),
            "t",
            TelemetryAction::Connect,
            None,
            false,
            None,
            Some(msg),
            &["ABCDEFABCDEF"],
        );
        assert!(!a.message.unwrap().contains("ABCDEF"));
    }

    #[test]
    fn redaction_keeps_what_is_useful() {
        for keep in [
            "java.lang.SecurityException: Permission Denial",
            "android.os.RemoteException: transaction failed",
            "error: device offline",
            "std::io::Error timed out",
            "exit code 1",
            "SHIELD Android TV",
            "AFTMM",
        ] {
            assert_eq!(redact_message(keep), keep);
        }
    }

    #[test]
    fn messages_are_capped_on_a_char_boundary() {
        let long = "é".repeat(MAX_MESSAGE_CHARS * 2);
        let out = redact_message(&long);
        assert_eq!(out.chars().count(), MAX_MESSAGE_CHARS);
    }

    #[test]
    fn error_codes_are_short_snake_case() {
        assert_eq!(sanitize_error_code("Device Offline"), "device_offline");
        assert_eq!(sanitize_error_code("  wrong-pin!! "), "wrong_pin");
        assert_eq!(sanitize_error_code(""), "unknown");
        assert_eq!(sanitize_error_code("42"), "unknown");
        assert_eq!(sanitize_error_code("10.0.0.1"), "unknown");
        assert!(sanitize_error_code(&"a".repeat(200)).len() <= MAX_CODE_CHARS);
    }

    #[test]
    fn action_names_parse_and_fall_back_to_other() {
        assert_eq!(
            TelemetryAction::parse("snapshot_apply"),
            TelemetryAction::SnapshotApply
        );
        assert_eq!(
            TelemetryAction::parse("Settings_Write"),
            TelemetryAction::SettingsWrite
        );
        assert_eq!(TelemetryAction::parse("teleport"), TelemetryAction::Other);
    }

    #[test]
    fn batches_respect_count_and_size_limits() {
        let events: Vec<_> = (0..120)
            .map(|_| {
                error_event(
                    &ctx(),
                    "t",
                    TelemetryAction::Other,
                    None,
                    "x",
                    &"m".repeat(500),
                    &[],
                )
            })
            .collect();
        let bodies = encode_batches(&events);
        let mut total = 0;
        for body in &bodies {
            assert!(body.len() <= MAX_BATCH_BYTES);
            let v: serde_json::Value = serde_json::from_str(body).unwrap();
            let n = v["events"].as_array().unwrap().len();
            assert!(n <= MAX_BATCH_EVENTS && n > 0);
            total += n;
        }
        assert_eq!(total, 120);
        assert!(encode_batches(&[]).is_empty());
    }

    fn err(code: &str, message: &str) -> TelemetryEvent {
        error_event(
            &ctx(),
            "t",
            TelemetryAction::Other,
            None,
            code,
            message,
            &[],
        )
    }

    #[test]
    fn excerpts_go_to_failures_and_connection_failures_only() {
        let failed = action_event(
            &ctx(),
            "t",
            TelemetryAction::Connect,
            None,
            false,
            Some("timeout"),
            Some("timed out"),
            &[],
        );
        assert_eq!(
            excerpt_key(&failed).as_deref(),
            Some("action|connect|timeout")
        );
        let ok = action_event(
            &ctx(),
            "t",
            TelemetryAction::Connect,
            None,
            true,
            None,
            None,
            &[],
        );
        assert_eq!(excerpt_key(&ok), None);
        let seen = device_seen_event(
            &ctx(),
            "t",
            telemetry_device(
                &shield_props(),
                DeviceType::Shield,
                TvEvidence::Tv,
                &SupportInfo {
                    tier: SupportTier::FullySupported,
                    note: None,
                },
            ),
        );
        assert_eq!(excerpt_key(&seen), None);
        let diag = |reason| diagnostic_event(&ctx(), "t", reason, None, Diag::default());
        assert_eq!(
            excerpt_key(&diag(DiagReason::ScanConnectFailed)).as_deref(),
            Some("diag|scan_connect_failed")
        );
        for reason in [
            DiagReason::ScanEmpty,
            DiagReason::DeviceDetailsEmpty,
            DiagReason::DiscoverEmpty,
            DiagReason::UsageStatsEmpty,
        ] {
            assert_eq!(excerpt_key(&diag(reason)), None, "{}", reason.code());
        }
    }

    #[test]
    fn error_excerpt_keys_tell_commands_apart_without_message_text() {
        assert_eq!(
            excerpt_key(&err(
                "unknown",
                "list_packages: pm list failed for 1324619053514"
            ))
            .as_deref(),
            Some("error|other|unknown|list_packages")
        );
        assert_eq!(
            excerpt_key(&err("unknown", "read_props: closed")).as_deref(),
            Some("error|other|unknown|read_props")
        );
        // Anything that isn't a bare command name falls back to the code.
        for message in [
            "TypeError: x is undefined",
            "Command plugin:dialog|confirm not allowed by ACL",
            "no colon at all",
            "",
        ] {
            assert_eq!(
                excerpt_key(&err("uncaught_error", message)).as_deref(),
                Some("error|other|uncaught_error"),
                "{message}"
            );
        }
    }

    #[test]
    fn an_excerpt_goes_with_the_first_of_each_key_per_day() {
        let mut ledger = DailyLedger::default();
        let day = "2026-11-27";
        assert!(ledger.take_excerpt(day, "error|other|panic"));
        assert!(!ledger.take_excerpt(day, "error|other|panic"));
        // Another code still gets its first one.
        assert!(ledger.take_excerpt(day, "action|connect|timeout"));
        assert!(!ledger.take_excerpt(day, "action|connect|timeout"));
        assert_eq!(ledger.excerpts, 2);
        // A new day starts over.
        assert!(ledger.take_excerpt("2026-11-28", "error|other|panic"));
        assert_eq!(ledger.day, "2026-11-28");
        assert_eq!(ledger.excerpts, 1);
    }

    #[test]
    fn an_excerpt_the_log_could_not_fill_is_given_back() {
        let mut ledger = DailyLedger::default();
        let day = "2026-11-27";
        assert!(ledger.take_excerpt(day, "k"));
        ledger.return_excerpt(day, "k");
        assert_eq!(ledger.excerpts, 0);
        assert!(ledger.take_excerpt(day, "k"));
        // Returning on another day touches nothing.
        ledger.return_excerpt("2026-11-28", "k");
        assert_eq!(ledger.excerpts, 1);
    }

    #[test]
    fn the_daily_excerpt_budget_still_binds() {
        let mut ledger = DailyLedger::default();
        let day = "2026-11-27";
        for i in 0..EXCERPTS_PER_DAY {
            assert!(ledger.take_excerpt(day, &format!("k{i}")));
        }
        assert!(!ledger.take_excerpt(day, "one_more"));
    }

    #[test]
    fn errors_are_capped_per_day_but_never_the_first_of_a_kind() {
        let mut ledger = DailyLedger::default();
        let day = "2026-11-27";
        let sent = (0..100)
            .filter(|_| ledger.allow_error(day, "error|other|permission_denied"))
            .count();
        assert_eq!(sent as u32, ERRORS_PER_DAY);
        // Over the cap, a different error is still sent once, and only once.
        assert!(ledger.first_error_today(day, "error|other|panic"));
        assert!(ledger.allow_error(day, "error|other|panic"));
        assert!(!ledger.first_error_today(day, "error|other|panic"));
        assert!(!ledger.allow_error(day, "error|other|panic"));
        assert!(ledger.allow_error("2026-11-28", "error|other|permission_denied"));
    }

    #[test]
    fn once_a_day_reports_are_once_per_reason_and_model() {
        let mut ledger = DailyLedger::default();
        let day = "2026-11-27";
        let device = telemetry_device(
            &shield_props(),
            DeviceType::Shield,
            TvEvidence::Tv,
            &SupportInfo {
                tier: SupportTier::FullySupported,
                note: None,
            },
        );
        let key = once_a_day_key(DiagReason::UsageStatsEmpty, Some(&device));
        assert_eq!(key, "usage_stats_empty|NVIDIA|SHIELD Android TV");
        assert!(!key.contains("1324619053514") && !key.contains("Living Room"));
        assert!(!ledger.sent_today(day, &key));
        assert!(ledger.mark_sent(day, &key));
        assert!(ledger.sent_today(day, &key));
        let other = once_a_day_key(DiagReason::DiskStatsEmpty, Some(&device));
        assert!(!ledger.sent_today(day, &other));
        ledger.unmark_sent(day, &key);
        assert!(!ledger.sent_today(day, &key));
        assert!(ledger.mark_sent(day, &key));
        assert!(!ledger.sent_today("2026-11-28", &key));
    }

    #[test]
    fn a_full_ledger_sends_less_never_more() {
        let mut ledger = DailyLedger::default();
        let day = "2026-11-27";
        for i in 0..MAX_LEDGER_KEYS {
            assert!(ledger.mark_sent(day, &format!("k{i}")));
        }
        assert!(!ledger.mark_sent(day, "late"));
        assert!(!ledger.sent_today(day, "late"));
        assert!(!ledger.take_excerpt(day, "late"));
        // An error the ledger can't record is still treated as a first one,
        // so the daily ceiling never drops it.
        ledger.errors = ERRORS_PER_DAY;
        assert!(ledger.first_error_today(day, "late"));
        assert!(ledger.allow_error(day, "late"));
        assert!(ledger.allow_error(day, "late"));
        ledger.forget_keys();
        assert!(ledger.mark_sent(day, "late"));
        assert_eq!(ledger.errors, 0);
    }

    #[test]
    fn the_ledger_reads_an_older_excerpt_budget() {
        let old: DailyLedger =
            serde_json::from_str(r#"{"excerpt_day":"2026-11-27","excerpts_today":7}"#).unwrap();
        assert_eq!(old.day, "2026-11-27");
        assert_eq!(old.excerpts, 7);
        assert!(old.keys.is_empty());
        let fresh = serde_json::to_value(DailyLedger::default()).unwrap();
        assert_eq!(
            fresh,
            serde_json::json!({ "excerpts_today": 0, "errors_today": 0 })
        );
    }
}
