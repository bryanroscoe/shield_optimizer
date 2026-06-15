// TypeScript counterparts of the Rust types in src-tauri/src/engine/types.rs
// and src-tauri/src/commands/*.rs. Keep in sync.

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
  /// Whether this package has a real Google Play listing at this exact id
  /// (audited). Controls whether the "Play Store" button shows.
  play_store: boolean;
  /// Discontinued service — safe to uninstall despite no Play Store listing.
  defunct?: boolean;
  /// "Remove if unused" tier — surfaced as a candidate with a usage signal.
  review?: boolean;
}

/// When an app was last opened (from dumpsys usagestats).
export interface AppUsage {
  /// "YYYY-MM-DD HH:MM:SS" of last use, or null if never opened.
  last_used: string | null;
  launch_count: number;
}

export interface LauncherEntry {
  name: string;
  package: string;
}

export interface LauncherStatus {
  entry: LauncherEntry;
  installed: boolean;
  enabled: boolean;
  /// Preinstalled launcher — shown so users can switch back to stock.
  stock: boolean;
  /// HOME-capable app outside both catalogs (e.g. Setup Wraith, a sideloaded
  /// HOME app).
  other: boolean;
}

export interface CurrentLauncher {
  package: string | null;
  activity: string | null;
}

export interface ConnectResult {
  ok: boolean;
  message: string;
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

export interface DeviceReport {
  serial: string;
  name: string;
  report: HealthReport | null;
  error: string | null;
}

export interface RestartResult {
  ok: boolean;
  message: string;
}

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

export interface ActionResult {
  ok: boolean;
  message: string;
}

export interface OtherPackage {
  package: string;
  system: boolean;
  enabled: boolean;
  /// Friendly name for recognized sideloads (Artemis, Overseerr, …); null otherwise.
  name?: string | null;
}

export interface SetLauncherResult {
  ok: boolean;
  strategy: string | null;
  current_launcher: string | null;
  last_error: string | null;
  /// Polite strategies failed, but disabling the active stock launcher would
  /// work — the UI confirms with the user and retries with allowStockDisable.
  stock_takeover_available: boolean;
}

export interface InstallApkResult {
  ok: boolean;
  path: string;
  message: string;
  hint: string | null;
}

export interface DiscoveredApk {
  path: string;
  name: string;
  size_bytes: number;
  package: string | null;
}

export interface BackupApkResult {
  ok: boolean;
  files: string[];
  /// More than one APK — a split APK that must be installed together.
  split: boolean;
  message: string;
}

export interface CloneAppResult {
  ok: boolean;
  message: string;
  hint: string | null;
}

export interface ScanResult {
  subnet: string | null;
  found: string[];
  connected: string[];
  unauthorized: string[];
  failed: string[];
  message: string;
}

export interface ScreenshotResult {
  path: string;
  base64: string;
}

export interface SendTextResult {
  ok: boolean;
  message: string;
}

export interface FileEntry {
  name: string;
  is_dir: boolean;
  is_symlink: boolean;
  size_bytes: number;
  modified: string;
}

export interface FileTransferResult {
  ok: boolean;
  message: string;
  local_path: string | null;
}

export interface AdbStatus {
  available: boolean;
  path: string | null;
  last_probe: string | null;
}

export interface UpdateInfo {
  current: string;
  latest: string | null;
  update_available: boolean;
  url: string;
}

export interface InstallResult {
  ok: boolean;
  path: string | null;
  message: string;
}

export interface PrivateDnsState {
  mode: string | null;
  hostname: string | null;
}

export interface PrivateDnsResult {
  ok: boolean;
  message: string;
  reverted: boolean;
}

export interface SnapshotApplyPlan {
  packages_to_disable: string[];
  packages_already_disabled: string[];
  packages_not_installed: string[];
  launcher_to_set: string | null;
  settings_to_write: Record<string, string>;
  settings_already_set: string[];
  cross_device_warning: string | null;
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

export type Safety =
  | { kind: "never_disable"; reason: string }
  | { kind: "caution"; reason: string }
  | { kind: "safe" };

export interface RecoveryFailure {
  package: string;
  error: string;
}

export interface RecoveryResult {
  restored: string[];
  failed: RecoveryFailure[];
  message: string;
}

export type RebootMode = "normal" | "recovery" | "bootloader";

export interface RebootResult {
  ok: boolean;
  message: string;
}

export interface TweaksState {
  hdmi_control_enabled: string | null;
  hdmi_control_auto_wakeup_enabled: string | null;
  hdmi_control_auto_device_off_enabled: string | null;
  hdmi_system_audio_control_enabled: string | null;
  match_content_frame_rate: string | null;
  long_press_timeout: string | null;
  window_animation_scale: string | null;
  transition_animation_scale: string | null;
  animator_duration_scale: string | null;
  /// Background process limit: null = Standard, "0" = none, "1"–"4" = at most N.
  background_process_limit: string | null;
}

export type SettingNamespace = "global" | "secure" | "system";

export interface WriteResult {
  ok: boolean;
  message: string;
}

export type DisplayScalePreset = "uhd_4k" | "fhd_1080p" | "reset";

export interface DisplayScaleResult {
  ok: boolean;
  message: string;
}

export interface CurrentDisplayScaling {
  size: string;
  density: string;
}

export type OptimizeMode = "optimize" | "restore";

export type SkipReason = "not_installed" | "already_disabled" | "already_enabled" | "user_choice";

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

export function deviceTypeLabel(t: DeviceType): string {
  switch (t) {
    case "shield":
      return "Nvidia Shield";
    case "google_tv":
      return "Google TV";
    case "unknown":
      return "Unknown";
  }
}

export function riskBadgeClass(r: RiskTier): string {
  return `risk-${r}`;
}
