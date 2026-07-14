// Typed contract for the ATV Optimizer mobile frontend.
// Keep in sync with v2/src/lib (desktop) and crates/core — these mirror the
// Rust types in crates/core/src/engine/types.rs and the command return shapes.
// Mobile is a subset of the desktop surface plus a few mobile-only shapes
// (wireless transport, entitlement).

export type ConnectionType = "network" | "usb";
export type DeviceStatus = "device" | "unauthorized" | "offline";
export type DeviceType = "shield" | "google_tv" | "unknown";
export type ActionMethod = "disable" | "uninstall";
export type RiskTier = "safe" | "medium" | "high" | "advanced";

export interface DeviceProperties {
  friendly_name: string | null;
  brand: string;
  model: string;
  device_codename: string;
  manufacturer: string;
  android_release: string;
  sdk_level: string;
  build_id: string;
  board_platform: string;
  characteristics?: string;
}

export interface Device {
  id: number;
  serial: string;
  name: string;
  model: string;
  device_type: DeviceType;
  status: DeviceStatus;
  connection: ConnectionType;
  properties: DeviceProperties | null;
}

export interface AppEntry {
  package: string;
  name: string;
  method: ActionMethod;
  risk: RiskTier;
  optimize_description: string;
  restore_description: string;
  default_optimize: boolean;
  default_restore: boolean;
  play_store: boolean;
  defunct?: boolean;
  review?: boolean;
}

export interface DisplayMode {
  resolution: string | null;
  refresh_hz: number | null;
  hdr_types: string[];
}

export interface MemoryEntry {
  package: string;
  mb: number;
}

export interface RamInfo {
  total_mb: number | null;
  used_mb: number | null;
  free_mb: number | null;
  swap_mb: number | null;
}

export interface StorageInfo {
  total: string | null;
  used: string | null;
  available: string | null;
  used_percent: number | null;
}

export interface HealthReport {
  display: DisplayMode;
  ram: RamInfo;
  storage: StorageInfo;
  temperature_c: number | null;
  audio_device: string | null;
  top_memory: MemoryEntry[];
}

export interface ActionResult {
  ok: boolean;
  message: string;
}

export interface ConnectResult {
  ok: boolean;
  message: string;
}

export interface OtherPackage {
  package: string;
  system: boolean;
  enabled: boolean;
  /// Friendly name for recognized sideloads; null otherwise.
  name?: string | null;
}

export interface ScreenshotResult {
  path: string;
  base64: string;
}

export interface SendTextResult {
  ok: boolean;
  message: string;
  /// "channel" = fast control socket, "shell" = legacy `input` fallback,
  /// "none" = nothing sent.
  transport: "channel" | "shell" | "none";
}

export type Safety =
  | { kind: "never_disable"; reason: string }
  | { kind: "caution"; reason: string }
  | { kind: "safe" };

export type RebootMode = "normal" | "recovery" | "bootloader";

export interface RebootResult {
  ok: boolean;
  message: string;
}

export type OptimizeMode = "optimize" | "restore";

export type SkipReason =
  | "not_installed"
  | "already_disabled"
  | "already_enabled"
  | "user_choice";

export type OptimizeAction =
  | { kind: "disable" }
  | { kind: "uninstall" }
  | { kind: "enable" }
  | { kind: "skip"; reason: SkipReason };

export interface OptimizePlanItem {
  entry: AppEntry;
  action: OptimizeAction;
  memory_mb?: number | null;
}

export interface OptimizePlan {
  mode: OptimizeMode;
  items: OptimizePlanItem[];
}

export type PerformanceProfile = "optimized" | "default";

export interface PerformanceResult {
  ok: boolean;
  message: string;
}

// ---- Mobile-only shapes (wireless transport + licensing) ----

/// One mDNS-discovered ADB service (pairing or connect, TLS or legacy).
export interface Discovery {
  name: string;
  host: string;
  port: number;
  service: string;
}

export interface DiscoveryResult {
  devices: Discovery[];
  message: string;
}

/// Cheap liveness probe result from `wireless_status`.
export interface WirelessStatus {
  connected: boolean;
  serial: string | null;
  host: string | null;
}

export type Entitlement = "free" | "pro";

// ---- Launcher (crates/core/src/commands/launcher.rs + engine/launcher.rs) ----

export interface LauncherEntry {
  name: string;
  package: string;
}

export interface LauncherStatus {
  entry: LauncherEntry;
  installed: boolean;
  enabled: boolean;
  /// Device's preinstalled launcher — STOCK badge, no Install button.
  stock: boolean;
  /// HOME-capable app outside both catalogs (e.g. Setup Wraith, a sideload).
  other: boolean;
}

export interface CurrentLauncher {
  package: string | null;
  activity: string | null;
}

export interface SetLauncherResult {
  ok: boolean;
  strategy: string | null;
  current_launcher: string | null;
  last_error: string | null;
  /// True when the only working switch is to disable the active stock launcher;
  /// the UI must confirm and retry with allow_stock_disable.
  stock_takeover_available: boolean;
}

// ---- Tweaks (crates/core/src/commands/tuning.rs) ----

/// Every field is `"1"` / `"0"` / a raw value string, or null when unset on the
/// device. Never fabricate a value for a null field.
export interface TweaksState {
  hdmi_control_enabled: string | null;
  hdmi_control_auto_wakeup_enabled: string | null;
  hdmi_control_auto_device_off_enabled: string | null;
  hdmi_system_audio_control_enabled: string | null;
  /// `0` = Never, `1` = Seamless only, `2` = Always.
  match_content_frame_rate: string | null;
  long_press_timeout: string | null;
  window_animation_scale: string | null;
  transition_animation_scale: string | null;
  animator_duration_scale: string | null;
  background_process_limit: string | null;
}

export interface WriteResult {
  ok: boolean;
  message: string;
}

/// Must stay in lockstep with the Rust `DisplayScalePreset` serde renames.
export type DisplayScalePreset = "uhd_4k" | "fhd_1080p" | "reset";

export interface CurrentDisplayScaling {
  size: string;
  density: string;
}

export interface DisplayScaleResult {
  ok: boolean;
  message: string;
}

export interface PrivateDnsState {
  /// `off` / `opportunistic` / `hostname`, or null if unset.
  mode: string | null;
  hostname: string | null;
}

export interface PrivateDnsResult {
  ok: boolean;
  message: string;
  /// True when a bad custom hostname was reverted to automatic.
  reverted: boolean;
}

// ---- Snapshots (crates/core/src/commands/snapshot.rs + engine/snapshot.rs) ----

export interface SnapshotFile {
  path: string;
  filename: string;
  saved_at: string;
  label: string | null;
  device_name: string;
  device_serial: string;
  device_type: DeviceType;
  disabled_count: number;
  settings_count: number;
  launcher: string | null;
}

export interface SnapshotApplyPlan {
  cross_device_warning: string | null;
  packages_to_disable: string[];
  packages_already_disabled: string[];
  packages_not_installed: string[];
  launcher_to_set: string | null;
  settings_to_write: Record<string, string>;
  settings_already_set: string[];
}

export interface ApplyResult {
  packages_disabled: string[];
  packages_failed: string[];
  launcher_set: boolean;
  launcher_message: string | null;
  settings_written: string[];
  settings_failed: string[];
  summary: string;
}

// ---- App detail (crates/core/src/adb/parse.rs) ----

export interface AppUsage {
  /// `"YYYY-MM-DD HH:MM:SS"` of last foreground use, or null if never opened.
  last_used: string | null;
  launch_count: number;
}

// ---- Devices hub (crates/core/src/commands/health.rs) ----

export interface DeviceReport {
  serial: string;
  name: string;
  report: HealthReport | null;
  error: string | null;
}

// ---- File transfer & backups (mobile-only, §7.2 / §7.3) ----

/// One entry from `ls -lA` on the device (crates/core parse::FileEntry).
export interface FileEntry {
  name: string;
  is_dir: boolean;
  is_symlink: boolean;
  size_bytes: number;
  /// `YYYY-MM-DD HH:MM`, as toybox prints it.
  modified: string;
}

/// A device file pulled into the app's scoped `downloads/` dir.
export interface PulledFile {
  name: string;
  path: string;
  size_bytes: number;
}

/// One APK backup living in the app's scoped `backups/` dir.
export interface BackupEntry {
  package: string;
  path: string;
  size_bytes: number;
  /// ISO-8601 (UTC) of the backup file's last-modified time.
  saved_at: string;
}

// ---- Saved-TV persistence (mobile-only, localStorage) ----

/// A previously-paired TV remembered on this phone so the app can offer a
/// one-tap reconnect on launch (design §1.0). The RSA pairing key is persisted
/// Kotlin-side, so a reconnect is silent — this is just app-side bookkeeping.
export interface SavedDevice {
  host: string;
  connectPort: number;
  name: string;
  deviceType: DeviceType;
  /// ISO timestamp of the last successful connect, for "last used" copy.
  lastUsed: string;
}

/// Canonical device label — prefer the friendly name, then the reported
/// name, then model, then a generic fallback. This is the ONLY place a
/// device label should be derived; every screen reads it via the session
/// store so the three old divergent derivations stay dead.
export function deviceLabelOf(device: Device | null): string {
  return (
    device?.properties?.friendly_name ||
    device?.name ||
    device?.model ||
    "Android TV"
  );
}

export function deviceTypeLabel(t: DeviceType | undefined): string {
  switch (t) {
    case "shield":
      return "Nvidia Shield";
    case "google_tv":
      return "Google TV";
    default:
      return "Android TV";
  }
}
