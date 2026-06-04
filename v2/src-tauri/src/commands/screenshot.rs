//! Screenshot capture — `adb exec-out screencap -p`, saved under the app
//! data dir with a base64 copy for an instant in-app preview.

use serde::Serialize;
use tauri::State;

use base64::Engine as _;

use super::AppState;

#[derive(Serialize)]
pub struct ScreenshotResult {
    /// Absolute path of the saved PNG.
    pub path: String,
    /// The same PNG, base64-encoded, for a `data:` URL preview in the UI.
    pub base64: String,
}

const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];

/// `screencap -p` writes the PNG to stdout; on failure (secure surface, DRM,
/// permission) it writes an error message instead. The magic bytes are the
/// reliable success signal.
fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(PNG_MAGIC) {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    let text = text.trim();
    if text.is_empty() {
        Err(
            "screencap returned no data — the screen may be off or showing protected content."
                .into(),
        )
    } else {
        Err(format!("screencap failed: {text}"))
    }
}

/// Serials like `192.168.42.71:5555` contain `:`, which is invalid in
/// Windows filenames — flatten everything non-alphanumeric to `-`.
fn filename_safe(serial: &str) -> String {
    serial
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[tauri::command]
pub async fn take_screenshot(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ScreenshotResult, String> {
    let adb = state.adb_snapshot().await;
    let bytes = adb
        .raw_bytes(&["-s", &serial, "exec-out", "screencap", "-p"])
        .await
        .map_err(|e| e.to_string())?;
    validate_png(&bytes)?;

    // Sibling of the snapshots dir in the app data root (e.g.
    // `…/ShieldOptimizer/screenshots`).
    let dir = state
        .snapshot_dir
        .parent()
        .map(|p| p.join("screenshots"))
        .unwrap_or_else(|| state.snapshot_dir.join("screenshots"));
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create {}: {e}", dir.display()))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let path = dir.join(format!("{}_{stamp}.png", filename_safe(&serial)));
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| format!("write {}: {e}", path.display()))?;

    Ok(ScreenshotResult {
        path: path.display().to_string(),
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn accepts_png_bytes() {
        assert!(validate_png(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]).is_ok());
    }

    #[test]
    fn rejects_error_text_with_message() {
        let err = validate_png(b"Unable to take screenshot: secure surface").unwrap_err();
        assert!(err.contains("secure surface"));
    }

    #[test]
    fn rejects_empty_output() {
        let err = validate_png(b"").unwrap_err();
        assert!(err.contains("no data"));
    }

    #[test]
    fn flattens_serial_for_filenames() {
        assert_eq!(filename_safe("192.168.42.71:5555"), "192-168-42-71-5555");
        assert_eq!(filename_safe("0123456789ABCDEF"), "0123456789ABCDEF");
    }
}
