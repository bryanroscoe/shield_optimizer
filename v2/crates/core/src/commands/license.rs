//! Licensing command. Verification itself lives in `crate::license` (pure);
//! this only reads back what the last successful validation stored in state.

use tauri::State;

use crate::license::LicenseInfo;

use super::AppState;

/// `license_info` — details of the license currently unlocking this install, or
/// `None` on Free. The stored key is deliberately not exposed: activation
/// records the verified [`LicenseInfo`] in state, and that is all the UI needs.
#[tauri::command]
pub async fn license_info(state: State<'_, AppState>) -> Result<Option<LicenseInfo>, String> {
    Ok(state.license_info())
}
