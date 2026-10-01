//! The bug-report bundle, as Markdown.
//!
//! Pure: every value is handed in by the host layer, which is where the
//! `getprop` reads, the `adb version` call and the log-file read live. The
//! formatter exists so what a user is asked to paste into a public issue is
//! one auditable function with a test, rather than string-building scattered
//! through a command handler.
//!
//! What it deliberately does *not* contain: any package inventory. A list of
//! everything installed on someone's TV is not needed to debug a device that
//! will not open, and it is not ours to publish on their behalf.

use super::detection::{DeviceType, TvEvidence};
use super::types::{ConnectionType, DeviceProperties};

/// Everything the bundle reports about the host.
pub struct DiagnosticsInput<'a> {
    pub app_version: &'a str,
    pub os: &'a str,
    pub arch: &'a str,
    /// Where the adb binary was found, or `None` if none was.
    pub adb_path: Option<&'a str>,
    /// Raw `adb version` output, or `None` when it could not be run.
    pub adb_version: Option<&'a str>,
    pub device: Option<DeviceDiagnostics<'a>>,
    /// A device was selected but could not be read at all: its serial and
    /// the error. Kept so the report names the device it was asked about
    /// instead of claiming nothing was selected.
    pub unreadable_device: Option<(&'a str, &'a str)>,
    /// Tail of today's log file, oldest line first.
    pub log_tail: &'a [String],
}

/// Everything the bundle reports about one device.
pub struct DeviceDiagnostics<'a> {
    pub serial: &'a str,
    pub connection: ConnectionType,
    /// `None` when the device could not be queried (unauthorized, offline).
    pub properties: Option<&'a DeviceProperties>,
    pub tv_evidence: TvEvidence,
    pub device_type: DeviceType,
    /// Component names that answered the HOME-handler query.
    pub home_handlers: &'a [String],
    /// The current Home app as the Launcher tab reads it (`None` when it
    /// could not be read), with the role/resolver disagreement note if any.
    pub current_home: Option<&'a super::launcher::HomeReading>,
}

fn label(evidence: TvEvidence) -> &'static str {
    match evidence {
        TvEvidence::Tv => "reported itself as a TV",
        TvEvidence::NotTv => "reported itself as something other than a TV",
        TvEvidence::Unknown => "said neither way (unknown)",
    }
}

fn transport(connection: ConnectionType) -> &'static str {
    match connection {
        ConnectionType::Network => "network (ADB over TCP)",
        ConnectionType::Usb => "USB",
    }
}

/// A value that may be empty. Empty prints as `(empty)` rather than nothing,
/// so a reader can tell "the device answered with nothing" from "we forgot to
/// ask".
fn shown(value: &str) -> String {
    if value.trim().is_empty() {
        "(empty)".to_string()
    } else {
        value.trim().to_string()
    }
}

fn leanback(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "(no answer)",
    }
}

/// Render the bundle. Stable enough to assert on; it is read by people, not
/// parsed by anything.
pub fn format_diagnostics(input: &DiagnosticsInput) -> String {
    let mut out = String::new();
    out.push_str("## ATV Optimizer diagnostics\n\n");
    out.push_str(&format!("- App version: {}\n", input.app_version));
    out.push_str(&format!("- OS: {} ({})\n", input.os, input.arch));
    out.push_str(&format!(
        "- adb path: {}\n",
        input.adb_path.unwrap_or("(not found)")
    ));
    out.push_str(&format!(
        "- adb version: {}\n",
        input
            .adb_version
            .map(|v| v.lines().next().unwrap_or("").trim())
            .filter(|v| !v.is_empty())
            .unwrap_or("(unavailable)")
    ));

    match &input.device {
        None => match input.unreadable_device {
            Some((serial, error)) => out.push_str(&format!(
                "\n### Device\n\n- Serial: {serial}\n- Could not be read: {error}\n- Properties, TV evidence and Home apps: unknown\n"
            )),
            None => out.push_str("\n### Device\n\nNo device selected.\n"),
        },
        Some(device) => {
            out.push_str(&format!("\n### Device `{}`\n\n", device.serial));
            out.push_str(&format!("- Transport: {}\n", transport(device.connection)));
            out.push_str(&format!(
                "- Detected type: {}\n",
                device.device_type.label()
            ));
            out.push_str(&format!(
                "- TV evidence: {} — {}\n",
                match device.tv_evidence {
                    TvEvidence::Tv => "tv",
                    TvEvidence::NotTv => "not_tv",
                    TvEvidence::Unknown => "unknown",
                },
                label(device.tv_evidence)
            ));

            match device.properties {
                None => out.push_str(
                    "\nProperties: unreadable (the device is unauthorized or offline), so \
                     nothing below is known about it.\n",
                ),
                Some(props) => {
                    out.push_str("\n#### Properties\n\n");
                    let friendly = props.friendly_name.as_deref().unwrap_or("(unset)");
                    for (key, value) in [
                        ("device_name", shown(friendly)),
                        ("ro.product.brand", shown(&props.brand)),
                        ("ro.product.model", shown(&props.model)),
                        ("ro.product.device", shown(&props.device_codename)),
                        ("ro.product.manufacturer", shown(&props.manufacturer)),
                        ("ro.build.version.release", shown(&props.android_release)),
                        ("ro.build.version.sdk", shown(&props.sdk_level)),
                        ("ro.build.id", shown(&props.build_id)),
                        ("ro.board.platform", shown(&props.board_platform)),
                        ("ro.build.characteristics", shown(&props.characteristics)),
                        ("ro.serialno", shown(&props.serial_number)),
                        (
                            "android.software.leanback",
                            leanback(props.leanback).to_string(),
                        ),
                    ] {
                        out.push_str(&format!("- {key}: `{value}`\n"));
                    }
                }
            }

            out.push_str("\n#### Current Home\n\n");
            match device.current_home.and_then(|h| h.package.as_deref()) {
                Some(pkg) => out.push_str(&format!("- `{pkg}`\n")),
                None => out.push_str("(unreadable)\n"),
            }
            if let Some(note) = device.current_home.and_then(|h| h.note.as_deref()) {
                out.push_str(&format!("- Note: {note}\n"));
            }

            out.push_str("\n#### HOME handlers\n\n");
            if device.home_handlers.is_empty() {
                out.push_str("(none reported)\n");
            } else {
                for handler in device.home_handlers {
                    out.push_str(&format!("- `{}`\n", handler.trim()));
                }
            }
        }
    }

    out.push_str("\n### Recent log\n\n");
    if input.log_tail.is_empty() {
        out.push_str("(no log lines)\n");
    } else {
        out.push_str("```\n");
        for line in input.log_tail {
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out.push_str("```\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn props() -> DeviceProperties {
        DeviceProperties {
            friendly_name: Some("Living Room".into()),
            brand: "Xiaomi".into(),
            model: "MiBOX4".into(),
            device_codename: "cezanne".into(),
            manufacturer: "Xiaomi".into(),
            android_release: "12".into(),
            sdk_level: "31".into(),
            build_id: "STTE".into(),
            board_platform: "amlogic".into(),
            characteristics: "nosdcard".into(),
            serial_number: "ABC123".into(),
            leanback: Some(true),
        }
    }

    #[test]
    fn a_host_only_bundle_says_no_device_rather_than_inventing_one() {
        let report = format_diagnostics(&DiagnosticsInput {
            app_version: "2.2.0",
            os: "macos",
            arch: "aarch64",
            adb_path: None,
            adb_version: None,
            device: None,
            unreadable_device: None,
            log_tail: &[],
        });

        assert!(report.contains("- App version: 2.2.0"), "{report}");
        assert!(report.contains("- OS: macos (aarch64)"), "{report}");
        assert!(report.contains("- adb path: (not found)"), "{report}");
        assert!(report.contains("No device selected."), "{report}");

        let unreadable = format_diagnostics(&DiagnosticsInput {
            app_version: "2.3.0",
            os: "macos",
            arch: "aarch64",
            adb_path: None,
            adb_version: None,
            device: None,
            unreadable_device: Some(("192.168.1.9:5555", "device offline")),
            log_tail: &[],
        });
        assert!(unreadable.contains("192.168.1.9:5555"), "{unreadable}");
        assert!(unreadable.contains("device offline"), "{unreadable}");
        assert!(!unreadable.contains("No device selected."), "{unreadable}");
        assert!(report.contains("(no log lines)"), "{report}");
    }

    /// The whole point of #120's bundle: the two signals that decide whether
    /// the tools open have to be in what the user pastes, verbatim.
    #[test]
    fn the_device_section_carries_both_tv_signals_and_the_verdict() {
        let props = props();
        let handlers = vec!["com.google.android.tvlauncher/.MainActivity".to_string()];
        let report = format_diagnostics(&DiagnosticsInput {
            app_version: "2.2.0",
            os: "linux",
            arch: "x86_64",
            adb_path: Some("/usr/bin/adb"),
            adb_version: Some("Android Debug Bridge version 1.0.41\nVersion 35.0.0"),
            device: Some(DeviceDiagnostics {
                serial: "192.168.1.42:5555",
                connection: ConnectionType::Network,
                properties: Some(&props),
                tv_evidence: TvEvidence::Tv,
                device_type: DeviceType::GoogleTv,
                home_handlers: &handlers,
                current_home: Some(&crate::engine::HomeReading {
                    package: Some("com.klevico.monet".to_string()),
                    activity: None,
                    note: Some("resolve-activity HOME named a setup helper".to_string()),
                }),
            }),
            unreadable_device: None,
            log_tail: &["first".to_string(), "second".to_string()],
        });

        assert!(
            report.contains("- ro.build.characteristics: `nosdcard`"),
            "{report}"
        );
        assert!(
            report.contains("- android.software.leanback: `true`"),
            "{report}"
        );
        assert!(report.contains("- TV evidence: tv —"), "{report}");
        assert!(report.contains("- Transport: network"), "{report}");
        assert!(
            report.contains("`com.google.android.tvlauncher/.MainActivity`"),
            "{report}"
        );
        assert!(
            report.contains(
                "#### Current Home\n\n- `com.klevico.monet`\n- Note: resolve-activity HOME named a setup helper\n"
            ),
            "{report}"
        );
        // Only the first line of `adb version`; the rest is build noise.
        assert!(
            report.contains("- adb version: Android Debug Bridge version 1.0.41\n"),
            "{report}"
        );
        assert!(report.contains("```\nfirst\nsecond\n```"), "{report}");
    }

    #[test]
    fn an_unreadable_device_claims_nothing_about_itself() {
        let report = format_diagnostics(&DiagnosticsInput {
            app_version: "2.2.0",
            os: "windows",
            arch: "x86_64",
            adb_path: Some("adb.exe"),
            adb_version: Some("   "),
            device: Some(DeviceDiagnostics {
                serial: "192.168.1.9:5555",
                connection: ConnectionType::Usb,
                properties: None,
                tv_evidence: TvEvidence::Unknown,
                device_type: DeviceType::Unknown,
                home_handlers: &[],
                current_home: None,
            }),
            unreadable_device: None,
            log_tail: &[],
        });

        assert!(report.contains("Properties: unreadable"), "{report}");
        assert!(report.contains("- TV evidence: unknown —"), "{report}");
        assert!(report.contains("(none reported)"), "{report}");
        assert!(
            report.contains("#### Current Home\n\n(unreadable)\n"),
            "{report}"
        );
        // A whitespace-only `adb version` is no version at all.
        assert_eq!(
            report.contains("- adb version: (unavailable)"),
            true,
            "{report}"
        );
    }

    #[test]
    fn a_package_inventory_is_never_part_of_the_bundle() {
        // Guard on the shape of the input, not on the output text: there is
        // no field that could carry one, so it cannot leak by accident.
        let report = format_diagnostics(&DiagnosticsInput {
            app_version: "2.2.0",
            os: "macos",
            arch: "aarch64",
            adb_path: None,
            adb_version: None,
            device: None,
            unreadable_device: None,
            log_tail: &[],
        });
        assert!(!report.contains("package:"), "{report}");
    }
}
