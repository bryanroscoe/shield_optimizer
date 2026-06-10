//! Remote input — keys and text to the TV.
//!
//! Two transports, tried in order:
//! 1. The persistent scrcpy control channel (`adb::remote_input`) — per-press
//!    cost is network RTT (~ms). Lazily started on first use; full UTF-8 text.
//! 2. `adb shell input …` — the slow (~690 ms/press, ASCII-only) but
//!    universally-available fallback when the channel can't start or its
//!    socket dies. A failed channel send drops the session so the next press
//!    retries a fresh start.

use serde::Serialize;
use tauri::State;

use super::state::resolve_scrcpy_server_jar;
use super::AppState;

#[derive(Serialize)]
pub struct SendTextResult {
    pub ok: bool,
    pub message: String,
    /// Which transport served this request: "channel" (scrcpy control socket)
    /// or "shell" (legacy `input` fallback). The Remote tab shows a live cue.
    pub transport: &'static str,
}

/// Make sure the scrcpy session for `serial` is up (starting it if needed).
async fn channel_ready(
    state: &AppState,
    app: &tauri::AppHandle,
    serial: &str,
) -> Result<(), String> {
    let jar = resolve_scrcpy_server_jar(app)?;
    let adb = state.adb_snapshot().await;
    state.ensure_remote_session(adb, &jar, serial).await
}

const MAX_TEXT_LEN: usize = 500;

/// Build the safely-quoted `input text` argument.
///
/// Two layers need escaping: the device-side shell (the whole command runs
/// through `sh -c`), handled by single-quoting with the `'\''` idiom; and
/// `input text` itself, which can't represent a literal space except as `%s`.
/// Non-ASCII and control characters are rejected outright — `input text`
/// mangles or drops them silently, which is worse than an honest error.
fn encode_input_text(text: &str) -> Result<String, String> {
    if text.is_empty() {
        return Err("Nothing to send.".to_string());
    }
    if text.len() > MAX_TEXT_LEN {
        return Err(format!(
            "Text too long ({} > {MAX_TEXT_LEN} chars).",
            text.len()
        ));
    }
    if let Some(bad) = text.chars().find(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Err(format!(
            "Unsupported character {bad:?} — `input text` only handles printable ASCII. \
             Type that part with the on-screen keyboard."
        ));
    }
    let spaced = text.replace(' ', "%s");
    Ok(format!("'{}'", spaced.replace('\'', r"'\''")))
}

/// `send_text` — type `text` into whatever input field has focus on the TV.
/// Channel first (full UTF-8, instant); `input text` (ASCII-only) fallback.
#[tauri::command]
pub async fn send_text(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    serial: String,
    text: String,
) -> Result<SendTextResult, String> {
    if text.is_empty() {
        return Ok(SendTextResult {
            ok: false,
            message: "Nothing to send.".to_string(),
            transport: "none",
        });
    }

    let channel = match channel_ready(&state, &app, &serial).await {
        Ok(()) => match state.remote_send_text(&serial, &text).await {
            Ok(()) => Ok(()),
            Err(e) => {
                state.drop_remote_session(&serial).await;
                Err(e)
            }
        },
        Err(e) => Err(e),
    };
    if channel.is_ok() {
        return Ok(SendTextResult {
            ok: true,
            message: format!(
                "Sent {} character(s) to the focused field.",
                text.chars().count()
            ),
            transport: "channel",
        });
    }
    tracing::warn!(error = ?channel, %serial, "scrcpy channel unavailable; using input text");

    let encoded = match encode_input_text(&text) {
        Ok(e) => e,
        Err(message) => {
            return Ok(SendTextResult {
                ok: false,
                message,
                transport: "shell",
            })
        }
    };
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, &format!("input text {encoded}"))
        .await
        .map_err(|e| format!("input text: {e}"))?;
    // `input` is silent on success and prints usage/exception text on failure.
    let noise = out.combined().trim().to_string();
    if noise.is_empty() {
        Ok(SendTextResult {
            ok: true,
            message: format!("Sent {} character(s) to the focused field.", text.len()),
            transport: "shell",
        })
    } else {
        Ok(SendTextResult {
            ok: false,
            message: noise,
            transport: "shell",
        })
    }
}

/// Allowlisted remote keys → Android keycodes. Anything outside this map is
/// refused — the frontend never gets to send arbitrary keycodes.
fn keycode_for(key: &str) -> Option<u32> {
    Some(match key {
        "up" => 19,
        "down" => 20,
        "left" => 21,
        "right" => 22,
        "select" => 23,
        "back" => 4,
        "home" => 3,
        "play_pause" => 85,
        "rewind" => 89,
        "fast_forward" => 90,
        "volume_up" => 24,
        "volume_down" => 25,
        "mute" => 164,
        "power" => 26,
        // WAKEUP (224) reliably turns the screen on; a plain D-pad press does
        // not wake a sleeping Android TV, and POWER (26) toggles (so it can
        // put an awake device back to sleep). Dedicated wake avoids that.
        "wakeup" => 224,
        "delete" => 67,
        "enter" => 66,
        _ => return None,
    })
}

/// `send_key` — one remote button press. Channel first (instant, real
/// down/up); `input keyevent` fallback. Used by the Remote panel's D-pad and
/// by live typing for Backspace/Enter.
#[tauri::command]
pub async fn send_key(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    serial: String,
    key: String,
) -> Result<SendTextResult, String> {
    let Some(code) = keycode_for(&key) else {
        return Err(format!("Unknown remote key: {key:?}"));
    };

    let channel = match channel_ready(&state, &app, &serial).await {
        Ok(()) => match state.remote_send_key_press(&serial, code).await {
            Ok(()) => Ok(()),
            Err(e) => {
                state.drop_remote_session(&serial).await;
                Err(e)
            }
        },
        Err(e) => Err(e),
    };
    if channel.is_ok() {
        return Ok(SendTextResult {
            ok: true,
            message: format!("Sent {key}."),
            transport: "channel",
        });
    }
    tracing::warn!(error = ?channel, %serial, "scrcpy channel unavailable; using input keyevent");

    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, &format!("input keyevent {code}"))
        .await
        .map_err(|e| format!("input keyevent: {e}"))?;
    let noise = out.combined().trim().to_string();
    Ok(SendTextResult {
        ok: noise.is_empty(),
        message: if noise.is_empty() {
            format!("Sent {key}.")
        } else {
            noise
        },
        transport: "shell",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn encodes_spaces_as_percent_s() {
        assert_eq!(
            encode_input_text("my wifi pass").unwrap(),
            "'my%swifi%spass'"
        );
    }

    #[test]
    fn quotes_shell_specials() {
        assert_eq!(encode_input_text("a$b`c\"d").unwrap(), "'a$b`c\"d'");
        // Single quote ends the quoted span, escapes, and reopens.
        assert_eq!(encode_input_text("it's").unwrap(), r"'it'\''s'");
    }

    #[test]
    fn rejects_unicode_control_and_empty() {
        assert!(encode_input_text("naïve").is_err());
        assert!(encode_input_text("line\nbreak").is_err());
        assert!(encode_input_text("").is_err());
        assert!(encode_input_text(&"x".repeat(501)).is_err());
    }

    #[test]
    fn remote_keys_map_to_android_keycodes() {
        assert_eq!(keycode_for("up"), Some(19));
        assert_eq!(keycode_for("select"), Some(23));
        assert_eq!(keycode_for("back"), Some(4));
        assert_eq!(keycode_for("delete"), Some(67));
        assert_eq!(keycode_for("wakeup"), Some(224));
        assert_eq!(keycode_for("power"), Some(26));
        // No arbitrary keycodes from the frontend.
        assert_eq!(keycode_for("42"), None);
        assert_eq!(keycode_for(""), None);
    }
}
