//! More › Report a bug: the phone's report to the developer.
//!
//! The user writes a description, reviews exactly what will be sent and
//! presses Send. The summary (the app, the phone and the connected TV's
//! public build properties, scrubbed by the shared redaction) is built once
//! by [`prepare_bug_report`], which profiles the TV, and kept here.
//! [`redact_bug_note`] redacts the description against it as it is typed,
//! and [`send_bug_report`] uploads that same summary with the description
//! redacted the same way, so what the preview showed is what goes. The
//! frontend never hands text back to be sent. No log, package list, TV
//! name, address or serial.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use atv_optimizer_core::commands::{devices, AppState};
use atv_optimizer_core::engine::bug_report::{
    self, device_identifiers, MobileSummary, MobileTv, UploadKind, UploadMeta,
};
use atv_optimizer_core::engine::log_redact::Scrub;
use atv_optimizer_core::telemetry::{host_device_model, post_upload};
use serde::{Deserialize, Serialize};
use tauri::State;

/// One connection step the frontend recorded: the command and whether it
/// worked. Anything else it holds stays on the phone.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionStep {
    pub command: String,
    pub ok: bool,
}

/// The prepared summary as the preview shows it.
#[derive(Debug, Serialize)]
pub struct BugReportSummary {
    /// What Send names to upload this summary.
    pub id: u64,
    pub text: String,
    pub bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct BugReportResult {
    pub support_id: String,
    pub bytes: usize,
    pub report_id: Option<u64>,
}

const UNAVAILABLE: &str = "Bug report isn't available in this build.";
const EXPIRED: &str =
    "Bug report failed: the report changed. Show what's sent again, then press Send.";

struct Prepared {
    id: u64,
    text: String,
    /// The identifiers the summary was scrubbed of, for the description.
    known: Vec<String>,
}

impl Prepared {
    fn note(&self, raw: &str) -> Option<String> {
        let known: Vec<&str> = self.known.iter().map(String::as_str).collect();
        bug_report::clean_note(
            raw,
            &Scrub {
                known: &known,
                words: &[],
            },
        )
    }
}

/// The one summary the open sheet is working with.
#[derive(Default)]
pub struct PreparedBugReport {
    current: Mutex<Option<Prepared>>,
    next_id: AtomicU64,
}

impl PreparedBugReport {
    fn with<T>(&self, id: u64, f: impl FnOnce(&Prepared) -> T) -> Result<T, String> {
        let current = self.current.lock().map_err(|_| EXPIRED.to_string())?;
        match current.as_ref() {
            Some(prepared) if prepared.id == id => Ok(f(prepared)),
            _ => Err(EXPIRED.to_string()),
        }
    }
}

async fn build_summary(
    state: &AppState,
    serial: Option<&str>,
    history: &[ConnectionStep],
) -> Result<(String, Vec<String>), String> {
    let telemetry = &state.telemetry;
    let target = telemetry.upload_target().ok_or(UNAVAILABLE)?;
    let profile = match serial {
        Some(serial) => Some(devices::device_profile_impl(state, serial).await),
        None => None,
    };
    let mut known = telemetry.known_identifiers();
    known.extend(serial.map(str::to_string));
    if let Some(Ok(device)) = &profile {
        known.extend(device_identifiers(device));
    }
    let tv = match &profile {
        None => MobileTv::NotConnected,
        Some(Ok(device)) => MobileTv::Connected(device),
        Some(Err(error)) => MobileTv::Unreadable(error),
    };
    let history: Vec<(String, bool)> = history
        .iter()
        .map(|step| (step.command.clone(), step.ok))
        .collect();
    let phone_model = host_device_model();
    let known_refs: Vec<&str> = known.iter().map(String::as_str).collect();
    let text = bug_report::mobile_summary(
        &MobileSummary {
            install_id: Some(target.install_id.as_str()),
            app_version: &target.app_version,
            os: &target.os,
            phone_model: phone_model.as_deref(),
            reports_enabled: telemetry.settings().enabled,
            test_build: target.test_build.as_deref(),
            tv,
            history: &history,
        },
        &Scrub {
            known: &known_refs,
            words: &[],
        },
    );
    Ok((text, known))
}

/// `prepare_bug_report` — build the summary Send will upload, once: this
/// profiles the connected TV. Replaces any earlier one. Nothing is sent.
#[tauri::command]
pub async fn prepare_bug_report(
    state: State<'_, AppState>,
    prepared: State<'_, PreparedBugReport>,
    serial: Option<String>,
    history: Vec<ConnectionStep>,
) -> Result<BugReportSummary, String> {
    let (text, known) = build_summary(state.inner(), serial.as_deref(), &history).await?;
    let id = prepared.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let summary = BugReportSummary {
        id,
        bytes: text.len(),
        text: text.clone(),
    };
    let mut current = prepared
        .current
        .lock()
        .map_err(|_| "Bug report failed: try again.".to_string())?;
    *current = Some(Prepared { id, text, known });
    Ok(summary)
}

/// `redact_bug_note` — the description as Send would upload it with the
/// prepared summary `id`. Cheap: no TV is read. `None` when nothing is left.
#[tauri::command]
pub fn redact_bug_note(
    prepared: State<'_, PreparedBugReport>,
    id: u64,
    note: String,
) -> Result<Option<String>, String> {
    prepared.with(id, |p| p.note(&note))
}

/// `send_bug_report` — upload the prepared summary `id` and the description
/// under the current install id, so the Support ID it returns finds it. The
/// user pressed Send, so this runs whether or not automatic reports are on.
/// A refused report keeps its summary, so trying again sends the same one.
#[tauri::command]
pub async fn send_bug_report(
    state: State<'_, AppState>,
    prepared: State<'_, PreparedBugReport>,
    id: u64,
    note: String,
) -> Result<BugReportResult, String> {
    if note.trim().is_empty() {
        return Err("Describe the problem before sending the report.".to_string());
    }
    let telemetry = state.telemetry.clone();
    let (Some(target), Some(client)) = (telemetry.upload_target(), telemetry.http_client()) else {
        return Err(UNAVAILABLE.to_string());
    };
    let (text, note) = prepared.with(id, |p| (p.text.clone(), p.note(&note)))?;
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let meta = UploadMeta {
        install_id: &target.install_id,
        app: target.app,
        app_version: &target.app_version,
        os: &target.os,
        ts: &ts,
        reports_enabled: telemetry.settings().enabled,
    };
    let body = bug_report::bug_body(&meta, note.as_deref(), &text)?;
    let body = serde_json::to_string(&body).map_err(|e| format!("Bug report failed: {e}"))?;
    let receipt = post_upload(&client, &target.url, UploadKind::Bug, body).await?;
    let bytes = text.len() + note.as_deref().map_or(0, str::len);
    tracing::info!(bytes, "bug report sent to the developer");
    if let Ok(mut current) = prepared.current.lock() {
        if current.as_ref().is_some_and(|p| p.id == id) {
            *current = None;
        }
    }
    Ok(BugReportResult {
        support_id: target.install_id,
        bytes,
        report_id: receipt.id,
    })
}
