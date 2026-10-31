// Demo fixture layer for the mobile screenshot pipeline and browser-only dev.
//
// With VITE_DEMO=1 there is no Tauri host, so this installs a stand-in for
// `window.__TAURI_INTERNALS__`. Commands the phone shares with desktop are
// answered by the desktop fixtures (one Nvidia Shield, the real merged app
// list), so both galleries describe the same TV; only the mobile-only
// commands (wireless transport, licensing, files, backups) are answered here.
//
// Wired in main.ts behind the VITE_DEMO flag and never ships in a real build.

import { handle as sharedHandle } from "../../../src/lib/demo-mock";
import type { BackupEntry, Device, FileEntry } from "./types";

const HOST = "192.168.1.42";
const PORT = 5555;
const SERIAL = `${HOST}:${PORT}`;

const backups: BackupEntry[] = [
  {
    package: "com.teamsmart.videomanager.tv",
    path: "/data/user/0/com.atvoptimizer.app/files/backups/com.teamsmart.videomanager.tv.apks",
    size_bytes: 83_800_064,
    apk_count: 3,
    complete: true,
    saved_at: "2026-10-02T19:14:08Z",
  },
  {
    package: "com.spocky.projengmenu",
    path: "/data/user/0/com.atvoptimizer.app/files/backups/com.spocky.projengmenu.apks",
    size_bytes: 24_862_720,
    apk_count: 1,
    complete: true,
    saved_at: "2026-09-21T08:40:51Z",
  },
];

let telemetryEnabled = true;

function handle(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "get_entitlement":
    case "activate_license":
      return "pro";
    case "license_info":
      return { plan: "pro", licensee: "demo@example.com", issued: "2026-09-01", expires: null, key_id: 1 };
    case "wireless_discover":
      return {
        devices: [
          { name: "adb-0323220012345", host: HOST, port: PORT, service: "_adb._tcp" },
          { name: "adb-9AB4C21D7E03-x7Kq2b", host: "192.168.1.77", port: 37215, service: "_adb-tls-connect._tcp" },
        ],
        message: "Found 2 TVs on this network.",
      };
    case "wireless_pair":
      return { ok: true, message: "Paired." };
    case "wireless_connect":
      return { ok: true, message: `Connected to ${String(args.host)}:${String(args.port)}.` };
    case "wireless_disconnect":
      return { ok: true, message: "Disconnected." };
    case "wireless_cancel_connect":
      return null;
    case "wireless_status":
      return { connected: true, serial: SERIAL, host: HOST };
    // The phone holds exactly one connection; its serial is the dialed address.
    case "list_devices": {
      const device = sharedHandle("device_profile", { serial: SERIAL }) as Device;
      // The desktop fixture carries `support` once it knows tiers; until then
      // say what the shipped rules say about a SHIELD.
      return [{
        ...device,
        serial: SERIAL,
        support: device.support ?? {
          tier: "fully_supported",
          note: "NVIDIA SHIELD is the device this app is built and tested on.",
        },
      }];
    }
    // Reports on, and the first-launch notice already dismissed so it never
    // covers a gallery frame.
    case "telemetry_settings":
    case "dismiss_telemetry_notice":
      return { enabled: telemetryEnabled, available: true, notice_seen: true };
    case "set_telemetry_enabled":
      telemetryEnabled = Boolean(args.enabled);
      return { enabled: telemetryEnabled, available: true, notice_seen: true };
    case "telemetry_action":
    case "telemetry_error":
      return null;
    case "list_remote_dir":
      return sharedHandle("list_dir", { path: args.path }) as FileEntry[];
    case "pull_file": {
      const remote = String(args.remotePath ?? "");
      return { name: remote.split("/").pop() ?? remote, path: `/storage/emulated/0/Download/${remote.split("/").pop()}`, size_bytes: 18432 };
    }
    case "list_backups":
      return backups;
    case "backup_apk":
      return backups[0];
    case "remote_warm":
      return { transport: "channel", message: "Fast control channel ready." };
    case "send_key":
    case "send_text":
    case "open_settings":
      return { ok: true, message: "", transport: "channel" };
    case "read_debug_log":
      return "INFO atv_optimizer: wireless_connect 192.168.1.42:5555 ok\nINFO atv_optimizer: list_devices 1 device";
    case "plugin:opener|open_url":
      return null;
    default:
      return sharedHandle(cmd, args);
  }
}

export function installDemoMock(): void {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args: Record<string, unknown> = {}) => Promise.resolve(handle(cmd, args)),
    transformCallback: (cb: unknown) => cb,
    unregisterCallback: () => {},
    convertFileSrc: (path: string) => path,
  };
}
