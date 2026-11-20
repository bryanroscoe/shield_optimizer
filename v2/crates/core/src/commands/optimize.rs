//! Optimize / Restore — host bridge between the wizard UI and the
//! pure engine planner. The frontend calls `prepare_optimize` to fetch the
//! plan, then iterates the items and calls the existing per-package commands
//! (`disable_package` / `uninstall_package` / `enable_package`) one at a time
//! so the UI can show live progress.
//!
//! `apply_performance_settings` is the post-pass that mirrors v1's
//! Performance Settings step (animation triple at 0.5 for Optimize, 1.0 for
//! Restore).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::adb::{
    checked_batch_command, parse_checked_batch, parse_disabled_packages_output,
    parse_installed_packages_output, parse_total_pss_by_process,
};
use crate::engine::{compute_plan, OptimizeInputs, OptimizeMode, OptimizePlan};
use crate::license::Feature;

use super::AppState;

/// `prepare_optimize` — fetch installed, disabled, and memory map for `serial`,
/// pick the device-appropriate app list, and run the engine planner.
///
/// `device_type` comes from the caller (the page already profiled the device on
/// load) instead of re-detecting here. Re-detecting through
/// `list_devices_impl` would harvest properties for *every* connected device —
/// slow with several devices, and worse when an unauthorized one stalls.
#[tauri::command]
pub async fn prepare_optimize(
    state: State<'_, AppState>,
    serial: String,
    device_type: crate::engine::DeviceType,
    mode: OptimizeMode,
) -> Result<OptimizePlan, String> {
    prepare_optimize_impl(state.inner(), &serial, device_type, mode).await
}

/// Device-free core of `prepare_optimize` so it can run against a mock driver.
pub async fn prepare_optimize_impl(
    state: &AppState,
    serial: &str,
    device_type: crate::engine::DeviceType,
    mode: OptimizeMode,
) -> Result<OptimizePlan, String> {
    state.require_pro(Feature::OptimizeWizard)?;
    let apps = state.app_lists.for_device(device_type);

    let adb = state.adb_snapshot().await;
    // One round-trip: the mobile transport serializes concurrent shells behind
    // a single connection, so four separate reads would cost four Wi-Fi RTTs
    // for no concurrency.
    let cmd = checked_batch_command(&[
        super::LIST_PACKAGES,
        super::LIST_DISABLED_PACKAGES,
        "dumpsys meminfo",
        super::LIST_THIRD_PARTY_PACKAGES,
    ]);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("pm list packages: {e}"))?;
    let sections = parse_checked_batch(&out.stdout, 4, &[0, 1])?;

    let installed_set: HashSet<String> = parse_installed_packages_output(&sections[0])
        .into_iter()
        .collect();
    // Never turn an unusable empty device read into an already-clean plan.
    if installed_set.is_empty() {
        return Err("pm list packages: no packages reported".to_string());
    }
    let disabled_set: HashSet<String> = parse_disabled_packages_output(&sections[1])
        .into_iter()
        .collect();
    let memory = parse_total_pss_by_process(&sections[2]);
    // Optional: a failed or empty read leaves every install source unknown,
    // which suggests Disable.
    let user_installed: Option<HashSet<String>> = Some(
        parse_installed_packages_output(&sections[3])
            .into_iter()
            .collect::<HashSet<String>>(),
    )
    .filter(|set| !set.is_empty());

    let plan = compute_plan(
        &apps,
        mode,
        &OptimizeInputs {
            installed_packages: &installed_set,
            disabled_packages: &disabled_set,
            memory_map: &memory,
            user_installed_packages: user_installed.as_ref(),
        },
    );
    Ok(plan)
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceProfile {
    /// v1's Optimize default — animation triple at 0.5×.
    Optimized,
    /// v1's Restore default — animation triple at 1.0× (system default).
    Default,
}

#[derive(Serialize)]
pub struct PerformanceResult {
    pub ok: bool,
    pub message: String,
}

/// `apply_performance_settings` — the post-wizard step that writes the
/// animation triple all at once. Optimize → 0.5, Restore → 1.0.
#[tauri::command]
pub async fn apply_performance_settings(
    state: State<'_, AppState>,
    serial: String,
    profile: PerformanceProfile,
) -> Result<PerformanceResult, String> {
    state.require_pro(Feature::TweaksWrite)?;
    let adb = state.adb_snapshot().await;
    Ok(apply_performance_settings_impl(adb.as_ref(), &serial, profile).await)
}

const ANIMATION_SCALES: [&str; 3] = [
    "window_animation_scale",
    "transition_animation_scale",
    "animator_duration_scale",
];

/// Each scale is written and read back on its own: `adb shell` reports only
/// the last command of a `;` chain, so a batch hides a failed write.
async fn apply_performance_settings_impl(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    profile: PerformanceProfile,
) -> PerformanceResult {
    let value = match profile {
        PerformanceProfile::Optimized => "0.5",
        PerformanceProfile::Default => "1",
    };
    let mut failed = Vec::new();
    for key in ANIMATION_SCALES {
        if let Err(e) =
            super::verification::put_setting_confirmed(adb, serial, "global", key, Some(value))
                .await
        {
            failed.push(e);
        }
    }
    if failed.is_empty() {
        PerformanceResult {
            ok: true,
            message: format!("Animations → {value}×"),
        }
    } else {
        PerformanceResult {
            ok: false,
            message: format!("Animations not set to {value}×: {}", failed.join("; ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn restore_refuses_a_failed_disabled_read_even_when_memory_succeeds() {
        let status = crate::adb::batch::BATCH_STATUS;
        let output = format!("package:com.example\n{status}0\n{BATCH_SEPARATOR}\n{status}1\n{BATCH_SEPARATOR}\nmemory\n{status}0\n{BATCH_SEPARATOR}\n{status}0\n");
        let state = crate::commands::test_support::state_with(
            MockAdb::default().on_shell(BATCH_SEPARATOR, &output),
        );
        assert!(
            prepare_optimize_impl(&state, "serial", DeviceType::Shield, OptimizeMode::Restore)
                .await
                .is_err()
        );
    }
    use crate::adb::BATCH_SEPARATOR;
    use crate::commands::test_support::MockAdb;

    #[tokio::test]
    async fn animation_scales_are_confirmed_one_by_one() {
        let all = batched(&[
            "window_animation_scale=0.5\ntransition_animation_scale=0.5\nanimator_duration_scale=0.5",
        ]);
        let mock = MockAdb::default().on_shell("settings list global", &all);
        let log = mock.shell_log();
        let res = apply_performance_settings_impl(&mock, "TV", PerformanceProfile::Optimized).await;
        assert!(res.ok, "{}", res.message);
        assert_eq!(
            log.lock()
                .unwrap()
                .iter()
                .filter(|c| c.starts_with("settings put global"))
                .count(),
            3
        );

        let one_missing = batched(&["window_animation_scale=0.5\ntransition_animation_scale=1"]);
        for mock in [
            MockAdb::default().on_shell("settings list global", &one_missing),
            MockAdb::default()
                .on_shell("settings put global animator", "Error: denied")
                .on_shell("settings list global", &all),
            MockAdb::default().on_shell_err("settings list global", "offline"),
        ] {
            let res =
                apply_performance_settings_impl(&mock, "TV", PerformanceProfile::Optimized).await;
            assert!(!res.ok, "{}", res.message);
        }
    }
    use crate::engine::{
        ActionMethod, AppEntry, AppListBundle, DeviceType, OptimizeAction, RiskTier,
    };
    use std::sync::Arc;

    /// Device output for a batched shell: sections joined by the sentinel the
    /// device would echo between sub-commands.
    fn batched(sections: &[&str]) -> String {
        sections
            .iter()
            .map(|s| format!("{s}\n{}0\n", crate::adb::batch::BATCH_STATUS))
            .collect::<Vec<_>>()
            .join(&format!("\n{BATCH_SEPARATOR}\n"))
    }

    fn bloat(pkg: &str) -> AppEntry {
        AppEntry {
            package: pkg.into(),
            name: pkg.into(),
            method: ActionMethod::Disable,
            risk: RiskTier::Safe,
            optimize_description: String::new(),
            restore_description: String::new(),
            default_optimize: true,
            default_restore: false,
            play_store: false,
            defunct: false,
            review: false,
            reviewed_at: None,
            sources: Vec::new(),
            device_scope: Vec::new(),
        }
    }

    #[tokio::test]
    async fn prepare_optimize_builds_plan_from_device_state() {
        let bundle = AppListBundle {
            common: vec![bloat("com.example.bloat"), bloat("com.example.gone")],
            shield: vec![],
            googletv: vec![],
        };
        // All three reads ride one batched shell, so the mock answers the
        // sentinel with the concatenated sections (installed / disabled /
        // meminfo) instead of one rule per sub-command — a per-command needle
        // would match the whole compound command.
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[
                "package:com.example.bloat\npackage:com.android.systemui",
                "",
                "",
                "",
            ]),
        );
        let log = mock.shell_log();
        let state = AppState::new(Arc::new(mock), bundle, std::env::temp_dir())
            .with_entitlement(crate::license::Entitlement::Pro);

        let plan =
            prepare_optimize_impl(&state, "serial", DeviceType::Shield, OptimizeMode::Optimize)
                .await
                .expect("plan");

        let installed = plan
            .items
            .iter()
            .find(|i| i.entry.package == "com.example.bloat")
            .expect("installed bloat in plan");
        assert!(
            matches!(installed.action, OptimizeAction::Disable),
            "installed default-optimize app should be actionable"
        );
        let absent = plan
            .items
            .iter()
            .find(|i| i.entry.package == "com.example.gone")
            .expect("absent app in plan");
        assert!(
            matches!(absent.action, OptimizeAction::Skip { .. }),
            "not-installed app should be skipped"
        );

        let calls = log.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "installed + disabled + meminfo must cost one round-trip: {calls:?}"
        );
        assert!(calls[0].contains(BATCH_SEPARATOR));
        assert!(
            !calls[0].contains("&&"),
            "sub-commands must run even if an earlier one fails"
        );
        // Same user as App List and every package action (#233).
        assert!(calls[0].contains("(pm list packages --user 0)"));
        assert!(calls[0].contains("(pm list packages -d --user 0)"));
        assert!(calls[0].contains("(pm list packages -3 --user 0)"));
    }

    fn store_review(pkg: &str) -> AppEntry {
        AppEntry {
            method: ActionMethod::Disable,
            default_optimize: false,
            play_store: true,
            review: true,
            ..bloat(pkg)
        }
    }

    async fn review_plan(third_party_section: &str, third_party_status: i32) -> OptimizePlan {
        let bundle = AppListBundle {
            common: vec![
                store_review("com.store.mine"),
                store_review("com.store.shipped"),
            ],
            shield: vec![],
            googletv: vec![],
        };
        let status = crate::adb::batch::BATCH_STATUS;
        let output = format!(
            "{}\n{BATCH_SEPARATOR}\n{third_party_section}\n{status}{third_party_status}\n",
            batched(&["package:com.store.mine\npackage:com.store.shipped", "", "",])
        );
        let state = AppState::new(
            Arc::new(MockAdb::default().on_shell(BATCH_SEPARATOR, &output)),
            bundle,
            std::env::temp_dir(),
        )
        .with_entitlement(crate::license::Entitlement::Pro);
        prepare_optimize_impl(&state, "serial", DeviceType::Shield, OptimizeMode::Optimize)
            .await
            .expect("plan")
    }

    fn row<'a>(plan: &'a OptimizePlan, pkg: &str) -> &'a crate::engine::OptimizePlanItem {
        plan.items.iter().find(|i| i.entry.package == pkg).unwrap()
    }

    #[tokio::test]
    async fn a_review_row_disables_store_apps_without_verified_installer_provenance() {
        use crate::engine::InstallSource;
        let plan = review_plan("package:com.store.mine", 0).await;
        let mine = row(&plan, "com.store.mine");
        assert_eq!(mine.install_source, InstallSource::User);
        assert_eq!(mine.action, OptimizeAction::Disable);
        let shipped = row(&plan, "com.store.shipped");
        assert_eq!(shipped.install_source, InstallSource::Preinstalled);
        assert_eq!(shipped.action, OptimizeAction::Disable);
    }

    #[tokio::test]
    async fn an_unread_install_source_claims_nothing_and_suggests_disable() {
        use crate::engine::InstallSource;
        for (section, status) in [("", 1), ("", 0)] {
            let plan = review_plan(section, status).await;
            for pkg in ["com.store.mine", "com.store.shipped"] {
                let item = row(&plan, pkg);
                assert_eq!(item.install_source, InstallSource::Unknown, "{pkg}");
                assert_eq!(item.action, OptimizeAction::Disable, "{pkg}");
            }
        }
    }

    #[tokio::test]
    async fn prepare_optimize_errors_when_the_installed_section_is_empty() {
        // A `;`-chained batch exits with the last command's status, so a broken
        // `pm list packages` looks like success with an empty section. Planning
        // against that would silently skip every app.
        let bundle = AppListBundle {
            common: vec![bloat("com.example.bloat")],
            shield: vec![],
            googletv: vec![],
        };
        let mock = MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", "", "", ""]));
        let state = AppState::new(Arc::new(mock), bundle, std::env::temp_dir())
            .with_entitlement(crate::license::Entitlement::Pro);

        let err =
            prepare_optimize_impl(&state, "serial", DeviceType::Shield, OptimizeMode::Optimize)
                .await
                .expect_err("empty package list must be an error, not an empty plan");
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }
}
