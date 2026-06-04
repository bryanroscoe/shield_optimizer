//! Tweaks & Display Scaling — settings the user can flip outside the Optimize
//! flow. Mirrors v1's Set-DisplayInputTuning (§7) and Set-DisplayScaling (§8).

use serde::Serialize;
use tauri::State;

use super::AppState;

/// Snapshot of all the settings the Tweaks UI reads/writes. Matches the keys
/// in `engine::snapshot::tracked_setting_keys` so a snapshot save captures
/// the same surface.
#[derive(Serialize)]
pub struct TweaksState {
    /// `1` / `0` / `null` for each HDMI-CEC sub-toggle.
    pub hdmi_control_enabled: Option<String>,
    pub hdmi_control_auto_wakeup_enabled: Option<String>,
    pub hdmi_control_auto_device_off_enabled: Option<String>,
    pub hdmi_system_audio_control_enabled: Option<String>,
    /// `0` = Never, `1` = Seamless only, `2` = Always.
    pub match_content_frame_rate: Option<String>,
    /// Milliseconds.
    pub long_press_timeout: Option<String>,
    pub window_animation_scale: Option<String>,
    pub transition_animation_scale: Option<String>,
    pub animator_duration_scale: Option<String>,
}

/// `get_tweaks` — batch-fetch all Tweaks-relevant settings in one shell call.
#[tauri::command]
pub async fn get_tweaks(state: State<'_, AppState>, serial: String) -> Result<TweaksState, String> {
    let adb = state.adb_snapshot().await;
    let cmd = "settings get global hdmi_control_enabled; \
               settings get global hdmi_control_auto_wakeup_enabled; \
               settings get global hdmi_control_auto_device_off_enabled; \
               settings get global hdmi_system_audio_control_enabled; \
               settings get secure match_content_frame_rate; \
               settings get secure long_press_timeout; \
               settings get global window_animation_scale; \
               settings get global transition_animation_scale; \
               settings get global animator_duration_scale";
    let out = adb
        .shell(&serial, cmd)
        .await
        .map_err(|e| format!("settings get: {e}"))?;
    let mut lines = out.stdout.lines().map(|s| {
        let v = s.trim();
        if v.is_empty() || v == "null" {
            None
        } else {
            Some(v.to_string())
        }
    });
    Ok(TweaksState {
        hdmi_control_enabled: lines.next().flatten(),
        hdmi_control_auto_wakeup_enabled: lines.next().flatten(),
        hdmi_control_auto_device_off_enabled: lines.next().flatten(),
        hdmi_system_audio_control_enabled: lines.next().flatten(),
        match_content_frame_rate: lines.next().flatten(),
        long_press_timeout: lines.next().flatten(),
        window_animation_scale: lines.next().flatten(),
        transition_animation_scale: lines.next().flatten(),
        animator_duration_scale: lines.next().flatten(),
    })
}

#[derive(Serialize)]
pub struct WriteResult {
    pub ok: bool,
    pub message: String,
}

/// `write_setting` — `settings put <namespace> <key> <value>`. Pass an empty
/// `value` to delete (resets to default).
#[tauri::command]
pub async fn write_setting(
    state: State<'_, AppState>,
    serial: String,
    namespace: String,
    key: String,
    value: String,
) -> Result<WriteResult, String> {
    // Whitelist namespaces so the command can't be tricked into writing to
    // somewhere unexpected via a malformed UI request.
    if !matches!(namespace.as_str(), "global" | "secure" | "system") {
        return Ok(WriteResult {
            ok: false,
            message: format!("namespace must be global/secure/system, got {namespace}"),
        });
    }
    let cmd = if value.is_empty() {
        format!("settings delete {namespace} {key}")
    } else {
        // Basic value validation — reject characters that could break out of
        // the shell quoting. Settings values in practice are numbers or short
        // identifiers, so this is more than permissive enough.
        if value.contains(';') || value.contains('|') || value.contains('&') {
            return Ok(WriteResult {
                ok: false,
                message: "value contains shell metacharacters".to_string(),
            });
        }
        format!("settings put {namespace} {key} {value}")
    };
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, &cmd)
        .await
        .map_err(|e| format!("{cmd}: {e}"))?;
    let msg = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    Ok(WriteResult {
        ok: !msg.contains("Error") && !msg.contains("Exception"),
        message: if msg.is_empty() {
            "ok".to_string()
        } else {
            msg
        },
    })
}

#[derive(Serialize)]
pub struct DisplayScaleResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct CurrentDisplayScaling {
    /// Trimmed `wm size` output — e.g. "Physical size: 3840x2160" or
    /// "Physical size: 3840x2160\nOverride size: 1920x1080".
    pub size: String,
    /// Trimmed `wm density` output — e.g. "Physical density: 540".
    pub density: String,
}

/// `get_display_scaling` — fetch current `wm size` + `wm density` so the UI
/// can show what the user is about to change. Mirrors v1's read at the top
/// of `Set-DisplayScaling` (§8).
#[tauri::command]
pub async fn get_display_scaling(
    state: State<'_, AppState>,
    serial: String,
) -> Result<CurrentDisplayScaling, String> {
    let adb = state.adb_snapshot().await;
    let (size_res, density_res) = tokio::join!(
        adb.shell(&serial, "wm size"),
        adb.shell(&serial, "wm density"),
    );
    let size = size_res
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();
    let density = density_res
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();
    Ok(CurrentDisplayScaling { size, density })
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub enum DisplayScalePreset {
    // Explicit renames: `rename_all = "snake_case"` produces `uhd4k` /
    // `fhd1080p` (no underscore before the digit), which mismatched the
    // frontend's `uhd_4k` / `fhd_1080p` and made every scaling click fail with
    // "unknown variant `uhd_4k`". Keep in lockstep with DisplayScalePreset in
    // src/lib/types.ts.
    /// 3839x2160 @ density 640. Shield TV won't accept 3840 width, and density
    /// 540 breaks some app menus (Disney+, HBO) — see issue #24.
    #[serde(rename = "uhd_4k")]
    Uhd4k,
    /// 1920x1080 @ density 320.
    #[serde(rename = "fhd_1080p")]
    Fhd1080p,
    /// Reset both to device defaults.
    #[serde(rename = "reset")]
    Reset,
}

#[tauri::command]
pub async fn set_display_scaling(
    state: State<'_, AppState>,
    serial: String,
    preset: DisplayScalePreset,
) -> Result<DisplayScaleResult, String> {
    let adb = state.adb_snapshot().await;
    let cmds: Vec<&str> = match preset {
        DisplayScalePreset::Uhd4k => vec!["wm size 3839x2160", "wm density 640"],
        DisplayScalePreset::Fhd1080p => vec!["wm size 1920x1080", "wm density 320"],
        DisplayScalePreset::Reset => vec!["wm size reset", "wm density reset"],
    };
    let cmd = cmds.join("; ");
    let out = adb
        .shell(&serial, &cmd)
        .await
        .map_err(|e| format!("wm: {e}"))?;
    Ok(DisplayScaleResult {
        ok: !out.stdout.contains("Error") && !out.stderr.contains("Error"),
        message: if out.stdout.is_empty() {
            out.stderr
        } else {
            out.stdout
        },
    })
}
