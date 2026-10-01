// TypeScript counterparts of the Rust types in crates/core/src/engine/types.rs
// and src-tauri/src/commands/*.rs. Keep in sync.

export type ConnectionType = "network" | "usb";
export type DeviceStatus = "device" | "unauthorized" | "offline";
export type DeviceType = "shield" | "google_tv" | "unknown";
/// What the device said about being a TV. Distinct from DeviceType, whose
/// "unknown" only means "no catalog match" — see engine/detection.rs.
export type TvEvidence = "tv" | "not_tv" | "unknown";
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
  serial_number?: string;
  /// `pm has-feature android.software.leanback`. null when the device gave no
  /// readable answer — which is not the same as "no".
  leanback?: boolean | null;
}

export interface Device {
  id: number;
  serial: string;
  name: string;
  model: string;
  device_type: DeviceType;
  tv_evidence: TvEvidence;
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
  /// When a person last reviewed this classification, `YYYY-MM-DD`.
  reviewed_at?: string;
  /// Short evidence notes or URLs behind the classification.
  sources?: string[];
  /// Device families the entry applies to; absent means the list decides.
  device_scope?: string[];
}

/// When an app was last opened (from dumpsys usagestats).
export interface AppUsage {
  /// "YYYY-MM-DD HH:MM:SS" of last use, or null if never opened.
  last_used: string | null;
  launch_count: number;
}

/// Installed storage for one package, in bytes. Disk, never memory. A field
/// the device did not report is null, which renders as unavailable — never 0.
/// `data_bytes` includes the cache, so the two are never added together.
export interface AppStorage {
  app_bytes: number | null;
  data_bytes: number | null;
  cache_bytes: number | null;
}

export interface LauncherEntry {
  name: string;
  package: string;
  /// The launcher's official page, for the "Get" link on a row that isn't
  /// installed. Null for stock launchers and for HOME handlers found on the
  /// device rather than in the catalog.
  source_url: string | null;
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
  /// Google TV's Setup Wraith: declares HOME but is the setup wizard. Never
  /// offered as the default.
  setup_helper: boolean;
}

export interface CurrentLauncher {
  package: string | null;
  activity: string | null;
  /// Set when Android's resolver named another app than the HOME role holder.
  note?: string | null;
}

export interface ConnectResult {
  ok: boolean;
  message: string;
}

export interface ForgetResult {
  ok: boolean;
  /// Every adb transport key dropped for this device, the requested one first.
  disconnected: string[];
  /// Still advertising Wireless debugging, so adb will re-attach it by itself.
  still_advertised: boolean;
  message: string;
}

export interface PairResult {
  ok: boolean;
  message: string;
  /// mDNS instance of the paired device's pairing service; null when adb
  /// never saw it, and then nothing advertised can be tied to this device.
  instance: string | null;
}

export interface PairedConnectResult {
  ok: boolean;
  /// It answered but its ro.serialno isn't the paired device's; it was
  /// disconnected again.
  not_the_paired_device: boolean;
  message: string;
}

/// One read of `adb mdns services` for a just-paired device's connect port.
export type PairedConnectProbe =
  | { state: "waiting" }
  | { state: "unidentified" }
  | { state: "attached"; serial: string }
  | { state: "endpoint"; address: string }
  | { state: "ambiguous"; addresses: string[] }
  | { state: "not_the_paired_device"; message: string };

export interface DisplayMode {
  resolution: string | null;
  refresh_hz: number | null;
  hdr_types: string[];
}

export type {
  SurroundMode, VerdictLevel, VideoFormat, DisplayModeEntry,
  AudioPassthrough, Verdict, MediaCapabilities,
} from "../../shared/media";

/// CPU + network rates over one device-side sampling window.
export interface ResourceSample {
  cpu_percent: number | null;
  interfaces: { name: string; rx_bytes_per_s: number | null; tx_bytes_per_s: number | null }[];
  interval_ms: number | null;
}

export interface ShellRunResult {
  stdout: string;
  stderr: string;
  exit_code: number | null;
  termination: "completed" | "output_limit" | "timeout";
  /// The safety gate refused it; nothing was sent to the device.
  blocked: boolean;
  blocked_reason: string | null;
}

export interface MemoryEntry {
  /// The full process name the device reported (`com.foo:remote`,
  /// `/system/bin/surfaceflinger`, `foo@2.1-service`).
  process: string;
  pid: number | null;
  /// The package this process name would belong to, or null when it has no
  /// package shape. Unverified until matched against the installed list.
  package: string | null;
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
  /// One line on what the app is, from known-names.json. Display only: the
  /// verdict for these packages stays Unknown.
  description?: string | null;
}

/// `set_home_any` — the Advanced picker. It never disables anything.
export interface SetHomeAnyResult {
  ok: boolean;
  current_launcher: string | null;
  /// null when the device can't answer (query-activities is Android 9+).
  declares_home: boolean | null;
  /// Stock still holds Home; only the separate "Disable stock launcher" step hands it over.
  stock_holds_home: boolean;
  message: string;
  diagnostics: string[];
}

export interface SetLauncherResult {
  ok: boolean;
  strategy: string | null;
  current_launcher: string | null;
  last_error: string | null;
  /// Polite strategies failed, but disabling the active stock launcher would
  /// work — the UI confirms with the user and retries with allowStockDisable.
  stock_takeover_available: boolean;
  /// Every command the attempt issued and what the device replied, in order.
  /// Offered as copyable detail on failure — launcher behavior varies enough
  /// between builds that a report is only actionable with the per-stage record.
  diagnostics: string[];
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

/// What an APK claims about itself, read before anything is installed.
/// `abi_compatible: null` means we could not establish it — never render that
/// as a mismatch.
export interface ApkInspection {
  path: string;
  name: string;
  size_bytes: number;
  package: string | null;
  abis: string[];
  device_abis: string[];
  abi_compatible: boolean | null;
  /** null when the TV could not be asked — never read that as "no". */
  already_installed: boolean | null;
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
  /// Pairing `host:port` for devices advertising only an Android 11+ pairing
  /// service. They need the 6-digit code from the TV before they can connect.
  needs_pairing: string[];
  message: string;
}

export interface FindResult {
  hits: string[];
  /// Directories whose search could not be run because the ADB call failed —
  /// distinct from a directory that simply holds no matches.
  unsearched: string[];
}

export interface ScreenshotResult {
  path: string;
  base64: string;
}

export interface SendTextResult {
  ok: boolean;
  message: string;
  /// "channel" = scrcpy control socket (instant), "shell" = legacy `input`
  /// fallback (~700 ms/press), "none" = nothing was sent.
  transport: "channel" | "shell" | "none";
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
  /// Notes for the version currently running — shown once after an update
  /// lands, including one installed by Homebrew or by hand.
  current_notes: string | null;
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
  settings_to_delete: string[];
  settings_already_set: string[];
  /// Device values for every setting the snapshot mentions; a missing key is unset.
  current_values: Record<string, string>;
  /// Home app when the plan was computed; null means the device couldn't say.
  current_launcher: string | null;
  /// The snapshot's launcher when it isn't installed here, so it's skipped.
  launcher_not_installed: string | null;
  cross_device_warning: string | null;
}

export interface ApplyResult {
  packages_disabled: string[];
  packages_failed: string[];
  launcher_set: boolean;
  launcher_message: string | null;
  settings_written: string[];
  settings_deleted: string[];
  settings_failed: string[];
  summary: string;
}

export type { Safety } from "../../shared/safety";

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
  /// global.background_process_limit, written by earlier versions. Android
  /// ignores it (#99); read only so a leftover value can be removed.
  background_process_limit: string | null;
  /// The cached background process limit Android is applying
  /// (CUR_MAX_CACHED_PROCESSES), or null when the device did not report it.
  cached_process_limit: number | null;
  /// Encoded audio passthrough: "0" Auto, "1" Never, "2" Always, "3" Manual.
  encoded_surround_output: string | null;
  /// Comma-separated AudioFormat encodings; applies only in Manual mode.
  encoded_surround_output_enabled_formats: string | null;
  /// secure.screensaver_components — the active Daydream's ComponentName, or
  /// null when no screensaver is configured.
  screensaver_components: string | null;
  /// secure.screensaver_enabled — whether Daydream runs at all. A cleared
  /// component with this still on can fall back to a framework/vendor
  /// default, so "None" has to turn this off too.
  screensaver_enabled: string | null;
}

export type SettingNamespace = "global" | "secure" | "system";

export interface WriteResult {
  ok: boolean;
  message: string;
}

export type DisplayScalePreset = "uhd_4k" | "fhd_1080p" | "hd_720p" | "reset";

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
