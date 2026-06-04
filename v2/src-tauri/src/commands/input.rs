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
}
