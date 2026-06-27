//! Reboot — normal / recovery / bootloader. Mirrors v1's Show-RebootMenu (§11).

use serde::{Deserialize, Serialize};
use tauri::State;

use super::AppState;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebootMode {
    /// `adb reboot` — normal restart.
    Normal,
    /// `adb reboot recovery` — boot into recovery menu.
    Recovery,
    /// `adb reboot bootloader` — boot into fastboot.
    Bootloader,
}

#[derive(Serialize)]
pub struct RebootResult {
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub async fn reboot_device(
    state: State<'_, AppState>,
    serial: String,
    mode: RebootMode,
) -> Result<RebootResult, String> {
    let adb = state.adb_snapshot().await;
    let args: Vec<&str> = match mode {
        RebootMode::Normal => vec!["-s", &serial, "reboot"],
        RebootMode::Recovery => vec!["-s", &serial, "reboot", "recovery"],
        RebootMode::Bootloader => vec!["-s", &serial, "reboot", "bootloader"],
    };

    match adb.raw(&args).await {
        Ok(_) => Ok(RebootResult {
            ok: true,
            message: format!("Reboot command sent ({:?}).", mode_label(mode)),
        }),
        Err(e) => Ok(RebootResult {
            ok: false,
            message: e.to_string(),
        }),
    }
}

fn mode_label(mode: RebootMode) -> &'static str {
    match mode {
        RebootMode::Normal => "normal",
        RebootMode::Recovery => "recovery",
        RebootMode::Bootloader => "bootloader",
    }
}
