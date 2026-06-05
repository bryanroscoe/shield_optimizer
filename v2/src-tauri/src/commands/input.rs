//! Send text to the TV — `input text` over ADB, for typing Wi-Fi passwords
//! and searches from a real keyboard instead of the on-screen D-pad grid.

use serde::Serialize;
use tauri::State;

use super::AppState;

#[derive(Serialize)]
pub struct SendTextResult {
    pub ok: bool,
    pub message: String,
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
#[tauri::command]
pub async fn send_text(
    state: State<'_, AppState>,
    serial: String,
    text: String,
) -> Result<SendTextResult, String> {
    let encoded = match encode_input_text(&text) {
        Ok(e) => e,
        Err(message) => return Ok(SendTextResult { ok: false, message }),
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
        })
    } else {
        Ok(SendTextResult {
            ok: false,
            message: noise,
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

/// `send_key` — one remote button press (`input keyevent <code>`). Used by
/// the Remote panel's D-pad and by live typing for Backspace/Enter.
#[tauri::command]
pub async fn send_key(
    state: State<'_, AppState>,
    serial: String,
    key: String,
) -> Result<SendTextResult, String> {
    let Some(code) = keycode_for(&key) else {
        return Err(format!("Unknown remote key: {key:?}"));
    };
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
