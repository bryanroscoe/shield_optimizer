//! The device-gathering half of the bug-report bundle, shared by desktop and
//! mobile.
//!
//! Each app supplies what only it knows (where its adb came from, its own log
//! tail) and this module reads the device and hands everything to the pure
//! [`crate::engine::diagnostics::format_diagnostics`]. Nothing here sends
//! anything anywhere: the apps return the text and the UI puts it on the
//! clipboard.

use crate::adb::AdbDriver;
use crate::engine::diagnostics::{format_diagnostics, DeviceDiagnostics, DiagnosticsInput};

use super::{devices, launcher, AppState};

/// Log lines carried in a bug report. Enough to hold a failed device scan and
/// everything around it, not so many that the report stops being readable.
pub const LOG_TAIL_LINES: usize = 200;

/// What the calling app knows about itself.
pub struct HostDiagnostics<'a> {
    pub app_version: &'a str,
    pub os: &'a str,
    pub arch: &'a str,
    /// Where the adb binary was found. `None` when there is none to find.
    pub adb_path: Option<&'a str>,
    /// Tail of the app's own log, oldest line first.
    pub log_tail: &'a [String],
}

/// The last `lines` lines of `text`, oldest first.
pub fn tail_lines(text: &str, lines: usize) -> Vec<String> {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| (*l).to_string())
        .collect()
}

/// `(package, enabled)` for each catalogued setup helper that is installed.
/// Read from the package lists because a disabled one never answers the HOME
/// query; `None` when they could not be read.
async fn setup_helper_states(adb: &dyn AdbDriver, serial: &str) -> Option<Vec<(String, bool)>> {
    let cmd = crate::adb::checked_batch_command(&["pm list packages", "pm list packages -d"]);
    let out = adb.shell(serial, &cmd).await.ok()?;
    let sections = crate::adb::parse_checked_batch(&out.stdout, 2, &[0, 1]).ok()?;
    setup_helper_states_from(&sections[0], &sections[1])
}

/// An Android device always has packages, so an empty installed list is an
/// unreadable answer, not proof the helper is absent.
fn setup_helper_states_from(installed: &str, disabled: &str) -> Option<Vec<(String, bool)>> {
    let installed = crate::adb::parse_installed_packages_output(installed);
    if installed.is_empty() {
        return None;
    }
    let disabled = crate::adb::parse_disabled_packages_output(disabled);
    Some(
        super::loader::launchers()
            .transient_home_holders
            .iter()
            .filter(|p| installed.contains(p))
            .map(|p| (p.clone(), !disabled.contains(p)))
            .collect(),
    )
}

/// The text a user pastes into a bug report.
///
/// Deliberately narrow — host facts, one device's identity and TV evidence,
/// its HOME handlers, setup helpers, the Support ID and the caller's log tail.
/// No package inventory and no device logs.
pub async fn collect_diagnostics_text(
    state: &AppState,
    host: HostDiagnostics<'_>,
    serial: Option<&str>,
) -> String {
    let adb = state.adb_snapshot().await;
    let adb_version = adb
        .raw(&["version"])
        .await
        .ok()
        .map(|out| out.stdout.trim().to_string())
        .filter(|v| !v.is_empty());

    // Resolve the device first so a failure there is reported as "no device
    // section" rather than failing the whole bundle — the host half is still
    // worth having when the device is the thing that is broken.
    let mut unreadable: Option<String> = None;
    let device = match serial {
        None => None,
        Some(serial) => match devices::device_profile_impl(state, serial).await {
            Ok(device) => {
                // Reuses the launcher tab's own parser: Android's ResolveInfo
                // dump names the package under `packageName=`, not `name=`.
                let handlers = adb
                    .shell(serial, launcher::HOME_HANDLER_QUERY)
                    .await
                    .ok()
                    .map(|out| launcher::parse_home_handler_packages(&out.stdout))
                    .unwrap_or_default();
                let current_home = launcher::read_current_home(&*adb, serial).await.ok();
                let helpers = setup_helper_states(&*adb, serial).await;
                Some((device, handlers, current_home, helpers))
            }
            Err(e) => {
                tracing::warn!(serial, error = %e, "diagnostics: device unavailable");
                unreadable = Some(e);
                None
            }
        },
    };

    let support_id = state.telemetry.settings().support_id;

    format_diagnostics(&DiagnosticsInput {
        app_version: host.app_version,
        os: host.os,
        arch: host.arch,
        adb_path: host.adb_path,
        adb_version: adb_version.as_deref(),
        device: device
            .as_ref()
            .map(
                |(device, handlers, current_home, helpers)| DeviceDiagnostics {
                    serial: &device.serial,
                    connection: device.connection,
                    properties: device.properties.as_ref(),
                    tv_evidence: device.tv_evidence,
                    device_type: device.device_type,
                    home_handlers: handlers,
                    current_home: current_home.as_ref(),
                    setup_helpers: helpers.as_deref(),
                },
            ),
        unreadable_device: match (serial, unreadable.as_deref()) {
            (Some(serial), Some(error)) => Some((serial, error)),
            _ => None,
        },
        log_tail: host.log_tail,
        support_id: support_id.as_deref(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::{state_with, MockAdb};

    fn host(tail: &[String]) -> HostDiagnostics<'_> {
        HostDiagnostics {
            app_version: "9.9.9",
            os: "android",
            arch: "aarch64",
            adb_path: Some("(built in)"),
            log_tail: tail,
        }
    }

    #[test]
    fn an_empty_installed_list_is_unknown_not_absent() {
        assert_eq!(setup_helper_states_from("", ""), None);
        assert_eq!(setup_helper_states_from("\n", "package:a\n"), None);
        let some = setup_helper_states_from("package:com.android.settings\n", "");
        assert_eq!(some, Some(vec![]));
    }

    #[test]
    fn home_handlers_are_read_from_the_real_resolveinfo_shape() {
        // `name=` is the bare class (no slash) and `packageName=` is the
        // separate field that actually names the package.
        let stdout = "Activity #0:\n  \
                      ActivityInfo:\n    \
                      name=com.spocky.projengmenu.ui.home.MainActivity\n    \
                      packageName=com.spocky.projengmenu\n\n\
                      Activity #1:\n  \
                      ActivityInfo:\n    \
                      name=com.google.android.tvlauncher.MainActivity\n    \
                      packageName=com.google.android.tvlauncher\n";
        assert_eq!(
            launcher::parse_home_handler_packages(stdout),
            vec![
                "com.spocky.projengmenu".to_string(),
                "com.google.android.tvlauncher".to_string(),
            ]
        );
    }

    #[test]
    fn the_tail_is_the_last_n_lines() {
        assert_eq!(tail_lines("a\nb\nc\n", 2), vec!["b", "c"]);
        assert_eq!(tail_lines("a\nb", 10), vec!["a", "b"]);
        assert!(tail_lines("", 10).is_empty());
    }

    #[tokio::test]
    async fn no_device_still_reports_the_host_half_and_the_tail() {
        let tail = vec!["last log line".to_string()];
        let text =
            collect_diagnostics_text(&state_with(MockAdb::default()), host(&tail), None).await;
        assert!(text.contains("9.9.9"), "{text}");
        assert!(text.contains("android"), "{text}");
        assert!(text.contains("(built in)"), "{text}");
        assert!(text.contains("last log line"), "{text}");
    }

    #[tokio::test]
    async fn an_unreadable_device_is_named_not_dropped() {
        let mock = MockAdb::default()
            .on_shell_err("getprop", "device offline")
            .on_shell_err("pm", "device offline");
        let text = collect_diagnostics_text(&state_with(mock), host(&[]), Some("tv-serial")).await;
        assert!(text.contains("tv-serial"), "{text}");
    }
}
