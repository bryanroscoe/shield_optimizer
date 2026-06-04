// Typed wrappers around Tauri's `invoke()`. Single point of contact with the
// Rust backend — every command goes through here.

import { invoke } from "@tauri-apps/api/core";
import type {
  ActionResult,
  AdbStatus,
  AppEntry,
  ApplyResult,
  ConnectResult,
  CurrentDisplayScaling,
  CurrentLauncher,
  Device,
  DeviceReport,
  DeviceType,
  DiscoveredApk,
  DisplayScalePreset,
  DisplayScaleResult,
  HealthReport,
  HomeHandler,
  InstallApkResult,
  InstallResult,
  LauncherStatus,
  OptimizeMode,
  OptimizePlan,
  PerformanceProfile,
  PerformanceResult,
  RebootMode,
  RebootResult,
  RecoveryResult,
  RestartResult,
  Safety,
  ScanResult,
  SendTextResult,
  SetLauncherResult,
  SettingNamespace,
  SnapshotApplyPlan,
  SnapshotFile,
  StockLauncherResult,
  TweaksState,
  WriteResult,
} from "./types";

export const api = {
  adbStatus: () => invoke<AdbStatus>("adb_status"),
  installAdb: () => invoke<InstallResult>("install_adb"),
  restartAdb: () => invoke<RestartResult>("restart_adb"),
  scanNetwork: () => invoke<ScanResult>("scan_network"),
  reportAll: () => invoke<DeviceReport[]>("report_all"),

  listDevices: () => invoke<Device[]>("list_devices"),
  deviceProfile: (serial: string) =>
    invoke<Device>("device_profile", { serial }),

  connectDevice: (address: string) =>
    invoke<ConnectResult>("connect_device", { address }),
  disconnectDevice: (serial: string) =>
    invoke<ConnectResult>("disconnect_device", { serial }),
  pairDevice: (pairAddress: string, pin: string) =>
    invoke<ConnectResult>("pair_device", { pairAddress, pin }),

  healthReport: (serial: string) =>
    invoke<HealthReport>("health_report", { serial }),
  appListForDevice: (deviceType: DeviceType) =>
    invoke<AppEntry[]>("app_list_for_device", { deviceType }),

  listLaunchers: (serial: string) =>
    invoke<LauncherStatus[]>("list_launchers", { serial }),
  currentLauncher: (serial: string) =>
    invoke<CurrentLauncher>("current_launcher", { serial }),
  channelProviderDisabled: (serial: string) =>
    invoke<boolean>("channel_provider_disabled", { serial }),
  setDefaultLauncher: (serial: string, pkg: string) =>
    invoke<SetLauncherResult>("set_default_launcher", { serial, package: pkg }),
  listHomeHandlers: (serial: string, targetPackage: string) =>
    invoke<HomeHandler[]>("list_home_handlers", { serial, targetPackage }),
  disableStockLaunchers: (serial: string, packages: string[]) =>
    invoke<StockLauncherResult>("disable_stock_launchers", { serial, packages }),
  restoreStockLaunchers: (serial: string, packages: string[]) =>
    invoke<StockLauncherResult>("restore_stock_launchers", { serial, packages }),

  disablePackage: (serial: string, pkg: string) =>
    invoke<ActionResult>("disable_package", { serial, package: pkg }),
  enablePackage: (serial: string, pkg: string) =>
    invoke<ActionResult>("enable_package", { serial, package: pkg }),
  uninstallPackage: (serial: string, pkg: string) =>
    invoke<ActionResult>("uninstall_package", { serial, package: pkg }),
  reinstallExisting: (serial: string, pkg: string) =>
    invoke<ActionResult>("reinstall_existing", { serial, package: pkg }),
  openPlayStore: (serial: string, pkg: string) =>
    invoke<ActionResult>("open_play_store", { serial, package: pkg }),
  packageStates: (serial: string, packages: string[]) =>
    invoke<Record<string, "enabled" | "disabled" | "missing">>(
      "package_states",
      { serial, packages },
    ),
  safetyInfo: (pkg: string) => invoke<Safety>("safety_info", { package: pkg }),
  trimCaches: (serial: string) => invoke<ActionResult>("trim_caches", { serial }),
  sendText: (serial: string, text: string) =>
    invoke<SendTextResult>("send_text", { serial, text }),

  installApk: (serial: string, apkPath: string, reinstall = true) =>
    invoke<InstallApkResult>("install_apk", { serial, apkPath, reinstall }),
  listApksInFolder: (folder: string) =>
    invoke<DiscoveredApk[]>("list_apks_in_folder", { folder }),

  listSnapshots: () => invoke<SnapshotFile[]>("list_snapshots"),
  saveSnapshot: (serial: string, deviceName: string) =>
    invoke<SnapshotFile>("save_snapshot", { serial, deviceName }),
  previewApply: (serial: string, snapshotPath: string) =>
    invoke<SnapshotApplyPlan>("preview_apply", { serial, snapshotPath }),
  applySnapshot: (serial: string, snapshotPath: string) =>
    invoke<ApplyResult>("apply_snapshot", { serial, snapshotPath }),
  deleteSnapshot: (snapshotPath: string) =>
    invoke<void>("delete_snapshot", { snapshotPath }),
  snapshotDirPath: () => invoke<string>("snapshot_dir_path"),

  panicRecovery: (serial: string) =>
    invoke<RecoveryResult>("panic_recovery", { serial }),
  rebootDevice: (serial: string, mode: RebootMode) =>
    invoke<RebootResult>("reboot_device", { serial, mode }),

  getTweaks: (serial: string) => invoke<TweaksState>("get_tweaks", { serial }),
  writeSetting: (
    serial: string,
    namespace: SettingNamespace,
    key: string,
    value: string,
  ) => invoke<WriteResult>("write_setting", { serial, namespace, key, value }),
  setDisplayScaling: (serial: string, preset: DisplayScalePreset) =>
    invoke<DisplayScaleResult>("set_display_scaling", { serial, preset }),
  getDisplayScaling: (serial: string) =>
    invoke<CurrentDisplayScaling>("get_display_scaling", { serial }),

  prepareOptimize: (serial: string, mode: OptimizeMode) =>
    invoke<OptimizePlan>("prepare_optimize", { serial, mode }),
  applyPerformanceSettings: (serial: string, profile: PerformanceProfile) =>
    invoke<PerformanceResult>("apply_performance_settings", { serial, profile }),
};
