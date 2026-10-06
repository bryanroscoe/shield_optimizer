// Typed wrappers around Tauri's `invoke()` — the single point of contact with
// the Rust backend. Every command goes through `call()`, which logs to the
// frontend debug ring buffer (see log.ts) and mirrors to console.
//
// Keep in sync with v2/src/lib (desktop) and crates/core. Argument keys are
// camelCase; Tauri maps them to the snake_case Rust params. Note the
// `pkg` -> `package` mapping and `mode` (NOT `rebootMode`) for reboot — the
// class of silent-failure bug this module exists to prevent.

import { Channel, invoke } from "@tauri-apps/api/core";
import { parseSafety } from "../../../shared/safety";
import type { MediaCapabilities } from "../../../shared/media";
import { logCall, summarizeArgs } from "./log";
import { connectionGeneration, emitConnectionLost, isConnectionLostError } from "./connectionEvents";
import type {
  ActionResult,
  AppEntry,
  AppStorage,
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
  ResourceSample,
  LauncherStatus,
  LicenseInfo,
  OptimizeMode,
  OptimizePlan,
  PerformanceProfile,
  PerformanceResult,
  OtherPackage,
  PermissionState,
  PrivateDnsResult,
  PrivateDnsState,
  RebootMode,
  RebootResult,
  RecoveryResult,
  RemoteWarmResult,
  Safety,
  ScreenshotResult,
  SendTextResult,
  SetHomeAnyResult,
  SetLauncherResult,
  SnapshotApplyPlan,
  SnapshotFile,
  TweaksState,
  WirelessStatus,
  WriteResult,
  FindResult,
} from "./types";

type PackageState = "enabled" | "disabled" | "missing";

async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const summary = summarizeArgs(args);
  const generation = connectionGeneration();
  try {
    const result = await invoke<T>(command, args);
    logCall(command, summary, true);
    return result;
  } catch (e) {
    const message = String(e);
    logCall(command, summary, false, message);
    // The backend already evicted the dead socket; let the session flip to
    // "lost" (and try one recovery) instead of leaving a green header up.
    if (isConnectionLostError(message)) emitConnectionLost(generation);
    throw e;
  }
}

export const api = {
  // ---- Wireless transport ----
  wirelessDiscover: () => call<DiscoveryResult>("wireless_discover"),
  wirelessPair: (host: string, port: number, code: string) =>
    call<ConnectResult>("wireless_pair", { host, port, code }),
  wirelessConnect: (host: string, port: number, requestId: number) =>
    call<ConnectResult>("wireless_connect", { host, port, requestId }),
  wirelessDisconnect: (requestId: number) => call<ConnectResult>("wireless_disconnect", { requestId }),
  wirelessCancelConnect: (requestId: number, canceledRequestId: number) =>
    call<void>("wireless_cancel_connect", { requestId, canceledRequestId }),
  wirelessStatus: () => call<WirelessStatus>("wireless_status"),

  // ---- Devices ----
  listDevices: () => call<Device[]>("list_devices"),
  deviceProfile: (serial: string) =>
    call<Device>("device_profile", { serial }),

  // ---- Health / catalog ----
  healthReport: (serial: string) =>
    call<HealthReport>("health_report", { serial }),
  mediaReport: (serial: string) => call<MediaCapabilities>("media_report", { serial }),
  resourceSample: (serial: string) => call<ResourceSample>("resource_sample", { serial }),
  appListForDevice: (deviceType: DeviceType) =>
    call<AppEntry[]>("app_list_for_device", { deviceType }),
  packageStates: (serial: string, packages: string[]) =>
    call<Record<string, PackageState>>("package_states", { serial, packages }),

  // ---- Apps ----
  listOtherPackages: (serial: string) =>
    call<OtherPackage[]>("list_other_packages", { serial }),
  listInstalledPackages: (serial: string) =>
    call<OtherPackage[]>("list_installed_packages", { serial }),
  disablePackage: (serial: string, pkg: string) =>
    call<ActionResult>("disable_package", { serial, package: pkg }),
  enablePackage: (serial: string, pkg: string) =>
    call<ActionResult>("enable_package", { serial, package: pkg }),
  uninstallPackage: (serial: string, pkg: string) =>
    call<ActionResult>("uninstall_package", { serial, package: pkg }),
  forceStop: (serial: string, pkg: string) =>
    call<ActionResult>("force_stop", { serial, package: pkg }),
  /// Backend-audited protected / caution / unknown verdict for one package.
  safetyInfo: async (pkg: string): Promise<Safety> =>
    parseSafety(await call<unknown>("safety_info", { package: pkg })),
  /// Classify a *process* name from a memory report. Catalog-free on purpose —
  /// see the Rust doc on `process_safety_info`.
  processSafetyInfo: async (process: string): Promise<Safety> =>
    parseSafety(await call<unknown>("process_safety_info", { process })),
  /// Runtime permission state. Read-only, free.
  appPermissionState: (serial: string, pkg: string, permission: string) =>
    call<PermissionState>("app_permission_state", { serial, package: pkg, permission }),
  /// Grant/revoke a runtime permission. Pro (AppPermissionWrite).
  setAppPermission: (serial: string, pkg: string, permission: string, grant: boolean) =>
    call<ActionResult>("set_app_permission", { serial, package: pkg, permission, grant }),
  /// Set an appop to allow/ignore. Pro (AppPermissionWrite).
  setAppOp: (serial: string, pkg: string, op: string, allow: boolean) =>
    call<ActionResult>("set_app_op", { serial, package: pkg, op, allow }),
  /// Raw `appops get` mode string for one op. Read-only, free.
  getAppOp: (serial: string, pkg: string, op: string) =>
    call<string>("get_app_op", { serial, package: pkg, op }),

  // ---- Maintenance ----
  trimCaches: (serial: string) =>
    call<ActionResult>("trim_caches", { serial }),
  takeScreenshot: (serial: string) =>
    call<ScreenshotResult>("take_screenshot", { serial }),
  /// Emergency recovery: re-enable every disabled package on the TV.
  panicRecovery: (serial: string) =>
    call<RecoveryResult>("panic_recovery", { serial }),
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
  applyPerformanceSettings: (
    serial: string,
    profile: PerformanceProfile,
  ) =>
    call<PerformanceResult>("apply_performance_settings", {
      serial,
      profile,
    }),

  // ---- App detail / catalog extras ----
  appMemoryMap: (serial: string) =>
    call<Record<string, number>>("app_memory_map", { serial }),
  appUsageMap: (serial: string) =>
    call<Record<string, AppUsage>>("app_usage_map", { serial }),
  /// Installed storage per package from one `dumpsys diskstats`. Read-only, free.
  appStorageMap: (serial: string) =>
    call<Record<string, AppStorage>>("app_storage_map", { serial }),
  /// One package's APK size (`pm path` + `stat`), for a package diskstats has
  /// no row for. Data and cache come back null. Read-only, free.
  appApkSize: (serial: string, pkg: string) =>
    call<AppStorage>("app_apk_size", { serial, package: pkg }),
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
  /// Turn off an enabled Setup Wraith while stock is already disabled. Pro.
  disableSetupHelper: (serial: string, pkg: string) =>
    call<ActionResult>("disable_setup_helper", { serial, package: pkg }),
  /// Advanced picker: try any app as Home. Never disables anything. Pro.
  setHomeAny: (serial: string, pkg: string, activity: string | null = null) =>
    call<SetHomeAnyResult>("set_home_any", { serial, package: pkg, activity }),
  /// The explicit, confirmed step that hands Home from stock to `target`. Pro.
  disableStockLauncher: (serial: string, target: string) =>
    call<SetLauncherResult>("disable_stock_launcher", { serial, target }),

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
  licenseInfo: () => call<LicenseInfo | null>("license_info"),

  // ---- File transfer (7.2) ----
  listRemoteDir: (serial: string, path: string) =>
    call<FileEntry[]>("list_remote_dir", { serial, path }),
  pullFile: (serial: string, remotePath: string) =>
    call<PulledFile>("pull_file", { serial, remotePath }),

  // ---- Backups (7.3) ----
  backupApk: (serial: string, pkg: string) =>
    call<BackupEntry>("backup_apk", { serial, package: pkg }),
  restoreApkBackup: (serial: string, backupPath: string) =>
    call<ActionResult>("restore_apk_backup", { serial, backupPath }),
  listBackups: () => call<BackupEntry[]>("list_backups"),

  // ---- Debug ----
  readDebugLog: () => call<string>("read_debug_log"),

  /// Delete one APK backup from the app's scoped storage (path-confined to the
  /// backups dir backend-side). The TV is not touched.
  deleteBackup: (backupPath: string) =>
    call<ActionResult>("delete_backup", { backupPath }),

  /// Start the scrcpy control channel ahead of the first press so the cold
  /// start isn't charged to the user's first button. Idempotent; resolves with
  /// the transport the next press will actually use.
  remoteWarm: (serial: string) =>
    call<RemoteWarmResult>("remote_warm", { serial }),

  /// App-files catalog search under /sdcard. Directories that could not be
  /// searched come back in `unsearched`, distinct from no matches.
  findFiles: (serial: string, dirs: string[], pattern: string) =>
    call<FindResult>("find_files", { serial, dirs, pattern }),
  /// Delete a file or folder on the TV. /sdcard-confined backend-side.
  deletePath: (serial: string, path: string) =>
    call<ActionResult>("delete_path", { serial, path }),
};
