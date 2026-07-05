// Typed wrappers around Tauri's `invoke()` — the single point of contact with
// the Rust backend. Every command goes through `call()`, which logs to the
// frontend debug ring buffer (see log.ts) and mirrors to console.
//
// Keep in sync with v2/src/lib (desktop) and crates/core. Argument keys are
// camelCase; Tauri maps them to the snake_case Rust params. Note the
// `pkg` -> `package` mapping and `mode` (NOT `rebootMode`) for reboot — the
// class of silent-failure bug this module exists to prevent.

import { invoke } from "@tauri-apps/api/core";
import { logCall, summarizeArgs } from "./log";
import type {
  ActionResult,
  AppEntry,
  ConnectResult,
  Device,
  DeviceType,
  DiscoveryResult,
  Entitlement,
  HealthReport,
  OptimizeMode,
  OptimizePlan,
  OtherPackage,
  RebootMode,
  RebootResult,
  Safety,
  ScreenshotResult,
  SendTextResult,
  WirelessStatus,
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

  // ---- Licensing ----
  getEntitlement: () => call<Entitlement>("get_entitlement"),
  activateLicense: (key: string) =>
    call<Entitlement>("activate_license", { key }),

  // ---- Debug ----
  readDebugLog: () => call<string>("read_debug_log"),
};
