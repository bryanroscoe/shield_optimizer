// Typed wrappers around Tauri's `invoke()`. Single point of contact with the
// Rust backend — every command goes through here.

import { invoke } from "@tauri-apps/api/core";
import type {
  ActionResult,
  AdbStatus,
  AppEntry,
  ConnectResult,
  CurrentLauncher,
  Device,
  DeviceType,
  HealthReport,
  InstallApkResult,
  InstallResult,
  LauncherStatus,
  ScanResult,
  SetLauncherResult,
  SnapshotApplyPlan,
  SnapshotFile,
} from "./types";

export const api = {
  adbStatus: () => invoke<AdbStatus>("adb_status"),
  installAdb: () => invoke<InstallResult>("install_adb"),
  scanNetwork: () => invoke<ScanResult>("scan_network"),

  listDevices: () => invoke<Device[]>("list_devices"),
  deviceProfile: (serial: string) =>
    invoke<Device>("device_profile", { serial }),

  connectDevice: (address: string) =>
    invoke<ConnectResult>("connect_device", { address }),
  disconnectDevice: (serial: string) =>
    invoke<ConnectResult>("disconnect_device", { serial }),

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

  disablePackage: (serial: string, pkg: string) =>
    invoke<ActionResult>("disable_package", { serial, package: pkg }),
  enablePackage: (serial: string, pkg: string) =>
    invoke<ActionResult>("enable_package", { serial, package: pkg }),
  uninstallPackage: (serial: string, pkg: string) =>
    invoke<ActionResult>("uninstall_package", { serial, package: pkg }),
  reinstallExisting: (serial: string, pkg: string) =>
    invoke<ActionResult>("reinstall_existing", { serial, package: pkg }),

  installApk: (serial: string, apkPath: string, reinstall = true) =>
    invoke<InstallApkResult>("install_apk", { serial, apkPath, reinstall }),

  listSnapshots: () => invoke<SnapshotFile[]>("list_snapshots"),
  saveSnapshot: (serial: string, deviceName: string) =>
    invoke<SnapshotFile>("save_snapshot", { serial, deviceName }),
  previewApply: (serial: string, snapshotPath: string) =>
    invoke<SnapshotApplyPlan>("preview_apply", { serial, snapshotPath }),
};
