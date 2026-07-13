// Typed wrappers around Tauri's `invoke()` — the single point of contact with
// the Rust backend. Every command goes through `call()`, which logs to the
// frontend debug ring buffer (see log.ts) and mirrors to console.
//
// Keep in sync with v2/src/lib (desktop) and crates/core. Argument keys are
// camelCase; Tauri maps them to the snake_case Rust params. Note the
// `pkg` -> `package` mapping and `mode` (NOT `rebootMode`) for reboot — the
// class of silent-failure bug this module exists to prevent.

import { Channel, invoke } from "@tauri-apps/api/core";
import { logCall, summarizeArgs } from "./log";
import type {
  ActionResult,
  AppEntry,
  AppUsage,
  ApplyResult,
  BackupEntry,
  FileEntry,
  PulledFile,
  ConnectResult,
  CurrentDisplayScaling,
  CurrentLauncher,
  Device,
  DeviceReport,
  DeviceType,
  DiscoveryResult,
  DisplayScalePreset,
  DisplayScaleResult,
  Entitlement,
  HealthReport,
  LauncherStatus,
  OptimizeMode,
  OptimizePlan,
  OtherPackage,
  PrivateDnsResult,
  PrivateDnsState,
  RebootMode,
  RebootResult,
  Safety,
  ScreenshotResult,
  SendTextResult,
  SetLauncherResult,
  SnapshotApplyPlan,
  SnapshotFile,
  TweaksState,
  WirelessStatus,
  WriteResult,
} from "./types";

type PackageState = "enabled" | "disabled" | "missing";

async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const summary = summarizeArgs(args);
  try {
    const result = await invoke<T>(command, args);
    logCall(command, summary, true);
    return result;
  } catch (e) {
    logCall(command, summary, false, String(e));
    throw e;
  }
}

export const api = {
  // ---- Wireless transport ----
  wirelessDiscover: () => call<DiscoveryResult>("wireless_discover"),
  wirelessPair: (host: string, port: number, code: string) =>
    call<ConnectResult>("wireless_pair", { host, port, code }),
  wirelessConnect: (host: string, port: number) =>
    call<ConnectResult>("wireless_connect", { host, port }),
  wirelessDisconnect: () => call<ConnectResult>("wireless_disconnect"),
  wirelessStatus: () => call<WirelessStatus>("wireless_status"),

  // ---- Devices ----
  listDevices: () => call<Device[]>("list_devices"),
  deviceProfile: (serial: string) =>
    call<Device>("device_profile", { serial }),

  // ---- Health / catalog ----
  healthReport: (serial: string) =>
    call<HealthReport>("health_report", { serial }),
  appListForDevice: (deviceType: DeviceType) =>
    call<AppEntry[]>("app_list_for_device", { deviceType }),
  packageStates: (serial: string, packages: string[]) =>
    call<Record<string, PackageState>>("package_states", { serial, packages }),

  // ---- Apps ----
  listOtherPackages: (serial: string) =>
    call<OtherPackage[]>("list_other_packages", { serial }),
  disablePackage: (serial: string, pkg: string) =>
    call<ActionResult>("disable_package", { serial, package: pkg }),
  enablePackage: (serial: string, pkg: string) =>
    call<ActionResult>("enable_package", { serial, package: pkg }),
  uninstallPackage: (serial: string, pkg: string) =>
    call<ActionResult>("uninstall_package", { serial, package: pkg }),
  forceStop: (serial: string, pkg: string) =>
    call<ActionResult>("force_stop", { serial, package: pkg }),
  /// Backend-audited safety classification for one package — the ONLY source
  /// of truth for "keep / caution / safe" tags. Never reimplement inline.
  safetyInfo: (pkg: string) => call<Safety>("safety_info", { package: pkg }),

  // ---- Maintenance ----
  trimCaches: (serial: string) =>
    call<ActionResult>("trim_caches", { serial }),
  takeScreenshot: (serial: string) =>
    call<ScreenshotResult>("take_screenshot", { serial }),
  rebootDevice: (serial: string, mode: RebootMode) =>
    call<RebootResult>("reboot_device", { serial, mode }),

  // ---- Remote input ----
  sendKey: (serial: string, key: string, forceShell = false) =>
    call<SendTextResult>("send_key", { serial, key, forceShell }),
  sendText: (serial: string, text: string, forceShell = false) =>
    call<SendTextResult>("send_text", { serial, text, forceShell }),
  openSettings: (serial: string) =>
    call<SendTextResult>("open_settings", { serial }),
  /// Nvidia-Shield-only remote locator (gated on device_type in the UI).
  findRemote: (serial: string) =>
    call<ConnectResult>("find_remote", { serial }),

  // ---- Optimize ----
  prepareOptimize: (serial: string, deviceType: DeviceType, mode: OptimizeMode) =>
    call<OptimizePlan>("prepare_optimize", { serial, deviceType, mode }),

  // ---- App detail / catalog extras ----
  appMemoryMap: (serial: string) =>
    call<Record<string, number>>("app_memory_map", { serial }),
  appUsageMap: (serial: string) =>
    call<Record<string, AppUsage>>("app_usage_map", { serial }),
  openPlayStore: (serial: string, pkg: string) =>
    call<ActionResult>("open_play_store", { serial, package: pkg }),
  reinstallExisting: (serial: string, pkg: string) =>
    call<ActionResult>("reinstall_existing", { serial, package: pkg }),

  // ---- Launcher (4.2) — set/disable are Pro (LauncherTakeover) ----
  listLaunchers: (serial: string) =>
    call<LauncherStatus[]>("list_launchers", { serial }),
  currentLauncher: (serial: string) =>
    call<CurrentLauncher>("current_launcher", { serial }),
  channelProviderDisabled: (serial: string) =>
    call<boolean>("channel_provider_disabled", { serial }),
  /// `set_default_launcher` takes a per-step progress Channel in core. Pass a
  /// callback to narrate the multi-second switch, or omit for a no-op channel.
  setDefaultLauncher: (
    serial: string,
    pkg: string,
    allowStockDisable = false,
    onProgress?: (msg: string) => void,
  ) => {
    const channel = new Channel<string>();
    if (onProgress) channel.onmessage = onProgress;
    return call<SetLauncherResult>("set_default_launcher", {
      serial,
      package: pkg,
      allowStockDisable,
      onProgress: channel,
    });
  },
  disableLauncher: (serial: string, pkg: string) =>
    call<ActionResult>("disable_launcher", { serial, package: pkg }),

  // ---- Tweaks (5.1) — writes are Pro (TweaksWrite) ----
  getTweaks: (serial: string) => call<TweaksState>("get_tweaks", { serial }),
  writeSetting: (serial: string, namespace: string, key: string, value: string) =>
    call<WriteResult>("write_setting", { serial, namespace, key, value }),
  getDisplayScaling: (serial: string) =>
    call<CurrentDisplayScaling>("get_display_scaling", { serial }),
  setDisplayScaling: (serial: string, preset: DisplayScalePreset) =>
    call<DisplayScaleResult>("set_display_scaling", { serial, preset }),
  getPrivateDns: (serial: string) =>
    call<PrivateDnsState>("get_private_dns", { serial }),
  setPrivateDns: (serial: string, mode: string, hostname?: string) =>
    call<PrivateDnsResult>("set_private_dns", { serial, mode, hostname }),

  // ---- Snapshots (5.2) — all Pro (Snapshot) ----
  listSnapshots: () => call<SnapshotFile[]>("list_snapshots"),
  saveSnapshot: (serial: string, deviceName: string, label?: string) =>
    call<SnapshotFile>("save_snapshot", { serial, deviceName, label }),
  previewApply: (serial: string, snapshotPath: string) =>
    call<SnapshotApplyPlan>("preview_apply", { serial, snapshotPath }),
  applySnapshot: (serial: string, snapshotPath: string) =>
    call<ApplyResult>("apply_snapshot", { serial, snapshotPath }),
  deleteSnapshot: (snapshotPath: string) =>
    call<void>("delete_snapshot", { snapshotPath }),
  snapshotDirPath: () => call<string>("snapshot_dir_path"),

  // ---- Devices hub (7.1) ----
  reportAll: () => call<DeviceReport[]>("report_all"),
  renameDevice: (serial: string, name: string) =>
    call<ActionResult>("rename_device", { serial, name }),

  // ---- Licensing ----
  getEntitlement: () => call<Entitlement>("get_entitlement"),
  activateLicense: (key: string) =>
    call<Entitlement>("activate_license", { key }),

  // ---- File transfer (7.2) ----
  listRemoteDir: (serial: string, path: string) =>
    call<FileEntry[]>("list_remote_dir", { serial, path }),
  pullFile: (serial: string, remotePath: string) =>
    call<PulledFile>("pull_file", { serial, remotePath }),

  // ---- Backups (7.3) ----
  backupApk: (serial: string, pkg: string) =>
    call<BackupEntry>("backup_apk", { serial, package: pkg }),
  listBackups: () => call<BackupEntry[]>("list_backups"),

  // ---- Debug ----
  readDebugLog: () => call<string>("read_debug_log"),
};
