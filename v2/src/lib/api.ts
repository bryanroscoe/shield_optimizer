// Typed wrappers around Tauri's `invoke()`. Single point of contact with the
// Rust backend — every command goes through here.

import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  ActionResult,
  AdbStatus,
  AppEntry,
  ApplyResult,
  BackupApkResult,
  CloneAppResult,
  ConnectResult,
  CurrentDisplayScaling,
  CurrentLauncher,
  Device,
  DeviceReport,
  DeviceType,
  DiscoveredApk,
  DisplayScalePreset,
  DisplayScaleResult,
  FileEntry,
  FileTransferResult,
  HealthReport,
  InstallApkResult,
  InstallResult,
  LauncherStatus,
  OptimizeMode,
  OptimizePlan,
  OtherPackage,
  PerformanceProfile,
  PerformanceResult,
  PrivateDnsResult,
  PrivateDnsState,
  RebootMode,
  RebootResult,
  RecoveryResult,
  RestartResult,
  Safety,
  ScanResult,
  ScreenshotResult,
  SendTextResult,
  SetLauncherResult,
  SettingNamespace,
  SnapshotApplyPlan,
  SnapshotFile,
  TweaksState,
  UpdateInfo,
  WriteResult,
} from "./types";

export const api = {
  adbStatus: () => invoke<AdbStatus>("adb_status"),
  checkForUpdate: () => invoke<UpdateInfo>("check_for_update"),
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
  renameDevice: (serial: string, name: string) =>
    invoke<ActionResult>("rename_device", { serial, name }),

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
  setDefaultLauncher: (
    serial: string,
    pkg: string,
    allowStockDisable = false,
    onProgress?: Channel<string>,
  ) =>
    invoke<SetLauncherResult>("set_default_launcher", {
      serial,
      package: pkg,
      allowStockDisable,
      // The command always expects a progress channel; callers that don't care
      // (e.g. the enable-then-restore path) get a throwaway one.
      onProgress: onProgress ?? new Channel<string>(),
    }),
  disableLauncher: (serial: string, pkg: string) =>
    invoke<ActionResult>("disable_launcher", { serial, package: pkg }),

  takeScreenshot: (serial: string) =>
    invoke<ScreenshotResult>("take_screenshot", { serial }),
  forceStop: (serial: string, pkg: string) =>
    invoke<ActionResult>("force_stop", { serial, package: pkg }),

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
  appPermissionState: (serial: string, pkg: string, permission: string) =>
    invoke<"granted" | "revoked" | "missing">("app_permission_state", {
      serial,
      package: pkg,
      permission,
    }),
  setAppPermission: (serial: string, pkg: string, permission: string, grant: boolean) =>
    invoke<ActionResult>("set_app_permission", {
      serial,
      package: pkg,
      permission,
      grant,
    }),
  setAppOp: (serial: string, pkg: string, op: string, allow: boolean) =>
    invoke<ActionResult>("set_app_op", { serial, package: pkg, op, allow }),
  getAppOp: (serial: string, pkg: string, op: string) =>
    invoke<string>("get_app_op", { serial, package: pkg, op }),
  listOtherPackages: (serial: string) =>
    invoke<OtherPackage[]>("list_other_packages", { serial }),
  appMemoryMap: (serial: string) =>
    invoke<Record<string, number>>("app_memory_map", { serial }),
  appUsageMap: (serial: string) =>
    invoke<Record<string, import("$lib/types").AppUsage>>("app_usage_map", { serial }),
  safetyInfo: (pkg: string) => invoke<Safety>("safety_info", { package: pkg }),
  trimCaches: (serial: string) => invoke<ActionResult>("trim_caches", { serial }),
  sendText: (serial: string, text: string, forceShell = false) =>
    invoke<SendTextResult>("send_text", { serial, text, forceShell }),
  sendKey: (serial: string, key: string, forceShell = false) =>
    invoke<SendTextResult>("send_key", { serial, key, forceShell }),
  openSettings: (serial: string) =>
    invoke<SendTextResult>("open_settings", { serial }),

  installApk: (serial: string, apkPath: string, reinstall = true) =>
    invoke<InstallApkResult>("install_apk", { serial, apkPath, reinstall }),
  backupApk: (serial: string, pkg: string, destDir: string) =>
    invoke<BackupApkResult>("backup_apk", { serial, package: pkg, destDir }),
  cloneApp: (sourceSerial: string, targetSerial: string, pkg: string) =>
    invoke<CloneAppResult>("clone_app", { sourceSerial, targetSerial, package: pkg }),

  listDir: (serial: string, path: string, allowSystem = false) =>
    invoke<FileEntry[]>("list_dir", { serial, path, allowSystem }),
  pullFile: (serial: string, remotePath: string, localDir: string, allowSystem = false) =>
    invoke<FileTransferResult>("pull_file", { serial, remotePath, localDir, allowSystem }),
  pushFile: (serial: string, localPath: string, remoteDir: string, allowSystem = false) =>
    invoke<FileTransferResult>("push_file", { serial, localPath, remoteDir, allowSystem }),
  deletePath: (serial: string, path: string, allowSystem = false) =>
    invoke<FileTransferResult>("delete_path", { serial, path, allowSystem }),
  findFiles: (serial: string, dirs: string[], pattern: string) =>
    invoke<string[]>("find_files", { serial, dirs, pattern }),
  copyFileToDevice: (sourceSerial: string, remotePath: string, targetSerial: string, targetDir: string) =>
    invoke<FileTransferResult>("copy_file_to_device", { sourceSerial, remotePath, targetSerial, targetDir }),
  listApksInFolder: (folder: string) =>
    invoke<DiscoveredApk[]>("list_apks_in_folder", { folder }),

  listSnapshots: () => invoke<SnapshotFile[]>("list_snapshots"),
  saveSnapshot: (serial: string, deviceName: string, label: string | null = null) =>
    invoke<SnapshotFile>("save_snapshot", { serial, deviceName, label }),
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
  getPrivateDns: (serial: string) =>
    invoke<PrivateDnsState>("get_private_dns", { serial }),
  setPrivateDns: (serial: string, mode: string, hostname: string | null = null) =>
    invoke<PrivateDnsResult>("set_private_dns", { serial, mode, hostname }),

  prepareOptimize: (serial: string, deviceType: DeviceType, mode: OptimizeMode) =>
    invoke<OptimizePlan>("prepare_optimize", { serial, deviceType, mode }),
  applyPerformanceSettings: (serial: string, profile: PerformanceProfile) =>
    invoke<PerformanceResult>("apply_performance_settings", { serial, profile }),
};
