//! Play Integrity, record-only.
//!
//! Play release builds attach a Standard Play Integrity token to the first
//! automatic report batch of the day (only while reports are on) and to a bug
//! report the user sends. The token's request hash is the SHA-256 of the body
//! it travels with, and the developer's server asks Google what it says and
//! stores a coarse summary. Nothing in the app ever sees a verdict: no
//! feature, Pro entitlement or safety gate depends on one, and a phone where
//! Play can't answer (no Play Store or Play services, an emulator, a
//! sideloaded or debug build) simply sends without a token.

use std::sync::Arc;
use std::time::Duration;

use atv_optimizer_core::telemetry::IntegritySource;
use tauri::AppHandle;
use tauri_plugin_atv_billing::BillingExt;

use crate::play_billing::Distribution;

/// The Google Cloud project linked to the app in Play Console
/// (`atv-optimizer-play`). Not a secret: it only names where Google keeps the
/// API's quota and settings.
pub const CLOUD_PROJECT_NUMBER: i64 = 694_361_256_028;

/// How long a bug report waits for a token before it is sent without one.
pub const BUG_REPORT_WAIT: Duration = Duration::from_secs(5);
/// The background report batch can wait longer; nobody is watching it.
const REPORT_WAIT: Duration = Duration::from_secs(15);

/// Whether this build asks at all: Play release builds on Android. Anything
/// else wasn't distributed by Play, so there is nothing to ask Play about.
pub fn available() -> bool {
    enabled_for(Distribution::current(), cfg!(debug_assertions)) && cfg!(target_os = "android")
}

fn enabled_for(distribution: Distribution, debug: bool) -> bool {
    distribution == Distribution::Play && !debug
}

/// A token bound to `request_hash`, or `None` after `wait` or whenever Play
/// can't give one.
pub async fn token(app: &AppHandle, request_hash: String, wait: Duration) -> Option<String> {
    if !available() {
        return None;
    }
    let app = app.clone();
    let task = tokio::task::spawn_blocking(move || {
        app.billing()
            .integrity_token(CLOUD_PROJECT_NUMBER, &request_hash)
    });
    match tokio::time::timeout(wait, task).await {
        Ok(Ok(Ok(response))) => {
            if let Some(error) = &response.error {
                tracing::debug!(%error, "no Play Integrity token");
            }
            response.token.filter(|t| !t.is_empty())
        }
        Ok(Ok(Err(error))) => {
            tracing::debug!(%error, "Play Integrity unavailable");
            None
        }
        Ok(Err(_)) | Err(_) => None,
    }
}

/// Prepare the token provider in the background, so a request soon after
/// (the bug report the user is writing) answers quickly. Best effort.
pub fn warm(app: &AppHandle) {
    if !available() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(response) = app.billing().prepare_integrity(CLOUD_PROJECT_NUMBER) {
            if let Some(error) = response.error {
                tracing::debug!(%error, "Play Integrity not prepared");
            }
        }
    });
}

/// The source the automatic reports ask, once a day.
pub fn report_source(app: &AppHandle) -> IntegritySource {
    let app = app.clone();
    Arc::new(move |hash: String| {
        let app = app.clone();
        Box::pin(async move { token(&app, hash, REPORT_WAIT).await })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_play_release_builds_ask() {
        assert!(enabled_for(Distribution::Play, false));
        assert!(!enabled_for(Distribution::Play, true));
        assert!(!enabled_for(Distribution::Sideload, false));
        assert!(!enabled_for(Distribution::Sideload, true));
    }

    #[test]
    fn host_builds_never_ask() {
        assert!(!available());
    }
}
