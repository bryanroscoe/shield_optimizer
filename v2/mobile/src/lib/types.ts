// Typed contract for the ATV Optimizer mobile frontend.
// Keep in sync with v2/src/lib (desktop) and crates/core — these mirror the
// Rust types in crates/core/src/engine/types.rs and the command return shapes.
// Mobile is a subset of the desktop surface plus a few mobile-only shapes
// (wireless transport, entitlement).

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
  /// `ro.serialno` — stable hardware identity; empty/missing when unreadable.
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

export interface DisplayMode {
  resolution: string | null;
  refresh_hz: number | null;
  hdr_types: string[];
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
  /// One line on what the app is, from known-names.json. Display only: the
  /// verdict for these packages stays Unknown.
  description?: string | null;
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

export type { Safety } from "../../../shared/safety";

export type RebootMode = "normal" | "recovery" | "bootloader";

/// Result of `panic_recovery` — `pm enable` for every disabled package.
export interface RecoveryResult {
  restored: string[];
  failed: { package: string; error: string }[];
  message: string;
}

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
  /// The launcher's official page, for the "Get" link on a row that isn't
  /// installed. Null for stock launchers and for HOME handlers found on the
  /// device rather than in the catalog.
  source_url: string | null;
  /// Setup helpers (Setup Wraith) a takeover turns off together with this
  /// stock launcher. Present only on a stock entry that has one.
  disable_with?: string[];
}

export interface LauncherStatus {
  entry: LauncherEntry;
  installed: boolean;
  enabled: boolean;
  /// Device's preinstalled launcher — STOCK badge, no Install button.
  stock: boolean;
  /// HOME-capable app outside both catalogs (e.g. Setup Wraith, a sideload).
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

export interface SetLauncherResult {
  ok: boolean;
  strategy: string | null;
  current_launcher: string | null;
  last_error: string | null;
  /// True when the only working switch is to disable the active stock launcher;
  /// the UI must confirm and retry with allow_stock_disable.
  stock_takeover_available: boolean;
  /// Every command the attempt issued and what the device replied, in order.
  /// Offered as copyable detail on failure.
  diagnostics: string[];
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
  /// `global.background_process_limit`, written by earlier versions. Android
  /// ignores it (#99); read only so a leftover value can be removed.
  background_process_limit: string | null;
  /// The cached background process limit Android is applying
  /// (CUR_MAX_CACHED_PROCESSES), or null when the device did not report it.
  cached_process_limit: number | null;
  /// Encoded audio passthrough: "0" Auto, "1" Never, "2" Always, "3" Manual.
  encoded_surround_output: string | null;
  /// Comma-separated AudioFormat encodings; applies only in Manual mode.
  encoded_surround_output_enabled_formats: string | null;
  /// `secure.screensaver_components`: the active Daydream's ComponentName, or
  /// null when no screensaver is configured.
  screensaver_components: string | null;
  /// `secure.screensaver_enabled`: whether Daydream runs at all.
  screensaver_enabled: string | null;
}

export interface WriteResult {
  ok: boolean;
  message: string;
}

export interface ResourceSample {
  cpu_percent: number | null;
  interfaces: Array<{
    name: string;
    rx_bytes_per_s: number | null;
    tx_bytes_per_s: number | null;
  }>;
  interval_ms: number | null;
}

/// Must stay in lockstep with the Rust `DisplayScalePreset` serde renames.
export type DisplayScalePreset = "uhd_4k" | "fhd_1080p" | "hd_720p" | "reset";

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
  /// The TV's Home app when the plan was computed; null means it couldn't say.
  current_launcher: string | null;
  /// The snapshot's launcher when it isn't installed on this TV.
  launcher_not_installed: string | null;
  settings_to_write: Record<string, string>;
  settings_to_delete: string[];
  settings_already_set: string[];
  /// The TV's current value for every setting the snapshot mentions; a key
  /// missing here is unset on the TV.
  current_values: Record<string, string>;
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

// ---- App detail (crates/core/src/adb/parse.rs) ----

export interface AppUsage {
  /// `"YYYY-MM-DD HH:MM:SS"` of last foreground use, or null if never opened.
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

/// `app_permission_state` — a runtime permission's grant state for one package.
export type PermissionState = "granted" | "revoked" | "missing";

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
  apk_count: number;
  /// False for base-only backups created before split-APK bundles existed.
  complete: boolean;
  /// ISO-8601 (UTC) of the backup file's last-modified time.
  saved_at: string;
}

// ---- Saved-TV persistence (mobile-only, localStorage) ----

/// A previously-paired TV remembered on this phone so the app can offer a
/// one-tap reconnect on launch (design §1.0). The RSA pairing key is persisted
/// Kotlin-side, so a reconnect is silent — this is just app-side bookkeeping.
/// A soft identity hint captured from a device's reported properties. Never
/// proof of identity -- see `deviceFingerprintOf`/`fingerprintMismatch` in
/// `identity.ts` -- only ever used to rule an id-less match *out*.
export interface DeviceFingerprint {
  model?: string;
  manufacturer?: string;
  deviceCodename?: string;
  /// The TV's own user-set device name (`friendly_name`), distinct from this
  /// row's possibly-synthesized `name`.
  name?: string;
}

export interface SavedDevice {
  host: string;
  connectPort: number;
  name: string;
  deviceType: DeviceType;
  /// Hardware serial when the TV reported one. Lets a TV keep its row and
  /// name across an IP change, and stops a reused IP from inheriting a name.
  hardwareId?: string;
  /// Stable random id assigned the first time a row with no hardware id is
  /// saved. Two TVs that never reported a serial and happened to share a
  /// host:port are still distinct rows -- the address alone cannot tell them
  /// apart, so each gets its own key instead of collapsing into one.
  localId?: string;
  /// Soft fingerprint for an id-less row only (hardware-identified rows don't
  /// need it). Lets a reconnect notice an obvious swap -- a different model
  /// or manufacturer now answering at this row's address -- without ever
  /// upgrading the match to "verified".
  fingerprint?: DeviceFingerprint;
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

/// Result of `remote_warm` — the transport the *next* press will use, learned
/// without sending one. "shell" carries the reason the fast channel is out.
export interface RemoteWarmResult {
  transport: "channel" | "shell";
  message: string;
}

/// Decoded, verified license as reported by `license_info`.
export interface LicenseInfo {
  plan: Entitlement;
  licensee: string;
  issued: string;
  expires: string | null;
  key_id: number;
}

/// Result of `find_files` (crates/core commands::files::FindResult).
/// `unsearched` lists directories the search could not run against at all —
/// never to be shown as "no matches" (GitHub #86).
export interface FindResult {
  hits: string[];
  unsearched: string[];
}
