//! Snapshot schema + version handling.
//!
//! Honors architectural commitment #5 — `schema_version` lives in every
//! snapshot file; the reader rejects unknown versions with a clear error rather
//! than crashing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::detection::DeviceType;
use super::launcher::{plan_home_safe_disables, LauncherCatalog};
use super::types::{AppEntry, RiskTier};

/// Current snapshot schema version. Bump when the structure changes; the
/// reader will refuse future versions explicitly.
pub const SCHEMA_VERSION: u32 = 4;

/// Setting keys we track in a snapshot — matches v1's `$Script:SnapshotSettingKeys`.
pub fn tracked_setting_keys() -> &'static [(&'static str, &'static str)] {
    &[
        ("global", "window_animation_scale"),
        ("global", "transition_animation_scale"),
        ("global", "animator_duration_scale"),
        ("global", "hdmi_control_enabled"),
        ("global", "hdmi_control_auto_wakeup_enabled"),
        ("global", "hdmi_control_auto_device_off_enabled"),
        ("global", "hdmi_system_audio_control_enabled"),
        ("secure", "match_content_frame_rate"),
        ("secure", "long_press_timeout"),
        ("global", "encoded_surround_output"),
        ("global", "encoded_surround_output_enabled_formats"),
        ("secure", "screensaver_components"),
        ("secure", "screensaver_enabled"),
    ]
}

/// Whether `key` (`"<namespace>.<key>"`) is one a snapshot may write or
/// delete. Anything else in a snapshot file is refused.
pub fn is_tracked_setting(key: &str) -> bool {
    key.split_once('.').is_some_and(|(ns, name)| {
        tracked_setting_keys()
            .iter()
            .any(|(t_ns, t_key)| *t_ns == ns && *t_key == name)
    })
}

/// On-disk snapshot file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp.
    pub saved_at: String,
    /// Optional user-given name for the snapshot (e.g. "before debloat").
    /// Schema v1 snapshots have none; `serde(default)` loads them with `None`.
    #[serde(default)]
    pub label: Option<String>,
    pub device_name: String,
    pub device_serial: String,
    pub device_type: DeviceType,
    pub android_version: String,
    pub disabled_packages: Vec<String>,
    pub current_launcher: Option<String>,
    /// Key format: `"<namespace>.<key>"` (e.g. `"global.window_animation_scale"`).
    /// Values are the raw strings the device returned.
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub absent_settings: Vec<String>,
    /// Catalog apps that weren't installed for user 0 when this was saved —
    /// uninstalled, or never there; the TV can't say which. Only apps a
    /// restore may turn off: Safe or Medium risk, and not ones the catalog
    /// says belong on the TV (`default_restore`). A restore disables any of
    /// these it finds installed and never uninstalls. Schema v4+; older
    /// snapshots load with none.
    #[serde(default)]
    pub absent_packages: Vec<String>,
}

/// Errors that arise from snapshot parsing / application.
#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot schema version {found} is newer than this build supports (max {supported})")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("snapshot is missing required field: {0}")]
    MissingField(&'static str),
    #[error("snapshot JSON is malformed: {0}")]
    Malformed(String),
}

impl Snapshot {
    /// Parse a snapshot from JSON. Rejects unknown future versions and
    /// nonsense low values (0, anything larger than u32). Uses a single
    /// `serde_json::Value` parse + `from_value` rather than parsing the
    /// string twice.
    pub fn from_json(json: &str) -> Result<Self, SnapshotError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| SnapshotError::Malformed(e.to_string()))?;
        let schema = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or(SnapshotError::MissingField("schema_version"))?;
        if schema == 0 || schema > u64::from(SCHEMA_VERSION) {
            return Err(SnapshotError::UnsupportedSchema {
                found: u32::try_from(schema).unwrap_or(u32::MAX),
                supported: SCHEMA_VERSION,
            });
        }
        let mut snapshot: Self =
            serde_json::from_value(value).map_err(|e| SnapshotError::Malformed(e.to_string()))?;
        if schema < 3 {
            // Older captures omitted both absent and unreadable settings. Neither
            // omission authorizes deleting a value on the target device.
            snapshot.absent_settings.clear();
        }
        if schema < 4 {
            // Older captures never recorded which apps were missing, so none
            // may be turned off on their account.
            snapshot.absent_packages.clear();
        }
        snapshot.schema_version = SCHEMA_VERSION;
        if snapshot
            .absent_packages
            .iter()
            .any(|pkg| snapshot.disabled_packages.contains(pkg))
        {
            return Err(SnapshotError::Malformed(
                "package cannot be both disabled and absent".into(),
            ));
        }
        if snapshot
            .absent_settings
            .iter()
            .any(|key| snapshot.settings.contains_key(key))
        {
            return Err(SnapshotError::Malformed(
                "setting cannot be both present and absent".into(),
            ));
        }
        Ok(snapshot)
    }

    pub fn to_json(&self) -> Result<String, SnapshotError> {
        serde_json::to_string_pretty(self).map_err(|e| SnapshotError::Malformed(e.to_string()))
    }
}

/// A catalog app a restore may turn off because the snapshot didn't have it:
/// Safe or Medium risk, and not one the catalog says belongs on the TV.
fn restore_may_disable_absent(entry: &AppEntry) -> bool {
    !entry.default_restore && matches!(entry.risk, RiskTier::Safe | RiskTier::Medium)
}

/// The catalog apps a snapshot records as absent: not installed for user 0,
/// and ones a restore may turn off. `installed` must come from a successful,
/// non-empty read.
pub fn absent_catalog_packages(catalog: &[AppEntry], installed: &[String]) -> Vec<String> {
    let installed: std::collections::HashSet<&str> = installed.iter().map(String::as_str).collect();
    let mut absent: Vec<String> = catalog
        .iter()
        .filter(|entry| {
            restore_may_disable_absent(entry) && !installed.contains(entry.package.as_str())
        })
        .map(|entry| entry.package.clone())
        .collect();
    absent.sort();
    absent.dedup();
    absent
}

/// What the engine plans to do when applying a snapshot — computed before any
/// ADB calls are made. The host layer executes these against the ADB driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotApplyPlan {
    /// Packages to be `pm disable-user`d (currently installed and enabled).
    /// Includes the snapshot's absent apps that are installed here.
    pub packages_to_disable: Vec<String>,
    /// Which of `packages_to_disable` the snapshot recorded as not installed,
    /// so the preview can say why they go off. A restore disables these; it
    /// never uninstalls.
    #[serde(default)]
    pub packages_absent_in_snapshot: Vec<String>,
    /// Packages already in the disabled state — no-op, but counted.
    pub packages_already_disabled: Vec<String>,
    /// Packages on the snapshot list but not present on the target device.
    pub packages_not_installed: Vec<String>,
    /// The launcher to set as default. `None` when the snapshot's launcher is
    /// already the device's Home app, when the snapshot recorded none, or when
    /// it isn't installed here — so a fresh snapshot previews no launcher
    /// change and applying it never re-runs the launcher switch.
    pub launcher_to_set: Option<String>,
    /// The device's Home app when the plan was computed. `None` means the
    /// device couldn't say, not that there is no Home app.
    #[serde(default)]
    pub current_launcher: Option<String>,
    /// The snapshot's launcher when it isn't installed on this device, so the
    /// preview can say why it's skipped instead of silently dropping it.
    #[serde(default)]
    pub launcher_not_installed: Option<String>,
    /// The snapshot's launcher when it is installed but disabled here. A
    /// restore never re-enables anything, so it is skipped and named, like
    /// `launcher_not_installed`.
    #[serde(default)]
    pub launcher_disabled: Option<String>,
    /// Packages the snapshot lists as disabled that stay on so the TV keeps a
    /// Home screen. Not in `packages_to_disable`.
    #[serde(default)]
    pub packages_kept: Vec<KeptPackage>,
    /// Settings whose current device value differs from the snapshot —
    /// these will be written. Same key format as `Snapshot::settings`.
    pub settings_to_write: BTreeMap<String, String>,
    pub settings_to_delete: Vec<String>,
    /// Settings already at the snapshot's value on the device — no-op, counted
    /// so the preview doesn't overstate the work.
    pub settings_already_set: Vec<String>,
    /// Settings the snapshot names that this app does not track. A snapshot
    /// file can be edited or shared, so these are never written or deleted;
    /// they are listed so a preview can say what was ignored.
    #[serde(default)]
    pub settings_refused: Vec<String>,
    /// The device's current value for every setting the snapshot mentions,
    /// so the preview's "Now" column shows what's there. A key missing here
    /// is unset on the device.
    #[serde(default)]
    pub current_values: BTreeMap<String, String>,
    /// Set when the snapshot's device type doesn't match the target's.
    pub cross_device_warning: Option<String>,
}

/// A package a restore leaves enabled, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptPackage {
    pub package: String,
    /// Short and display-ready, e.g. "only Home app".
    pub reason: String,
}

/// Inputs the engine needs to compute an apply plan, expressed as facts
/// about the device's *current* state (no I/O — caller fetches and passes in).
#[derive(Debug, Clone)]
pub struct ApplyPlanInputs<'a> {
    pub target_device_type: DeviceType,
    pub currently_disabled: &'a [String],
    pub currently_installed: &'a [String],
    /// Successfully read current tracked values. Missing keys are absent;
    /// callers must not substitute an empty map for a failed read.
    pub current_settings: &'a BTreeMap<String, String>,
    /// The package HOME resolves to now, or `None` when the device couldn't
    /// say. Unknown is treated as "may differ", so the launcher is still set.
    pub current_launcher: Option<&'a str>,
    /// The packages the HOME query listed. `None` or empty when it couldn't
    /// be read; every package that may be a Home app is then kept.
    pub home_handlers: Option<&'a [String]>,
    /// The launcher catalog, which says which packages may be Home apps.
    pub launchers: &'a LauncherCatalog,
    /// The target's app catalog. A snapshot's absent app is turned off only
    /// while this still lists it as one a restore may turn off.
    pub catalog: &'a [AppEntry],
}

/// Compute the plan for applying `snap` to a device in `inputs`' state.
/// Per commitment #1: this function is pure; the host layer executes the plan.
pub fn compute_apply_plan(snap: &Snapshot, inputs: &ApplyPlanInputs<'_>) -> SnapshotApplyPlan {
    let disabled_set: std::collections::HashSet<&str> = inputs
        .currently_disabled
        .iter()
        .map(String::as_str)
        .collect();
    let installed_set: std::collections::HashSet<&str> = inputs
        .currently_installed
        .iter()
        .map(String::as_str)
        .collect();

    let mut to_disable = Vec::new();
    let mut already_disabled = Vec::new();
    let mut not_installed = Vec::new();

    let mut seen = std::collections::HashSet::new();
    // A hand-edited file may list a package twice; plan it once.
    for pkg in snap
        .disabled_packages
        .iter()
        .filter(|p| seen.insert(p.as_str()))
    {
        let s = pkg.as_str();
        if !installed_set.contains(s) {
            not_installed.push(pkg.clone());
        } else if disabled_set.contains(s) {
            already_disabled.push(pkg.clone());
        } else {
            to_disable.push(pkg.clone());
        }
    }
    // An app the snapshot didn't have that this TV runs goes off, the
    // reversible way. One already off or not here needs nothing. The target's
    // catalog is checked again: the snapshot may be older than the catalog,
    // from another device type, or edited by hand.
    let mut absent_to_disable: Vec<String> = Vec::new();
    for pkg in &snap.absent_packages {
        let still_allowed = inputs
            .catalog
            .iter()
            .any(|entry| &entry.package == pkg && restore_may_disable_absent(entry));
        if still_allowed
            && installed_set.contains(pkg.as_str())
            && !disabled_set.contains(pkg.as_str())
            && !to_disable.contains(pkg)
            && !absent_to_disable.contains(pkg)
        {
            absent_to_disable.push(pkg.clone());
        }
    }
    to_disable.extend(absent_to_disable.iter().cloned());

    let cross_device_warning = if snap.device_type != inputs.target_device_type {
        Some(format!(
            "Snapshot was taken from a {} device; current device is detected as {}.",
            snap.device_type.label(),
            inputs.target_device_type.label()
        ))
    } else {
        None
    };

    // Only write settings whose current value differs from the snapshot.
    let mut settings_to_write = BTreeMap::new();
    let mut settings_already_set = Vec::new();
    let mut settings_refused = Vec::new();
    for (key, value) in &snap.settings {
        if !is_tracked_setting(key) {
            settings_refused.push(key.clone());
        } else if inputs.current_settings.get(key) == Some(value) {
            settings_already_set.push(key.clone());
        } else {
            settings_to_write.insert(key.clone(), value.clone());
        }
    }

    let mut settings_to_delete = Vec::new();
    for key in &snap.absent_settings {
        if !is_tracked_setting(key) {
            if !settings_refused.contains(key) {
                settings_refused.push(key.clone());
            }
        } else if inputs.current_settings.contains_key(key) {
            settings_to_delete.push(key.clone());
        } else {
            settings_already_set.push(key.clone());
        }
    }

    let current_values = snap
        .settings
        .keys()
        .chain(snap.absent_settings.iter())
        .filter(|key| is_tracked_setting(key))
        .filter_map(|key| {
            inputs
                .current_settings
                .get(key)
                .map(|value| (key.clone(), value.clone()))
        })
        .collect();

    let mut launcher_to_set = None;
    let mut launcher_not_installed = None;
    let mut launcher_disabled = None;
    if let Some(wanted) = snap.current_launcher.as_deref() {
        if !installed_set.contains(wanted) {
            launcher_not_installed = Some(wanted.to_string());
        } else if disabled_set.contains(wanted) {
            launcher_disabled = Some(wanted.to_string());
        } else if inputs.current_launcher != Some(wanted) {
            launcher_to_set = Some(wanted.to_string());
        }
    }

    let (to_disable, kept) = plan_home_safe_disables(
        &to_disable,
        inputs.home_handlers,
        inputs.current_launcher,
        inputs.launchers,
    );
    let packages_absent_in_snapshot = to_disable
        .iter()
        .filter(|pkg| absent_to_disable.contains(pkg))
        .cloned()
        .collect();
    let packages_kept = kept
        .into_iter()
        .map(|(package, reason)| KeptPackage {
            package,
            reason: reason.label().to_string(),
        })
        .collect();

    SnapshotApplyPlan {
        packages_to_disable: to_disable,
        packages_absent_in_snapshot,
        packages_already_disabled: already_disabled,
        packages_not_installed: not_installed,
        launcher_to_set,
        current_launcher: inputs.current_launcher.map(str::to_string),
        launcher_not_installed,
        launcher_disabled,
        packages_kept,
        settings_to_write,
        settings_to_delete,
        settings_already_set,
        settings_refused,
        current_values,
        cross_device_warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn sample_snapshot() -> Snapshot {
        let mut settings = BTreeMap::new();
        settings.insert(
            "global.window_animation_scale".to_string(),
            "0.5".to_string(),
        );
        Snapshot {
            schema_version: SCHEMA_VERSION,
            saved_at: "2026-05-27T12:00:00Z".to_string(),
            label: None,
            device_name: "Living Room TV".to_string(),
            device_serial: "192.168.42.71:5555".to_string(),
            device_type: DeviceType::Shield,
            android_version: "11".to_string(),
            disabled_packages: vec!["com.foo".into(), "com.bar".into(), "com.missing".into()],
            current_launcher: Some("com.spocky.projengmenu".to_string()),
            settings,
            absent_settings: Vec::new(),
            absent_packages: vec!["com.gone".into()],
        }
    }

    #[test]
    fn roundtrip_json() {
        let snap = sample_snapshot();
        let json = snap.to_json().unwrap();
        let parsed = Snapshot::from_json(&json).unwrap();
        assert_eq!(parsed.device_name, snap.device_name);
        assert_eq!(parsed.disabled_packages, snap.disabled_packages);
        assert_eq!(parsed.absent_packages, ["com.gone"]);
        assert_eq!(parsed.schema_version, 4);
    }

    #[test]
    fn older_snapshots_load_with_no_absent_packages() {
        for version in [1, 2, 3] {
            let mut snap = sample_snapshot();
            snap.schema_version = version;
            // Even a pre-v4 file that somehow carries the field records
            // nothing on its account.
            let with_field = Snapshot::from_json(&snap.to_json().unwrap()).unwrap();
            assert!(with_field.absent_packages.is_empty(), "v{version}");
            assert_eq!(with_field.schema_version, SCHEMA_VERSION);
            let mut json = serde_json::to_value(&snap).unwrap();
            json.as_object_mut().unwrap().remove("absent_packages");
            let migrated = Snapshot::from_json(&json.to_string()).unwrap();
            assert!(migrated.absent_packages.is_empty(), "v{version}");
            assert_eq!(migrated.disabled_packages, snap.disabled_packages);
        }
    }

    fn entry(pkg: &str, risk: RiskTier, default_restore: bool) -> AppEntry {
        AppEntry {
            package: pkg.into(),
            name: pkg.into(),
            method: super::super::types::ActionMethod::Disable,
            risk,
            optimize_description: String::new(),
            restore_description: String::new(),
            default_optimize: false,
            default_restore,
            play_store: true,
            defunct: false,
            review: true,
            reviewed_at: None,
            sources: Vec::new(),
            device_scope: Vec::new(),
        }
    }

    #[test]
    fn only_missing_catalog_apps_a_restore_may_turn_off_are_recorded() {
        let catalog = [
            entry("com.safe.gone", RiskTier::Safe, false),
            entry("com.medium.gone", RiskTier::Medium, false),
            entry("com.safe.here", RiskTier::Safe, false),
            entry("com.high.gone", RiskTier::High, false),
            entry("com.advanced.gone", RiskTier::Advanced, false),
            entry("com.belongs.gone", RiskTier::Safe, true),
            entry("com.safe.gone", RiskTier::Safe, false),
        ];
        let installed = ["com.safe.here".to_string(), "com.not.catalog".to_string()];
        assert_eq!(
            absent_catalog_packages(&catalog, &installed),
            ["com.medium.gone", "com.safe.gone"]
        );
    }

    #[test]
    fn rejects_a_package_both_disabled_and_absent() {
        let mut snap = sample_snapshot();
        snap.absent_packages.push("com.foo".into());
        assert!(matches!(
            Snapshot::from_json(&snap.to_json().unwrap()),
            Err(SnapshotError::Malformed(_))
        ));
    }

    #[test]
    fn absent_apps_installed_here_are_disabled_and_named() {
        let mut snap = sample_snapshot();
        snap.absent_packages = vec![
            "com.gone.here".into(),
            "com.gone.off".into(),
            "com.gone.running".into(),
            "com.gone.running".into(),
            "com.now.high".into(),
            "com.now.belongs".into(),
            "com.not.catalog".into(),
        ];
        let installed: Vec<String> = [
            "com.foo",
            "com.gone.off",
            "com.gone.running",
            "com.now.high",
            "com.now.belongs",
            "com.not.catalog",
        ]
        .map(String::from)
        .to_vec();
        let disabled = vec!["com.gone.off".into()];
        let no_settings = BTreeMap::new();
        // Today's catalog: two entries a restore may no longer turn off.
        let catalog = [
            entry("com.gone.here", RiskTier::Safe, false),
            entry("com.gone.off", RiskTier::Safe, false),
            entry("com.gone.running", RiskTier::Medium, false),
            entry("com.now.high", RiskTier::High, false),
            entry("com.now.belongs", RiskTier::Safe, true),
        ];
        let plan = compute_apply_plan(
            &snap,
            &ApplyPlanInputs {
                target_device_type: DeviceType::Shield,
                currently_disabled: &disabled,
                currently_installed: &installed,
                current_settings: &no_settings,
                current_launcher: None,
                home_handlers: None,
                launchers: &LauncherCatalog::default(),
                catalog: &catalog,
            },
        );
        // Once, even though the file lists it twice.
        assert_eq!(plan.packages_to_disable, ["com.foo", "com.gone.running"]);
        assert_eq!(plan.packages_absent_in_snapshot, ["com.gone.running"]);
        // Already off, or not on this TV either: nothing to do, nothing listed.
        assert_eq!(plan.packages_already_disabled, Vec::<String>::new());
        assert_eq!(plan.packages_not_installed, ["com.bar", "com.missing"]);
    }

    #[test]
    fn v1_snapshot_loads_with_no_label() {
        // A pre-label (schema v1) file must still parse — label defaults to None.
        let payload = r#"{
            "schema_version": 1,
            "saved_at": "2026-05-27T12:00:00Z",
            "device_name": "Old Shield",
            "device_serial": "x",
            "device_type": "shield",
            "android_version": "11",
            "disabled_packages": [],
            "current_launcher": null,
            "settings": {}
        }"#;
        let snap = Snapshot::from_json(payload).unwrap();
        assert_eq!(snap.label, None);
        assert_eq!(snap.device_name, "Old Shield");
    }

    #[test]
    fn label_roundtrips() {
        let mut snap = sample_snapshot();
        snap.label = Some("before debloat".to_string());
        let parsed = Snapshot::from_json(&snap.to_json().unwrap()).unwrap();
        assert_eq!(parsed.label.as_deref(), Some("before debloat"));
    }

    #[test]
    fn absent_and_empty_settings_roundtrip_and_plan_separately() {
        let mut snap = sample_snapshot();
        let empty_key = "global.encoded_surround_output_enabled_formats".to_string();
        let absent_key = "global.encoded_surround_output".to_string();
        snap.settings.insert(empty_key.clone(), String::new());
        snap.absent_settings.push(absent_key.clone());
        let parsed = Snapshot::from_json(&snap.to_json().unwrap()).unwrap();
        assert_eq!(parsed.settings.get(&empty_key), Some(&String::new()));
        assert_eq!(parsed.absent_settings, std::slice::from_ref(&absent_key));
        let current = BTreeMap::from([
            (absent_key.clone(), "3".into()),
            ("global.unrelated".into(), "keep".into()),
        ]);
        let plan = compute_apply_plan(
            &parsed,
            &ApplyPlanInputs {
                target_device_type: DeviceType::Shield,
                currently_disabled: &[],
                currently_installed: &[],
                current_settings: &current,
                current_launcher: None,
                home_handlers: None,
                launchers: &LauncherCatalog::default(),
                catalog: &[],
            },
        );
        assert_eq!(plan.settings_to_delete, [absent_key]);
        assert_eq!(plan.settings_to_write.get(&empty_key), Some(&String::new()));
        assert!(!plan.settings_to_write.contains_key("global.unrelated"));
        let absent = BTreeMap::new();
        let unchanged = compute_apply_plan(
            &parsed,
            &ApplyPlanInputs {
                target_device_type: DeviceType::Shield,
                currently_disabled: &[],
                currently_installed: &[],
                current_settings: &absent,
                current_launcher: None,
                home_handlers: None,
                launchers: &LauncherCatalog::default(),
                catalog: &[],
            },
        );
        assert!(unchanged.settings_to_delete.is_empty());
        assert!(unchanged
            .settings_already_set
            .contains(&"global.encoded_surround_output".into()));
    }

    #[test]
    fn legacy_omissions_never_become_deletions() {
        for version in [1, 2] {
            let mut snap = sample_snapshot();
            snap.schema_version = version;
            let mut json = serde_json::to_value(snap).unwrap();
            json.as_object_mut().unwrap().remove("absent_settings");
            let migrated = Snapshot::from_json(&json.to_string()).unwrap();
            assert_eq!(migrated.schema_version, SCHEMA_VERSION);
            assert!(migrated.absent_settings.is_empty());
            let current = BTreeMap::from([("global.encoded_surround_output".into(), "3".into())]);
            let plan = compute_apply_plan(
                &migrated,
                &ApplyPlanInputs {
                    target_device_type: DeviceType::Shield,
                    currently_disabled: &[],
                    currently_installed: &[],
                    current_settings: &current,
                    current_launcher: None,
                    home_handlers: None,
                    launchers: &LauncherCatalog::default(),
                    catalog: &[],
                },
            );
            assert!(plan.settings_to_delete.is_empty());
        }
    }

    #[test]
    fn an_untracked_setting_in_a_snapshot_is_refused_never_written_or_deleted() {
        for absent in [false, true] {
            let mut snap = sample_snapshot();
            if absent {
                snap.absent_settings.push("global.adb_enabled".into());
            } else {
                snap.settings
                    .insert("global.adb_enabled".into(), "0".into());
            }
            let parsed = Snapshot::from_json(&snap.to_json().unwrap()).unwrap();
            let current = BTreeMap::from([("global.adb_enabled".into(), "1".into())]);
            let plan = compute_apply_plan(
                &parsed,
                &ApplyPlanInputs {
                    target_device_type: DeviceType::Shield,
                    currently_disabled: &[],
                    currently_installed: &[],
                    current_settings: &current,
                    current_launcher: None,
                    home_handlers: None,
                    launchers: &LauncherCatalog::default(),
                    catalog: &[],
                },
            );
            assert!(!plan.settings_to_write.contains_key("global.adb_enabled"));
            assert!(!plan
                .settings_to_delete
                .contains(&"global.adb_enabled".into()));
            assert!(!plan
                .settings_already_set
                .contains(&"global.adb_enabled".into()));
            assert!(!plan.current_values.contains_key("global.adb_enabled"));
            assert_eq!(plan.settings_refused, ["global.adb_enabled"]);
            // The tracked setting in the same file still applies.
            assert!(plan
                .settings_to_write
                .contains_key("global.window_animation_scale"));
        }
        assert!(is_tracked_setting("secure.screensaver_enabled"));
        assert!(!is_tracked_setting("system.screensaver_enabled"));
        assert!(!is_tracked_setting("screensaver_enabled"));
    }

    #[test]
    fn a_package_listed_twice_is_planned_once() {
        let mut snap = sample_snapshot();
        snap.disabled_packages = vec![
            "com.foo".into(),
            "com.foo".into(),
            "com.bar".into(),
            "com.bar".into(),
        ];
        let installed = vec!["com.foo".to_string(), "com.bar".to_string()];
        let disabled = vec!["com.bar".to_string()];
        let plan = compute_apply_plan(
            &snap,
            &ApplyPlanInputs {
                target_device_type: DeviceType::Shield,
                currently_disabled: &disabled,
                currently_installed: &installed,
                current_settings: &BTreeMap::new(),
                current_launcher: None,
                home_handlers: None,
                launchers: &LauncherCatalog::default(),
                catalog: &[],
            },
        );
        assert_eq!(plan.packages_to_disable, ["com.foo"]);
        assert_eq!(plan.packages_already_disabled, ["com.bar"]);
    }

    #[test]
    fn rejects_conflicting_present_and_absent_values() {
        let mut snap = sample_snapshot();
        snap.absent_settings
            .push("global.window_animation_scale".into());
        assert!(Snapshot::from_json(&snap.to_json().unwrap()).is_err());
    }

    #[test]
    fn rejects_zero_schema_version() {
        let payload = r#"{
            "schema_version": 0,
            "saved_at": "2026-05-27T12:00:00Z",
            "device_name": "x",
            "device_serial": "x",
            "device_type": "shield",
            "android_version": "11",
            "disabled_packages": [],
            "current_launcher": null,
            "settings": {}
        }"#;
        let err = Snapshot::from_json(payload).unwrap_err();
        assert!(matches!(
            err,
            SnapshotError::UnsupportedSchema {
                found: 0,
                supported: 4
            }
        ));
    }

    #[test]
    fn rejects_future_schema() {
        let payload = r#"{
            "schema_version": 999,
            "saved_at": "2026-05-27T12:00:00Z",
            "device_name": "x",
            "device_serial": "x",
            "device_type": "shield",
            "android_version": "11",
            "disabled_packages": [],
            "current_launcher": null,
            "settings": {}
        }"#;
        let err = Snapshot::from_json(payload).unwrap_err();
        match err {
            SnapshotError::UnsupportedSchema {
                found: 999,
                supported: 4,
            } => {}
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_schema_version() {
        let payload = r#"{
            "saved_at": "2026-05-27T12:00:00Z",
            "device_name": "x",
            "disabled_packages": []
        }"#;
        let err = Snapshot::from_json(payload).unwrap_err();
        assert!(matches!(err, SnapshotError::MissingField("schema_version")));
    }

    #[test]
    fn rejects_malformed_json() {
        let err = Snapshot::from_json("not json").unwrap_err();
        assert!(matches!(err, SnapshotError::Malformed(_)));
    }

    #[test]
    fn apply_plan_categorizes_packages() {
        let snap = sample_snapshot();
        let installed = vec!["com.foo".into(), "com.bar".into()];
        let disabled = vec!["com.bar".into()];
        let no_settings = BTreeMap::new();
        let inputs = ApplyPlanInputs {
            target_device_type: DeviceType::Shield,
            currently_disabled: &disabled,
            currently_installed: &installed,
            current_settings: &no_settings,
            current_launcher: None,
            home_handlers: None,
            launchers: &LauncherCatalog::default(),
            catalog: &[],
        };
        let plan = compute_apply_plan(&snap, &inputs);
        assert_eq!(plan.packages_to_disable, vec!["com.foo"]);
        assert_eq!(plan.packages_already_disabled, vec!["com.bar"]);
        assert_eq!(plan.packages_not_installed, vec!["com.missing"]);
        assert!(plan.cross_device_warning.is_none());
        // No current settings known → everything is a write.
        assert_eq!(plan.settings_to_write.len(), 1);
        assert!(plan.settings_already_set.is_empty());
    }

    #[test]
    fn apply_plan_skips_settings_already_at_target() {
        let snap = sample_snapshot(); // has global.window_animation_scale = "0.5"
        let installed: Vec<String> = vec![];
        let disabled: Vec<String> = vec![];
        let mk = |val: &str| {
            let mut m = BTreeMap::new();
            m.insert("global.window_animation_scale".to_string(), val.to_string());
            m
        };

        let matched = mk("0.5");
        let plan = compute_apply_plan(
            &snap,
            &ApplyPlanInputs {
                target_device_type: DeviceType::Shield,
                currently_disabled: &disabled,
                currently_installed: &installed,
                current_settings: &matched,
                current_launcher: None,
                home_handlers: None,
                launchers: &LauncherCatalog::default(),
                catalog: &[],
            },
        );
        assert!(plan.settings_to_write.is_empty());
        assert_eq!(
            plan.settings_already_set,
            vec!["global.window_animation_scale"]
        );

        // A different current value → it IS written.
        let differs = mk("1.0");
        let plan = compute_apply_plan(
            &snap,
            &ApplyPlanInputs {
                target_device_type: DeviceType::Shield,
                currently_disabled: &disabled,
                currently_installed: &installed,
                current_settings: &differs,
                current_launcher: None,
                home_handlers: None,
                launchers: &LauncherCatalog::default(),
                catalog: &[],
            },
        );
        assert_eq!(plan.settings_to_write.len(), 1);
        assert!(plan.settings_already_set.is_empty());
    }

    #[test]
    fn apply_plan_warns_on_cross_device() {
        let snap = sample_snapshot();
        let installed: Vec<String> = vec![];
        let disabled: Vec<String> = vec![];
        let no_settings = BTreeMap::new();
        let inputs = ApplyPlanInputs {
            target_device_type: DeviceType::GoogleTv,
            currently_disabled: &disabled,
            currently_installed: &installed,
            current_settings: &no_settings,
            current_launcher: None,
            home_handlers: None,
            launchers: &LauncherCatalog::default(),
            catalog: &[],
        };
        let plan = compute_apply_plan(&snap, &inputs);
        assert!(plan.cross_device_warning.is_some());
        assert!(plan.cross_device_warning.unwrap().contains("Nvidia Shield"));
    }

    static NO_LAUNCHERS: LauncherCatalog = LauncherCatalog {
        custom: Vec::new(),
        stock: Vec::new(),
        home_handler_names: BTreeMap::new(),
        transient_home_holders: Vec::new(),
    };

    fn launcher_inputs<'a>(
        installed: &'a [String],
        settings: &'a BTreeMap<String, String>,
        current_launcher: Option<&'a str>,
    ) -> ApplyPlanInputs<'a> {
        ApplyPlanInputs {
            target_device_type: DeviceType::Shield,
            currently_disabled: &[],
            currently_installed: installed,
            current_settings: settings,
            current_launcher,
            home_handlers: None,
            launchers: &NO_LAUNCHERS,
            catalog: &[],
        }
    }

    #[test]
    fn launcher_already_home_is_not_a_change() {
        // A snapshot taken a moment ago records the launcher that is Home now.
        // Previewing it must not list the launcher as a change, and applying it
        // must not re-run the launcher switch.
        let snap = sample_snapshot();
        let installed = vec!["com.spocky.projengmenu".to_string()];
        let settings = BTreeMap::new();
        let plan = compute_apply_plan(
            &snap,
            &launcher_inputs(&installed, &settings, Some("com.spocky.projengmenu")),
        );
        assert_eq!(plan.launcher_to_set, None);
        assert_eq!(
            plan.current_launcher.as_deref(),
            Some("com.spocky.projengmenu")
        );
        assert_eq!(plan.launcher_not_installed, None);
    }

    #[test]
    fn launcher_that_differs_or_is_unknown_is_set() {
        let snap = sample_snapshot();
        let installed = vec!["com.spocky.projengmenu".to_string()];
        let settings = BTreeMap::new();
        let differs = compute_apply_plan(
            &snap,
            &launcher_inputs(&installed, &settings, Some("com.google.android.tvlauncher")),
        );
        assert_eq!(
            differs.launcher_to_set.as_deref(),
            Some("com.spocky.projengmenu")
        );
        // Unknown is not "the same": the switch still runs, and verifies.
        let unknown = compute_apply_plan(&snap, &launcher_inputs(&installed, &settings, None));
        assert_eq!(
            unknown.launcher_to_set.as_deref(),
            Some("com.spocky.projengmenu")
        );
        assert_eq!(unknown.current_launcher, None);
    }

    #[test]
    fn launcher_not_installed_is_skipped_and_named() {
        let snap = sample_snapshot();
        let settings = BTreeMap::new();
        let plan = compute_apply_plan(
            &snap,
            &launcher_inputs(&[], &settings, Some("com.google.android.tvlauncher")),
        );
        assert_eq!(plan.launcher_to_set, None);
        assert_eq!(
            plan.launcher_not_installed.as_deref(),
            Some("com.spocky.projengmenu")
        );
    }

    #[test]
    fn a_disabled_launcher_is_skipped_and_named_never_re_enabled() {
        // A restore disables and writes settings; it never re-enables, so a
        // disabled snapshot launcher is skipped the way a missing one is.
        let snap = sample_snapshot();
        let installed = vec!["com.spocky.projengmenu".to_string()];
        let disabled = installed.clone();
        let settings = BTreeMap::new();
        let plan = compute_apply_plan(
            &snap,
            &ApplyPlanInputs {
                currently_disabled: &disabled,
                ..launcher_inputs(&installed, &settings, Some("com.google.android.tvlauncher"))
            },
        );
        assert_eq!(plan.launcher_to_set, None);
        assert_eq!(plan.launcher_not_installed, None);
        assert_eq!(
            plan.launcher_disabled.as_deref(),
            Some("com.spocky.projengmenu")
        );
    }

    #[test]
    fn the_only_home_app_is_kept_out_of_the_disable_list() {
        // Recorded with Projectivy as Home and stock disabled, applied where
        // Projectivy isn't installed: disabling stock would leave no Home app.
        let stock = "com.google.android.tvlauncher";
        let mut snap = sample_snapshot();
        snap.disabled_packages = vec![stock.into(), "com.foo".into()];
        let installed = vec![stock.to_string(), "com.foo".to_string()];
        let settings = BTreeMap::new();
        let launchers = LauncherCatalog {
            stock: vec![super::super::launcher::LauncherEntry {
                name: "Stock".into(),
                package: stock.into(),
                source_url: None,
                disable_with: Vec::new(),
            }],
            ..NO_LAUNCHERS.clone()
        };
        let handlers = [stock.to_string(), "com.android.tv.settings".to_string()];
        for home_handlers in [Some(&handlers[..]), None] {
            let plan = compute_apply_plan(
                &snap,
                &ApplyPlanInputs {
                    home_handlers,
                    launchers: &launchers,
                    catalog: &[],
                    ..launcher_inputs(&installed, &settings, Some(stock))
                },
            );
            assert_eq!(
                plan.launcher_not_installed.as_deref(),
                Some("com.spocky.projengmenu")
            );
            assert_eq!(plan.packages_to_disable, ["com.foo"]);
            assert_eq!(plan.packages_kept.len(), 1);
            assert_eq!(plan.packages_kept[0].package, stock);
        }
    }

    #[test]
    fn current_values_cover_every_setting_the_snapshot_mentions() {
        // The preview's "Now" column: changed keys and unchanged keys both
        // carry the device's value, and a key unset on the device is absent
        // rather than invented.
        let mut snap = sample_snapshot();
        snap.settings
            .insert("secure.long_press_timeout".into(), "400".into());
        snap.absent_settings
            .push("global.encoded_surround_output".into());
        let settings = BTreeMap::from([
            (
                "global.window_animation_scale".to_string(),
                "1.0".to_string(),
            ),
            ("secure.long_press_timeout".to_string(), "400".to_string()),
            (
                "global.encoded_surround_output".to_string(),
                "3".to_string(),
            ),
            ("global.unrelated".to_string(), "x".to_string()),
        ]);
        let plan = compute_apply_plan(&snap, &launcher_inputs(&[], &settings, None));
        assert_eq!(
            plan.current_values,
            BTreeMap::from([
                (
                    "global.encoded_surround_output".to_string(),
                    "3".to_string()
                ),
                (
                    "global.window_animation_scale".to_string(),
                    "1.0".to_string()
                ),
                ("secure.long_press_timeout".to_string(), "400".to_string()),
            ])
        );
        assert!(plan
            .settings_already_set
            .contains(&"secure.long_press_timeout".to_string()));
        assert!(plan
            .settings_to_write
            .contains_key("global.window_animation_scale"));
    }
}
