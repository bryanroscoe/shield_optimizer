//! Tweaks & Display Scaling — settings the user can flip outside the Optimize
//! flow. Mirrors v1's Set-DisplayInputTuning (§7) and Set-DisplayScaling (§8).

use serde::Serialize;
use tauri::State;

use super::{is_valid_setting_key, quote_shell_arg, AppState};

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
    /// Developer-options "Background process limit". `null`/absent = Standard,
    /// `0` = none, `1`–`4` = at most N. Frees RAM, but Android resets it on
    /// reboot (see issue #11) — the UI says so.
    pub background_process_limit: Option<String>,
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
               settings get global animator_duration_scale; \
               settings get global background_process_limit";
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
        background_process_limit: lines.next().flatten(),
    })
}

#[derive(Serialize)]
pub struct WriteResult {
    pub ok: bool,
    pub message: String,
}

/// Build the `settings` shell command for a write/delete, with the safety
/// checks that keep a malformed UI request from writing somewhere unexpected
/// or breaking out of the shell. Pure so it can be unit-tested.
fn build_setting_command(namespace: &str, key: &str, value: &str) -> Result<String, String> {
    // Whitelist namespaces.
    if !matches!(namespace, "global" | "secure" | "system") {
        return Err(format!(
            "namespace must be global/secure/system, got {namespace}"
        ));
    }
    if !is_valid_setting_key(key) {
        return Err(format!("key contains invalid characters: {key:?}"));
    }
    if value.is_empty() {
        return Ok(format!("settings delete {namespace} {key}"));
    }
    Ok(format!(
        "settings put {namespace} {key} {}",
        quote_shell_arg(value)
    ))
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
    let cmd = match build_setting_command(&namespace, &key, &value) {
        Ok(c) => c,
        Err(message) => return Ok(WriteResult { ok: false, message }),
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

#[derive(Serialize)]
pub struct PrivateDnsState {
    /// `off` / `opportunistic` / `hostname`, or None if unset.
    pub mode: Option<String>,
    /// The DoT hostname when mode is `hostname`.
    pub hostname: Option<String>,
}

#[derive(Serialize)]
pub struct PrivateDnsResult {
    pub ok: bool,
    pub message: String,
    /// True when a custom hostname failed to resolve and we reverted to
    /// automatic to keep the device online.
    pub reverted: bool,
}

/// A DNS hostname safe to interpolate into a `settings put` shell command.
/// Mirrors v1's regex (Set-PrivateDns): dot-separated labels of
/// `[A-Za-z0-9-]`, no label starting/ending with `-`, at least two labels.
/// Pure so it can be unit-tested; the value is still shell-quoted on use.
fn is_valid_dns_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    labels.iter().all(|l| {
        !l.is_empty()
            && l.len() <= 63
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !l.starts_with('-')
            && !l.ends_with('-')
    })
}

/// `get_private_dns` — read the device's current Private DNS mode + hostname.
#[tauri::command]
pub async fn get_private_dns(
    state: State<'_, AppState>,
    serial: String,
) -> Result<PrivateDnsState, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(
            &serial,
            "settings get global private_dns_mode; settings get global private_dns_specifier",
        )
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
    Ok(PrivateDnsState {
        mode: lines.next().flatten(),
        hostname: lines.next().flatten(),
    })
}

/// `set_private_dns` — switch Private DNS (DNS-over-TLS) mode. For a custom
/// hostname, validate it, apply it, then probe DNS resolution and revert to
/// automatic if the host is dead — a bad DoT host otherwise strands the device
/// with no working DNS (Android won't fall back). Mirrors v1's Set-PrivateDns.
#[tauri::command]
pub async fn set_private_dns(
    state: State<'_, AppState>,
    serial: String,
    mode: String,
    hostname: Option<String>,
) -> Result<PrivateDnsResult, String> {
    let adb = state.adb_snapshot().await;
    match mode.as_str() {
        "off" => {
            adb.shell(&serial, "settings put global private_dns_mode off")
                .await
                .map_err(|e| format!("settings put: {e}"))?;
            Ok(PrivateDnsResult {
                ok: true,
                message: "Private DNS off.".to_string(),
                reverted: false,
            })
        }
        "opportunistic" => {
            adb.shell(
                &serial,
                "settings put global private_dns_mode opportunistic",
            )
            .await
            .map_err(|e| format!("settings put: {e}"))?;
            Ok(PrivateDnsResult {
                ok: true,
                message: "Private DNS set to automatic.".to_string(),
                reverted: false,
            })
        }
        "hostname" => {
            let host = hostname.unwrap_or_default();
            let host = host.trim();
            if !is_valid_dns_hostname(host) {
                return Ok(PrivateDnsResult {
                    ok: false,
                    message: format!(
                        "Invalid hostname {host:?}. Expected something like 'dns.adguard.com'."
                    ),
                    reverted: false,
                });
            }
            let set_cmd = format!(
                "settings put global private_dns_specifier {}; \
                 settings put global private_dns_mode hostname",
                quote_shell_arg(host)
            );
            adb.shell(&serial, &set_cmd)
                .await
                .map_err(|e| format!("settings put: {e}"))?;

            // Probe: a custom DoT host that can't be reached leaves the device
            // with no DNS. Retry a few times (DoT validation lags a few seconds)
            // before deciding it's dead.
            let mut resolved = false;
            for _ in 0..3 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if let Ok(out) = adb
                    .shell(&serial, "ping -c1 -W3 connectivitycheck.gstatic.com")
                    .await
                {
                    if out.combined().contains("bytes from") {
                        resolved = true;
                        break;
                    }
                }
            }
            if resolved {
                Ok(PrivateDnsResult {
                    ok: true,
                    message: format!("Private DNS set to {host}; resolution verified."),
                    reverted: false,
                })
            } else {
                adb.shell(
                    &serial,
                    "settings put global private_dns_mode opportunistic",
                )
                .await
                .map_err(|e| format!("settings put: {e}"))?;
                Ok(PrivateDnsResult {
                    ok: false,
                    message: format!(
                        "No DNS resolution via {host} — reverted to automatic to keep the device \
                         online. Check the hostname."
                    ),
                    reverted: true,
                })
            }
        }
        other => Ok(PrivateDnsResult {
            ok: false,
            message: format!("unknown Private DNS mode: {other}"),
            reverted: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_setting_command, is_valid_dns_hostname};

    #[test]
    fn accepts_real_dot_hostnames() {
        assert!(is_valid_dns_hostname("dns.adguard.com"));
        assert!(is_valid_dns_hostname("one.one.one.one"));
        assert!(is_valid_dns_hostname("abcd.dns.nextdns.io"));
        assert!(is_valid_dns_hostname("a-b.example-host.net"));
    }

    #[test]
    fn rejects_bad_or_unsafe_hostnames() {
        assert!(!is_valid_dns_hostname("")); // empty
        assert!(!is_valid_dns_hostname("localhost")); // no dot
        assert!(!is_valid_dns_hostname("-bad.com")); // label starts with -
        assert!(!is_valid_dns_hostname("bad-.com")); // label ends with -
        assert!(!is_valid_dns_hostname("a..b")); // empty label
        assert!(!is_valid_dns_hostname("dns.adguard.com; reboot")); // shell metachar
        assert!(!is_valid_dns_hostname("a$(whoami).com")); // injection attempt
        assert!(!is_valid_dns_hostname("space host.com")); // space
    }

    #[test]
    fn builds_put_with_quoted_value() {
        assert_eq!(
            build_setting_command("global", "window_animation_scale", "0.5").unwrap(),
            "settings put global window_animation_scale '0.5'"
        );
        assert_eq!(
            build_setting_command("secure", "match_content_frame_rate", "2").unwrap(),
            "settings put secure match_content_frame_rate '2'"
        );
    }

    #[test]
    fn empty_value_deletes() {
        assert_eq!(
            build_setting_command("system", "foo", "").unwrap(),
            "settings delete system foo"
        );
    }

    #[test]
    fn rejects_bad_namespace_and_bad_key() {
        assert!(build_setting_command("evil", "k", "1").is_err());
        assert!(build_setting_command("global", "x; reboot", "1").is_err());
        assert!(build_setting_command("global", "x`whoami`", "1").is_err());
        assert!(build_setting_command("global", "$(reboot)", "1").is_err());
        assert!(build_setting_command("global", "", "1").is_err());
    }

    #[test]
    fn value_quoting_handles_spaces_and_single_quotes() {
        // Spaces in values (e.g. device names) must be quoted, not rejected.
        let cmd = build_setting_command("global", "device_name", "My Shield TV").unwrap();
        assert!(
            cmd.contains("'My Shield TV'"),
            "spaces in value must be single-quoted: {cmd}"
        );

        // Single quotes inside values use the standard `'\''` idiom.
        let cmd = build_setting_command("secure", "device_name", "it's").unwrap();
        assert!(
            cmd.contains(r"'it'\''s'"),
            "embedded single-quote must use the backslash idiom: {cmd}"
        );
    }

    #[test]
    fn value_metacharacters_are_quoted_not_rejected() {
        // Values containing shell metacharacters are now quoted rather than
        // rejected — the device shell cannot interpret them inside single quotes.
        assert!(build_setting_command("global", "k", "1; rm -rf /").is_ok());
        assert!(build_setting_command("global", "k", "a|b").is_ok());
        assert!(build_setting_command("global", "k", "a&b").is_ok());
        assert!(build_setting_command("global", "k", "$(whoami)").is_ok());
        assert!(build_setting_command("global", "k", "`id`").is_ok());
    }
}
