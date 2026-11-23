//! What a user sends the developer by hand: Send logs and Report a bug.
//!
//! Pure: the request body, the summary lines and the reading of the reply.
//! The apps gather the facts, redact them and post the result
//! ([`crate::telemetry::post_upload`]).
//!
//! Send logs keeps the original body (schema 1). A bug report is schema 2,
//! `kind: "bug"`, with the user's description in `note`, scrubbed by
//! [`super::log_redact::redact_note`] before it leaves.

use serde::Serialize;

use super::log_redact::{self, Scrub};
use super::telemetry::{format_support_id, telemetry_device, AppKind};
use super::telemetry_diag::classify_error_text;
use super::types::Device;

/// The longest description a report carries, in characters.
pub const NOTE_MAX_CHARS: usize = 4000;
/// The phone's report is a short summary, never a log.
pub const MOBILE_TEXT_MAX_BYTES: usize = 64 * 1024;
/// What the server accepts from desktop: a 16 KiB summary and 256 KiB of log.
pub const DESKTOP_TEXT_MAX_BYTES: usize = 300 * 1024;
pub const LOGS_SCHEMA: u32 = 1;
pub const BUG_SCHEMA: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UploadKind {
    Logs,
    Bug,
}

impl UploadKind {
    /// How errors name the upload, so the frontend's error codes still match
    /// Send logs' messages (`shared/errors.ts`).
    pub fn noun(self) -> &'static str {
        match self {
            UploadKind::Logs => "Log upload",
            UploadKind::Bug => "Bug report",
        }
    }
}

/// Who is sending, from [`crate::telemetry::UploadTarget`].
#[derive(Debug, Clone, Copy)]
pub struct UploadMeta<'a> {
    pub install_id: &'a str,
    pub app: AppKind,
    pub app_version: &'a str,
    pub os: &'a str,
    /// RFC 3339.
    pub ts: &'a str,
    pub reports_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct UploadBody<'a> {
    pub schema: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<UploadKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'a str>,
    pub install_id: &'a str,
    pub app: AppKind,
    pub app_version: &'a str,
    pub os: &'a str,
    pub ts: &'a str,
    pub reports_enabled: bool,
    pub text: &'a str,
}

/// The largest summary an app may send.
pub fn text_limit(app: AppKind) -> usize {
    match app {
        AppKind::Desktop => DESKTOP_TEXT_MAX_BYTES,
        AppKind::Mobile => MOBILE_TEXT_MAX_BYTES,
    }
}

/// Send logs: the schema 1 body every released server accepts.
pub fn logs_body<'a>(meta: &UploadMeta<'a>, text: &'a str) -> UploadBody<'a> {
    UploadBody {
        schema: LOGS_SCHEMA,
        kind: None,
        note: None,
        install_id: meta.install_id,
        app: meta.app,
        app_version: meta.app_version,
        os: meta.os,
        ts: meta.ts,
        reports_enabled: meta.reports_enabled,
        text,
    }
}

/// Report a bug. Refuses a report with nothing in it, and a summary over
/// the app's limit (the builders cap theirs, so that is a bug here).
pub fn bug_body<'a>(
    meta: &UploadMeta<'a>,
    note: Option<&'a str>,
    text: &'a str,
) -> Result<UploadBody<'a>, String> {
    let note = note.filter(|n| !n.trim().is_empty());
    if note.is_none() && text.trim().is_empty() {
        return Err("Describe the problem before sending the report.".to_string());
    }
    if text.len() > text_limit(meta.app) {
        return Err("Bug report failed: the report is too large to send.".to_string());
    }
    Ok(UploadBody {
        schema: BUG_SCHEMA,
        kind: Some(UploadKind::Bug),
        note,
        install_id: meta.install_id,
        app: meta.app,
        app_version: meta.app_version,
        os: meta.os,
        ts: meta.ts,
        reports_enabled: meta.reports_enabled,
        text,
    })
}

/// The user's description, trimmed, scrubbed of the identifiers `scrub`
/// knows and of anything shaped like an address, and cut to
/// [`NOTE_MAX_CHARS`]. `None` when nothing is left.
pub fn clean_note(raw: &str, scrub: &Scrub) -> Option<String> {
    // Bound the work before the regexes run; redaction only shortens text
    // by a little, so a little over the limit is enough.
    let bounded: String = raw.trim().chars().take(NOTE_MAX_CHARS * 2).collect();
    let redacted = log_redact::redact_note(&bounded, scrub);
    let cut: String = redacted.trim().chars().take(NOTE_MAX_CHARS).collect();
    let cut = cut.trim_end().to_string();
    (!cut.is_empty()).then_some(cut)
}

/// `text` cut to at most `max_bytes`, at a line boundary where there is one,
/// with a line saying it was cut.
pub fn cap_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    const MARK: &str = "\n(cut to fit the report)\n";
    let budget = max_bytes.saturating_sub(MARK.len());
    let mut end = budget.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    let head = head.rfind('\n').map_or(head, |i| &head[..i]);
    format!("{head}{MARK}")
}

/// One line for a TV: connection, state and its public build properties.
/// No serial, address or name.
pub fn device_summary_line(d: &Device) -> String {
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
}

/// The identifiers a TV's own record carries, to scrub from anything sent.
pub fn device_identifiers(d: &Device) -> Vec<String> {
    let mut out = vec![d.serial.clone(), d.name.clone()];
    if let Some(props) = &d.properties {
        out.push(props.serial_number.clone());
        if let Some(name) = &props.friendly_name {
            out.push(name.clone());
        }
    }
    out.retain(|s| !s.trim().is_empty());
    out
}

/// The phone's view of the TV it was connected to when the report was made.
#[derive(Debug, Clone, Copy)]
pub enum MobileTv<'a> {
    /// No TV connected: nothing about a TV is claimed.
    NotConnected,
    Connected(&'a Device),
    /// Connected, but its details couldn't be read; only the kind of error.
    Unreadable(&'a str),
}

/// The connection steps the frontend recorded, and nothing else it might
/// send: commands outside this list are dropped.
pub const CONNECTION_COMMANDS: [&str; 4] = [
    "wireless_discover",
    "wireless_pair",
    "wireless_connect",
    "wireless_status",
];
const CONNECTION_HISTORY_MAX: usize = 8;

/// What the phone app's bug report says about itself, before redaction.
#[derive(Debug, Clone, Copy)]
pub struct MobileSummary<'a> {
    pub install_id: Option<&'a str>,
    pub app_version: &'a str,
    /// `android 15`.
    pub os: &'a str,
    /// `Google Pixel 10 Pro`, when the phone said.
    pub phone_model: Option<&'a str>,
    pub reports_enabled: bool,
    pub test_build: Option<&'a str>,
    /// A Play release build attaches a Play Integrity token to the upload.
    /// The summary says so, so the preview shows everything that goes.
    pub play_integrity: bool,
    pub tv: MobileTv<'a>,
    /// `(command, ok)`, oldest first.
    pub history: &'a [(String, bool)],
}

/// The phone app's bug report summary: the app and phone, the support id,
/// the connected TV's public build properties and recent connection
/// outcomes. No package list, TV name, address or serial; the TV and
/// history lines go through the same redaction as a desktop upload.
pub fn mobile_summary(s: &MobileSummary, scrub: &Scrub) -> String {
    let mut text = String::new();
    if let Some(commit) = s.test_build {
        text.push_str(&format!("Test build {commit}\n\n"));
    }
    text.push_str("## ATV Optimizer bug report (Android)\n\n");
    match s.install_id {
        Some(id) => text.push_str(&format!(
            "- Support ID: {} (`{id}`)\n",
            format_support_id(id)
        )),
        None => text.push_str("- Support ID: none\n"),
    }
    text.push_str(&format!("- App version: {}\n", s.app_version));
    text.push_str(&format!(
        "- Phone: {}, {}\n",
        s.os,
        s.phone_model.unwrap_or("model unknown")
    ));
    text.push_str(&format!(
        "- Automatic reports: {}\n",
        if s.reports_enabled { "on" } else { "off" }
    ));
    text.push_str("- ADB: built-in wireless client\n");
    if s.play_integrity {
        text.push_str("- Google Play Integrity check: attached when Play provides one\n");
    }

    let mut rest = String::from("\n### TV\n\n");
    match s.tv {
        MobileTv::NotConnected => rest.push_str("- No TV connected\n"),
        MobileTv::Connected(device) => {
            rest.push_str(&device_summary_line(device));
            rest.push('\n');
        }
        MobileTv::Unreadable(error) => rest.push_str(&format!(
            "- Connected, details unreadable ({})\n",
            classify_error_text(error)
        )),
    }
    let history: Vec<&(String, bool)> = s
        .history
        .iter()
        .filter(|(command, _)| CONNECTION_COMMANDS.contains(&command.as_str()))
        .collect();
    rest.push_str("\n### Recent connection steps\n\n");
    if history.is_empty() {
        rest.push_str("- None recorded\n");
    }
    let start = history.len().saturating_sub(CONNECTION_HISTORY_MAX);
    for (command, ok) in &history[start..] {
        rest.push_str(&format!(
            "- {command}: {}\n",
            if *ok { "ok" } else { "failed" }
        ));
    }
    text.push_str(&log_redact::redact_text(
        &rest,
        scrub,
        MOBILE_TEXT_MAX_BYTES,
    ));
    text.push('\n');
    cap_text(&text, MOBILE_TEXT_MAX_BYTES)
}

/// The report number from a 201 answer (`{ ok, id, bytes }`), when it has one.
pub fn receipt_id(body: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    match value.get("id")? {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// What to tell the user when the server refused an upload.
pub fn refusal_message(kind: UploadKind, status: u16, body: &str) -> String {
    let code = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.as_str().map(str::to_string))
        .filter(|c| {
            !c.is_empty()
                && c.len() <= 40
                && c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_')
        });
    match (kind, status, code.as_deref()) {
        (UploadKind::Logs, 429, _) => "Log upload limit reached for today. Try again tomorrow, or attach the bug report instead.".to_string(),
        (UploadKind::Bug, 429, Some("upload_global_cap")) => "Bug report limit reached: the developer's server has taken all the reports it can for today. Try again tomorrow, or report it on GitHub instead.".to_string(),
        (UploadKind::Bug, 429, _) => "Bug report limit reached: this app has sent today's maximum. Try again tomorrow, or report it on GitHub instead.".to_string(),
        (kind, 400, Some(code)) => format!("{} failed: the server refused it ({code}).", kind.noun()),
        (kind, status, _) => format!("{} failed: the server responded with {status}.", kind.noun()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ConnectionType, DeviceProperties, DeviceStatus};
    use crate::engine::{DeviceType, SupportInfo, TvEvidence};

    const ID: &str = "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11";

    fn meta(app: AppKind) -> UploadMeta<'static> {
        UploadMeta {
            install_id: ID,
            app,
            app_version: "0.1.4",
            os: "android 15",
            ts: "2026-11-22T10:00:00.000Z",
            reports_enabled: false,
        }
    }

    fn shield() -> Device {
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
                device_codename: "mdarcy".into(),
                serial_number: "1324619053514".into(),
                android_release: "11".into(),
                sdk_level: "30".into(),
                ..Default::default()
            }),
            support: SupportInfo::default(),
        }
    }

    #[test]
    fn a_bug_report_is_schema_two_with_its_kind_and_note() {
        let body = bug_body(&meta(AppKind::Mobile), Some("It froze"), "summary").unwrap();
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["schema"], 2);
        assert_eq!(json["kind"], "bug");
        assert_eq!(json["note"], "It froze");
        assert_eq!(json["app"], "mobile");
        assert_eq!(json["install_id"], ID);
        assert_eq!(json["app_version"], "0.1.4");
        assert_eq!(json["os"], "android 15");
        assert_eq!(json["ts"], "2026-11-22T10:00:00.000Z");
        assert_eq!(json["reports_enabled"], false);
        assert_eq!(json["text"], "summary");
    }

    #[test]
    fn send_logs_keeps_the_schema_one_body() {
        let json = serde_json::to_value(logs_body(&meta(AppKind::Desktop), "log")).unwrap();
        assert_eq!(json["schema"], 1);
        assert!(json.get("kind").is_none(), "{json}");
        assert!(json.get("note").is_none(), "{json}");
        assert_eq!(json["app"], "desktop");
    }

    #[test]
    fn an_empty_bug_report_is_refused_and_a_blank_note_is_left_out() {
        assert!(bug_body(&meta(AppKind::Desktop), None, "  ").is_err());
        assert!(bug_body(&meta(AppKind::Desktop), Some("  "), "").is_err());
        let body = bug_body(&meta(AppKind::Desktop), Some(" "), "summary").unwrap();
        assert!(serde_json::to_value(&body).unwrap().get("note").is_none());
    }

    #[test]
    fn the_phone_summary_never_exceeds_its_cap() {
        let big = "x".repeat(MOBILE_TEXT_MAX_BYTES + 10);
        assert!(bug_body(&meta(AppKind::Mobile), Some("n"), &big).is_err());
        assert!(bug_body(&meta(AppKind::Desktop), Some("n"), &big).is_ok());

        let capped = cap_text(&format!("first line\n{big}"), MOBILE_TEXT_MAX_BYTES);
        assert!(capped.len() <= MOBILE_TEXT_MAX_BYTES, "{}", capped.len());
        assert!(capped.starts_with("first line"));
        assert!(capped.ends_with("(cut to fit the report)\n"));
        assert_eq!(cap_text("short", 100), "short");
        // Never splits a character.
        let wide = "é".repeat(100);
        let cut = cap_text(&wide, 61);
        assert!(cut.len() <= 61, "{cut}");
    }

    #[test]
    fn the_note_is_scrubbed_of_addresses_and_known_identifiers() {
        let scrub = Scrub {
            known: &["QWERTYUIOPAS", "Bryan's Den"],
            words: &[],
        };
        let note = clean_note(
            "  Bryan's Den TV at 192.168.1.20:5555 (serial QWERTYUIOPAS, mac aa:bb:cc:dd:ee:ff) froze.\nSecond line\r\n",
            &scrub,
        )
        .unwrap();
        for secret in ["Bryan's Den", "192.168", "QWERTYUIOPAS", "aa:bb:cc"] {
            assert!(!note.contains(secret), "{secret} leaked: {note}");
        }
        assert!(note.contains("froze."), "{note}");
        assert!(note.contains("\nSecond line"), "{note}");
        assert!(!note.contains('\r'));
        assert_eq!(clean_note("   \n ", &scrub), None);
    }

    #[test]
    fn the_note_loses_the_tv_name_however_it_is_typed() {
        let device = shield();
        let ids = device_identifiers(&device);
        let known: Vec<&str> = ids.iter().map(String::as_str).collect();
        let scrub = Scrub {
            known: &known,
            words: &[],
        };
        let note = clean_note("living room tv drops after a minute", &scrub).unwrap();
        assert!(!note.to_lowercase().contains("living room"), "{note}");
        assert!(note.ends_with("tv drops after a minute"), "{note}");
    }

    #[test]
    fn the_note_is_cut_to_its_limit() {
        let note = clean_note(&"word ".repeat(2000), &Scrub::default()).unwrap();
        assert!(note.chars().count() <= NOTE_MAX_CHARS);
        assert!(note.chars().count() > NOTE_MAX_CHARS - 10);
    }

    #[test]
    fn the_phone_summary_carries_the_tv_build_and_no_identity() {
        let device = shield();
        let ids = device_identifiers(&device);
        let known: Vec<&str> = ids.iter().map(String::as_str).collect();
        let history = vec![
            ("wireless_discover".to_string(), true),
            ("wireless_connect".to_string(), false),
            ("list_packages 192.168.1.20:5555".to_string(), true),
        ];
        let text = mobile_summary(
            &MobileSummary {
                install_id: Some(ID),
                app_version: "0.1.4",
                os: "android 15",
                phone_model: Some("Google Pixel 10 Pro"),
                reports_enabled: true,
                test_build: None,
                play_integrity: true,
                tv: MobileTv::Connected(&device),
                history: &history,
            },
            &Scrub {
                known: &known,
                words: &[],
            },
        );
        assert!(
            text.contains("- Support ID: 7D0B-0B9E (`7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11`)"),
            "{text}"
        );
        assert!(
            text.contains("- Phone: android 15, Google Pixel 10 Pro"),
            "{text}"
        );
        assert!(text.contains("- Automatic reports: on"), "{text}");
        assert!(
            text.contains("- Google Play Integrity check: attached when Play provides one"),
            "{text}"
        );
        assert!(
            text.contains("NVIDIA SHIELD Android TV (mdarcy), Android 11 (SDK 30)"),
            "{text}"
        );
        assert!(text.contains("- wireless_connect: failed"), "{text}");
        assert!(!text.contains("list_packages"), "{text}");
        for secret in ["192.168", "Living Room", "1324619053514"] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }

    #[test]
    fn the_phone_summary_claims_nothing_it_did_not_read() {
        let summary = |tv| {
            mobile_summary(
                &MobileSummary {
                    install_id: None,
                    app_version: "0.1.4",
                    os: "android",
                    phone_model: None,
                    reports_enabled: false,
                    test_build: Some("4f156e4"),
                    play_integrity: false,
                    tv,
                    history: &[],
                },
                &Scrub::default(),
            )
        };
        let none = summary(MobileTv::NotConnected);
        assert!(none.starts_with("Test build 4f156e4\n"), "{none}");
        assert!(none.contains("- Support ID: none"), "{none}");
        assert!(none.contains("- Phone: android, model unknown"), "{none}");
        assert!(none.contains("- No TV connected"), "{none}");
        assert!(none.contains("- None recorded"), "{none}");
        assert!(!none.contains("Integrity"), "{none}");
        let unreadable = summary(MobileTv::Unreadable(
            "error: device '192.168.1.20:5555' not found",
        ));
        assert!(
            unreadable.contains("- Connected, details unreadable ("),
            "{unreadable}"
        );
        assert!(!unreadable.contains("192.168"), "{unreadable}");
    }

    #[test]
    fn device_summaries_carry_no_serial_or_name() {
        let unauthorized = Device {
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
        };
        let lines = [shield(), unauthorized]
            .iter()
            .map(device_summary_line)
            .collect::<Vec<_>>()
            .join("\n");
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
    fn the_reply_gives_the_report_number_and_refusals_say_why() {
        assert_eq!(receipt_id(r#"{"ok":true,"id":42,"bytes":10}"#), Some(42));
        assert_eq!(receipt_id(r#"{"ok":true,"id":"43"}"#), Some(43));
        assert_eq!(receipt_id("not json"), None);

        let daily = refusal_message(UploadKind::Bug, 429, r#"{"error":"upload_daily_cap"}"#);
        assert!(
            daily.contains("this app has sent today's maximum"),
            "{daily}"
        );
        let global = refusal_message(UploadKind::Bug, 429, r#"{"error":"upload_global_cap"}"#);
        assert!(
            global.contains("all the reports it can for today"),
            "{global}"
        );
        assert_eq!(
            refusal_message(UploadKind::Bug, 400, r#"{"error":"bad_note"}"#),
            "Bug report failed: the server refused it (bad_note)."
        );
        // A reason that isn't a plain code is not repeated to the user.
        assert_eq!(
            refusal_message(UploadKind::Bug, 400, r#"{"error":"<b>x</b>"}"#),
            "Bug report failed: the server responded with 400."
        );
        assert!(refusal_message(UploadKind::Logs, 429, "").starts_with("Log upload limit reached"));
        assert_eq!(
            refusal_message(UploadKind::Logs, 503, ""),
            "Log upload failed: the server responded with 503."
        );
    }
}
