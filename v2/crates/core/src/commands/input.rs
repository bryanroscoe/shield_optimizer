//! Remote input — keys and text to the TV.
//!
//! Two transports, tried in order:
//! 1. The persistent scrcpy control channel (`adb::remote_input`) — per-press
//!    cost is network RTT (~ms). Lazily started on first use; full UTF-8 text.
//! 2. `adb shell input …` — the slow (~690 ms/press, ASCII-only) but
//!    compatibility path when selected explicitly or the channel cannot start.
//!    A failed channel send closes the session without replay: delivery is
//!    uncertain, so the user must check the TV before retrying.

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
async fn channel_ready<R: tauri::Runtime>(
    state: &AppState,
    app: &tauri::AppHandle<R>,
    serial: &str,
) -> Result<(), String> {
    let jar = resolve_scrcpy_server_jar(app)?;
    let adb = state.adb_snapshot().await;
    state.ensure_remote_session(adb, &jar, serial).await
}

#[derive(Serialize)]
pub struct RemoteWarmResult {
    /// "channel" once the scrcpy control session is live, "shell" when it
    /// could not start and presses will use the `input` fallback.
    pub transport: &'static str,
    pub message: String,
}

/// `remote_warm` — bring the scrcpy control channel up *before* the first
/// press. The channel is otherwise started lazily by `send_key`/`send_text`,
/// so the first button push pays the whole multi-second cold start (push jar,
/// launch server, connect). Calling this on entering the Remote screen moves
/// that cost off the user's first press.
///
/// Never fails the caller: a missing jar or a device that refuses the server
/// comes back as `transport: "shell"` with the reason, which is exactly what
/// the next press would fall back to anyway. Idempotent — an already-running
/// session returns immediately.
#[tauri::command]
pub async fn remote_warm<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
) -> Result<RemoteWarmResult, String> {
    match channel_ready(&state, &app, &serial).await {
        Ok(()) => Ok(RemoteWarmResult {
            transport: "channel",
            message: "Fast remote ready.".to_string(),
        }),
        Err(e) => {
            tracing::warn!(error = %e, %serial, "remote warm-up unavailable; presses will use input");
            Ok(RemoteWarmResult {
                transport: "shell",
                message: e,
            })
        }
    }
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

async fn dispatch_remote_input<Ready, Send, Sent, Cleanup, Cleaned, Fallback, FallbackResult>(
    force_shell: bool,
    ready: Ready,
    send: Send,
    cleanup: Cleanup,
    fallback: Fallback,
) -> Result<SendTextResult, String>
where
    Ready: std::future::Future<Output = Result<(), String>>,
    Send: FnOnce() -> Sent,
    Sent: std::future::Future<Output = Result<SendTextResult, String>>,
    Cleanup: FnOnce() -> Cleaned,
    Cleaned: std::future::Future<Output = ()>,
    Fallback: FnOnce() -> FallbackResult,
    FallbackResult: std::future::Future<Output = Result<SendTextResult, String>>,
{
    if force_shell {
        return fallback().await;
    }
    if let Err(error) = ready.await {
        tracing::warn!(%error, "remote channel unavailable before input; using selected driver's shell");
        return fallback().await;
    }
    match send().await {
        Ok(result) => Ok(result),
        Err(error) => {
            cleanup().await;
            // A failed write or flush may have delivered part or all of the input.
            Err(format!(
                "Remote input outcome unknown. It was not resent. Check the TV before retrying. {error}"
            ))
        }
    }
}

/// `send_text` — type `text` into whatever input field has focus on the TV.
/// Channel first (full UTF-8); shell compatibility only before input is attempted.
#[tauri::command]
pub async fn send_text<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
    text: String,
    // When true, skip the fast channel and use `input text` directly — the
    // Remote tab's "Force compatible mode" escape hatch for devices where the
    // channel starts but misbehaves.
    force_shell: bool,
) -> Result<SendTextResult, String> {
    if text.is_empty() {
        return Ok(SendTextResult {
            ok: false,
            message: "Nothing to send.".to_string(),
            transport: "none",
        });
    }

    dispatch_remote_input(
        force_shell,
        channel_ready(&state, &app, &serial),
        || async {
            state.remote_send_text(&serial, &text).await?;
            Ok(SendTextResult {
                ok: true,
                message: format!(
                    "Sent {} character(s) to the focused field.",
                    text.chars().count()
                ),
                transport: "channel",
            })
        },
        || state.drop_remote_session(&serial),
        || send_compatible_text(&state, &serial, &text),
    )
    .await
}

async fn send_compatible_text(
    state: &AppState,
    serial: &str,
    text: &str,
) -> Result<SendTextResult, String> {
    let encoded = match encode_input_text(text) {
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
        .shell(serial, &format!("input text {encoded}"))
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
        "recents" => 187,
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
/// down/up); shell compatibility only before input is attempted. Used by the D-pad and
/// by live typing for Backspace/Enter.
#[tauri::command]
pub async fn send_key<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
    key: String,
    // See `send_text` — forces the slow `input keyevent` path.
    force_shell: bool,
) -> Result<SendTextResult, String> {
    let Some(code) = keycode_for(&key) else {
        return Err(format!("Unknown remote key: {key:?}"));
    };

    dispatch_remote_input(
        force_shell,
        channel_ready(&state, &app, &serial),
        || async {
            state.remote_send_key_press(&serial, code).await?;
            Ok(SendTextResult {
                ok: true,
                message: format!("Sent {key}."),
                transport: "channel",
            })
        },
        || state.drop_remote_session(&serial),
        || send_compatible_key(&state, &serial, &key, code),
    )
    .await
}

async fn send_compatible_key(
    state: &AppState,
    serial: &str,
    key: &str,
    code: u32,
) -> Result<SendTextResult, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, &format!("input keyevent {code}"))
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

/// `open_settings` — launch the system Settings activity. The Shield remote's
/// hamburger/gear button opens Settings via an *intent*, not a keycode — which
/// is why `KEYCODE_SETTINGS` (176) and `KEYCODE_MENU` (82) both no-op when
/// injected. `am start` reliably opens Settings on Android TV / Google TV.
#[tauri::command]
pub async fn open_settings(
    state: State<'_, AppState>,
    serial: String,
) -> Result<SendTextResult, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, "am start -a android.settings.SETTINGS")
        .await
        .map_err(|e| format!("am start: {e}"))?;
    // `am start` prints "Starting: Intent { ... }" on success and an
    // "Error"/"Exception" line on failure.
    let combined = out.combined();
    let ok = !combined.contains("Error") && !combined.contains("Exception");
    Ok(SendTextResult {
        ok,
        message: if ok {
            "Opened Settings.".to_string()
        } else {
            combined.trim().to_string()
        },
        transport: "shell",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    async fn exercise_dispatch(
        forced: bool,
        startup_error: Option<&str>,
        write_error: Option<&str>,
    ) -> (Result<SendTextResult, String>, Vec<&'static str>) {
        let calls = std::sync::Mutex::new(Vec::new());
        let result = dispatch_remote_input(
            forced,
            async {
                calls.lock().unwrap().push("start");
                startup_error.map_or(Ok(()), |error| Err(error.to_string()))
            },
            || async {
                calls.lock().unwrap().push("write");
                match write_error {
                    Some(error) => Err(error.to_string()),
                    None => Ok(SendTextResult {
                        ok: true,
                        message: "Sent é漢字😀.".into(),
                        transport: "channel",
                    }),
                }
            },
            || async {
                calls.lock().unwrap().push("close");
            },
            || async {
                calls.lock().unwrap().push("shell");
                Ok(SendTextResult {
                    ok: true,
                    message: "Explicit or pre-send compatibility path.".into(),
                    transport: "shell",
                })
            },
        )
        .await;
        (result, calls.into_inner().unwrap())
    }

    #[tokio::test]
    async fn explicit_compatible_input_never_starts_or_writes_channel() {
        let (result, calls) = exercise_dispatch(true, None, None).await;
        assert_eq!(result.unwrap().transport, "shell");
        assert_eq!(calls, ["shell"]);
    }

    #[tokio::test]
    async fn startup_failure_can_use_same_driver_shell_before_any_input() {
        let (result, calls) = exercise_dispatch(false, Some("unavailable"), None).await;
        assert_eq!(result.unwrap().transport, "shell");
        assert_eq!(calls, ["start", "shell"]);
    }

    #[tokio::test]
    async fn channel_input_success_is_not_replayed() {
        let (result, calls) = exercise_dispatch(false, None, None).await;
        let result = result.unwrap();
        assert_eq!(result.transport, "channel");
        assert!(result.message.contains("é漢字😀"));
        assert_eq!(calls, ["start", "write"]);
    }

    #[tokio::test]
    async fn uncertain_partial_write_flush_or_key_up_never_replays_input() {
        for error in ["partial text write", "flush failed", "key-up failed"] {
            let (result, calls) = exercise_dispatch(false, None, Some(error)).await;
            let message = result.err().expect("must not claim input success");
            assert!(message.contains("outcome unknown"));
            assert!(message.contains("not resent"));
            assert!(message.contains(error));
            assert_eq!(calls, ["start", "write", "close"]);
        }
    }

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
        assert_eq!(keycode_for("recents"), Some(187));
        assert_eq!(keycode_for("delete"), Some(67));
        assert_eq!(keycode_for("wakeup"), Some(224));
        assert_eq!(keycode_for("power"), Some(26));
        // No arbitrary keycodes from the frontend.
        assert_eq!(keycode_for("42"), None);
        assert_eq!(keycode_for(""), None);
    }
}
