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
