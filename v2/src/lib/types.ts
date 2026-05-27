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
}

export interface LauncherEntry {
  name: string;
  package: string;
}

export interface LauncherStatus {
  entry: LauncherEntry;
  installed: boolean;
  enabled: boolean;
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
  top_memory: MemoryEntry[];
}

export interface SnapshotFile {
  path: string;
  filename: string;
  saved_at: string;
  device_name: string;
  disabled_count: number;
}

export interface ActionResult {
  ok: boolean;
  message: string;
}

export interface SetLauncherResult {
  ok: boolean;
  strategy: string | null;
  current_launcher: string | null;
  last_error: string | null;
}

export interface InstallApkResult {
  ok: boolean;
  path: string;
  message: string;
  hint: string | null;
}

export interface ScanResult {
  subnet: string | null;
  found: string[];
  connected: string[];
  failed: string[];
  message: string;
}

export interface AdbStatus {
  available: boolean;
  path: string | null;
  last_probe: string | null;
}

export interface InstallResult {
  ok: boolean;
  path: string | null;
  message: string;
}

export interface SnapshotApplyPlan {
  packages_to_disable: string[];
  packages_already_disabled: string[];
  packages_not_installed: string[];
  launcher_to_set: string | null;
  settings_to_write: Record<string, string>;
  cross_device_warning: string | null;
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
