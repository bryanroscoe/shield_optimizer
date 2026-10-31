//! The telemetry setting and the frontend's report hooks.
//!
//! The frontend tells Rust what happened (an action's outcome, an uncaught
//! error); Rust decides whether anything is sent, builds the payload and
//! redacts it. A disabled or unconfigured hub makes all of these no-ops, and
//! none of them can fail a user action: they return `Ok` either way.

use tauri::{AppHandle, Runtime, State};

use crate::engine::telemetry::TelemetryAction;
use crate::telemetry::TelemetrySettings;

use super::AppState;

#[tauri::command]
pub async fn telemetry_settings(state: State<'_, AppState>) -> Result<TelemetrySettings, String> {
    Ok(state.telemetry.settings())
}

/// Turning reports back on also refreshes the support tiers, which are only
/// fetched while reports are on.
#[tauri::command]
pub async fn set_telemetry_enabled<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<TelemetrySettings, String> {
    let was_enabled = state.telemetry.settings().enabled;
    let settings = state.telemetry.set_enabled(enabled)?;
    if enabled && !was_enabled {
        crate::telemetry::spawn_support_refresh(app);
    }
    Ok(settings)
}

#[tauri::command]
pub async fn dismiss_telemetry_notice(
    state: State<'_, AppState>,
) -> Result<TelemetrySettings, String> {
    state.telemetry.dismiss_notice()
}

/// The outcome of one tracked action (connect, disable, snapshot apply, …).
#[tauri::command]
pub async fn telemetry_action(
    state: State<'_, AppState>,
    action: String,
    serial: Option<String>,
    ok: bool,
    error_code: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    state.telemetry.record_action(
        TelemetryAction::parse(&action),
        serial.as_deref(),
        ok,
        error_code.as_deref(),
        message.as_deref(),
    );
    Ok(())
}

/// An uncaught frontend error, an unhandled rejection, or a failed command
/// outside the tracked actions. Rate limited in the hub.
#[tauri::command]
pub async fn telemetry_error(
    state: State<'_, AppState>,
    error_code: String,
    message: String,
    action: Option<String>,
    serial: Option<String>,
) -> Result<(), String> {
    state.telemetry.record_error(
        action
            .as_deref()
            .map(TelemetryAction::parse)
            .unwrap_or(TelemetryAction::Other),
        serial.as_deref(),
        &error_code,
        &message,
    );
    Ok(())
}
