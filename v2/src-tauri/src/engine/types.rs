//! Core types shared across the engine and the host layer.

use serde::{Deserialize, Serialize};

/// Connection type for a device — network (ADB-over-TCP) or USB (cable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionType {
    Network,
    Usb,
}

/// State a device can be in per `adb devices` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    /// Authorized and ready to accept commands.
    Device,
    /// Awaiting user acceptance of the "Allow USB debugging" prompt.
    Unauthorized,
    /// Listed by ADB but not reachable.
    Offline,
}

impl DeviceStatus {
    pub fn from_adb_str(s: &str) -> Option<Self> {
        match s {
            "device" => Some(Self::Device),
            "unauthorized" => Some(Self::Unauthorized),
            "offline" => Some(Self::Offline),
            _ => None,
        }
    }
}

/// Properties harvested from a device via `getprop` and `settings get`.
/// Used by `detect_device_type` and shown in the Profile view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceProperties {
    /// User-set device name from `settings get global device_name`.
    pub friendly_name: Option<String>,
    /// `getprop ro.product.brand`.
    pub brand: String,
    /// `getprop ro.product.model`.
    pub model: String,
    /// `getprop ro.product.device` (codename — e.g. mdarcy, sif, foster).
    pub device_codename: String,
    /// `getprop ro.product.manufacturer`.
    pub manufacturer: String,
    /// `getprop ro.build.version.release` (e.g. "11", "12").
    pub android_release: String,
    /// `getprop ro.build.version.sdk` (numeric API level).
    pub sdk_level: String,
    /// `getprop ro.build.id`.
    pub build_id: String,
    /// `getprop ro.board.platform`.
    pub board_platform: String,
}

/// A connected device — what the device list shows and what every action targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// 1-based menu index (matches the v1 numbered-shortcut layer).
    pub id: u32,
    /// ADB serial — `IP:port` for network, OEM serial for USB.
    pub serial: String,
    /// User-visible name. Custom from `device_name` setting if set, else brand-based.
    pub name: String,
    /// Friendly model string (e.g. "Shield TV Pro (2019)").
    pub model: String,
    /// Detected device type.
    pub device_type: super::detection::DeviceType,
    /// Current ADB state.
    pub status: DeviceStatus,
    /// USB vs Network.
    pub connection: ConnectionType,
    /// Properties from getprop, if status is `Device`. None otherwise.
    pub properties: Option<DeviceProperties>,
}

/// What action the wizard should take against an app — disable vs uninstall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionMethod {
    /// `pm disable-user --user 0 <pkg>` — reversible.
    Disable,
    /// `pm uninstall --user 0 <pkg>` — semi-reversible (Play Store / install-existing).
    Uninstall,
}

/// Risk tier surfaced in the UI — controls coloring and confirmation defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    Safe,
    Medium,
    High,
    Advanced,
}

impl RiskTier {
    /// Parse a risk-tier label permissively. Accepts v1's strings
    /// ("Safe" / "Medium" / "High Risk" / "Advanced") case-insensitively.
    /// Defaults to `Medium` for unrecognized strings — safer than `Safe`.
    pub fn parse_label(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "safe" => Self::Safe,
            "medium" => Self::Medium,
            "advanced" => Self::Advanced,
            "high" | "high risk" => Self::High,
            _ => Self::Medium,
        }
    }
}

/// One entry in the bloat list — direct port of v1's app schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    /// Android package id, e.g. `com.netflix.ninja`.
    pub package: String,
    /// User-visible display name.
    pub name: String,
    /// Disable vs uninstall.
    pub method: ActionMethod,
    /// Risk tier.
    pub risk: RiskTier,
    /// One-line description shown when Optimize mode prompts.
    pub optimize_description: String,
    /// One-line description shown when Restore mode prompts.
    pub restore_description: String,
    /// Pre-select YES in the Optimize prompt's default.
    #[serde(default)]
    pub default_optimize: bool,
    /// Pre-select YES in the Restore prompt's default.
    #[serde(default)]
    pub default_restore: bool,
    /// Whether this package has a real Google Play listing at this exact id
    /// (audited per-package). Drives whether the UI shows a "Play Store" button
    /// — system components and defunct apps have no listing, so the button
    /// would 404. Defaults to false for safety if a list omits it.
    #[serde(default)]
    pub play_store: bool,
    /// The service/app is discontinued (Stadia, Quibi, …). Defunct apps are safe
    /// to uninstall even without a Play Store listing — there's nothing to get
    /// back. Feeds the uninstall-safety gate via `reinstallable`.
    #[serde(default)]
    pub defunct: bool,
    /// Surface this app for the user to review and remove *if they don't use it*
    /// (streaming services, etc.) — distinct from auto-remove bloat (`default_
    /// optimize`) and apps you keep. The Optimize wizard shows these as
    /// candidates with a usage signal but defaults them to Skip.
    #[serde(default)]
    pub review: bool,
}

impl AppEntry {
    /// Can the user get this app back easily after uninstalling? True when it
    /// has a Play Store listing, or it's defunct (nothing to get back). Drives
    /// whether uninstall is a safe recommendation vs disable.
    pub fn reinstallable(&self) -> bool {
        self.play_store || self.defunct
    }
}

/// Mode for an Optimize / Restore run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptimizeMode {
    Optimize,
    Restore,
}

/// What the engine computed for one app in an Optimize / Restore plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptimizeAction {
    /// Run `pm disable-user --user 0 <pkg>`.
    Disable,
    /// Run `pm uninstall --user 0 <pkg>`.
    Uninstall,
    /// Run `pm enable <pkg>`.
    Enable,
    /// No-op — already in the right state.
    Skip { reason: SkipReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Package not present on the device at all.
    NotInstalled,
    /// Already disabled — Optimize is a no-op.
    AlreadyDisabled,
    /// Already enabled — Restore is a no-op.
    AlreadyEnabled,
    /// User explicitly chose skip in the wizard.
    UserChoice,
}

/// One row in a computed Optimize/Restore plan — what the engine intends to do,
/// before any ADB call is made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizePlanItem {
    pub entry: AppEntry,
    pub action: OptimizeAction,
    /// Bytes of RAM currently in use by this package's processes, if running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<f64>,
}
