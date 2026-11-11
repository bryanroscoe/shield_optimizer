//! Tweaks & Display Scaling — settings the user can flip outside the Optimize
//! flow. Mirrors v1's Set-DisplayInputTuning (§7) and Set-DisplayScaling (§8).

use serde::Serialize;
use tauri::State;

use crate::license::Feature;

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
    /// `global.background_process_limit`. Android does not read this key: on a
    /// Shield TV (Android 11) setting it to 1 left the cached-process limit at
    /// 32 (#99). Read only so the UI can offer to remove a value an older app
    /// version left behind.
    pub background_process_limit: Option<String>,
    /// The limit Android is actually applying to cached background processes,
    /// `CUR_MAX_CACHED_PROCESSES` from `dumpsys activity settings`. Developer
    /// options' "Background process limit" sets this in memory. `None` when
    /// the device did not report it.
    pub cached_process_limit: Option<u32>,
    /// Encoded-audio passthrough: `0` Auto, `1` Never, `2` Always, `3` Manual.
    /// Decides whether TrueHD / DTS-HD reach the receiver untouched or get
    /// decoded to PCM on the device first.
    pub encoded_surround_output: Option<String>,
    /// Comma-separated `AudioFormat.ENCODING_*` values — the allow-list that
    /// applies only when the mode is Manual.
    pub encoded_surround_output_enabled_formats: Option<String>,
    /// `secure.screensaver_components` — the ComponentName of the active
    /// Daydream (Android's screensaver), e.g. AOSP's Basic Daydream or a
    /// vendor ambient mode. `None` when no screensaver is configured.
    pub screensaver_components: Option<String>,
    /// `secure.screensaver_enabled` — whether Daydream runs at all. Clearing
    /// `screensaver_components` alone can still leave a framework/vendor
    /// default screensaver running while this stays on, so "None" has to
    /// turn this off too, not just blank the component.
    pub screensaver_enabled: Option<String>,
}

/// AOSP's built-in "Basic Daydream" — a component every Android TV build
/// ships, unlike vendor screensavers (Ambient Mode, Glance) which vary by
/// device and aren't guessed here.
pub const BASIC_DAYDREAM_COMPONENT: &str =
    "com.android.dreams.basic/com.android.dreams.basic.BasicDream";

/// `get_tweaks` — batch-fetch all Tweaks-relevant settings in one shell call.
#[tauri::command]
pub async fn get_tweaks(state: State<'_, AppState>, serial: String) -> Result<TweaksState, String> {
    let adb = state.adb_snapshot().await;
    get_tweaks_for(adb.as_ref(), &serial).await
}

pub(crate) async fn get_tweaks_for(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<TweaksState, String> {
    let commands = [
        "settings get global hdmi_control_enabled",
        "settings get global hdmi_control_auto_wakeup_enabled",
        "settings get global hdmi_control_auto_device_off_enabled",
        "settings get global hdmi_system_audio_control_enabled",
        "settings get secure match_content_frame_rate",
        "settings get secure long_press_timeout",
        "settings get global window_animation_scale",
        "settings get global transition_animation_scale",
        "settings get global animator_duration_scale",
        "settings get global background_process_limit",
        "settings get global encoded_surround_output",
        "settings get global encoded_surround_output_enabled_formats",
        "settings get secure screensaver_components",
        "settings get secure screensaver_enabled",
        CACHED_LIMIT_READ,
    ];
    let cmd = crate::adb::checked_batch_command(&commands);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("settings get: {e}"))?;
    // The cached-limit read is optional: a device that hides it still gets its
    // settings, and the UI shows the limit as unknown.
    let required = (0..commands.len() - 1).collect::<Vec<_>>();
    let mut sections = crate::adb::parse_checked_batch(&out.stdout, commands.len(), &required)?;
    let cached_process_limit = sections.pop().as_deref().and_then(parse_cached_limit);
    let mut values = sections
        .into_iter()
        .map(|value| (value != "null").then_some(value));
    Ok(TweaksState {
        hdmi_control_enabled: values.next().flatten(),
        hdmi_control_auto_wakeup_enabled: values.next().flatten(),
        hdmi_control_auto_device_off_enabled: values.next().flatten(),
        hdmi_system_audio_control_enabled: values.next().flatten(),
        match_content_frame_rate: values.next().flatten(),
        long_press_timeout: values.next().flatten(),
        window_animation_scale: values.next().flatten(),
        transition_animation_scale: values.next().flatten(),
        animator_duration_scale: values.next().flatten(),
        background_process_limit: values.next().flatten(),
        encoded_surround_output: values.next().flatten(),
        encoded_surround_output_enabled_formats: values.next().flatten(),
        screensaver_components: values.next().flatten(),
        screensaver_enabled: values.next().flatten(),
        cached_process_limit,
    })
}

const CACHED_LIMIT_READ: &str = "dumpsys activity settings | grep CUR_MAX_CACHED_PROCESSES";

fn parse_cached_limit(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("CUR_MAX_CACHED_PROCESSES="))
        .and_then(|v| v.trim().parse().ok())
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
    state.require_pro(Feature::TweaksWrite)?;
    write_setting_for(state.inner(), &serial, &namespace, &key, &value).await
}

async fn write_setting_for(
    state: &AppState,
    serial: &str,
    namespace: &str,
    key: &str,
    value: &str,
) -> Result<WriteResult, String> {
    let cmd = match build_setting_command(namespace, key, value) {
        Ok(c) => c,
        Err(message) => return Ok(WriteResult { ok: false, message }),
    };
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("{cmd}: {e}"))?;
    if !super::verification::succeeded(&out) {
        return Ok(WriteResult {
            ok: false,
            message: super::verification::failure_text(&out),
        });
    }
    match super::verification::verify_setting(
        adb.as_ref(),
        serial,
        namespace,
        key,
        (!value.is_empty()).then_some(value),
    )
    .await
    {
        Ok(()) => Ok(WriteResult {
            ok: true,
            message: "Setting confirmed.".into(),
        }),
        Err(e) => Ok(WriteResult {
            ok: false,
            message: e.message().to_string(),
        }),
    }
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

/// `get_display_scaling` — fetch current `wm size` + `wm density` in one
/// batched shell call so the UI can show what the user is about to change.
/// Mirrors v1's read at the top of `Set-DisplayScaling` (§8). A failed read
/// is an error, not blank values.
#[tauri::command]
pub async fn get_display_scaling(
    state: State<'_, AppState>,
    serial: String,
) -> Result<CurrentDisplayScaling, String> {
    let adb = state.adb_snapshot().await;
    read_display_scaling(adb.as_ref(), &serial).await
}

async fn read_display_scaling(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<CurrentDisplayScaling, String> {
    let cmd = crate::adb::checked_batch_command(&["wm size", "wm density"]);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("wm: {e}"))?;
    let sections =
        crate::adb::parse_checked_batch(&out.stdout, 2, &[0, 1]).map_err(|e| format!("wm: {e}"))?;
    let size = sections[0].trim().to_string();
    let density = sections[1].trim().to_string();
    if size.is_empty() || density.is_empty() {
        return Err("wm: the TV reported no display size or density".into());
    }
    Ok(CurrentDisplayScaling { size, density })
}

/// The value Android is using from `wm size` / `wm density` output: the
/// override when there is one, else the physical value.
fn effective_display_value(output: &str) -> Option<&str> {
    let value = |prefix: &str| {
        output.lines().find_map(|line| {
            let (label, value) = line.split_once(':')?;
            label
                .trim()
                .starts_with(prefix)
                .then_some(value.trim())
                .filter(|v| !v.is_empty())
        })
    };
    value("Override").or_else(|| value("Physical"))
}

fn has_display_override(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.trim_start().starts_with("Override"))
}

/// Whether the TV now reports `preset`. Android drops an override equal to
/// the physical value, so a matching physical value with no override counts.
fn display_scaling_applied(preset: DisplayScalePreset, now: &CurrentDisplayScaling) -> bool {
    let (size, density) = match preset {
        DisplayScalePreset::Uhd4k => ("3839x2160", "640"),
        DisplayScalePreset::Fhd1080p => ("1920x1080", "320"),
        DisplayScalePreset::Hd720p => ("1280x720", "213"),
        DisplayScalePreset::Reset => {
            return !has_display_override(&now.size) && !has_display_override(&now.density)
        }
    };
    effective_display_value(&now.size) == Some(size)
        && effective_display_value(&now.density) == Some(density)
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub enum DisplayScalePreset {
    // Explicit renames: `rename_all = "snake_case"` would produce `uhd4k` /
    // `fhd1080p` (no underscore before the digit), which doesn't match the
    // frontend's `uhd_4k` / `fhd_1080p`. Keep in lockstep with
    // DisplayScalePreset in src/lib/types.ts.
    /// 3839x2160 @ density 640. Shield TV won't accept 3840 width, and density
    /// 540 breaks some app menus (Disney+, HBO) — see issue #24.
    #[serde(rename = "uhd_4k")]
    Uhd4k,
    /// 1920x1080 @ density 320.
    #[serde(rename = "fhd_1080p")]
    Fhd1080p,
    /// 1280x720 @ density 213. A quarter of 1080p's pixels again — the
    /// cheapest the UI can render while staying a supported TV resolution.
    /// Density 213 keeps UI elements the same physical size as the other two
    /// presets (720/1080 x 320, rounded to Android's tvdpi bucket).
    #[serde(rename = "hd_720p")]
    Hd720p,
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
    state.require_pro(Feature::TweaksWrite)?;
    let adb = state.adb_snapshot().await;
    let cmds: Vec<&str> = match preset {
        DisplayScalePreset::Uhd4k => vec!["wm size 3839x2160", "wm density 640"],
        DisplayScalePreset::Fhd1080p => vec!["wm size 1920x1080", "wm density 320"],
        DisplayScalePreset::Hd720p => vec!["wm size 1280x720", "wm density 213"],
        DisplayScalePreset::Reset => vec!["wm size reset", "wm density reset"],
    };
    let cmd = cmds.join("; ");
    let out = adb
        .shell(&serial, &cmd)
        .await
        .map_err(|e| format!("wm: {e}"))?;
    Ok(display_scaling_outcome(adb.as_ref(), &serial, preset, &out).await)
}

/// Judge a `wm` write by its exit status and failure output, then by what
/// the TV reports afterwards.
async fn display_scaling_outcome(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    preset: DisplayScalePreset,
    out: &crate::adb::AdbOutput,
) -> DisplayScaleResult {
    if !super::verification::succeeded(out) {
        return DisplayScaleResult {
            ok: false,
            message: super::verification::failure_text(out),
        };
    }
    match read_display_scaling(adb, serial).await {
        Ok(now) if display_scaling_applied(preset, &now) => DisplayScaleResult {
            ok: true,
            message: format!("{}\n{}", now.size, now.density),
        },
        Ok(now) => DisplayScaleResult {
            ok: false,
            message: format!(
                "The TV did not take the new scaling. It reports:\n{}\n{}",
                now.size, now.density
            ),
        },
        Err(e) => DisplayScaleResult {
            ok: false,
            message: format!("Sent, but the TV's display scaling could not be read back ({e})."),
        },
    }
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
    state.require_pro(Feature::TweaksWrite)?;
    let adb = state.adb_snapshot().await;
    set_private_dns_impl(
        adb.as_ref(),
        &serial,
        &mode,
        hostname.as_deref(),
        std::time::Duration::from_secs(2),
    )
    .await
}

const PRIVATE_DNS_MODE: &str = "private_dns_mode";
const PRIVATE_DNS_SPECIFIER: &str = "private_dns_specifier";

/// The Private DNS mode and hostname, from a checked read. `None` means
/// unset, never "couldn't read".
async fn read_private_dns(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<PrivateDnsState, String> {
    let cmd = crate::adb::checked_batch_command(&[
        "settings get global private_dns_mode",
        "settings get global private_dns_specifier",
    ]);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("settings get: {e}"))?;
    let sections = crate::adb::parse_checked_batch(&out.stdout, 2, &[0, 1])?;
    let value = |s: &str| {
        let v = s.trim();
        (!v.is_empty() && v != "null").then(|| v.to_string())
    };
    Ok(PrivateDnsState {
        mode: value(&sections[0]),
        hostname: value(&sections[1]),
    })
}

fn private_dns_label(state: &PrivateDnsState) -> String {
    match state.mode.as_deref() {
        Some("off") => "Off".to_string(),
        Some("hostname") => state
            .hostname
            .clone()
            .unwrap_or_else(|| "a custom hostname".to_string()),
        _ => "Automatic".to_string(),
    }
}

/// Put back the hostname and mode read before a change. True only when the
/// TV confirms both.
async fn restore_private_dns(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    previous: &PrivateDnsState,
) -> bool {
    let host = super::verification::put_setting_confirmed(
        adb,
        serial,
        "global",
        PRIVATE_DNS_SPECIFIER,
        previous.hostname.as_deref(),
    )
    .await;
    let mode = super::verification::put_setting_confirmed(
        adb,
        serial,
        "global",
        PRIVATE_DNS_MODE,
        previous.mode.as_deref(),
    )
    .await;
    host.is_ok() && mode.is_ok()
}

async fn set_private_dns_impl(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    mode: &str,
    hostname: Option<&str>,
    probe_wait: std::time::Duration,
) -> Result<PrivateDnsResult, String> {
    let simple = |ok: bool, message: String| PrivateDnsResult {
        ok,
        message,
        reverted: false,
    };
    match mode {
        "off" | "opportunistic" => {
            let label = if mode == "off" {
                "off"
            } else {
                "set to automatic"
            };
            Ok(
                match super::verification::put_setting_confirmed(
                    adb,
                    serial,
                    "global",
                    PRIVATE_DNS_MODE,
                    Some(mode),
                )
                .await
                {
                    Ok(()) => simple(true, format!("Private DNS {label}.")),
                    Err(e) => simple(false, format!("Private DNS was not changed: {e}")),
                },
            )
        }
        "hostname" => {
            let host = hostname.unwrap_or_default().trim();
            if !is_valid_dns_hostname(host) {
                return Ok(simple(
                    false,
                    format!("{host:?} is not a valid hostname. Use one like dns.adguard.com."),
                ));
            }
            // What to go back to if the new host can't resolve. Unreadable
            // means Automatic, the setting that keeps the TV online.
            let previous = read_private_dns(adb, serial)
                .await
                .unwrap_or(PrivateDnsState {
                    mode: Some("opportunistic".to_string()),
                    hostname: None,
                });
            let write = async {
                super::verification::put_setting_confirmed(
                    adb,
                    serial,
                    "global",
                    PRIVATE_DNS_SPECIFIER,
                    Some(host),
                )
                .await?;
                super::verification::put_setting_confirmed(
                    adb,
                    serial,
                    "global",
                    PRIVATE_DNS_MODE,
                    Some("hostname"),
                )
                .await
            }
            .await;
            if let Err(e) = write {
                let restored = restore_private_dns(adb, serial, &previous).await;
                return Ok(PrivateDnsResult {
                    ok: false,
                    message: if restored {
                        format!("Private DNS was not changed: {e}")
                    } else {
                        format!(
                            "Setting Private DNS failed ({e}), and putting back {} could not be \
                             confirmed. Check Private DNS on the TV.",
                            private_dns_label(&previous)
                        )
                    },
                    reverted: restored,
                });
            }

            // Probe: a custom DoT host that can't be reached leaves the device
            // with no DNS. Retry a few times (DoT validation lags a few seconds)
            // before deciding it's dead.
            let mut resolved = false;
            for _ in 0..3 {
                tokio::time::sleep(probe_wait).await;
                if let Ok(out) = adb
                    .shell(serial, "ping -c1 -W3 connectivitycheck.gstatic.com")
                    .await
                {
                    if out.combined().contains("bytes from") {
                        resolved = true;
                        break;
                    }
                }
            }
            if resolved {
                return Ok(simple(
                    true,
                    format!("Private DNS set to {host}. A test lookup worked."),
                ));
            }
            let restored = restore_private_dns(adb, serial, &previous).await;
            let label = private_dns_label(&previous);
            Ok(PrivateDnsResult {
                ok: false,
                message: if restored {
                    format!(
                        "A test lookup through {host} failed, so Private DNS was set back to \
                         {label} to keep the TV online. Check the hostname."
                    )
                } else {
                    format!(
                        "A test lookup through {host} failed, and setting Private DNS back to \
                         {label} could not be confirmed. The TV may have no working DNS: set \
                         Private DNS to Automatic or Off on the TV."
                    )
                },
                reverted: restored,
            })
        }
        other => Ok(simple(false, format!("unknown Private DNS mode: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_setting_command, get_tweaks_for, is_valid_dns_hostname, parse_cached_limit,
        BASIC_DAYDREAM_COMPONENT,
    };
    use crate::adb::{batch::BATCH_STATUS, BATCH_SEPARATOR};
    use crate::commands::test_support::MockAdb;
    use std::time::Duration;

    fn dns_list(lines: &str) -> String {
        settings_output(&[lines])
    }

    #[tokio::test]
    async fn off_and_automatic_are_reported_only_when_the_tv_confirms_them() {
        let confirmed =
            MockAdb::default().on_shell("settings list global", &dns_list("private_dns_mode=off"));
        let res = super::set_private_dns_impl(&confirmed, "TV", "off", None, Duration::ZERO)
            .await
            .unwrap();
        assert!(res.ok, "{}", res.message);
        for mock in [
            MockAdb::default().on_shell(
                "settings list global",
                &dns_list("private_dns_mode=hostname"),
            ),
            MockAdb::default().on_shell_exit("settings put", "", 1),
            MockAdb::default().on_shell("settings put", "Error: permission denied"),
            MockAdb::default().on_shell_err("settings list global", "offline"),
        ] {
            let res =
                super::set_private_dns_impl(&mock, "TV", "opportunistic", None, Duration::ZERO)
                    .await
                    .unwrap();
            assert!(!res.ok, "{}", res.message);
        }
    }

    #[tokio::test]
    async fn a_dead_host_restores_the_previous_mode_not_automatic() {
        let mock = MockAdb::default()
            .on_shell(
                "settings get global private_dns_mode",
                &settings_output(&["off", "null"]),
            )
            .on_shell_seq(
                "settings list global",
                &[
                    &dns_list("private_dns_mode=off\nprivate_dns_specifier=dns.example.com"),
                    &dns_list("private_dns_mode=hostname\nprivate_dns_specifier=dns.example.com"),
                    &dns_list("private_dns_mode=hostname"),
                    &dns_list("private_dns_mode=off"),
                ],
            )
            .on_shell("ping", "unknown host");
        let log = mock.shell_log();
        let res = super::set_private_dns_impl(
            &mock,
            "TV",
            "hostname",
            Some("dns.example.com"),
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert!(!res.ok);
        assert!(res.reverted, "{}", res.message);
        assert!(res.message.contains("set back to Off"), "{}", res.message);
        let calls = log.lock().unwrap();
        assert!(calls.contains(&"settings delete global private_dns_specifier".to_string()));
        assert!(calls.contains(&"settings put global private_dns_mode 'off'".to_string()));
        assert!(
            !calls.iter().any(|c| c.contains("opportunistic")),
            "{calls:?}"
        );
    }

    #[tokio::test]
    async fn a_revert_that_fails_is_not_reported_as_reverted() {
        let mock = MockAdb::default()
            .on_shell(
                "settings get global private_dns_mode",
                &settings_output(&["opportunistic", "null"]),
            )
            .on_shell_seq(
                "settings list global",
                &[
                    &dns_list("private_dns_specifier=dns.example.com"),
                    &dns_list("private_dns_mode=hostname\nprivate_dns_specifier=dns.example.com"),
                    // The revert does not take.
                    &dns_list("private_dns_mode=hostname\nprivate_dns_specifier=dns.example.com"),
                ],
            )
            .on_shell("ping", "");
        let res = super::set_private_dns_impl(
            &mock,
            "TV",
            "hostname",
            Some("dns.example.com"),
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert!(!res.ok);
        assert!(!res.reverted, "{}", res.message);
        assert!(
            res.message.contains("could not be confirmed"),
            "{}",
            res.message
        );
    }

    #[tokio::test]
    async fn a_working_host_is_kept() {
        let mock = MockAdb::default()
            .on_shell(
                "settings get global private_dns_mode",
                &settings_output(&["opportunistic", "null"]),
            )
            .on_shell_seq(
                "settings list global",
                &[
                    &dns_list(
                        "private_dns_mode=opportunistic\nprivate_dns_specifier=dns.example.com",
                    ),
                    &dns_list("private_dns_mode=hostname\nprivate_dns_specifier=dns.example.com"),
                ],
            )
            .on_shell("ping", "64 bytes from 1.2.3.4");
        let res = super::set_private_dns_impl(
            &mock,
            "TV",
            "hostname",
            Some("dns.example.com"),
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert!(res.ok && !res.reverted, "{}", res.message);
    }

    fn wm(size: &str, density: &str) -> String {
        settings_output(&[size, density])
    }

    #[tokio::test]
    async fn display_scaling_is_judged_by_exit_status_and_what_the_tv_reports() {
        use super::{display_scaling_outcome, DisplayScalePreset as P};
        let ok_out = crate::adb::AdbOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        };
        let applied = MockAdb::default().on_shell(
            "wm size",
            &wm(
                "Physical size: 3840x2160\nOverride size: 1920x1080",
                "Physical density: 320",
            ),
        );
        assert!(
            display_scaling_outcome(&applied, "TV", P::Fhd1080p, &ok_out)
                .await
                .ok
        );
        // The same report is not a reset, nor a 720p switch.
        assert!(
            !display_scaling_outcome(&applied, "TV", P::Reset, &ok_out)
                .await
                .ok
        );
        assert!(
            !display_scaling_outcome(&applied, "TV", P::Hd720p, &ok_out)
                .await
                .ok
        );
        let reset = MockAdb::default().on_shell(
            "wm size",
            &wm("Physical size: 1920x1080", "Physical density: 320"),
        );
        assert!(
            display_scaling_outcome(&reset, "TV", P::Reset, &ok_out)
                .await
                .ok
        );
        assert!(
            display_scaling_outcome(&reset, "TV", P::Fhd1080p, &ok_out)
                .await
                .ok
        );

        let unreadable = MockAdb::default().on_shell_err("wm size", "offline");
        assert!(
            !display_scaling_outcome(&unreadable, "TV", P::Reset, &ok_out)
                .await
                .ok
        );
        let failed = crate::adb::AdbOutput {
            stdout: "Error: bad size".into(),
            ..ok_out.clone()
        };
        assert!(
            !display_scaling_outcome(&reset, "TV", P::Reset, &failed)
                .await
                .ok
        );
        let nonzero = crate::adb::AdbOutput {
            exit_code: Some(255),
            ..ok_out
        };
        assert!(
            !display_scaling_outcome(&reset, "TV", P::Reset, &nonzero)
                .await
                .ok
        );
    }

    #[tokio::test]
    async fn a_failed_display_read_is_an_error_not_blank_values() {
        for mock in [
            MockAdb::default().on_shell_err("wm size", "offline"),
            MockAdb::default().on_shell("wm size", ""),
            MockAdb::default().on_shell("wm size", &wm("", "")),
        ] {
            assert!(super::read_display_scaling(&mock, "TV").await.is_err());
        }
    }

    fn settings_output(values: &[&str]) -> String {
        values
            .iter()
            .map(|value| format!("{value}\n{BATCH_STATUS}0\n"))
            .collect::<Vec<_>>()
            .join(&format!("{BATCH_SEPARATOR}\n"))
    }

    #[tokio::test]
    async fn tweaks_preserve_empty_values_without_shifting_audio_settings() {
        let output = settings_output(&[
            "",
            "1",
            "0",
            "1",
            "null",
            "400",
            "0.5",
            "1",
            "1",
            "null",
            "3",
            "5,6,99",
            "com.android.dreams.basic/com.android.dreams.basic.BasicDream",
            "1",
            "  CUR_MAX_CACHED_PROCESSES=32",
        ]);
        let adb = MockAdb::default().on_shell("settings get", &output);
        let tweaks = get_tweaks_for(&adb, "serial").await.unwrap();
        assert_eq!(tweaks.background_process_limit, None);
        assert_eq!(tweaks.cached_process_limit, Some(32));
        assert_eq!(tweaks.hdmi_control_enabled.as_deref(), Some(""));
        assert_eq!(
            tweaks.hdmi_control_auto_wakeup_enabled.as_deref(),
            Some("1")
        );
        assert_eq!(tweaks.match_content_frame_rate, None);
        assert_eq!(tweaks.encoded_surround_output.as_deref(), Some("3"));
        assert_eq!(
            tweaks.encoded_surround_output_enabled_formats.as_deref(),
            Some("5,6,99")
        );
        assert_eq!(
            tweaks.screensaver_components.as_deref(),
            Some("com.android.dreams.basic/com.android.dreams.basic.BasicDream")
        );
        assert_eq!(tweaks.screensaver_enabled.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn tweaks_reject_failed_or_truncated_readback() {
        let output = settings_output(&["null"; 15]);
        let failed = output.replacen(&format!("{BATCH_STATUS}0"), &format!("{BATCH_STATUS}1"), 1);
        let truncated = settings_output(&["null"; 14]);
        for output in [failed, truncated, "null\n".into()] {
            let adb = MockAdb::default().on_shell("settings get", &output);
            assert!(get_tweaks_for(&adb, "serial").await.is_err());
        }
        let adb = MockAdb::default().on_shell_err("settings get", "device offline");
        assert!(get_tweaks_for(&adb, "serial").await.is_err());
    }

    #[tokio::test]
    async fn an_unreadable_cached_limit_is_unknown_not_fatal() {
        let mut values = vec!["null"; 14];
        values.push("");
        let output = settings_output(&values);
        let failed_last = {
            let at = output.rfind(&format!("{BATCH_STATUS}0")).unwrap();
            format!("{}{BATCH_STATUS}1\n", &output[..at])
        };
        let adb = MockAdb::default().on_shell("settings get", &failed_last);
        let tweaks = get_tweaks_for(&adb, "serial").await.unwrap();
        assert_eq!(tweaks.cached_process_limit, None);
    }

    #[test]
    fn cached_limit_parses_only_the_current_value() {
        assert_eq!(
            parse_cached_limit("  CUR_MAX_CACHED_PROCESSES=32\n"),
            Some(32)
        );
        assert_eq!(parse_cached_limit("  max_cached_processes=32\n"), None);
        assert_eq!(parse_cached_limit("CUR_MAX_CACHED_PROCESSES=abc"), None);
        assert_eq!(parse_cached_limit(""), None);
    }

    #[test]
    fn basic_daydream_is_the_aosp_component_name() {
        assert_eq!(
            BASIC_DAYDREAM_COMPONENT,
            "com.android.dreams.basic/com.android.dreams.basic.BasicDream"
        );
    }

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
        // Values containing shell metacharacters are quoted rather than
        // rejected — the device shell cannot interpret them inside single quotes.
        assert!(build_setting_command("global", "k", "1; rm -rf /").is_ok());
        assert!(build_setting_command("global", "k", "a|b").is_ok());
        assert!(build_setting_command("global", "k", "a&b").is_ok());
        assert!(build_setting_command("global", "k", "$(whoami)").is_ok());
        assert!(build_setting_command("global", "k", "`id`").is_ok());
    }
    #[tokio::test]
    async fn write_setting_rejects_mobile_nonzero_and_hidden_stderr_error() {
        for output in [
            crate::adb::AdbOutput {
                stdout: "".into(),
                stderr: "".into(),
                exit_code: Some(1),
            },
            crate::adb::AdbOutput {
                stdout: "ok".into(),
                stderr: "SecurityException: denied".into(),
                exit_code: Some(0),
            },
            crate::adb::AdbOutput {
                stdout: "Failure".into(),
                stderr: "".into(),
                exit_code: Some(0),
            },
        ] {
            let mock = MockAdb::default().on_shell_output("settings put", output);
            let log = mock.shell_log();
            let state = crate::commands::test_support::state_with(mock);
            let result =
                super::write_setting_for(&state, "TV", "secure", "screensaver_enabled", "1")
                    .await
                    .unwrap();
            assert!(!result.ok);
            assert_eq!(
                log.lock().unwrap().len(),
                1,
                "failure must not be treated as verified"
            );
        }
    }

    #[tokio::test]
    async fn write_setting_requires_matching_readback() {
        for (value, expected) in [("1", true), ("0", false)] {
            let mock = MockAdb::default().on_shell(
                "settings list secure",
                &format!("screensaver_enabled={value}\n{BATCH_STATUS}0\n"),
            );
            let state = crate::commands::test_support::state_with(mock);
            assert_eq!(
                super::write_setting_for(&state, "TV", "secure", "screensaver_enabled", "1")
                    .await
                    .unwrap()
                    .ok,
                expected
            );
        }
    }
}
