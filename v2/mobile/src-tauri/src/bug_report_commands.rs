//! More › Report a bug: the phone's report to the developer.
//!
//! The user writes a description, reviews exactly what will be sent
//! ([`preview_bug_report`]) and presses Send ([`send_bug_report`]). Both
//! build the same text: the description and a short summary of the app, the
//! phone and the connected TV's public build properties, scrubbed by the
//! shared redaction. No log, package list, TV name, address or serial.

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

#[derive(Debug, Serialize)]
pub struct BugReportPreview {
    /// The description as it will be sent, after redaction.
    pub note: Option<String>,
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

struct Built {
    note: Option<String>,
    text: String,
}

async fn build(
    state: &AppState,
    note: &str,
    serial: Option<&str>,
    history: &[ConnectionStep],
) -> Result<Built, String> {
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
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let scrub = Scrub {
        known: &known,
        words: &[],
    };
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
        &scrub,
    );
    Ok(Built {
        note: bug_report::clean_note(note, &scrub),
        text,
    })
}

fn size(built: &Built) -> usize {
    built.text.len() + built.note.as_deref().map_or(0, str::len)
}

/// `preview_bug_report` — exactly what Send would upload. Nothing is sent.
#[tauri::command]
pub async fn preview_bug_report(
    state: State<'_, AppState>,
    note: String,
    serial: Option<String>,
    history: Vec<ConnectionStep>,
) -> Result<BugReportPreview, String> {
    let built = build(state.inner(), &note, serial.as_deref(), &history).await?;
    Ok(BugReportPreview {
        bytes: size(&built),
        note: built.note,
        text: built.text,
    })
}

/// `send_bug_report` — upload the report under the current install id, so
/// the Support ID it returns finds it. The user pressed Send, so this runs
/// whether or not automatic reports are on.
#[tauri::command]
pub async fn send_bug_report(
    state: State<'_, AppState>,
    note: String,
    serial: Option<String>,
    history: Vec<ConnectionStep>,
) -> Result<BugReportResult, String> {
    if note.trim().is_empty() {
        return Err("Describe the problem before sending the report.".to_string());
    }
    let telemetry = state.telemetry.clone();
    let (Some(target), Some(client)) = (telemetry.upload_target(), telemetry.http_client()) else {
        return Err(UNAVAILABLE.to_string());
    };
    let built = build(state.inner(), &note, serial.as_deref(), &history).await?;
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let meta = UploadMeta {
        install_id: &target.install_id,
        app: target.app,
        app_version: &target.app_version,
        os: &target.os,
        ts: &ts,
        reports_enabled: telemetry.settings().enabled,
    };
    let body = bug_report::bug_body(&meta, built.note.as_deref(), &built.text)?;
    let body = serde_json::to_string(&body).map_err(|e| format!("Bug report failed: {e}"))?;
    let receipt = post_upload(&client, &target.url, UploadKind::Bug, body).await?;
    tracing::info!(bytes = size(&built), "bug report sent to the developer");
    Ok(BugReportResult {
        support_id: target.install_id,
        bytes: size(&built),
        report_id: receipt.id,
    })
}
